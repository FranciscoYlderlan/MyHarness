//! Processos externos: achar um executável no `PATH` e rodá-lo com tempo-limite,
//! juntando a saída ou acompanhando-a linha a linha.
//!
//! Não mascara nada: quem chama decide o que vai à tela ou ao log e passa pela máscara
//! (RN-04) antes.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Acha `nome` nas pastas de `caminho_busca` (o valor de `PATH`).
///
/// No Windows, tenta também cada extensão de `extensoes` (o valor de `PATHEXT`; sem ele,
/// `.COM;.EXE;.BAT;.CMD`), para achar `claude.exe` ou o `claude.cmd` do npm. Fora do
/// Windows, `extensoes` é ignorado. Devolve o primeiro arquivo que existe.
pub fn localizar_executavel(
    nome: &str,
    caminho_busca: Option<&OsStr>,
    extensoes: Option<&OsStr>,
) -> Option<PathBuf> {
    let caminho_busca = caminho_busca?;
    let candidatos = nomes_candidatos(nome, extensoes);
    std::env::split_paths(caminho_busca)
        .filter(|pasta| !pasta.as_os_str().is_empty())
        .flat_map(|pasta| candidatos.iter().map(move |c| pasta.join(c)))
        .find(|caminho| caminho.is_file())
}

fn nomes_candidatos(nome: &str, extensoes: Option<&OsStr>) -> Vec<OsString> {
    let mut nomes = Vec::new();
    if cfg!(windows) {
        let padrao = OsString::from(".COM;.EXE;.BAT;.CMD");
        let lista = extensoes.unwrap_or(&padrao).to_string_lossy().into_owned();
        for ext in lista.split(';').filter(|e| !e.is_empty()) {
            nomes.push(OsString::from(format!(
                "{nome}{}",
                ext.to_ascii_lowercase()
            )));
        }
    }
    nomes.push(OsString::from(nome));
    nomes
}

/// Saída de um processo que terminou dentro do tempo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saida {
    /// Código de saída; `None` quando o processo foi morto por sinal.
    pub codigo: Option<i32>,
    /// Stdout inteiro, como texto (bytes inválidos em UTF-8 trocados).
    pub stdout: String,
    /// Stderr inteiro, como texto.
    pub stderr: String,
}

impl Saida {
    /// `true` quando saiu com código 0.
    pub fn sucesso(&self) -> bool {
        self.codigo == Some(0)
    }
}

/// Por que o processo não produziu uma [`Saida`].
#[derive(Debug, thiserror::Error)]
pub enum FalhaProcesso {
    /// O executável não existe.
    #[error("executável não encontrado")]
    NaoEncontrado,
    /// O sistema não conseguiu abrir o processo (permissão, formato inválido...).
    #[error("não foi possível abrir o processo: {0}")]
    NaoAbriu(io::Error),
    /// Passou do tempo-limite; o processo foi encerrado.
    #[error("tempo esgotado ({0:?})")]
    TempoEsgotado(Duration),
    /// Erro de leitura da saída ou de espera do processo.
    #[error("falha ao ler o processo: {0}")]
    Leitura(io::Error),
}

/// De onde veio uma linha acompanhada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Canal {
    /// Saída padrão.
    Stdout,
    /// Saída de erro.
    Stderr,
}

/// Roda `programa` com `argumentos`, stdin fechado, e espera no máximo `tempo`.
pub async fn executar(
    programa: &Path,
    argumentos: &[&str],
    tempo: Duration,
) -> Result<Saida, FalhaProcesso> {
    executar_acompanhando(programa, argumentos, tempo, |_, _| {}).await
}

/// Como [`executar`], chamando `ao_ler` a cada linha do stdout e do stderr assim que ela
/// chega (sem o fim de linha). A [`Saida`] traz as duas saídas inteiras.
///
/// Se o processo não termina no tempo (ou a espera é abandonada), a **árvore inteira**
/// morre, não só o filho direto: o `claude.cmd` do npm roda `cmd.exe` → `node`, e um
/// shell roda os próprios filhos. No Unix, o filho abre um grupo de processos próprio e o
/// grupo recebe `SIGKILL`; no Windows, `taskkill /T /F` no filho (ver [`matar_arvore`]).
/// As tarefas que leem as linhas são abortadas junto, para nenhuma ficar presa num pipe.
pub async fn executar_acompanhando(
    programa: &Path,
    argumentos: &[&str],
    tempo: Duration,
    mut ao_ler: impl FnMut(Canal, &str),
) -> Result<Saida, FalhaProcesso> {
    let mut comando = Command::new(programa);
    comando
        .args(argumentos)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    comando.process_group(0);
    let filho = comando.spawn().map_err(|erro| match erro.kind() {
        io::ErrorKind::NotFound => FalhaProcesso::NaoEncontrado,
        _ => FalhaProcesso::NaoAbriu(erro),
    })?;
    let mut arvore = Arvore {
        filho,
        leitores: Vec::new(),
        terminou: false,
    };

    let (tx, mut rx) = mpsc::unbounded_channel::<(Canal, io::Result<String>)>();
    if let Some(stdout) = arvore.filho.stdout.take() {
        arvore
            .leitores
            .push(ler_linhas(stdout, Canal::Stdout, tx.clone()));
    }
    if let Some(stderr) = arvore.filho.stderr.take() {
        arvore
            .leitores
            .push(ler_linhas(stderr, Canal::Stderr, tx.clone()));
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    let filho = &mut arvore.filho;
    let acompanhar = async {
        while let Some((canal, linha)) = rx.recv().await {
            let linha = linha.map_err(FalhaProcesso::Leitura)?;
            ao_ler(canal, &linha);
            let destino = match canal {
                Canal::Stdout => &mut stdout,
                Canal::Stderr => &mut stderr,
            };
            destino.push_str(&linha);
            destino.push('\n');
        }
        filho.wait().await.map_err(FalhaProcesso::Leitura)
    };

    let resultado = tokio::time::timeout(tempo, acompanhar).await;
    match resultado {
        Ok(Ok(status)) => {
            arvore.terminou = true;
            Ok(Saida {
                codigo: status.code(),
                stdout,
                stderr,
            })
        }
        Ok(Err(falha)) => {
            arvore.encerrar().await;
            Err(falha)
        }
        Err(_) => {
            arvore.encerrar().await;
            Err(FalhaProcesso::TempoEsgotado(tempo))
        }
    }
}

/// O processo filho com as tarefas que leem a saída dele. Ao sair de cena sem o filho ter
/// terminado (tempo esgotado, erro, espera abandonada), mata a árvore inteira e aborta as
/// leituras.
struct Arvore {
    filho: Child,
    leitores: Vec<JoinHandle<()>>,
    terminou: bool,
}

impl Arvore {
    /// Mata a árvore, aborta as leituras e recolhe o filho (sem zumbi).
    async fn encerrar(&mut self) {
        self.matar();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.filho.wait()).await;
    }

    fn matar(&mut self) {
        if !self.terminou {
            matar_arvore(&mut self.filho);
            self.terminou = true;
        }
        for leitor in &self.leitores {
            leitor.abort();
        }
    }
}

impl Drop for Arvore {
    fn drop(&mut self) {
        self.matar();
    }
}

/// Unix: o filho é líder do próprio grupo (`process_group(0)`), então o grupo tem o id do
/// filho; `SIGKILL` no grupo leva o filho e todos os descendentes que não saíram do grupo.
/// Só roda com o filho ainda não recolhido (`id()` é `None` depois do `wait`), então o id
/// não foi reaproveitado.
#[cfg(unix)]
fn matar_arvore(filho: &mut Child) {
    use rustix::process::{Pid, Signal, kill_process_group};
    if let Some(grupo) = filho
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(Pid::from_raw)
    {
        let _ = kill_process_group(grupo, Signal::KILL);
    }
    let _ = filho.start_kill();
}

/// Windows: `taskkill /T /F /PID <filho>` mata o filho e os descendentes
/// (`claude.cmd` → `cmd.exe` → `node`). Job Object exigiria FFI (`unsafe`), proibido no
/// workspace (`unsafe_code = "forbid"`). O `Child` ainda aberto segura o handle do
/// processo, então o PID não foi reaproveitado. Bloqueia enquanto o `taskkill` roda
/// (décimos de segundo), aceitável só no caminho de falha.
#[cfg(windows)]
fn matar_arvore(filho: &mut Child) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW`: o `taskkill` não abre console.
    const SEM_JANELA: u32 = 0x0800_0000;
    if let Some(pid) = filho.id() {
        let taskkill = std::env::var_os("SystemRoot")
            .map(|raiz| PathBuf::from(raiz).join("System32").join("taskkill.exe"))
            .filter(|caminho| caminho.is_file())
            .unwrap_or_else(|| PathBuf::from("taskkill.exe"));
        let _ = std::process::Command::new(taskkill)
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(SEM_JANELA)
            .status();
    }
    let _ = filho.start_kill();
}

fn ler_linhas(
    leitor: impl AsyncRead + Unpin + Send + 'static,
    canal: Canal,
    tx: mpsc::UnboundedSender<(Canal, io::Result<String>)>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut leitor = BufReader::new(leitor);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match leitor.read_until(b'\n', &mut bytes).await {
                Ok(0) => break,
                Ok(_) => {
                    let linha = String::from_utf8_lossy(&bytes);
                    let linha = linha.trim_end_matches(['\n', '\r']).to_owned();
                    if tx.send((canal, Ok(linha))).is_err() {
                        break;
                    }
                }
                Err(erro) => {
                    let _ = tx.send((canal, Err(erro)));
                    break;
                }
            }
        }
    })
}

/// Sondas de teste: o processo ainda está vivo?
#[cfg(all(test, unix))]
pub(crate) mod sonda {
    use std::time::Duration;

    /// `true` enquanto `pid` existe e não é zumbi (zumbi já morreu, só falta recolher;
    /// num contêiner sem init que recolha, o neto órfão fica assim).
    pub(crate) fn vivo(pid: i32) -> bool {
        let Some(id) = rustix::process::Pid::from_raw(pid) else {
            return false;
        };
        if rustix::process::test_kill_process(id).is_err() {
            return false;
        }
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat
                .rsplit_once(')')
                .is_some_and(|(_, resto)| resto.trim_start().starts_with('Z')),
            Err(_) => !cfg!(target_os = "linux"),
        }
    }

    /// Espera até 3 s que `pid` morra.
    pub(crate) async fn morre(pid: i32) -> bool {
        for _ in 0..60 {
            if !vivo(pid) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::sonda::morre;
    use super::*;

    #[test]
    fn localiza_no_path_na_ordem_das_pastas() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let nome = if cfg!(windows) {
            "ferramenta.exe"
        } else {
            "ferramenta"
        };
        std::fs::write(b.path().join(nome), b"").unwrap();
        let busca = std::env::join_paths([a.path(), b.path()]).unwrap();

        let achado = localizar_executavel("ferramenta", Some(&busca), None).unwrap();

        assert_eq!(achado, b.path().join(nome));
    }

    #[test]
    fn sem_path_ou_sem_arquivo_nao_localiza() {
        let a = tempfile::tempdir().unwrap();
        std::fs::create_dir(a.path().join("ferramenta")).unwrap();
        let busca = std::env::join_paths([a.path()]).unwrap();

        assert_eq!(localizar_executavel("ferramenta", None, None), None);
        // Uma pasta com o nome não é executável.
        assert_eq!(localizar_executavel("ferramenta", Some(&busca), None), None);
    }

    #[cfg(windows)]
    #[test]
    fn no_windows_acha_o_cmd_do_npm() {
        let a = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("claude.cmd"), b"").unwrap();
        let busca = std::env::join_paths([a.path()]).unwrap();

        let achado =
            localizar_executavel("claude", Some(&busca), Some(OsStr::new(".EXE;.CMD"))).unwrap();

        assert_eq!(achado, a.path().join("claude.cmd"));
    }

    #[tokio::test]
    async fn executavel_inexistente_e_nao_encontrado() {
        let falha = executar(
            Path::new("/caminho/que/nao/existe/claude"),
            &[],
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(falha, FalhaProcesso::NaoEncontrado), "{falha:?}");
    }

    /// Prova da correção: um shell (filho) roda `sleep` (neto) e espera; no tempo
    /// esgotado, filho e neto morrem, e a chamada volta logo, sem leitura presa.
    #[cfg(unix)]
    #[tokio::test]
    async fn tempo_esgotado_mata_a_arvore_inteira() {
        let mut pids = Vec::new();
        let inicio = std::time::Instant::now();
        let falha = executar_acompanhando(
            Path::new("/bin/sh"),
            &["-c", "echo $$; sleep 47 & echo $!; wait"],
            Duration::from_millis(800),
            |_, linha| pids.push(linha.parse::<i32>().unwrap()),
        )
        .await
        .unwrap_err();
        let decorrido = inicio.elapsed();

        assert!(
            matches!(falha, FalhaProcesso::TempoEsgotado(_)),
            "{falha:?}"
        );
        assert!(decorrido < Duration::from_secs(3), "{decorrido:?}");
        let [filho, neto] = pids[..] else {
            panic!("pids: {pids:?}")
        };
        assert!(morre(filho).await, "o filho {filho} sobrou");
        assert!(morre(neto).await, "o neto {neto} (sleep 47) sobrou");
    }

    /// O neto que herda os pipes e segue vivo depois de o filho sair não prende a
    /// leitura: o tempo esgota, a árvore morre e a chamada volta.
    #[cfg(unix)]
    #[tokio::test]
    async fn neto_com_os_pipes_nao_prende_a_leitura() {
        let mut pids = Vec::new();
        let falha = executar_acompanhando(
            Path::new("/bin/sh"),
            &["-c", "sleep 47 & echo $!"],
            Duration::from_millis(800),
            |_, linha| pids.push(linha.parse::<i32>().unwrap()),
        )
        .await
        .unwrap_err();

        assert!(
            matches!(falha, FalhaProcesso::TempoEsgotado(_)),
            "{falha:?}"
        );
        let [neto] = pids[..] else {
            panic!("pids: {pids:?}")
        };
        assert!(morre(neto).await, "o neto {neto} sobrou");
    }

    /// Abandonar a espera (o futuro some antes do fim) também mata a árvore.
    #[cfg(unix)]
    #[tokio::test]
    async fn espera_abandonada_mata_a_arvore() {
        let pids = std::sync::Mutex::new(Vec::new());
        let execucao = executar_acompanhando(
            Path::new("/bin/sh"),
            &["-c", "echo $$; sleep 47 & echo $!; wait"],
            Duration::from_secs(60),
            |_, linha| pids.lock().unwrap().push(linha.parse::<i32>().unwrap()),
        );
        let _ = tokio::time::timeout(Duration::from_millis(500), execucao).await;

        let pids = pids.into_inner().unwrap();
        assert_eq!(pids.len(), 2, "{pids:?}");
        for pid in pids {
            assert!(morre(pid).await, "{pid} sobrou");
        }
    }
}
