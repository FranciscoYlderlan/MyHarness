//! O núcleo pelos comandos IPC: integrações reais (GitHub sobre o falso), RN-08 e ERR-55.

use std::sync::{Arc, Mutex};

use my_harness_core::application::integracao::Integracao as IntegracaoCore;
use my_harness_core::domain::segredo::Segredo;
use tauri::ipc::InvokeResponseBody;

use super::*;
use crate::commands::erro::ErroCatalogo;
use crate::commands::integracoes::{IntegracaoSemConfiguracao, MascaraNucleo, RegistroEmMemoria};
use crate::infra::github::falso::{CODIGO_DO_USUARIO, ENDERECO, GitHubFalso};
use crate::infra::github::{ConfigGitHub, GitHub};
use crate::infra::secrets::CofreEmMemoria;

fn nucleo() -> (NucleoMyHarness, Arc<GitHubFalso>, Arc<RegistroEmMemoria>) {
    let falso = Arc::new(GitHubFalso::novo());
    let github: Arc<dyn IntegracaoCore> = Arc::new(GitHub::new(
        ConfigGitHub::nova("cid"),
        falso.clone(),
        Arc::new(CofreEmMemoria::novo()),
    ));
    let claude: Arc<dyn IntegracaoCore> = Arc::new(IntegracaoSemConfiguracao::new(
        TipoCore::ClaudeCli,
        "claude não está no PATH do teste",
    ));
    let obsidian: Arc<dyn IntegracaoCore> = Arc::new(IntegracaoSemConfiguracao::new(
        TipoCore::ObsidianCli,
        "sem servidor no teste",
    ));
    let registro = Arc::new(RegistroEmMemoria::default());
    let servico = ServicoIntegracoes::new(
        vec![claude, obsidian, github],
        Arc::new(MascaraNucleo::default()),
        registro.clone(),
    );
    (
        NucleoMyHarness::new(servico, registro.clone()),
        falso,
        registro,
    )
}

fn catalogo(erro: ErroIpc) -> ErroCatalogo {
    let ErroIpc::Catalogo { erro } = erro;
    erro
}

/// Canal que guarda o JSON de cada instrução enviada.
fn canal() -> (
    Channel<InstrucaoConexao>,
    Arc<Mutex<Vec<serde_json::Value>>>,
) {
    let recebidas = Arc::new(Mutex::new(Vec::new()));
    let destino = recebidas.clone();
    let canal = Channel::new(move |corpo| {
        if let InvokeResponseBody::Json(json) = corpo {
            destino
                .lock()
                .unwrap()
                .push(serde_json::from_str(&json).unwrap());
        }
        Ok(())
    });
    (canal, recebidas)
}

// Comandos IPC devolvem o estado real das três; conectar manda o código do device
// flow pelo canal; desconectar volta a desconectado.
#[tokio::test(start_paused = true)]
async fn comandos_de_integracao_usam_as_integracoes_reais() {
    let (n, falso, _) = nucleo();
    let n: Arc<dyn Nucleo> = Arc::new(n);

    let lista = n.listar_integracoes().await.unwrap();
    assert_eq!(
        lista.iter().map(|i| (i.tipo, i.status)).collect::<Vec<_>>(),
        [
            (TipoIntegracao::ClaudeCli, StatusIntegracao::Desconectado),
            (TipoIntegracao::ObsidianCli, StatusIntegracao::Desconectado),
            (TipoIntegracao::GitHub, StatusIntegracao::Desconectado),
        ]
    );

    falso.autorizar(&Segredo::new("token-falso-123"), "octocat", &["repo"]);
    let (instrucoes, recebidas) = canal();
    let github = n
        .conectar_integracao(TipoIntegracao::GitHub, instrucoes)
        .await
        .unwrap();
    assert_eq!(github.status, StatusIntegracao::Conectado);
    assert_eq!(github.conta.as_deref(), Some("octocat"));
    assert_eq!(
        recebidas.lock().unwrap().as_slice(),
        [serde_json::json!({
            "tipo": "codigo_do_dispositivo",
            "codigo": CODIGO_DO_USUARIO,
            "endereco": ENDERECO,
        })]
    );
    assert_eq!(
        n.testar_integracao(TipoIntegracao::GitHub)
            .await
            .unwrap()
            .status,
        StatusIntegracao::Conectado
    );

    let github = n
        .desconectar_integracao(TipoIntegracao::GitHub)
        .await
        .unwrap();
    assert_eq!(github.status, StatusIntegracao::Desconectado);

    // ERR-53 de uma integração sem configuração chega à tela com o motivo.
    let erro = catalogo(
        n.testar_integracao(TipoIntegracao::ClaudeCli)
            .await
            .unwrap_err(),
    );
    assert_eq!(erro.codigo, CodigoErro::IntegracaoFalhaInesperada);
    assert_eq!(
        erro.parametros["motivo"],
        "claude não está no PATH do teste"
    );
}

// RN-08 nos comandos que dependem de integração: ERR-05 e ERR-07.
#[tokio::test]
async fn rn_08_bloqueia_os_comandos_que_dependem_de_integracao() {
    let (n, _, _) = nucleo();
    let saida = Channel::new(|_| Ok(()));

    // Sem Claude CLI: a checagem falha (ERR-53), e nada é enviado.
    let erro = catalogo(
        n.enviar_prompt(ChatId("c1".into()), "oi".into(), None, saida)
            .await
            .unwrap_err(),
    );
    assert_eq!(erro.codigo, CodigoErro::IntegracaoFalhaInesperada);

    // Desconectada pelo Humano: o ERR de RN-08 aponta qual integração falta.
    n.desconectar_integracao(TipoIntegracao::ClaudeCli)
        .await
        .unwrap();
    let erro = catalogo(
        n.enviar_prompt(
            ChatId("c1".into()),
            "oi".into(),
            None,
            Channel::new(|_| Ok(())),
        )
        .await
        .unwrap_err(),
    );
    assert_eq!(erro.codigo, CodigoErro::ClaudeCliDesconectado);

    n.desconectar_integracao(TipoIntegracao::ObsidianCli)
        .await
        .unwrap();
    let erro = catalogo(n.testar_cofre(CofreId("k1".into())).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::ObsidianDesconectado);
    assert_eq!(erro.id, "ERR-07");
}

// O que ainda não existe é ERR-55 (decisão `geral-decisao-2026-11-23-falha-interna`),
// com o caso no registro; nada é inventado.
#[tokio::test]
async fn o_que_nao_existe_e_err_55_e_nada_e_inventado() {
    let (n, _, registro) = nucleo();

    let erro = catalogo(n.aprovar_plano(PlanoId("p1".into())).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::FalhaInterna);
    assert_eq!(erro.id, "ERR-55");
    assert!(erro.parametros["motivo"].contains("aprovar_plano"));
    let entrada = registro.entradas().pop().unwrap();
    assert_eq!(entrada.codigo, CodigoErro::FalhaInterna);
    assert_eq!(entrada.comando, "aprovar_plano");

    assert_eq!(
        catalogo(n.ler_consumo().await.unwrap_err()).codigo,
        CodigoErro::ConsumoIndisponivel
    );
    assert!(n.listar_projetos().await.unwrap().is_empty());
}
