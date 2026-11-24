//! Testes da integração com o Claude CLI sobre o `fake-claude` (nunca a CLI real).
//!
//! Cada teste copia o binário `fake-claude` para uma pasta temporária, com o nome
//! `claude`, e escreve ao lado o cenário (`<executável>.cenario.json`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use my_harness_core::application::integracao::{
    InstrucaoConexao, Integracao, ObservadorConexao, checar_status,
};
use my_harness_core::domain::integracao::{
    ErroIntegracao, StatusIntegracao, TipoIntegracao, exigir_conectada,
};
use my_harness_core::domain::segredo::MASCARA;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;

const TOKEN: &str = "ghp_A1b2C3d4E5f6G7h8I9j0KlMnOpQrStUvWx";

/// Caminho do binário `fake-claude` do workspace, compilado se ainda não existir.
fn binario_fake() -> &'static Path {
    static CAMINHO: OnceLock<PathBuf> = OnceLock::new();
    CAMINHO.get_or_init(|| {
        let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let alvo = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| raiz.join("target"));
        let nome = format!("fake-claude{}", std::env::consts::EXE_SUFFIX);
        let caminho = alvo.join("debug").join(nome);
        if !caminho.is_file() {
            let status = Command::new(env!("CARGO"))
                .args(["build", "-p", "fake-claude"])
                .current_dir(raiz)
                .status()
                .expect("cargo build do fake-claude");
            assert!(status.success(), "fake-claude não compilou");
        }
        assert!(caminho.is_file(), "fake-claude em {}", caminho.display());
        caminho
    })
}

/// Uma CLI falsa numa pasta temporária, com o cenário dado.
struct CliFalsa {
    _pasta: TempDir,
    executavel: PathBuf,
}

impl CliFalsa {
    fn nova(cenario: Value) -> Self {
        // Pasta ao lado do binário (mesmo disco), para o link físico funcionar.
        let base = binario_fake().parent().unwrap().join("fake-claude-testes");
        std::fs::create_dir_all(&base).unwrap();
        let pasta = tempfile::tempdir_in(base).unwrap();
        let executavel = pasta
            .path()
            .join(format!("claude{}", std::env::consts::EXE_SUFFIX));
        // Link físico, não cópia: copiar abre o arquivo para escrita, e um processo aberto
        // por outro teste nesse instante herda o descritor e faz o exec falhar com
        // ETXTBSY ("Text file busy").
        if std::fs::hard_link(binario_fake(), &executavel).is_err() {
            std::fs::copy(binario_fake(), &executavel).unwrap();
        }
        let falsa = Self {
            _pasta: pasta,
            executavel,
        };
        falsa.cenario(cenario);
        falsa
    }

    fn cenario(&self, cenario: Value) {
        let mut arquivo = self.executavel.clone().into_os_string();
        arquivo.push(".cenario.json");
        std::fs::write(arquivo, cenario.to_string()).unwrap();
    }

    fn pasta(&self) -> &Path {
        self.executavel.parent().unwrap()
    }

    fn claude(&self) -> ClaudeCli {
        ClaudeCli::new(config(Some(self.executavel.clone())))
    }
}

fn config(executavel: Option<PathBuf>) -> ConfigClaudeCli {
    ConfigClaudeCli {
        executavel,
        caminho_busca: Some(Default::default()),
        tempo_checagem: Duration::from_secs(10),
        tempo_login: Duration::from_secs(10),
    }
}

#[derive(Default)]
struct Gravador(Mutex<Vec<InstrucaoConexao>>);

impl ObservadorConexao for Gravador {
    fn instrucao(&self, instrucao: InstrucaoConexao) {
        self.0.lock().unwrap().push(instrucao);
    }
}

impl Gravador {
    fn mensagens(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|i| match i {
                InstrucaoConexao::Mensagem(m) => m.clone(),
                outra => panic!("instrução inesperada: {outra:?}"),
            })
            .collect()
    }
}

// ---------- AC-02 / UC-01 FA-01: logado mostra a versão sem pedir login ----------

#[tokio::test]
async fn ac_02_logado_mostra_versao_e_conta_sem_pedir_login() {
    let cli = CliFalsa::nova(json!({
        "logado": true,
        "email": "pessoa@exemplo.com",
        "versao": { "stdout": "2.1.330 (Claude Code)\n" }
    }));
    let claude = cli.claude();

    let estado = claude.verificar().await.unwrap();
    assert_eq!(estado.tipo, TipoIntegracao::ClaudeCli);
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.versao.as_deref(), Some("2.1.330"));
    assert_eq!(estado.conta.as_deref(), Some("pessoa@exemplo.com"));
    assert_eq!(exigir_conectada(&estado), Ok(()));

    // Conectar já conectado não inicia login nem mostra instrução.
    let gravador = Gravador::default();
    let estado = claude.conectar(&gravador).await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert!(gravador.mensagens().is_empty());
}

// ---------- AC-03 / UC-01 FE-01 / ERR-01: CLI ausente ----------

#[tokio::test]
async fn ac_03_caminho_configurado_inexistente_e_err_01() {
    let pasta = tempfile::tempdir().unwrap();
    let claude = ClaudeCli::new(config(Some(pasta.path().join("claude"))));

    let checado = checar_status(&claude).await;

    assert_eq!(checado.estado.status, StatusIntegracao::NaoInstalado);
    assert_eq!(checado.erro, Some(ErroIntegracao::ClaudeCliNaoInstalado));
    assert_eq!(
        checado.erro.unwrap().to_string(),
        "Claude CLI não encontrado"
    );
    // Conectar também devolve ERR-01, sem instrução.
    let gravador = Gravador::default();
    let erro = claude.conectar(&gravador).await.unwrap_err();
    assert_eq!(erro.id(), "ERR-01");
    assert!(gravador.mensagens().is_empty());
}

#[tokio::test]
async fn ac_03_sem_claude_no_path_e_err_01() {
    let vazia = tempfile::tempdir().unwrap();
    let mut cfg = config(None);
    cfg.caminho_busca = Some(std::env::join_paths([vazia.path()]).unwrap());

    let erro = ClaudeCli::new(cfg).checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-01");
    assert_eq!(erro.codigo(), "CLAUDE_CLI_NAO_INSTALADO");
    assert_eq!(erro.estado().status, StatusIntegracao::NaoInstalado);
}

#[tokio::test]
async fn sem_caminho_acha_claude_no_path() {
    let cli = CliFalsa::nova(json!({ "logado": true }));
    let vazia = tempfile::tempdir().unwrap();
    let mut cfg = config(None);
    cfg.caminho_busca = Some(std::env::join_paths([vazia.path(), cli.pasta()]).unwrap());
    let claude = ClaudeCli::new(cfg);

    assert_eq!(claude.localizar().unwrap(), cli.executavel);
    let estado = claude.checar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.versao.as_deref(), Some("2.1.0"));
}

// ---------- AC-88 / RF-01: sem login, e o login pode ser iniciado ----------

#[tokio::test]
async fn ac_88_sem_login_mostra_versao_e_login_conecta() {
    let cli = CliFalsa::nova(json!({
        "email": "pessoa@exemplo.com",
        "login": {
            "stdout": "Opening browser to sign in…\nhttps://claude.ai/oauth/authorize?code=1\n\nLogin successful.\n"
        }
    }));
    let claude = cli.claude();

    let estado = claude.verificar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::SemLogin);
    assert_eq!(estado.versao.as_deref(), Some("2.1.0"));
    assert_eq!(estado.conta, None);
    assert_eq!(
        exigir_conectada(&estado),
        Err(ErroIntegracao::ClaudeCliDesconectado)
    );

    let gravador = Gravador::default();
    let estado = claude.conectar(&gravador).await.unwrap();

    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.conta.as_deref(), Some("pessoa@exemplo.com"));
    assert_eq!(
        gravador.mensagens(),
        [
            INSTRUCAO_INICIO_LOGIN,
            "Opening browser to sign in…",
            "https://claude.ai/oauth/authorize?code=1",
            "Login successful.",
        ]
    );
    // A CLI segue logada para a próxima checagem.
    assert_eq!(
        claude.verificar().await.unwrap().status,
        StatusIntegracao::Conectado
    );
}

// ---------- ERR-04: o login falha ----------

#[tokio::test]
async fn err_04_login_com_codigo_de_erro_fica_sem_login_com_o_motivo() {
    let cli = CliFalsa::nova(json!({
        "login": { "stderr": "detalhe\nOAuth error: invalid_grant\n", "codigo": 1 }
    }));
    let claude = cli.claude();

    let erro = claude.fazer_login(&Gravador::default()).await.unwrap_err();

    assert_eq!(erro.id(), "ERR-04");
    assert_eq!(erro.codigo(), "LOGIN_CLAUDE_CLI_FALHOU");
    assert_eq!(erro.to_string(), "OAuth error: invalid_grant");
    assert_eq!(erro.estado().status, StatusIntegracao::SemLogin);
    assert_eq!(
        checar_status(&claude).await.estado.status,
        StatusIntegracao::SemLogin
    );
}

#[tokio::test]
async fn err_04_login_que_termina_sem_logar() {
    let cli = CliFalsa::nova(json!({ "login": { "stdout": "ok\n", "loga": false } }));

    let erro = cli
        .claude()
        .conectar(&Gravador::default())
        .await
        .unwrap_err();

    assert_eq!(erro.id(), "ERR-04");
    assert_eq!(erro.status_resultante(), Some(StatusIntegracao::SemLogin));
}

#[tokio::test]
async fn err_04_login_sem_resposta_no_tempo() {
    let cli = CliFalsa::nova(json!({
        "login": { "stdout": "https://claude.ai/oauth/authorize?x\n", "dormir_ms": 5000 }
    }));
    let mut cfg = config(Some(cli.executavel.clone()));
    cfg.tempo_login = Duration::from_millis(500);
    let gravador = Gravador::default();

    let erro = ClaudeCli::new(cfg)
        .fazer_login(&gravador)
        .await
        .unwrap_err();

    assert_eq!(erro.id(), "ERR-04");
    assert_eq!(erro.to_string(), "tempo esgotado esperando o login");
    // O link chegou ao Humano antes do tempo acabar.
    assert!(
        gravador
            .mensagens()
            .contains(&"https://claude.ai/oauth/authorize?x".to_owned())
    );
}

// ---------- AC-154 / RN-04: mensagem da CLI mascarada ----------

#[tokio::test]
async fn ac_154_mensagem_de_erro_do_login_sai_com_o_token_mascarado() {
    let cli = CliFalsa::nova(json!({
        "login": {
            "stdout": format!("usando GITHUB_TOKEN={TOKEN}\n"),
            "stderr": format!("falhou: token {TOKEN} recusado\n"),
            "codigo": 2
        }
    }));
    let claude = cli.claude();
    let gravador = Gravador::default();

    let erro = claude.fazer_login(&gravador).await.unwrap_err();

    assert_eq!(erro.id(), "ERR-04");
    for texto in [
        erro.to_string(),
        format!("{erro:?}"),
        ErroIntegracao::from(erro.clone()).to_string(),
    ] {
        assert!(!texto.contains("A1b2C3d4"), "{texto}");
    }
    assert_eq!(
        erro.to_string(),
        format!("falhou: token {MASCARA} recusado")
    );
    // As instruções mostradas durante o login também saem mascaradas.
    let mensagens = gravador.mensagens();
    assert!(mensagens.iter().all(|m| !m.contains("A1b2C3d4")));
    assert!(mensagens.contains(&format!("usando GITHUB_TOKEN={MASCARA}")));
}

#[tokio::test]
async fn ac_154_motivo_do_err_53_sai_mascarado() {
    let cli = CliFalsa::nova(json!({
        "status": { "stdout": "nada de JSON", "stderr": format!("erro com {TOKEN}\n"), "codigo": 1 }
    }));

    let erro = cli.claude().checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    let texto = erro.to_string();
    assert!(!texto.contains("A1b2C3d4"), "{texto}");
    assert_eq!(
        texto,
        format!(
            "Falha inesperada com Claude CLI: status de login ilegível: erro com {MASCARA}. Tente de novo."
        )
    );
}

// ---------- ERR-53 (UC-01 FE-04, AC-153 no Claude CLI): falha inesperada ----------

#[tokio::test]
async fn err_53_checagem_sem_resposta_no_tempo_e_repetir_funciona() {
    let cli = CliFalsa::nova(json!({
        "logado": true,
        "versao": { "stdout": "2.1.0 (Claude Code)\n", "dormir_ms": 5000 }
    }));
    let mut cfg = config(Some(cli.executavel.clone()));
    cfg.tempo_checagem = Duration::from_millis(500);
    let claude = ClaudeCli::new(cfg);

    let erro = claude.checar().await.unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert_eq!(erro.codigo(), "INTEGRACAO_FALHA_INESPERADA");
    assert_eq!(erro.estado().status, StatusIntegracao::Desconectado);
    assert!(
        erro.to_string()
            .starts_with("Falha inesperada com Claude CLI: ")
    );
    assert!(erro.to_string().ends_with(". Tente de novo."));
    // Pelo trait, o estado final é `desconectado`.
    assert_eq!(
        checar_status(&claude).await.estado.status,
        StatusIntegracao::Desconectado
    );

    // A causa some (tratamento "repetir"): a próxima checagem conecta.
    cli.cenario(json!({ "logado": true }));
    assert_eq!(
        claude.checar().await.unwrap().status,
        StatusIntegracao::Conectado
    );
}

#[tokio::test]
async fn err_53_versao_ilegivel_ou_com_erro() {
    for versao in [
        json!({ "stdout": "isto não é versão\n" }),
        json!({ "stdout": "" }),
        json!({ "stderr": "quebrou\n", "codigo": 3 }),
    ] {
        let cli = CliFalsa::nova(json!({ "logado": true, "versao": versao }));
        let erro = cli.claude().checar().await.unwrap_err();
        assert_eq!(erro.id(), "ERR-53", "{versao}");
    }
}

#[tokio::test]
async fn err_53_status_de_login_ilegivel() {
    let cli = CliFalsa::nova(json!({ "status": { "stdout": "{\"semCampo\": true}\n" } }));

    let erro = cli.claude().checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert_eq!(erro.estado().status, StatusIntegracao::Desconectado);
}

// ---------- trait e desconectar ----------

#[tokio::test]
async fn trait_por_dyn_e_desconectar() {
    let cli = CliFalsa::nova(json!({ "logado": true }));
    let integracao: Box<dyn Integracao> = Box::new(cli.claude());

    assert_eq!(integracao.tipo(), TipoIntegracao::ClaudeCli);
    let estado = integracao.desconectar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    assert_eq!(estado.versao, None);
}

// ---------- unidades ----------

#[test]
fn interpreta_a_versao_da_cli() {
    assert_eq!(
        interpretar_versao("2.1.330 (Claude Code)\n").as_deref(),
        Some("2.1.330")
    );
    assert_eq!(
        interpretar_versao("\n v1.0.0-beta.1\n").as_deref(),
        Some("1.0.0-beta.1")
    );
    assert_eq!(interpretar_versao("Claude Code 2.1"), None);
    assert_eq!(interpretar_versao("2"), None);
    assert_eq!(interpretar_versao(""), None);
}

#[test]
fn err_53_sai_pelo_trait_com_o_motivo_e_o_mesmo_estado_final() {
    let erro = ErroClaudeCli::FalhaInesperada { motivo: "x".into() };
    let pelo_trait = ErroIntegracao::from(erro.clone());
    assert_eq!(
        pelo_trait,
        ErroIntegracao::inesperada(TipoIntegracao::ClaudeCli, "x")
    );
    assert_eq!(pelo_trait.id(), "ERR-53");
    assert_eq!(pelo_trait.to_string(), erro.to_string());
    assert_eq!(
        EstadoIntegracao::a_partir_do_erro(&pelo_trait),
        erro.estado()
    );
}
