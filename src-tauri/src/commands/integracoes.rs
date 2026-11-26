//! O núcleo das integrações (UC-01, ENT-13, RN-04, RN-08): liga Claude CLI, Obsidian CLI
//! e GitHub (T1, T2, T3) aos comandos `listar`, `testar`, `conectar` e `desconectar`.
//!
//! - **Erro genérico por fronteira** (decisão `geral-decisao-2026-11-24-erro-generico-por-fronteira`):
//!   qualquer falha sem ERR próprio sai como ERR-53, inclusive o pânico de um adaptador,
//!   com o motivo mascarado na mensagem e no registro do núcleo ([`RegistroNucleo`]).
//! - **Máscara do núcleo** ([`MascaraNucleo`], RN-04): além dos prefixos conhecidos, o
//!   valor exato de todo segredo que o núcleo guarda (o token do GitHub) some de todo
//!   motivo, instrução e registro.
//! - **Desconectar sem logout** (decisão `geral-decisao-2026-11-24-desconectar-sem-logout`):
//!   a integração desconectada pelo Humano fica `desconectado` no MyHarness até o
//!   próximo "Conectar", sem que o núcleo volte a checá-la; o login da CLI fora do app
//!   continua valendo.
//! - **RN-08** ([`ServicoIntegracoes::exigir`]): ação que depende de uma integração só
//!   segue com ela `conectado`; senão, ERR-05, ERR-06 ou ERR-07.

use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use my_harness_core::application::integracao::{InstrucaoConexao, Integracao, ObservadorConexao};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, StatusIntegracao, TipoIntegracao, exigir_conectada,
};
use my_harness_core::domain::segredo::{Segredo, TextoMascarado};

use super::erro::{CodigoErro, ErroIpc};
use crate::infra;

// ---------------------------------------------------------------------------
// Máscara do núcleo (RN-04)
// ---------------------------------------------------------------------------

/// Segredos que o núcleo guarda, para a máscara tirar o valor exato deles de todo texto
/// (RN-04, decisão `geral-decisao-2026-11-20-mascaramento-sem-adivinhar`, regra 2).
#[derive(Default)]
pub struct MascaraNucleo {
    segredos: RwLock<Vec<Segredo>>,
}

impl fmt::Debug for MascaraNucleo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.segredos.read().map(|s| s.len()).unwrap_or(0);
        write!(f, "MascaraNucleo({n} segredos)")
    }
}

impl MascaraNucleo {
    /// Passa a mascarar `segredo` (vazio e repetido são ignorados).
    pub fn registrar(&self, segredo: Segredo) {
        if segredo.expor().is_empty() {
            return;
        }
        let mut segredos = self.segredos.write().unwrap_or_else(|e| e.into_inner());
        if !segredos.contains(&segredo) {
            segredos.push(segredo);
        }
    }

    /// Quantos segredos estão registrados.
    pub fn quantidade(&self) -> usize {
        self.segredos.read().map(|s| s.len()).unwrap_or(0)
    }

    /// `texto` com os prefixos conhecidos e os segredos registrados trocados por `••••`.
    pub fn mascarar(&self, texto: &str) -> TextoMascarado {
        let segredos = self.segredos.read().unwrap_or_else(|e| e.into_inner());
        let refs: Vec<&Segredo> = segredos.iter().collect();
        TextoMascarado::com_segredos(texto, &refs)
    }

    /// O mesmo erro, com todo texto livre passado de novo pela máscara do núcleo.
    pub fn mascarar_erro(&self, erro: ErroIntegracao) -> ErroIntegracao {
        match erro {
            ErroIntegracao::ObsidianFechado { mensagem_da_cli } => {
                ErroIntegracao::ObsidianFechado {
                    mensagem_da_cli: self.mascarar(mensagem_da_cli.as_str()),
                }
            }
            ErroIntegracao::LoginClaudeCliFalhou { motivo } => {
                ErroIntegracao::LoginClaudeCliFalhou {
                    motivo: self.mascarar(motivo.as_str()),
                }
            }
            ErroIntegracao::FalhaInesperada { integracao, motivo } => {
                ErroIntegracao::FalhaInesperada {
                    integracao,
                    motivo: self.mascarar(motivo.as_str()),
                }
            }
            outro => outro,
        }
    }
}

// ---------------------------------------------------------------------------
// Registro do núcleo (onde ERR-53 e ERR-55 deixam o motivo)
// ---------------------------------------------------------------------------

/// Um caso registrado pelo núcleo: o ERR, a operação e o motivo mascarado.
///
/// O Log do contrato (ENT-12) exige `execucao`, e checar ou conectar uma integração não
/// é uma execução de agente: até o contrato dizer onde fica esse caso, o núcleo o
/// entrega a um [`RegistroNucleo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaRegistro {
    /// Código do catálogo (ERR-53 ou ERR-55).
    pub codigo: CodigoErro,
    /// Operação em que aconteceu (ex.: `verificar GitHub`).
    pub comando: String,
    /// Motivo, mascarado (RN-04).
    pub motivo: TextoMascarado,
}

impl fmt::Display for EntradaRegistro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} em `{}`: {}",
            self.codigo.id(),
            self.codigo.codigo(),
            self.comando,
            self.motivo
        )
    }
}

/// Para onde vão os casos que o núcleo registra.
pub trait RegistroNucleo: Send + Sync {
    /// Guarda o caso; não falha nem bloqueia.
    fn registrar(&self, entrada: EntradaRegistro);
}

/// Registro na saída de erro do processo (o app ainda não tem onde gravar o caso: ver
/// [`EntradaRegistro`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct RegistroNoTerminal;

impl RegistroNucleo for RegistroNoTerminal {
    fn registrar(&self, entrada: EntradaRegistro) {
        eprintln!("[myharness] {entrada}");
    }
}

/// Registro em memória, para os testes.
#[derive(Debug, Default)]
pub struct RegistroEmMemoria(Mutex<Vec<EntradaRegistro>>);

impl RegistroEmMemoria {
    /// Os casos registrados, em ordem.
    pub fn entradas(&self) -> Vec<EntradaRegistro> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl RegistroNucleo for RegistroEmMemoria {
    fn registrar(&self, entrada: EntradaRegistro) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entrada);
    }
}

// ---------------------------------------------------------------------------
// Serviço das integrações
// ---------------------------------------------------------------------------

/// Quem recebe cada estado novo de integração (o evento `IntegracaoMudou` no app).
pub type AvisoEstado = Arc<dyn Fn(&EstadoIntegracao) + Send + Sync>;

/// Repassa as instruções da conexão com o texto passado pela máscara do núcleo.
struct ObservadorMascarado {
    destino: Arc<dyn ObservadorConexao>,
    mascara: Arc<MascaraNucleo>,
}

impl ObservadorConexao for ObservadorMascarado {
    fn instrucao(&self, instrucao: InstrucaoConexao) {
        let instrucao = match instrucao {
            InstrucaoConexao::Mensagem(texto) => {
                InstrucaoConexao::Mensagem(self.mascara.mascarar(&texto).as_str().to_owned())
            }
            outra => outra,
        };
        self.destino.instrucao(instrucao);
    }
}

/// As três integrações atrás do trait [`Integracao`], com o estado que o núcleo guarda.
pub struct ServicoIntegracoes {
    integracoes: Vec<Arc<dyn Integracao>>,
    desconectadas: Mutex<HashSet<TipoIntegracao>>,
    mascara: Arc<MascaraNucleo>,
    registro: Arc<dyn RegistroNucleo>,
    aviso: AvisoEstado,
}

impl fmt::Debug for ServicoIntegracoes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tipos: Vec<_> = self.integracoes.iter().map(|i| i.tipo()).collect();
        f.debug_struct("ServicoIntegracoes")
            .field("integracoes", &tipos)
            .field("mascara", &self.mascara)
            .finish_non_exhaustive()
    }
}

impl ServicoIntegracoes {
    /// Serviço com estas integrações; registra na máscara os segredos que elas já
    /// guardam (o token do GitHub de uma sessão anterior).
    pub fn new(
        integracoes: Vec<Arc<dyn Integracao>>,
        mascara: Arc<MascaraNucleo>,
        registro: Arc<dyn RegistroNucleo>,
    ) -> Self {
        let servico = Self {
            integracoes,
            desconectadas: Mutex::new(HashSet::new()),
            mascara,
            registro,
            aviso: Arc::new(|_| {}),
        };
        for integracao in &servico.integracoes {
            servico.registrar_segredos(integracao.as_ref());
        }
        servico
    }

    /// Define quem recebe cada estado novo (o evento `IntegracaoMudou`).
    pub fn com_aviso(mut self, aviso: AvisoEstado) -> Self {
        self.aviso = aviso;
        self
    }

    /// A máscara do núcleo.
    pub fn mascara(&self) -> &Arc<MascaraNucleo> {
        &self.mascara
    }

    fn registrar_segredos(&self, integracao: &dyn Integracao) {
        for segredo in integracao.segredos_guardados() {
            self.mascara.registrar(segredo);
        }
    }

    fn integracao(&self, tipo: TipoIntegracao) -> Result<Arc<dyn Integracao>, ErroIpc> {
        self.integracoes
            .iter()
            .find(|i| i.tipo() == tipo)
            .cloned()
            .ok_or_else(|| {
                self.falha_interna(
                    &format!("integracao {tipo}"),
                    &format!("a integração {tipo} não está ligada ao núcleo"),
                )
            })
    }

    fn desconectada_pelo_humano(&self, tipo: TipoIntegracao) -> bool {
        self.desconectadas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&tipo)
    }

    fn marcar_desconectada(&self, tipo: TipoIntegracao, sim: bool) {
        let mut desconectadas = self.desconectadas.lock().unwrap_or_else(|e| e.into_inner());
        if sim {
            desconectadas.insert(tipo);
        } else {
            desconectadas.remove(&tipo);
        }
    }

    /// ERR-55 com o motivo mascarado e registrado.
    fn falha_interna(&self, comando: &str, motivo: &str) -> ErroIpc {
        let motivo = self.mascara.mascarar(motivo);
        self.registro.registrar(EntradaRegistro {
            codigo: CodigoErro::FalhaInterna,
            comando: comando.to_owned(),
            motivo: motivo.clone(),
        });
        ErroIpc::falha_interna(&motivo)
    }

    /// Roda a operação do adaptador numa tarefa própria: pânico vira ERR-53. Todo erro
    /// sai pela máscara do núcleo; o ERR-53 vai ao registro. Avisa o estado resultante.
    async fn rodar<F>(
        &self,
        tipo: TipoIntegracao,
        operacao: &str,
        futuro: F,
    ) -> Result<EstadoIntegracao, ErroIntegracao>
    where
        F: Future<Output = Result<EstadoIntegracao, ErroIntegracao>> + Send + 'static,
    {
        let resultado = match tokio::spawn(futuro).await {
            Ok(resultado) => resultado,
            Err(falha) => Err(ErroIntegracao::inesperada(
                tipo,
                format!("a integração parou no meio: {}", motivo_da_tarefa(falha)),
            )),
        };
        let resultado = resultado
            .and_then(|estado| {
                // Estado de outra integração é erro de programa no adaptador.
                if estado.tipo == tipo {
                    Ok(estado)
                } else {
                    Err(ErroIntegracao::inesperada(
                        tipo,
                        format!("o adaptador devolveu o estado de {}", estado.tipo),
                    ))
                }
            })
            .map(|mut estado| {
                // Erro que o adaptador pôs no estado também passa pela máscara (RN-04).
                estado.erro = estado.erro.map(|erro| self.mascara.mascarar_erro(erro));
                estado
            })
            .map_err(|erro| self.mascara.mascarar_erro(erro));
        let estado = match &resultado {
            Ok(estado) => estado.clone(),
            Err(erro) => {
                if let ErroIntegracao::FalhaInesperada { motivo, .. } = erro {
                    self.registro.registrar(EntradaRegistro {
                        codigo: CodigoErro::IntegracaoFalhaInesperada,
                        comando: format!("{operacao} {tipo}"),
                        motivo: motivo.clone(),
                    });
                }
                EstadoIntegracao::a_partir_do_erro(erro)
            }
        };
        (self.aviso)(&estado);
        resultado
    }

    /// Estado de uma integração sem pedir nada ao Humano. Desconectada pelo Humano:
    /// `desconectado`, sem checar. O erro de fora é o da integração (o ERR do catálogo
    /// que explica o estado) ou, sem a integração ligada, o ERR-55.
    async fn checar(
        &self,
        tipo: TipoIntegracao,
    ) -> Result<Result<EstadoIntegracao, ErroIntegracao>, ErroIpc> {
        let integracao = self.integracao(tipo)?;
        if self.desconectada_pelo_humano(tipo) {
            let estado = EstadoIntegracao::novo(tipo, StatusIntegracao::Desconectado);
            (self.aviso)(&estado);
            return Ok(Ok(estado));
        }
        Ok(self
            .rodar(
                tipo,
                "verificar",
                async move { integracao.verificar().await },
            )
            .await)
    }

    /// UC-01 passo 1: o estado das três, na ordem do painel. A falha de uma vira o
    /// estado final do erro dela, com o erro no campo `erro` (M-46: ex.: `não
    /// instalado` com ERR-01, `desconectado` com o motivo do ERR-53), e não esconde as
    /// outras. Integração que o app não ligou (ERR-55, no registro) sai `desconectado`.
    pub async fn listar(&self) -> Vec<EstadoIntegracao> {
        let mut estados = Vec::with_capacity(TipoIntegracao::TODAS.len());
        for tipo in TipoIntegracao::TODAS {
            let estado = match self.checar(tipo).await {
                Ok(Ok(estado)) => estado,
                Ok(Err(erro)) => EstadoIntegracao::a_partir_do_erro(&erro),
                Err(_) => EstadoIntegracao::novo(tipo, StatusIntegracao::Desconectado),
            };
            estados.push(estado);
        }
        estados
    }

    /// "Testar conexão" (CMP-11): o estado atual, ou o erro do catálogo que o explica.
    pub async fn testar(&self, tipo: TipoIntegracao) -> Result<EstadoIntegracao, ErroIpc> {
        self.checar(tipo).await?.map_err(ErroIpc::from)
    }

    /// "Conectar" (UC-01 passos 2 e 3): tira a marca de desconectada e conecta; as
    /// instruções (código do device flow, mensagens do login) vão ao `observador`,
    /// mascaradas. Conectou: o segredo novo entra na máscara.
    pub async fn conectar(
        &self,
        tipo: TipoIntegracao,
        observador: Arc<dyn ObservadorConexao>,
    ) -> Result<EstadoIntegracao, ErroIpc> {
        let integracao = self.integracao(tipo)?;
        self.marcar_desconectada(tipo, false);
        let observador = ObservadorMascarado {
            destino: observador,
            mascara: self.mascara.clone(),
        };
        let adaptador = integracao.clone();
        let resultado = self
            .rodar(tipo, "conectar", async move {
                adaptador.conectar(&observador).await
            })
            .await;
        if resultado.is_ok() {
            self.registrar_segredos(integracao.as_ref());
        }
        resultado.map_err(ErroIpc::from)
    }

    /// "Desconectar" (CMP-11): o adaptador desfaz o que é dele (o GitHub apaga o token;
    /// o Claude CLI não faz logout) e o núcleo guarda a integração como desconectada
    /// até o próximo "Conectar", mesmo se o adaptador falhou (ERR-53).
    pub async fn desconectar(&self, tipo: TipoIntegracao) -> Result<EstadoIntegracao, ErroIpc> {
        let integracao = self.integracao(tipo)?;
        self.marcar_desconectada(tipo, true);
        self.rodar(tipo, "desconectar", async move {
            integracao.desconectar().await
        })
        .await
        .map_err(ErroIpc::from)
    }

    /// RN-08: segue só com a integração `conectado`; senão, ERR-05, ERR-06 ou ERR-07
    /// (ou o erro que impediu a checagem).
    pub async fn exigir(&self, tipo: TipoIntegracao) -> Result<(), ErroIpc> {
        let estado = self.testar(tipo).await?;
        exigir_conectada(&estado).map_err(ErroIpc::from)
    }
}

/// Texto do pânico ou do cancelamento de uma tarefa.
fn motivo_da_tarefa(falha: tokio::task::JoinError) -> String {
    if falha.is_cancelled() {
        return "tarefa cancelada".to_owned();
    }
    let carga = falha.into_panic();
    if let Some(texto) = carga.downcast_ref::<&str>() {
        (*texto).to_owned()
    } else if let Some(texto) = carga.downcast_ref::<String>() {
        texto.clone()
    } else {
        "pânico sem mensagem".to_owned()
    }
}

// ---------------------------------------------------------------------------
// Integrações do app
// ---------------------------------------------------------------------------

/// Endereço padrão do servidor MCP da Obsidian CLI, igual no Windows e no WSL em
/// `mirrored` (decisão `geral-decisao-2026-11-24-wsl-em-mirrored`).
pub const OBSIDIAN_MCP_URL_PADRAO: &str = "http://127.0.0.1:27180/mcp";

/// Variável com o nome do cofre do projeto no Obsidian, até o projeto (ENT-01, ENT-02)
/// ter o cofre de memória dele.
pub const VARIAVEL_COFRE: &str = "MYHARNESS_OBSIDIAN_COFRE";

/// Integração que não pôde ser montada (falta configuração): toda operação é ERR-53
/// com o motivo, e a integração fica `desconectado`.
#[derive(Debug)]
pub struct IntegracaoSemConfiguracao {
    tipo: TipoIntegracao,
    motivo: String,
}

impl IntegracaoSemConfiguracao {
    /// Integração `tipo` indisponível por `motivo`.
    pub fn new(tipo: TipoIntegracao, motivo: impl Into<String>) -> Self {
        Self {
            tipo,
            motivo: motivo.into(),
        }
    }

    fn erro(&self) -> ErroIntegracao {
        ErroIntegracao::inesperada(self.tipo, self.motivo.as_str())
    }
}

#[async_trait]
impl Integracao for IntegracaoSemConfiguracao {
    fn tipo(&self) -> TipoIntegracao {
        self.tipo
    }

    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        Err(self.erro())
    }

    async fn conectar(
        &self,
        _observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        Err(self.erro())
    }

    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        Ok(EstadoIntegracao::novo(
            self.tipo,
            StatusIntegracao::Desconectado,
        ))
    }
}

/// As três integrações reais, configuradas pelo ambiente:
/// - Claude CLI: `claude` no `PATH`.
/// - Obsidian CLI: `OBSIDIAN_MCP_URL` (padrão [`OBSIDIAN_MCP_URL_PADRAO`]) e o cofre de
///   [`VARIAVEL_COFRE`].
/// - GitHub: `MYHARNESS_GITHUB_CLIENT_ID` do ambiente ou da compilação; token no
///   cofre de credenciais do sistema.
pub fn integracoes_do_ambiente() -> Vec<Arc<dyn Integracao>> {
    use infra::claude::{ClaudeCli, ConfigClaudeCli};
    use infra::github::{ConfigGitHub, GitHub, VARIAVEL_CLIENT_ID, http::GitHubHttp};
    use infra::obsidian::{ConfigObsidian, ObsidianCli, VARIAVEL_URL};
    use infra::secrets::CofreDoSistema;

    let claude: Arc<dyn Integracao> = Arc::new(ClaudeCli::new(ConfigClaudeCli::default()));

    let cofre = std::env::var(VARIAVEL_COFRE)
        .ok()
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty());
    let obsidian: Arc<dyn Integracao> = match cofre {
        Some(cofre) => {
            let config = ConfigObsidian::do_ambiente(cofre.clone())
                .unwrap_or_else(|| ConfigObsidian::nova(OBSIDIAN_MCP_URL_PADRAO, cofre));
            Arc::new(ObsidianCli::new(config))
        }
        None => Arc::new(IntegracaoSemConfiguracao::new(
            TipoIntegracao::ObsidianCli,
            format!(
                "cofre do projeto não definido ({VARIAVEL_COFRE}); servidor em \
                 {VARIAVEL_URL}"
            ),
        )),
    };

    let github: Arc<dyn Integracao> = match (ConfigGitHub::do_ambiente(), GitHubHttp::novo()) {
        (Some(config), Ok(http)) => Arc::new(GitHub::new(
            config,
            Arc::new(http),
            Arc::new(CofreDoSistema),
        )),
        (None, _) => Arc::new(IntegracaoSemConfiguracao::new(
            TipoIntegracao::GitHub,
            format!("client_id do app OAuth não definido ({VARIAVEL_CLIENT_ID})"),
        )),
        (_, Err(erro)) => Arc::new(IntegracaoSemConfiguracao::new(
            TipoIntegracao::GitHub,
            erro.to_string(),
        )),
    };

    vec![claude, obsidian, github]
}

#[cfg(test)]
mod tests;
