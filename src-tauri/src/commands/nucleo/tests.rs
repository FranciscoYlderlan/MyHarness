//! O núcleo pelos comandos IPC: integrações reais (GitHub sobre o falso), RN-08 e ERR-55.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use my_harness_core::application::integracao::Integracao as IntegracaoCore;
use my_harness_core::domain::integracao::{EstadoIntegracao, StatusIntegracao as StatusCore};
use my_harness_core::domain::segredo::Segredo;
use tauri::ipc::InvokeResponseBody;

use super::*;
use crate::commands::erro::ErroCatalogo;
use crate::commands::integracoes::{IntegracaoSemConfiguracao, MascaraNucleo, RegistroEmMemoria};
use crate::infra::github::falso::{CODIGO_DO_USUARIO, ENDERECO, GitHubFalso};
use crate::infra::github::host::RespostaToken;
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

/// Núcleo com o Claude CLI dado, os avisos de estado e o registro de operações guardados.
struct Montado {
    nucleo: Arc<dyn Nucleo>,
    falso: Arc<GitHubFalso>,
    avisos: Arc<Mutex<Vec<EstadoIntegracao>>>,
    operacoes: Arc<Mutex<Vec<String>>>,
}

impl Montado {
    fn novo(claude: Arc<dyn IntegracaoCore>) -> Self {
        let falso = Arc::new(GitHubFalso::novo());
        let github: Arc<dyn IntegracaoCore> = Arc::new(GitHub::new(
            ConfigGitHub::nova("cid"),
            falso.clone(),
            Arc::new(CofreEmMemoria::novo()),
        ));
        let obsidian: Arc<dyn IntegracaoCore> = Arc::new(IntegracaoSemConfiguracao::new(
            TipoCore::ObsidianCli,
            "sem servidor no teste",
        ));
        let registro = Arc::new(RegistroEmMemoria::default());
        let avisos = Arc::new(Mutex::new(Vec::new()));
        let gravados = avisos.clone();
        let servico = ServicoIntegracoes::new(
            vec![claude, obsidian, github],
            Arc::new(MascaraNucleo::default()),
            registro.clone(),
        )
        .com_aviso(Arc::new(move |estado: &EstadoIntegracao| {
            gravados.lock().unwrap().push(estado.clone())
        }));
        let operacoes = Arc::new(Mutex::new(Vec::new()));
        let destino = operacoes.clone();
        let nucleo = NucleoMyHarness::new(servico, registro).com_registro_de_operacoes(Arc::new(
            move |texto: &str| destino.lock().unwrap().push(texto.to_owned()),
        ));
        Self {
            nucleo: Arc::new(nucleo),
            falso,
            avisos,
            operacoes,
        }
    }

    /// Status dos avisos de `tipo`, em ordem.
    fn avisos(&self, tipo: TipoCore) -> Vec<StatusCore> {
        self.avisos
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tipo == tipo)
            .map(|e| e.status)
            .collect()
    }

    fn operacoes_com(&self, trecho: &str) -> usize {
        self.operacoes
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.contains(trecho))
            .count()
    }
}

/// Espera o canal ter `n` instruções (relógio do tokio).
async fn esperar_instrucoes(recebidas: &Mutex<Vec<serde_json::Value>>, n: usize) {
    for _ in 0..500 {
        if recebidas.lock().unwrap().len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "esperava {n} instruções; chegaram {:?}",
        recebidas.lock().unwrap()
    );
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

// ---------- AC-157 (UC-01 FA-02): conectar repetido reaproveita a conexão ----------

// GitHub: dois `conectar` ao mesmo tempo (o segundo com outro canal, como a tela
// recarregada) abrem um device flow só, mostram o mesmo código e terminam `conectado`;
// depois do tempo de vida do código, nenhum fluxo órfão desfaz o `conectado`.
#[tokio::test(start_paused = true)]
async fn ac_157_github_conectar_repetido_abre_um_fluxo_so() {
    let m = Montado::novo(Arc::new(IntegracaoSemConfiguracao::new(
        TipoCore::ClaudeCli,
        "sem claude no teste",
    )));
    let token = Segredo::new("token-falso-157");
    m.falso.conta(&token, "octocat", &["repo"]);
    m.falso.roteiro([
        RespostaToken::Pendente,
        RespostaToken::Pendente,
        RespostaToken::Concedido(token),
    ]);

    let (canal_a, recebidas_a) = canal();
    let (canal_b, recebidas_b) = canal();
    let n = m.nucleo.clone();
    let primeiro =
        tokio::spawn(async move { n.conectar_integracao(TipoIntegracao::GitHub, canal_a).await });
    esperar_instrucoes(&recebidas_a, 1).await;
    let segundo = m
        .nucleo
        .conectar_integracao(TipoIntegracao::GitHub, canal_b)
        .await
        .unwrap();
    let primeiro = primeiro.await.unwrap().unwrap();

    for estado in [&primeiro, &segundo] {
        assert_eq!(estado.status, StatusIntegracao::Conectado);
        assert_eq!(estado.conta.as_deref(), Some("octocat"));
    }
    assert_eq!(m.falso.pedidos_de_codigo().len(), 1, "um device flow só");
    let codigo = serde_json::json!({
        "tipo": "codigo_do_dispositivo",
        "codigo": CODIGO_DO_USUARIO,
        "endereco": ENDERECO,
    });
    assert_eq!(
        recebidas_a.lock().unwrap().as_slice(),
        std::slice::from_ref(&codigo)
    );
    assert_eq!(recebidas_b.lock().unwrap().as_slice(), [codigo]);
    assert_eq!(m.operacoes_com("conectar GitHub repetido: reaproveita"), 1);

    // O código venceria em 15 min: nada expira e o GitHub segue conectado.
    tokio::time::sleep(Duration::from_secs(1000)).await;
    assert_eq!(m.avisos(TipoCore::GitHub), [StatusCore::Conectado]);
    assert_eq!(
        m.nucleo
            .testar_integracao(TipoIntegracao::GitHub)
            .await
            .unwrap()
            .status,
        StatusIntegracao::Conectado
    );
}

// B2 no GitHub: "Desconectar" durante o device flow cancela; o `conectar` devolve o
// ERR-02 cancelado, cujo estado final (`desconectado`) é o que o núcleo guarda.
#[tokio::test(start_paused = true)]
async fn desconectar_durante_o_device_flow_cancela_e_devolve_o_estado_guardado() {
    let m = Montado::novo(Arc::new(IntegracaoSemConfiguracao::new(
        TipoCore::ClaudeCli,
        "sem claude no teste",
    )));
    let (canal_a, recebidas) = canal();
    let n = m.nucleo.clone();
    let conexao =
        tokio::spawn(async move { n.conectar_integracao(TipoIntegracao::GitHub, canal_a).await });
    esperar_instrucoes(&recebidas, 1).await;

    let desconectado = m
        .nucleo
        .desconectar_integracao(TipoIntegracao::GitHub)
        .await
        .unwrap();
    assert_eq!(desconectado.status, StatusIntegracao::Desconectado);
    let erro = catalogo(conexao.await.unwrap().unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::GithubOauthNaoConcluido);
    assert_eq!(erro.parametros["motivo"], "cancelado");
    assert_eq!(m.operacoes_com("desconectar GitHub durante a conexão"), 1);

    assert_eq!(m.avisos(TipoCore::GitHub), [StatusCore::Desconectado]);
    assert_eq!(
        m.nucleo
            .testar_integracao(TipoIntegracao::GitHub)
            .await
            .unwrap()
            .status,
        StatusIntegracao::Desconectado
    );
    // Um "Conectar" novo abre outro device flow (a guarda não ficou presa).
    let (canal_b, recebidas_b) = canal();
    let n = m.nucleo.clone();
    let _nova =
        tokio::spawn(async move { n.conectar_integracao(TipoIntegracao::GitHub, canal_b).await });
    esperar_instrucoes(&recebidas_b, 1).await;
    assert_eq!(m.falso.pedidos_de_codigo().len(), 2);
}
