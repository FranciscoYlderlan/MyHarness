//! O núcleo das integrações com adaptadores falsos e os reais de T1, T2 e T3 contra
//! processos, servidores e GitHub falsos (nenhum serviço real).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use my_harness_core::application::integracao::{InstrucaoConexao, Integracao, ObservadorConexao};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, StatusIntegracao, TipoIntegracao,
};
use my_harness_core::domain::segredo::{MASCARA, Segredo};

use super::*;
use crate::erro::{CodigoErro, ErroCatalogo, ErroIpc};
use crate::infra::claude::{ClaudeCli, ConfigClaudeCli};
use crate::infra::github::falso::GitHubFalso;
use crate::infra::github::{ConfigGitHub, GitHub};
use crate::infra::obsidian::{ConfigObsidian, ObsidianCli};
use crate::infra::secrets::{ChaveCredencial, CofreCredenciais, CofreEmMemoria};

/// Token sem prefixo conhecido: só a máscara do núcleo, com o segredo registrado, o tira.
const TOKEN: &str = "tokenDoNucleoSemPrefixo_7Hq2Lm9Zx4";

/// O que a integração falsa faz em cada operação.
#[derive(Clone)]
enum Comportamento {
    Conectada,
    Erro(ErroIntegracao),
    Panico(String),
}

/// Integração falsa: conta as chamadas e faz o que o teste mandou.
struct Falsa {
    tipo: TipoIntegracao,
    comportamento: Mutex<Comportamento>,
    chamadas: AtomicUsize,
    instrucao: Option<InstrucaoConexao>,
}

impl Falsa {
    fn nova(tipo: TipoIntegracao, comportamento: Comportamento) -> Arc<Self> {
        Arc::new(Self {
            tipo,
            comportamento: Mutex::new(comportamento),
            chamadas: AtomicUsize::new(0),
            instrucao: None,
        })
    }

    fn agir(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        self.chamadas.fetch_add(1, Ordering::SeqCst);
        match self.comportamento.lock().unwrap().clone() {
            Comportamento::Conectada => Ok(EstadoIntegracao::conectado(
                self.tipo,
                Some("conta".into()),
                Some("1.0".into()),
            )),
            Comportamento::Erro(erro) => Err(erro),
            Comportamento::Panico(texto) => panic!("{texto}"),
        }
    }
}

#[async_trait]
impl Integracao for Falsa {
    fn tipo(&self) -> TipoIntegracao {
        self.tipo
    }
    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        self.agir()
    }
    async fn conectar(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        if let Some(instrucao) = &self.instrucao {
            observador.instrucao(instrucao.clone());
        }
        self.agir()
    }
    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        // Como o Claude CLI: nada de logout, só o estado.
        Ok(EstadoIntegracao::novo(
            self.tipo,
            StatusIntegracao::Desconectado,
        ))
    }
}

#[derive(Default)]
struct Gravador(Mutex<Vec<InstrucaoConexao>>);

impl ObservadorConexao for Gravador {
    fn instrucao(&self, instrucao: InstrucaoConexao) {
        self.0.lock().unwrap().push(instrucao);
    }
}

fn montar(
    integracoes: Vec<Arc<dyn Integracao>>,
) -> (
    ServicoIntegracoes,
    Arc<RegistroEmMemoria>,
    Arc<Mutex<Vec<EstadoIntegracao>>>,
) {
    let registro = Arc::new(RegistroEmMemoria::default());
    let avisos = Arc::new(Mutex::new(Vec::new()));
    let gravados = avisos.clone();
    let servico = ServicoIntegracoes::new(
        integracoes,
        Arc::new(MascaraNucleo::default()),
        registro.clone(),
    )
    .com_aviso(Arc::new(move |estado: &EstadoIntegracao| {
        gravados.lock().unwrap().push(estado.clone())
    }));
    (servico, registro, avisos)
}

fn catalogo(erro: ErroIpc) -> ErroCatalogo {
    let ErroIpc::Catalogo { erro } = erro;
    erro
}

/// GitHub real (T3) sobre o falso, com `TOKEN` já no cofre de credenciais.
fn github_com_token() -> (Arc<GitHub>, Arc<GitHubFalso>, Arc<CofreEmMemoria>) {
    let falso = Arc::new(GitHubFalso::novo());
    let cofre = Arc::new(CofreEmMemoria::novo());
    cofre
        .gravar(&ChaveCredencial::github(), &Segredo::new(TOKEN))
        .unwrap();
    let github = Arc::new(GitHub::new(
        ConfigGitHub::nova("cid"),
        falso.clone(),
        cofre.clone(),
    ));
    (github, falso, cofre)
}

/// Porta onde ninguém escuta.
async fn url_sem_servidor() -> String {
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = ouvinte.local_addr().unwrap().port();
    drop(ouvinte);
    format!("http://127.0.0.1:{porta}/mcp")
}

// Decisão `geral-decisao-2026-11-24-erro-generico-por-fronteira`, item 3: um teste só
// prova que qualquer falha não catalogada vira ERR-53 com o motivo mascarado no registro.
// Falhas: processo que não abre (Claude CLI real), servidor MCP fora do ar (Obsidian CLI
// real), rede caída que ecoa o token (GitHub real), erro cru de um adaptador que esqueceu
// a máscara e pânico com o token na mensagem.
#[tokio::test]
async fn qualquer_falha_nao_catalogada_vira_err_53_com_o_motivo_mascarado_no_registro() {
    let pasta = tempfile::tempdir().unwrap();
    let nao_executavel = pasta.path().join("claude.txt");
    std::fs::write(&nao_executavel, "não sou um programa").unwrap();
    let claude_real: Arc<dyn Integracao> = Arc::new(ClaudeCli::new(ConfigClaudeCli {
        executavel: Some(nao_executavel),
        ..ConfigClaudeCli::default()
    }));
    let mut config = ConfigObsidian::nova(url_sem_servidor().await, "cofre-teste");
    config.tempo_limite = std::time::Duration::from_secs(2);
    let obsidian_real: Arc<dyn Integracao> = Arc::new(ObsidianCli::new(config));
    let (github_real, falso, _cofre) = github_com_token();
    falso.fora_do_ar(true);
    let github_real: Arc<dyn Integracao> = github_real;
    let cru = Falsa::nova(
        TipoIntegracao::ClaudeCli,
        Comportamento::Erro(ErroIntegracao::inesperada(
            TipoIntegracao::ClaudeCli,
            // `TextoMascarado::new` só tira prefixos conhecidos: o token sem prefixo passa.
            format!("saída ilegível: {{\"token\":\"{TOKEN}\"}}"),
        )),
    );
    let panico = Falsa::nova(
        TipoIntegracao::ObsidianCli,
        Comportamento::Panico(format!("estado impossível com {TOKEN}")),
    );

    // Duas montagens do núcleo, cada uma com o GitHub real (o token entra na máscara).
    let montagens: Vec<Vec<Arc<dyn Integracao>>> = vec![
        vec![claude_real, obsidian_real, github_real.clone()],
        vec![cru, panico, github_real],
    ];
    let mut casos = 0;
    for integracoes in montagens {
        let (servico, registro, avisos) = montar(integracoes);
        assert_eq!(servico.mascara().quantidade(), 1, "o token do GitHub");
        for tipo in TipoIntegracao::TODAS {
            for operacao in ["testar", "conectar"] {
                let antes = registro.entradas().len();
                let erro = match operacao {
                    "testar" => servico.testar(tipo).await,
                    _ => servico.conectar(tipo, Arc::new(Gravador::default())).await,
                }
                .unwrap_err();
                let erro = catalogo(erro);
                let contexto = format!("{operacao} {tipo}: {erro:?}");

                assert_eq!(
                    erro.codigo,
                    CodigoErro::IntegracaoFalhaInesperada,
                    "{contexto}"
                );
                assert_eq!(erro.id, "ERR-53", "{contexto}");
                assert_eq!(erro.parametros["integracao"], tipo.nome(), "{contexto}");
                let motivo = &erro.parametros["motivo"];
                assert!(!motivo.contains(TOKEN), "{contexto}");
                assert!(!motivo.is_empty(), "{contexto}");

                // O mesmo motivo, mascarado, no registro do núcleo.
                let entradas = registro.entradas();
                assert_eq!(entradas.len(), antes + 1, "{contexto}");
                let entrada = entradas.last().unwrap();
                assert_eq!(entrada.codigo, CodigoErro::IntegracaoFalhaInesperada);
                assert_eq!(entrada.comando, contexto_comando(operacao, tipo));
                assert_eq!(entrada.motivo.as_str(), motivo, "{contexto}");
                assert!(!entrada.to_string().contains(TOKEN), "{contexto}");

                // Estado final do ERR-53: desconectado, avisado à tela com o erro (M-46).
                let ultimo = avisos.lock().unwrap().last().cloned().unwrap();
                assert_eq!(
                    (ultimo.tipo, ultimo.status),
                    (tipo, StatusIntegracao::Desconectado)
                );
                let no_estado = ultimo.erro.as_ref().expect("erro no estado");
                assert_eq!(ErroCatalogo::from(no_estado), erro, "{contexto}");
                casos += 1;
            }
        }
        // `listar` não falha: cada falha vira o estado, com o ERR-53 e o motivo mascarado
        // no campo `erro` (M-46), e vai ao registro.
        let antes = registro.entradas().len();
        let estados = servico.listar().await;
        for estado in &estados {
            assert_eq!(estado.status, StatusIntegracao::Desconectado);
            let erro = ErroCatalogo::from(estado.erro.as_ref().expect("erro no estado"));
            assert_eq!(erro.codigo, CodigoErro::IntegracaoFalhaInesperada);
            assert!(!erro.parametros["motivo"].contains(TOKEN));
            assert!(!erro.parametros["motivo"].is_empty());
        }
        assert_eq!(registro.entradas().len(), antes + 3);
    }
    assert_eq!(casos, 12);
}

fn contexto_comando(operacao: &str, tipo: TipoIntegracao) -> String {
    match operacao {
        "testar" => format!("verificar {tipo}"),
        _ => format!("conectar {tipo}"),
    }
}

// Decisão `geral-decisao-2026-11-24-desconectar-sem-logout`: "Desconectar" o Claude CLI
// marca `desconectado` no MyHarness sem logout; a CLI segue logada, o núcleo não a
// checa até o próximo "Conectar".
#[tokio::test]
async fn desconectar_claude_cli_vale_so_no_myharness() {
    let claude = Falsa::nova(TipoIntegracao::ClaudeCli, Comportamento::Conectada);
    let (servico, registro, _) = montar(vec![claude.clone()]);

    assert_eq!(
        servico
            .testar(TipoIntegracao::ClaudeCli)
            .await
            .unwrap()
            .status,
        StatusIntegracao::Conectado
    );
    let estado = servico
        .desconectar(TipoIntegracao::ClaudeCli)
        .await
        .unwrap();
    assert_eq!(estado.status, StatusIntegracao::Desconectado);
    let chamadas = claude.chamadas.load(Ordering::SeqCst);

    // A CLI continua logada (a falsa diz conectado), mas o MyHarness guarda desconectado.
    for _ in 0..2 {
        assert_eq!(
            servico
                .testar(TipoIntegracao::ClaudeCli)
                .await
                .unwrap()
                .status,
            StatusIntegracao::Desconectado
        );
    }
    assert_eq!(
        claude.chamadas.load(Ordering::SeqCst),
        chamadas,
        "não checa"
    );
    // RN-08: desconectada no MyHarness bloqueia com ERR-05.
    let erro = catalogo(servico.exigir(TipoIntegracao::ClaudeCli).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::ClaudeCliDesconectado);

    // "Conectar" volta a usar a CLI, sem novo login (já estava logada).
    let estado = servico
        .conectar(TipoIntegracao::ClaudeCli, Arc::new(Gravador::default()))
        .await
        .unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert!(servico.exigir(TipoIntegracao::ClaudeCli).await.is_ok());
    assert!(registro.entradas().is_empty());
}

// RN-08: cada integração fora de `conectado` bloqueia com o ERR que aponta qual falta.
#[tokio::test]
async fn rn_08_bloqueia_e_aponta_a_integracao_que_falta() {
    let claude = Falsa::nova(
        TipoIntegracao::ClaudeCli,
        Comportamento::Erro(ErroIntegracao::ClaudeCliNaoInstalado),
    );
    let obsidian = Falsa::nova(TipoIntegracao::ObsidianCli, Comportamento::Conectada);
    let github = Falsa::nova(TipoIntegracao::GitHub, Comportamento::Conectada);
    let (servico, _, _) = montar(vec![claude, obsidian, github]);
    servico.desconectar(TipoIntegracao::GitHub).await.unwrap();

    // Claude CLI não instalado: a checagem já diz por quê (ERR-01).
    let erro = catalogo(servico.exigir(TipoIntegracao::ClaudeCli).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::ClaudeCliNaoInstalado);
    assert!(servico.exigir(TipoIntegracao::ObsidianCli).await.is_ok());
    let erro = catalogo(servico.exigir(TipoIntegracao::GitHub).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::GithubDesconectado);
    assert_eq!(erro.id, "ERR-06");
}

// Estado real das três, na ordem do painel, com conta e versão (UC-01 passo 1, FA-01).
#[tokio::test]
async fn listar_devolve_o_estado_real_das_tres() {
    let claude = Falsa::nova(TipoIntegracao::ClaudeCli, Comportamento::Conectada);
    let obsidian = Falsa::nova(
        TipoIntegracao::ObsidianCli,
        Comportamento::Erro(ErroIntegracao::ObsidianFechado {
            mensagem_da_cli: "The CLI is unable to find Obsidian".into(),
        }),
    );
    let (github, _, _) = github_com_token();
    // Ordem trocada de propósito: o painel mostra na ordem do contrato.
    let (servico, registro, avisos) = montar(vec![github, obsidian, claude]);

    let estados = servico.listar().await;

    let resumo: Vec<_> = estados.iter().map(|e| (e.tipo, e.status)).collect();
    assert_eq!(
        resumo,
        [
            (TipoIntegracao::ClaudeCli, StatusIntegracao::Conectado),
            (TipoIntegracao::ObsidianCli, StatusIntegracao::Desconectado),
            // Token no cofre, mas o GitHub falso não conhece a conta: token recusado.
            (TipoIntegracao::GitHub, StatusIntegracao::Desconectado),
        ]
    );
    assert_eq!(estados[0].versao.as_deref(), Some("1.0"));
    assert_eq!(estados[0].erro, None);
    // M-46: o erro que explica cada estado vem junto, para a tela mostrar o motivo.
    assert_eq!(estados[1].erro.as_ref().map(|e| e.id()), Some("ERR-03"));
    let github = ErroCatalogo::from(estados[2].erro.as_ref().unwrap());
    assert_eq!(github.id, "ERR-53");
    assert!(!github.parametros["motivo"].contains(TOKEN));
    assert_eq!(avisos.lock().unwrap().len(), 3);
    // ERR-03 é catalogado: não vai ao registro; o token recusado (ERR-53), sim.
    let codigos: Vec<_> = registro.entradas().iter().map(|e| e.codigo).collect();
    assert_eq!(codigos, [CodigoErro::IntegracaoFalhaInesperada]);
}

// Verificação da T4: `listar` mostra `não instalado` (ERR-01), não `desconectado`; o
// estado de cada falha catalogada é o da coluna "Estado final" do catálogo, com o erro.
#[tokio::test]
async fn listar_usa_o_estado_final_do_erro() {
    let claude = Falsa::nova(
        TipoIntegracao::ClaudeCli,
        Comportamento::Erro(ErroIntegracao::ClaudeCliNaoInstalado),
    );
    let obsidian = Falsa::nova(
        TipoIntegracao::ObsidianCli,
        Comportamento::Erro(ErroIntegracao::inesperada(
            TipoIntegracao::ObsidianCli,
            "cofre myHarness-vault não encontrado pela Obsidian CLI",
        )),
    );
    let github = Falsa::nova(
        TipoIntegracao::GitHub,
        Comportamento::Erro(ErroIntegracao::GithubOauthNaoConcluido(
            my_harness_core::domain::integracao::MotivoOauthNaoConcluido::Expirado,
        )),
    );
    let (servico, registro, _) = montar(vec![claude, obsidian, github]);

    let estados = servico.listar().await;

    assert_eq!(estados[0].status, StatusIntegracao::NaoInstalado);
    assert_eq!(estados[0].erro, Some(ErroIntegracao::ClaudeCliNaoInstalado));
    assert_eq!(estados[1].status, StatusIntegracao::Desconectado);
    let motivo = ErroCatalogo::from(estados[1].erro.as_ref().unwrap());
    assert_eq!(
        motivo.parametros["motivo"],
        "cofre myHarness-vault não encontrado pela Obsidian CLI"
    );
    assert_eq!(estados[2].status, StatusIntegracao::Desconectado);
    assert_eq!(estados[2].erro.as_ref().map(|e| e.id()), Some("ERR-02"));
    // Só o ERR-53 vai ao registro.
    let codigos: Vec<_> = registro.entradas().iter().map(|e| e.codigo).collect();
    assert_eq!(codigos, [CodigoErro::IntegracaoFalhaInesperada]);
}

// AC-128 na API: Obsidian aberto, `listar` e `testar` trazem o número de notas da
// Obsidian CLI real (T2) contra o servidor MCP falso.
#[tokio::test]
async fn listar_e_testar_trazem_as_notas_indexadas_da_obsidian_cli() {
    use crate::infra::mcp::servidor_falso::ServidorFalso;
    use crate::infra::obsidian::FERRAMENTA_CONTAGEM;

    let servidor = ServidorFalso::iniciar().await;
    servidor.config(|c| c.ferramentas.push(FERRAMENTA_CONTAGEM.into()));
    servidor.responder(|ferramenta, _| match ferramenta {
        FERRAMENTA_CONTAGEM => ("75\n".into(), false),
        _ => ("No matches found.".into(), false),
    });
    let obsidian: Arc<dyn Integracao> = Arc::new(ObsidianCli::new(ConfigObsidian::nova(
        servidor.url(),
        "myHarness-vault",
    )));
    let (servico, _, avisos) = montar(vec![obsidian]);

    let estado = servico.testar(TipoIntegracao::ObsidianCli).await.unwrap();
    assert_eq!(estado.status, StatusIntegracao::Conectado);
    assert_eq!(estado.notas_indexadas, Some(75));
    let listada = servico
        .listar()
        .await
        .into_iter()
        .find(|e| e.tipo == TipoIntegracao::ObsidianCli)
        .unwrap();
    assert_eq!(listada.notas_indexadas, Some(75));
    assert_eq!(
        avisos.lock().unwrap().last().unwrap().notas_indexadas,
        Some(75)
    );
    let dto = crate::dto::Integracao::from(listada);
    assert_eq!(dto.notas_indexadas, Some(75));
}

// Critério 6: o token do GitHub entra na máscara do núcleo (o guardado na abertura e o
// novo do device flow), e as instruções da conexão saem mascaradas.
#[tokio::test(start_paused = true)]
async fn token_do_github_e_segredo_guardado_na_mascara_do_nucleo() {
    let falso = Arc::new(GitHubFalso::novo());
    let cofre = Arc::new(CofreEmMemoria::novo());
    let github = Arc::new(GitHub::new(
        ConfigGitHub::nova("cid"),
        falso.clone(),
        cofre.clone(),
    ));
    let (servico, _, _) = montar(vec![github.clone()]);
    assert_eq!(
        servico.mascara().quantidade(),
        0,
        "sem token, nada a registrar"
    );

    let token = Segredo::new(TOKEN);
    falso.autorizar(&token, "octocat", &["repo"]);
    let gravador = Arc::new(Gravador::default());
    let estado = servico
        .conectar(TipoIntegracao::GitHub, gravador.clone())
        .await
        .unwrap();

    assert_eq!(estado.conta.as_deref(), Some("octocat"));
    assert_eq!(
        gravador.0.lock().unwrap().as_slice(),
        [InstrucaoConexao::CodigoDoDispositivo {
            codigo: crate::infra::github::falso::CODIGO_DO_USUARIO.into(),
            endereco: crate::infra::github::falso::ENDERECO.into(),
        }]
    );
    assert_eq!(servico.mascara().quantidade(), 1);
    assert_eq!(
        servico
            .mascara()
            .mascarar(&format!("push com {TOKEN}"))
            .as_str(),
        format!("push com {MASCARA}")
    );

    // Numa abertura nova do app, o token já guardado entra na máscara de saída.
    let (outro, _, _) = montar(vec![github]);
    assert_eq!(outro.mascara().quantidade(), 1);

    // Mensagem de login com o token: chega à tela mascarada.
    let falsa = Arc::new(Falsa {
        tipo: TipoIntegracao::ClaudeCli,
        comportamento: Mutex::new(Comportamento::Conectada),
        chamadas: AtomicUsize::new(0),
        instrucao: Some(InstrucaoConexao::Mensagem(format!("abrindo com {TOKEN}"))),
    });
    let servico = ServicoIntegracoes::new(
        vec![falsa],
        servico.mascara().clone(),
        Arc::new(RegistroEmMemoria::default()),
    );
    let gravador = Arc::new(Gravador::default());
    servico
        .conectar(TipoIntegracao::ClaudeCli, gravador.clone())
        .await
        .unwrap();
    assert_eq!(
        gravador.0.lock().unwrap().as_slice(),
        [InstrucaoConexao::Mensagem(format!("abrindo com {MASCARA}"))]
    );
}

// Integração que o app não ligou: ERR-55, registrado.
#[tokio::test]
async fn integracao_ausente_e_falha_interna() {
    let (servico, registro, _) = montar(vec![]);
    let erro = catalogo(servico.testar(TipoIntegracao::GitHub).await.unwrap_err());
    assert_eq!(erro.codigo, CodigoErro::FalhaInterna);
    assert_eq!(registro.entradas()[0].codigo, CodigoErro::FalhaInterna);
}

// Sem configuração (ex.: sem `client_id`): ERR-53 com o motivo, desconectar funciona.
#[tokio::test]
async fn sem_configuracao_e_err_53_com_o_motivo() {
    let sem = Arc::new(IntegracaoSemConfiguracao::new(
        TipoIntegracao::GitHub,
        "client_id do app OAuth não definido (MYHARNESS_GITHUB_CLIENT_ID)",
    ));
    let (servico, _, _) = montar(vec![sem]);
    let erro = catalogo(servico.testar(TipoIntegracao::GitHub).await.unwrap_err());
    assert_eq!(erro.id, "ERR-53");
    assert!(erro.parametros["motivo"].contains("MYHARNESS_GITHUB_CLIENT_ID"));
    assert_eq!(
        servico
            .desconectar(TipoIntegracao::GitHub)
            .await
            .unwrap()
            .status,
        StatusIntegracao::Desconectado
    );
}
