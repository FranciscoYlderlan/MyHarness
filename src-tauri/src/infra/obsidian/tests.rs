//! RF-02 contra um servidor MCP falso da Obsidian CLI (AC-05, AC-89, AC-128, AC-153, AC-154).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use my_harness_core::application::integracao::{
    InstrucaoConexao, Integracao, ObservadorConexao, checar_status,
};
use my_harness_core::domain::integracao::{StatusIntegracao, TipoIntegracao};
use my_harness_core::domain::segredo::MASCARA;
use serde_json::json;

use super::*;
use crate::infra::mcp::servidor_falso::ServidorFalso;

const COFRE: &str = "myHarness-vault";
const FECHADO: &str =
    "The CLI is unable to find Obsidian. Please make sure Obsidian is running and try again.";
const TOKEN: &str = "ghp_A1b2C3d4E5f6G7h8I9j0KlMnOpQrStUvWx";

struct SemInstrucao;
impl ObservadorConexao for SemInstrucao {
    fn instrucao(&self, _: InstrucaoConexao) {
        panic!("a Obsidian CLI não mostra instrução ao Humano");
    }
}

fn config(url: &str) -> ConfigObsidian {
    ConfigObsidian {
        tempo_limite: Duration::from_secs(2),
        espera_carregando: Duration::from_millis(10),
        ..ConfigObsidian::nova(url, COFRE)
    }
}

fn com_contagem(servidor: &ServidorFalso, notas: &'static str) {
    servidor.config(|c| c.ferramentas.push(FERRAMENTA_CONTAGEM.into()));
    servidor.responder(move |ferramenta, _| match ferramenta {
        FERRAMENTA_CONTAGEM => (notas.into(), false),
        _ => ("No matches found.".into(), false),
    });
}

// AC-128: Obsidian aberto → aparece o número de notas indexadas.
#[tokio::test]
async fn aberto_conecta_e_conta_as_notas_do_cofre() {
    let servidor = ServidorFalso::iniciar().await;
    com_contagem(&servidor, "128\n");
    let obsidian = ObsidianCli::new(config(&servidor.url()));

    let status = obsidian.checar().await.unwrap();

    assert_eq!(status.estado.tipo, TipoIntegracao::ObsidianCli);
    assert_eq!(status.estado.status, StatusIntegracao::Conectado);
    assert_eq!(status.cofre, COFRE);
    assert_eq!(status.notas_indexadas, Some(128));
    assert_eq!(
        servidor.chamadas(),
        [(
            FERRAMENTA_CONTAGEM.to_string(),
            json!({ "vault": COFRE, "ext": "md", "total": true })
        )]
    );
    // M-46: o número chega pelo trait, no estado da integração (AC-128 na API).
    let estado = obsidian.verificar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.notas_indexadas, Some(128));
    assert_eq!(status.estado.notas_indexadas, Some(128));
    let estado = obsidian.conectar(&SemInstrucao).await.unwrap();
    assert_eq!(estado.notas_indexadas, Some(128));
}

#[tokio::test]
async fn sem_ferramenta_de_contagem_sonda_pela_busca_no_cofre() {
    let servidor = ServidorFalso::iniciar().await;
    let obsidian = ObsidianCli::new(config(&servidor.url()));

    let status = obsidian.checar().await.unwrap();

    assert_eq!(status.estado.status, StatusIntegracao::Conectado);
    assert_eq!(status.notas_indexadas, None);
    let chamadas = servidor.chamadas();
    assert_eq!(chamadas.len(), 1);
    assert_eq!(chamadas[0].0, FERRAMENTA_BUSCA);
    assert_eq!(chamadas[0].1["vault"], COFRE);
}

// AC-05 e AC-89: Obsidian fechado → desconectado, com a mensagem da CLI (ERR-03).
#[tokio::test]
async fn fechado_da_err_03_com_a_mensagem_da_cli() {
    let servidor = ServidorFalso::iniciar().await;
    servidor.responder(|_, _| (FECHADO.into(), true));
    let obsidian = ObsidianCli::new(config(&servidor.url()));

    let erro = obsidian.checar().await.unwrap_err();
    assert_eq!(erro.id(), "ERR-03");
    assert_eq!(erro.codigo(), "OBSIDIAN_FECHADO");
    assert_eq!(erro.to_string(), FECHADO);

    // Pelo trait, como o núcleo checa no passo 1 do UC-01.
    let checado = checar_status(&obsidian).await;
    assert_eq!(checado.estado.status, StatusIntegracao::Desconectado);
    let erro = checado.erro.unwrap();
    assert_eq!(erro.id(), "ERR-03");
    assert_eq!(erro.to_string(), FECHADO);
}

// AC-154: mensagem da CLI com token do GitHub sai com o token como ••••.
#[tokio::test]
async fn token_na_mensagem_da_cli_sai_mascarado_no_err_03_e_no_err_53() {
    let servidor = ServidorFalso::iniciar().await;
    servidor.responder(|_, _| (format!("{FECHADO} token={TOKEN}"), true));
    let obsidian = ObsidianCli::new(config(&servidor.url()));
    let erro_03 = obsidian.verificar().await.unwrap_err();

    servidor.responder(|_, _| (format!("Error: falhou com {TOKEN}"), true));
    let erro_53 = obsidian.checar().await.unwrap_err();
    assert_eq!(erro_53.id(), "ERR-53");

    for saida in [
        erro_03.to_string(),
        format!("{erro_03:?}"),
        erro_53.to_string(),
        format!("{erro_53:?}"),
    ] {
        assert!(!saida.contains("A1b2C3d4"), "{saida}");
        assert!(saida.contains(MASCARA), "{saida}");
    }
    assert_eq!(erro_03.to_string(), format!("{FECHADO} token={MASCARA}"));
}

// AC-153: servidor fora do ar → ERR-53 e desconectado; de volta, conectar funciona.
#[tokio::test]
async fn servidor_fora_do_ar_da_err_53_e_conectar_de_novo_funciona() {
    let porta = ServidorFalso::porta_livre().await;
    let url = format!("http://127.0.0.1:{porta}/mcp");
    let obsidian = ObsidianCli::new(ConfigObsidian {
        tempo_limite: ServidorFalso::LIMITE_FORA_DO_AR,
        ..config(&url)
    });

    let erro = obsidian.testar().await.unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert_eq!(erro.codigo(), "INTEGRACAO_FALHA_INESPERADA");
    assert_eq!(erro.estado_final().status, StatusIntegracao::Desconectado);
    let texto = erro.to_string();
    assert!(
        texto.starts_with("Falha inesperada com Obsidian CLI: servidor MCP não responde"),
        "{texto}"
    );
    assert!(texto.ends_with(". Tente de novo."), "{texto}");
    // Pelo trait, o mesmo ERR-53, com o motivo; o estado final é desconectado.
    let pelo_trait = obsidian.conectar(&SemInstrucao).await.unwrap_err();
    assert_eq!(pelo_trait.id(), "ERR-53");
    assert_eq!(pelo_trait.to_string(), texto);
    assert_eq!(
        pelo_trait.status_resultante(),
        Some(StatusIntegracao::Desconectado)
    );

    let servidor = ServidorFalso::iniciar_na_porta(porta).await;
    com_contagem(&servidor, "7");
    let estado = obsidian.conectar(&SemInstrucao).await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(obsidian.testar().await.unwrap().notas_indexadas, Some(7));
}

#[tokio::test]
async fn servidor_mudo_da_err_53_por_tempo_esgotado() {
    let servidor = ServidorFalso::iniciar().await;
    servidor.config(|c| c.atraso = Duration::from_millis(500));
    let obsidian = ObsidianCli::new(ConfigObsidian {
        tempo_limite: Duration::from_millis(100),
        ..config(&servidor.url())
    });

    let erro = obsidian.checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("tempo esgotado"), "{erro}");
}

#[tokio::test]
async fn recem_aberto_espera_os_comandos_carregarem() {
    let servidor = ServidorFalso::iniciar().await;
    let vezes = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&vezes);
    servidor.responder(move |_, _| {
        if contador.fetch_add(1, Ordering::SeqCst) < 2 {
            (
                "Command \"search\" not found. Did you mean: search:open?".into(),
                true,
            )
        } else {
            ("No matches found.".into(), false)
        }
    });
    let obsidian = ObsidianCli::new(config(&servidor.url()));

    let status = obsidian.checar().await.unwrap();

    assert_eq!(status.estado.status, StatusIntegracao::Conectado);
    assert_eq!(vezes.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn comandos_que_nunca_carregam_dao_err_53() {
    let servidor = ServidorFalso::iniciar().await;
    servidor.responder(|_, _| ("Command \"files\" not found.".into(), true));
    let obsidian = ObsidianCli::new(ConfigObsidian {
        tentativas_carregando: 1,
        ..config(&servidor.url())
    });

    let erro = obsidian.checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("carregando"), "{erro}");
    assert_eq!(servidor.chamadas().len(), 2);
}

#[tokio::test]
async fn cofre_desconhecido_da_err_53_citando_o_cofre() {
    let servidor = ServidorFalso::iniciar().await;
    servidor.responder(|_, _| ("Vault not found.".into(), false));
    let obsidian = ObsidianCli::new(config(&servidor.url()));

    let erro = obsidian.checar().await.unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert!(
        erro.to_string()
            .contains("cofre myHarness-vault não encontrado"),
        "{erro}"
    );
}

#[tokio::test]
async fn saidas_que_nao_dao_para_ler_dao_err_53() {
    // Contagem que não é número.
    let servidor = ServidorFalso::iniciar().await;
    com_contagem(&servidor, "muitas");
    let erro = ObsidianCli::new(config(&servidor.url()))
        .checar()
        .await
        .unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("muitas"), "{erro}");

    // Servidor sem as ferramentas da Obsidian CLI.
    let servidor = ServidorFalso::iniciar().await;
    servidor.config(|c| c.ferramentas = vec!["outra".into()]);
    let erro = ObsidianCli::new(config(&servidor.url()))
        .checar()
        .await
        .unwrap_err();
    assert_eq!(erro.id(), "ERR-53");

    // Corpo que não é JSON-RPC.
    let servidor = ServidorFalso::iniciar().await;
    servidor.config(|c| c.formato = crate::infra::mcp::servidor_falso::Formato::Lixo);
    let erro = ObsidianCli::new(config(&servidor.url()))
        .checar()
        .await
        .unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("não dá para ler"), "{erro}");
}

#[tokio::test]
async fn desconectar_fica_desconectado_ate_conectar_de_novo() {
    let servidor = ServidorFalso::iniciar().await;
    let obsidian = ObsidianCli::new(config(&servidor.url()));
    let integracao: &dyn Integracao = &obsidian;
    assert_eq!(integracao.tipo(), TipoIntegracao::ObsidianCli);

    let estado = integracao.desconectar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    let estado = integracao.verificar().await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    assert!(
        servidor.chamadas().is_empty(),
        "desconectada não fala com o servidor"
    );

    let estado = integracao.conectar(&SemInstrucao).await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert!(!obsidian.esta_desconectada());
}

#[test]
fn config_do_ambiente_e_padroes() {
    let c = ConfigObsidian::nova("http://h:1/mcp", COFRE);
    assert_eq!(c.tempo_limite, Duration::from_secs(5));
    assert_eq!(c.tentativas_carregando, 3);
    assert_eq!(VARIAVEL_URL, "OBSIDIAN_MCP_URL");
}

#[test]
fn le_as_respostas_conhecidas_da_cli() {
    assert_eq!(ler_resposta(FECHADO, true), Leitura::ObsidianFechado);
    assert_eq!(ler_resposta(FECHADO, false), Leitura::ObsidianFechado);
    assert_eq!(
        ler_resposta(
            "Command \"read\" not found. Did you mean: sync:read, daily:read, template:read?",
            true
        ),
        Leitura::Carregando
    );
    assert_eq!(
        ler_resposta("Vault not found.", false),
        Leitura::CofreNaoEncontrado
    );
    assert_eq!(ler_resposta("No matches found.", false), Leitura::Ok);
    assert_eq!(ler_resposta("qualquer erro", true), Leitura::Erro);
}
