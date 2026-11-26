//! RF-03 contra o GitHub falso (AC-01, AC-04, AC-90, AC-127, AC-129, AC-153) e a
//! busca do token em toda saída (RNF-05). O relógio do tokio fica pausado: as esperas
//! do device flow passam na hora, e os instantes das consultas são conferidos.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use my_harness_core::application::integracao::{
    InstrucaoConexao, Integracao, ObservadorConexao, checar_status,
};
use my_harness_core::domain::integracao::{
    ErroIntegracao, MotivoOauthNaoConcluido, StatusIntegracao, TipoIntegracao,
};
use my_harness_core::domain::segredo::Segredo;

use super::falso::{CODIGO_DO_USUARIO, ENDERECO, GitHubFalso};
use super::host::RespostaToken;
use super::*;
use crate::infra::secrets::{CofreCredenciais, CofreEmMemoria};

/// Token sem prefixo conhecido: só a máscara do segredo guardado o esconde.
const TOKEN: &str = "tokenfalsoSemPrefixo_Q9w8E7r6T5y4";
/// Token no formato real do OAuth do GitHub.
const TOKEN_GHO: &str = "gho_16C7e42F292c6912E7710c838347Ae178B4a";
const CLIENT_ID: &str = "Ov23li904l2z5LWigHyr";

#[derive(Default)]
struct Observador(Mutex<Vec<InstrucaoConexao>>);

impl ObservadorConexao for Observador {
    fn instrucao(&self, instrucao: InstrucaoConexao) {
        self.0.lock().unwrap().push(instrucao);
    }
}

impl Observador {
    fn instrucoes(&self) -> Vec<InstrucaoConexao> {
        self.0.lock().unwrap().clone()
    }
}

struct Cenario {
    github: Arc<GitHub>,
    falso: Arc<GitHubFalso>,
    cofre: Arc<CofreEmMemoria>,
}

impl Cenario {
    fn novo() -> Self {
        Self::com(GitHubFalso::novo())
    }

    fn com(falso: GitHubFalso) -> Self {
        let falso = Arc::new(falso);
        let cofre = Arc::new(CofreEmMemoria::novo());
        let github = Arc::new(GitHub::new(
            ConfigGitHub::nova(CLIENT_ID),
            falso.clone(),
            cofre.clone(),
        ));
        Self {
            github,
            falso,
            cofre,
        }
    }

    fn token_no_cofre(&self) -> Option<String> {
        self.cofre
            .ler(&ChaveCredencial::github())
            .unwrap()
            .map(|s| s.expor().to_string())
    }

    /// Conecta pelo device flow com `token` de `login`.
    async fn conectado(token: &str, login: &str) -> Self {
        let c = Self::novo();
        c.falso.autorizar(&Segredo::new(token), login, &["repo"]);
        c.github.conectar_com(&Observador::default()).await.unwrap();
        c
    }
}

// AC-01, AC-127: o GitHub desconectado; o OAuth conclui → conectado com o nome da conta.
#[tokio::test(start_paused = true)]
async fn ac_01_ac_127_device_flow_conclui_com_a_conta() {
    let c = Cenario::novo();
    let token = Segredo::new(TOKEN);
    c.falso.conta(&token, "octocat", &["repo", "read:org"]);
    c.falso.roteiro([
        RespostaToken::Pendente,
        RespostaToken::Concedido(token.clone()),
    ]);
    let observador = Observador::default();
    let integracao: &dyn Integracao = c.github.as_ref();
    assert_eq!(
        integracao.verificar().await.unwrap().status,
        StatusIntegracao::Desconectado
    );

    let estado = integracao.conectar(&observador).await.unwrap();

    assert_eq!(estado.tipo, TipoIntegracao::GitHub);
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.conta.as_deref(), Some("octocat"));
    // UC-01 passo 2: o código e o endereço vão ao Humano.
    assert_eq!(
        observador.instrucoes(),
        [InstrucaoConexao::CodigoDoDispositivo {
            codigo: CODIGO_DO_USUARIO.into(),
            endereco: ENDERECO.into(),
        }]
    );
    assert_eq!(
        c.falso.pedidos_de_codigo(),
        [(CLIENT_ID.to_string(), vec!["repo".to_string()])]
    );
    // D12: o token mora no cofre de credenciais.
    assert_eq!(c.token_no_cofre().as_deref(), Some(TOKEN));
}

// AC-90: o OAuth concluído; o status é checado → conectado com o nome da conta (e escopos).
#[tokio::test(start_paused = true)]
async fn ac_90_status_conectado_com_conta_e_escopos() {
    let c = Cenario::conectado(TOKEN, "octocat").await;

    let status = c.github.checar().await.unwrap();
    assert_eq!(status.estado.status, StatusIntegracao::Conectado);
    assert_eq!(status.estado.conta.as_deref(), Some("octocat"));
    assert_eq!(status.escopos, ["repo"]);

    let checado = checar_status(c.github.as_ref()).await;
    assert_eq!(checado.erro, None);
    assert_eq!(checado.estado.conta.as_deref(), Some("octocat"));
}

// UC-01 FA-01: já conectado, conectar não pede código de novo.
#[tokio::test(start_paused = true)]
async fn ja_conectado_nao_inicia_outro_device_flow() {
    let c = Cenario::conectado(TOKEN, "octocat").await;
    let observador = Observador::default();

    let status = c.github.conectar_com(&observador).await.unwrap();

    assert_eq!(status.estado.conta.as_deref(), Some("octocat"));
    assert!(observador.instrucoes().is_empty());
    assert_eq!(c.falso.pedidos_de_codigo().len(), 1);
}

// AC-04, AC-129: o Humano recusa no GitHub (`access_denied`) → desconectado, ERR-02 com o motivo.
#[tokio::test(start_paused = true)]
async fn ac_04_ac_129_access_denied_fica_desconectado_com_err_02() {
    let c = Cenario::novo();
    c.falso
        .roteiro([RespostaToken::Pendente, RespostaToken::Recusado]);
    let integracao: &dyn Integracao = c.github.as_ref();

    let erro = integracao
        .conectar(&Observador::default())
        .await
        .unwrap_err();

    assert_eq!(
        erro,
        ErroIntegracao::GithubOauthNaoConcluido(MotivoOauthNaoConcluido::Recusado)
    );
    assert_eq!(erro.id(), "ERR-02");
    assert_eq!(erro.to_string(), "Conexão com o GitHub não concluída");
    let detalhado = c.github.conectar_com(&Observador::default()).await;
    // Sem roteiro, o falso fica pendente até o código vencer: ERR-02 expirado.
    assert_eq!(
        detalhado.unwrap_err().motivo_oauth(),
        Some(MotivoOauthNaoConcluido::Expirado)
    );
    assert_eq!(
        descrever_motivo(MotivoOauthNaoConcluido::Recusado),
        "a autorização foi recusada no GitHub"
    );
    assert_eq!(
        checar_status(integracao).await.estado.status,
        StatusIntegracao::Desconectado
    );
    assert_eq!(c.token_no_cofre(), None);
}

// AC-04, AC-129: o GitHub devolve `expired_token` → desconectado, ERR-02 expirado.
#[tokio::test(start_paused = true)]
async fn ac_04_ac_129_expired_token_fica_desconectado_com_err_02() {
    let c = Cenario::novo();
    c.falso.roteiro([RespostaToken::Expirado]);

    let erro = c
        .github
        .conectar_com(&Observador::default())
        .await
        .unwrap_err();

    assert_eq!(erro.id(), "ERR-02");
    assert_eq!(erro.codigo(), "GITHUB_OAUTH_NAO_CONCLUIDO");
    assert_eq!(erro.motivo_oauth(), Some(MotivoOauthNaoConcluido::Expirado));
    assert_eq!(erro.estado_final().status, StatusIntegracao::Desconectado);
    assert_eq!(
        c.github.checar().await.unwrap().estado.status,
        StatusIntegracao::Desconectado
    );
}

// AC-04: o código vence (`expires_in`) sem resposta final → ERR-02 expirado, sem
// consultar depois do vencimento.
#[tokio::test(start_paused = true)]
async fn codigo_vencido_sem_resposta_e_err_02_expirado() {
    let mut falso = GitHubFalso::novo();
    falso.expira_em = Duration::from_secs(12);
    let c = Cenario::com(falso);
    let inicio = tokio::time::Instant::now();

    let erro = c
        .github
        .conectar_com(&Observador::default())
        .await
        .unwrap_err();

    assert_eq!(erro.motivo_oauth(), Some(MotivoOauthNaoConcluido::Expirado));
    // Consultas em 5 s e 10 s; a de 15 s cairia depois dos 12 s.
    let instantes: Vec<_> = c.falso.consultas().iter().map(|t| *t - inicio).collect();
    assert_eq!(instantes, [Duration::from_secs(5), Duration::from_secs(10)]);
    assert_eq!(
        tokio::time::Instant::now() - inicio,
        Duration::from_secs(12)
    );
}

// `authorization_pending` e `slow_down`: o intervalo é respeitado e cresce no slow_down.
#[tokio::test(start_paused = true)]
async fn respeita_o_intervalo_e_o_slow_down() {
    let c = Cenario::novo();
    let token = Segredo::new(TOKEN);
    c.falso.conta(&token, "octocat", &["repo"]);
    c.falso.roteiro([
        RespostaToken::Pendente,
        RespostaToken::Devagar {
            intervalo: Some(Duration::from_secs(10)),
        },
        RespostaToken::Pendente,
        RespostaToken::Devagar { intervalo: None },
        RespostaToken::Concedido(token),
    ]);
    let inicio = tokio::time::Instant::now();

    c.github.conectar_com(&Observador::default()).await.unwrap();

    let instantes: Vec<_> = c
        .falso
        .consultas()
        .iter()
        .map(|t| (*t - inicio).as_secs())
        .collect();
    // 5 (pendente), +5 (slow_down → 10), +10 (pendente), +10 (slow_down → 15), +15.
    assert_eq!(instantes, [5, 10, 20, 30, 45]);
}

// ERR-02 cancelado: o Humano cancela (ou desconecta) durante o device flow.
#[tokio::test(start_paused = true)]
async fn cancelar_durante_o_device_flow_e_err_02_cancelado() {
    let c = Cenario::novo();
    let github = c.github.clone();
    let tarefa = tokio::spawn(async move { github.conectar_com(&Observador::default()).await });
    tokio::time::sleep(Duration::from_secs(7)).await;

    let estado = c.github.desconectar().await.unwrap();
    let erro = tarefa.await.unwrap().unwrap_err();

    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    assert_eq!(
        erro.motivo_oauth(),
        Some(MotivoOauthNaoConcluido::Cancelado)
    );
    assert_eq!(c.falso.consultas().len(), 1);
}

// AC-153: o GitHub conectado e a rede fora do ar → desconectado com o ERR-53 e a ação de
// conectar de novo; com a rede de volta, conectar de novo funciona.
#[tokio::test(start_paused = true)]
async fn ac_153_rede_fora_do_ar_e_err_53_e_volta_ao_conectar() {
    let c = Cenario::conectado(TOKEN, "octocat").await;
    c.falso.fora_do_ar(true);

    let erro = c.github.checar().await.unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert_eq!(erro.codigo(), "INTEGRACAO_FALHA_INESPERADA");
    assert_eq!(erro.estado_final().status, StatusIntegracao::Desconectado);
    let mensagem = erro.to_string();
    assert!(
        mensagem.starts_with("Falha inesperada com GitHub: erro de rede com o GitHub (dns error"),
        "{mensagem}"
    );
    assert!(mensagem.ends_with(". Tente de novo."), "{mensagem}");
    assert!(!mensagem.contains(TOKEN), "{mensagem}");
    // Pelo trait: o estado final do ERR-53.
    let integracao: &dyn Integracao = c.github.as_ref();
    assert_eq!(
        integracao.verificar().await.unwrap().status,
        StatusIntegracao::Desconectado
    );
    assert_eq!(
        integracao
            .conectar(&Observador::default())
            .await
            .unwrap()
            .status,
        StatusIntegracao::Desconectado
    );

    c.falso.fora_do_ar(false);
    let observador = Observador::default();
    let estado = integracao.conectar(&observador).await.unwrap();

    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.conta.as_deref(), Some("octocat"));
    // O token continuou no cofre: não precisou de outro código.
    assert!(observador.instrucoes().is_empty());
}

// AC-153 no device flow: a rede cai durante as consultas → ERR-53.
#[tokio::test(start_paused = true)]
async fn rede_fora_do_ar_no_device_flow_e_err_53() {
    let c = Cenario::novo();
    let github = c.github.clone();
    let tarefa = tokio::spawn(async move { github.conectar_com(&Observador::default()).await });
    tokio::time::sleep(Duration::from_secs(6)).await;
    c.falso.fora_do_ar(true);

    let erro = tarefa.await.unwrap().unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert!(!erro.to_string().contains("device-code-falso"), "{erro}");
}

// Token revogado no GitHub: o status cai para desconectado (ERR-53) e conectar começa
// um device flow novo, trocando o token.
#[tokio::test(start_paused = true)]
async fn token_revogado_pede_conexao_nova() {
    let c = Cenario::conectado(TOKEN, "octocat").await;
    c.falso.revogar(&Segredo::new(TOKEN));

    let erro = c.github.checar().await.unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("recusou o token"), "{erro}");

    c.falso
        .autorizar(&Segredo::new(TOKEN_GHO), "octocat", &["repo"]);
    let observador = Observador::default();
    let status = c.github.conectar_com(&observador).await.unwrap();

    assert_eq!(status.estado.status, StatusIntegracao::Conectado);
    assert_eq!(observador.instrucoes().len(), 1);
    assert_eq!(c.token_no_cofre().as_deref(), Some(TOKEN_GHO));
}

// Erro do protocolo fora do ERR-02 (ex.: device flow desligado no app) → ERR-53.
#[tokio::test(start_paused = true)]
async fn erro_do_protocolo_e_err_53() {
    let c = Cenario::novo();
    c.falso.falhar_ao_pedir("device_flow_disabled");
    let erro = c
        .github
        .conectar_com(&Observador::default())
        .await
        .unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("device_flow_disabled"), "{erro}");

    c.falso.roteiro([RespostaToken::Erro {
        codigo: "incorrect_client_credentials".into(),
        descricao: None,
    }]);
    let erro = c
        .github
        .conectar_com(&Observador::default())
        .await
        .unwrap_err();
    assert_eq!(erro.id(), "ERR-53");
    assert!(
        erro.to_string().contains("incorrect_client_credentials"),
        "{erro}"
    );
}

// Cofre de credenciais falhando ao guardar → ERR-53 e nada conectado.
#[tokio::test(start_paused = true)]
async fn cofre_falhando_ao_guardar_e_err_53() {
    let c = Cenario::novo();
    c.falso
        .autorizar(&Segredo::new(TOKEN), "octocat", &["repo"]);
    let cofre = c.cofre.clone();
    let github = c.github.clone();
    let tarefa = tokio::spawn(async move { github.conectar_com(&Observador::default()).await });
    // Depois da leitura inicial do cofre, antes da consulta do token (5 s).
    tokio::time::sleep(Duration::from_secs(1)).await;
    cofre.quebrar(Some(&format!("acesso negado ao gravar {TOKEN}")));

    let erro = tarefa.await.unwrap().unwrap_err();

    assert_eq!(erro.id(), "ERR-53");
    assert!(erro.to_string().contains("cofre de credenciais"), "{erro}");
    assert!(!erro.to_string().contains(TOKEN), "{erro}");
    cofre.quebrar(None);
    assert_eq!(c.token_no_cofre(), None);
}

// Desconectar apaga o token do cofre.
#[tokio::test(start_paused = true)]
async fn desconectar_apaga_o_token() {
    let c = Cenario::conectado(TOKEN, "octocat").await;

    let estado = c.github.desconectar().await.unwrap();

    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    assert_eq!(c.token_no_cofre(), None);
    assert_eq!(
        c.github.checar().await.unwrap().estado.status,
        StatusIntegracao::Desconectado
    );
}

// RNF-05: o token não aparece em nenhuma saída de `Debug`, erro, instrução ou status,
// nem com o prefixo `gho_` nem sem prefixo.
#[tokio::test(start_paused = true)]
async fn rnf_05_token_nunca_em_texto_puro() {
    for token in [TOKEN, TOKEN_GHO] {
        let c = Cenario::novo();
        let segredo = Segredo::new(token);
        c.falso.conta(&segredo, "octocat", &["repo"]);
        c.falso.roteiro([
            RespostaToken::Pendente,
            RespostaToken::Concedido(segredo.clone()),
        ]);
        let observador = Observador::default();
        let mut saidas: Vec<String> = Vec::new();

        let status = c.github.conectar_com(&observador).await.unwrap();
        saidas.push(format!("{status:?} {status:#?}"));
        saidas.push(format!("{:?}", observador.instrucoes()));
        saidas.push(format!("{:?} {:#?}", c.github, c.github));
        saidas.push(format!("{:?}", c.cofre));
        saidas.push(format!("{:?}", c.falso));
        saidas.push(format!("{:?}", c.github.token().unwrap()));
        saidas.push(format!("{:?}", RespostaToken::Concedido(segredo.clone())));

        // Erros com o token na mão: rede caída (o falso ecoa o token na mensagem),
        // cofre falhando com o token no motivo e token revogado.
        c.falso.fora_do_ar(true);
        for erro in [
            c.github.checar().await.unwrap_err(),
            c.github
                .conectar_com(&Observador::default())
                .await
                .unwrap_err(),
        ] {
            saidas.push(format!("{erro} {erro:?} {erro:#?}"));
        }
        c.falso.fora_do_ar(false);
        // Cofre que ecoa o token ao falhar na gravação (o único ponto em que o cofre
        // recebe o token): o motivo sai mascarado.
        let outro = Cenario::novo();
        outro.falso.autorizar(&segredo, "octocat", &["repo"]);
        let (cofre, github) = (outro.cofre.clone(), outro.github.clone());
        let tarefa = tokio::spawn(async move { github.conectar_com(&Observador::default()).await });
        tokio::time::sleep(Duration::from_secs(1)).await;
        cofre.quebrar(Some(&format!("falhou ao gravar {token}")));
        let erro = tarefa.await.unwrap().unwrap_err();
        saidas.push(format!("{erro} {erro:?}"));
        c.falso.revogar(&segredo);
        let erro = c.github.checar().await.unwrap_err();
        saidas.push(format!("{erro} {erro:?}"));
        let checado = checar_status(c.github.as_ref()).await;
        saidas.push(format!("{checado:?}"));

        for saida in &saidas {
            assert!(!saida.contains(token), "token em texto puro: {saida}");
            assert!(!saida.contains(&token[6..]), "pedaço do token: {saida}");
        }
        // O token continua disponível só pelo `Segredo`.
        assert_eq!(c.token_no_cofre().as_deref(), Some(token));
    }
}

#[test]
fn config_do_ambiente_e_escopos_padrao() {
    let config = ConfigGitHub::nova(CLIENT_ID);
    assert_eq!(config.escopos, ["repo"]);
    assert_eq!(config.chave, ChaveCredencial::github());
    assert_eq!(VARIAVEL_CLIENT_ID, "MYHARNESS_GITHUB_CLIENT_ID");
}
