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
use tokio::process::Command;
use tokio::sync::mpsc;

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
pub async fn executar_acompanhando(
    programa: &Path,
    argumentos: &[&str],
    tempo: Duration,
    mut ao_ler: impl FnMut(Canal, &str),
) -> Result<Saida, FalhaProcesso> {
    let mut filho = Command::new(programa)
        .args(argumentos)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|erro| match erro.kind() {
            io::ErrorKind::NotFound => FalhaProcesso::NaoEncontrado,
            _ => FalhaProcesso::NaoAbriu(erro),
        })?;

    let (tx, mut rx) = mpsc::unbounded_channel::<(Canal, io::Result<String>)>();
    if let Some(stdout) = filho.stdout.take() {
        ler_linhas(stdout, Canal::Stdout, tx.clone());
    }
    if let Some(stderr) = filho.stderr.take() {
        ler_linhas(stderr, Canal::Stderr, tx.clone());
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
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

    match tokio::time::timeout(tempo, acompanhar).await {
        Ok(Ok(status)) => Ok(Saida {
            codigo: status.code(),
            stdout,
            stderr,
        }),
        Ok(Err(falha)) => Err(falha),
        Err(_) => {
            // `kill_on_drop` garante o encerramento mesmo se o kill falhar aqui.
            let _ = filho.kill().await;
            Err(FalhaProcesso::TempoEsgotado(tempo))
        }
    }
}

fn ler_linhas(
    leitor: impl AsyncRead + Unpin + Send + 'static,
    canal: Canal,
    tx: mpsc::UnboundedSender<(Canal, io::Result<String>)>,
) {
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
    });
}

#[cfg(test)]
mod tests {
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
}
