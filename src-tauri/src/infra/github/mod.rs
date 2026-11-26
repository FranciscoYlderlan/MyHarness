//! Integração com o GitHub (RF-03, ENT-13, UC-01): device flow do app OAuth próprio do
//! MyHarness (D10, decisão `geral-decisao-2026-11-19-github-oauth-proprio`).
//!
//! - **Conectar:** pede o código (`login/device/code`), manda o `user_code` e o endereço
//!   ao Humano pela [`InstrucaoConexao::CodigoDoDispositivo`] e consulta
//!   `login/oauth/access_token` no intervalo dado, respeitando `authorization_pending`,
//!   `slow_down` (intervalo novo), `expired_token` e `access_denied` (ERR-02). Com o
//!   token, lê a conta e os escopos (`api.github.com/user`, `X-OAuth-Scopes`) e guarda o
//!   token no cofre de credenciais do sistema (Gerenciador de Credenciais no Windows, D12).
//! - **Verificar:** sem token, `desconectado`; com token, `conectado` com a conta.
//! - **Desconectar:** cancela o device flow em andamento e apaga o token do cofre.
//! - Rede, resposta ilegível, erro do protocolo ou do cofre: ERR-53, com o motivo
//!   mascarado (AC-153).
//!
//! O token só existe como [`Segredo`]: nunca vai a SQLite, arquivo, log, erro ou `Debug`
//! (RN-04, RNF-05); todo motivo de erro passa pela máscara com ele.

pub mod falso;
pub mod host;
pub mod http;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::application::integracao::{InstrucaoConexao, Integracao, ObservadorConexao};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, MotivoOauthNaoConcluido, StatusIntegracao, TipoIntegracao,
};
use my_harness_core::domain::segredo::{Segredo, TextoMascarado};
use tokio::sync::Notify;
use tokio::time::Instant;

use self::host::{ErroHost, GitHost, RespostaToken, UsuarioGitHub};
use super::secrets::{ChaveCredencial, CofreCredenciais, ErroCofre};

/// Variável de ambiente com o `client_id` do app OAuth (`.env.example`).
pub const VARIAVEL_CLIENT_ID: &str = "MYHARNESS_GITHUB_CLIENT_ID";

/// Escopos pedidos no device flow: `repo`, para o núcleo fazer push e abrir PR (RF-27)
/// [a confirmar].
pub const ESCOPOS_PADRAO: [&str; 1] = ["repo"];

/// Espera mínima entre consultas, mesmo que o GitHub mande menos.
const INTERVALO_MINIMO: Duration = Duration::from_secs(1);
/// Acréscimo do `slow_down` quando o GitHub não manda o intervalo novo (RFC 8628).
const ACRESCIMO_SLOW_DOWN: Duration = Duration::from_secs(5);

/// Configuração do app OAuth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigGitHub {
    /// `client_id` do app OAuth do MyHarness (público; não há client secret).
    pub client_id: String,
    /// Escopos pedidos.
    pub escopos: Vec<String>,
    /// Onde o token mora no cofre de credenciais.
    pub chave: ChaveCredencial,
}

impl ConfigGitHub {
    /// Configuração com os escopos e a chave padrão.
    pub fn nova(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            escopos: ESCOPOS_PADRAO.iter().map(|s| s.to_string()).collect(),
            chave: ChaveCredencial::github(),
        }
    }

    /// `client_id` de [`VARIAVEL_CLIENT_ID`] no ambiente ou, sem ele, no ambiente da
    /// compilação; `None` se nenhum dos dois existe.
    pub fn do_ambiente() -> Option<Self> {
        let valor = std::env::var(VARIAVEL_CLIENT_ID)
            .ok()
            .or_else(|| option_env!("MYHARNESS_GITHUB_CLIENT_ID").map(str::to_string))?;
        let valor = valor.trim();
        (!valor.is_empty()).then(|| Self::nova(valor))
    }
}

/// Resultado de checar ou conectar (RF-03: status, conta, escopos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusGitHub {
    /// Estado de ENT-13; conectado traz a conta em `conta`.
    pub estado: EstadoIntegracao,
    /// Escopos do token (vazio quando desconectado).
    pub escopos: Vec<String>,
}

impl StatusGitHub {
    fn desconectado() -> Self {
        Self {
            estado: EstadoIntegracao::novo(TipoIntegracao::GitHub, StatusIntegracao::Desconectado),
            escopos: Vec::new(),
        }
    }

    fn conectado(usuario: UsuarioGitHub) -> Self {
        Self {
            estado: EstadoIntegracao::conectado(TipoIntegracao::GitHub, Some(usuario.login), None),
            escopos: usuario.escopos,
        }
    }
}

/// Erro de checar, conectar ou desconectar o GitHub: o ERR-02 ou o ERR-53 do catálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroGitHub {
    /// ERR-02 (`ErroIntegracao::GithubOauthNaoConcluido`): cancelado, recusado ou expirado.
    Catalogo(ErroIntegracao),
    /// ERR-53 `INTEGRACAO_FALHA_INESPERADA`: rede, resposta ilegível, erro do protocolo,
    /// token recusado ou cofre de credenciais. O motivo já vem mascarado (RN-04).
    FalhaInesperada {
        /// Motivo, mascarado.
        motivo: TextoMascarado,
    },
}

impl ErroGitHub {
    fn oauth(motivo: MotivoOauthNaoConcluido) -> Self {
        ErroGitHub::Catalogo(ErroIntegracao::GithubOauthNaoConcluido(motivo))
    }

    fn inesperada(motivo: &str, segredos: &[&Segredo]) -> Self {
        ErroGitHub::FalhaInesperada {
            motivo: TextoMascarado::com_segredos(motivo, segredos),
        }
    }

    fn do_host(erro: &ErroHost, segredos: &[&Segredo]) -> Self {
        Self::inesperada(&erro.to_string(), segredos)
    }

    fn do_cofre(erro: &ErroCofre, segredos: &[&Segredo]) -> Self {
        Self::inesperada(&erro.to_string(), segredos)
    }

    /// ID no catálogo (`ERR-02` ou `ERR-53`).
    pub fn id(&self) -> &'static str {
        match self {
            ErroGitHub::Catalogo(e) => e.id(),
            ErroGitHub::FalhaInesperada { .. } => "ERR-53",
        }
    }

    /// Código no servidor, como no catálogo.
    pub fn codigo(&self) -> &'static str {
        match self {
            ErroGitHub::Catalogo(e) => e.codigo(),
            ErroGitHub::FalhaInesperada { .. } => "INTEGRACAO_FALHA_INESPERADA",
        }
    }

    /// Estado final: GitHub `desconectado` (ERR-02 e ERR-53).
    pub fn estado_final(&self) -> EstadoIntegracao {
        EstadoIntegracao::novo(TipoIntegracao::GitHub, StatusIntegracao::Desconectado)
    }

    /// Motivo do ERR-02 (cancelado, recusado ou expirado), para o cartão (AC-129).
    pub fn motivo_oauth(&self) -> Option<MotivoOauthNaoConcluido> {
        match self {
            ErroGitHub::Catalogo(ErroIntegracao::GithubOauthNaoConcluido(m)) => Some(*m),
            _ => None,
        }
    }

    /// Motivo do ERR-53 (`<motivo>` da mensagem), mascarado.
    pub fn motivo(&self) -> Option<&TextoMascarado> {
        match self {
            ErroGitHub::FalhaInesperada { motivo } => Some(motivo),
            ErroGitHub::Catalogo(_) => None,
        }
    }
}

/// Texto do motivo do ERR-02 para o Humano (AC-129).
pub fn descrever_motivo(motivo: MotivoOauthNaoConcluido) -> &'static str {
    match motivo {
        MotivoOauthNaoConcluido::Cancelado => "a conexão foi cancelada",
        MotivoOauthNaoConcluido::Recusado => "a autorização foi recusada no GitHub",
        MotivoOauthNaoConcluido::Expirado => "o código expirou antes de ser digitado",
    }
}

impl fmt::Display for ErroGitHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroGitHub::Catalogo(e) => e.fmt(f),
            ErroGitHub::FalhaInesperada { motivo } => write!(
                f,
                "Falha inesperada com {}: {motivo}. Tente de novo.",
                TipoIntegracao::GitHub
            ),
        }
    }
}

impl std::error::Error for ErroGitHub {}

/// Integração com o GitHub pelo device flow.
#[derive(Debug)]
pub struct GitHub {
    config: ConfigGitHub,
    host: Arc<dyn GitHost>,
    cofre: Arc<dyn CofreCredenciais>,
    cancelado: AtomicBool,
    aviso: Notify,
}

impl GitHub {
    /// Integração com `host` (real: [`http::GitHubHttp`]) e `cofre`
    /// (real: [`super::secrets::CofreDoSistema`]).
    pub fn new(
        config: ConfigGitHub,
        host: Arc<dyn GitHost>,
        cofre: Arc<dyn CofreCredenciais>,
    ) -> Self {
        Self {
            config,
            host,
            cofre,
            cancelado: AtomicBool::new(false),
            aviso: Notify::new(),
        }
    }

    /// Configuração em uso.
    pub fn config(&self) -> &ConfigGitHub {
        &self.config
    }

    /// Token guardado, para quem precisa dele (push e PR do núcleo, RF-27; registro do
    /// segredo na máscara do núcleo). Nunca passe o valor a log, SQLite, cofre ou tela.
    pub fn token(&self) -> Result<Option<Segredo>, ErroGitHub> {
        self.cofre
            .ler(&self.config.chave)
            .map_err(|e| ErroGitHub::do_cofre(&e, &[]))
    }

    /// Checa o status com conta e escopos (UC-01 passo 1, AC-90), sem pedir nada ao
    /// Humano. Erros: ERR-53 (rede, token recusado, cofre).
    pub async fn checar(&self) -> Result<StatusGitHub, ErroGitHub> {
        let Some(token) = self.token()? else {
            return Ok(StatusGitHub::desconectado());
        };
        self.host
            .usuario(&token)
            .await
            .map(StatusGitHub::conectado)
            .map_err(|e| ErroGitHub::do_host(&e, &[&token]))
    }

    /// Conecta pelo device flow (UC-01 passos 2 e 3). Já conectado, devolve o status sem
    /// novo código (UC-01 FA-01). Erros: ERR-02 (cancelado, `access_denied`,
    /// `expired_token` ou código vencido), ERR-53.
    pub async fn conectar_com(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<StatusGitHub, ErroGitHub> {
        self.cancelado.store(false, Ordering::SeqCst);
        if let Some(token) = self.token()? {
            match self.host.usuario(&token).await {
                Ok(usuario) => return Ok(StatusGitHub::conectado(usuario)),
                // Token que o GitHub não aceita mais: sai do cofre e começa o device flow.
                Err(ErroHost::TokenRecusado) => self.apagar_token()?,
                Err(e) => return Err(ErroGitHub::do_host(&e, &[&token])),
            }
        }

        let codigo = self
            .host
            .pedir_codigo(&self.config.client_id, &self.config.escopos)
            .await
            .map_err(|e| ErroGitHub::do_host(&e, &[]))?;
        let device_code = codigo.device_code;
        observador.instrucao(InstrucaoConexao::CodigoDoDispositivo {
            codigo: codigo.user_code,
            endereco: codigo.verification_uri,
        });

        let limite = Instant::now() + codigo.expira_em;
        let mut intervalo = codigo.intervalo.max(INTERVALO_MINIMO);
        loop {
            self.esperar(intervalo, limite).await?;
            let resposta = self
                .host
                .trocar_codigo(&self.config.client_id, &device_code)
                .await
                .map_err(|e| ErroGitHub::do_host(&e, &[&device_code]))?;
            match resposta {
                RespostaToken::Pendente => {}
                RespostaToken::Devagar { intervalo: novo } => {
                    intervalo = novo
                        .unwrap_or(intervalo + ACRESCIMO_SLOW_DOWN)
                        .max(intervalo);
                }
                RespostaToken::Expirado => {
                    return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Expirado));
                }
                RespostaToken::Recusado => {
                    return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Recusado));
                }
                RespostaToken::Erro { codigo, descricao } => {
                    let descricao = descricao.map(|d| format!(": {d}")).unwrap_or_default();
                    return Err(ErroGitHub::inesperada(
                        &format!("o GitHub devolveu `{codigo}` no device flow{descricao}"),
                        &[&device_code],
                    ));
                }
                RespostaToken::Concedido(token) => return self.guardar(token).await,
            }
        }
    }

    /// Lê a conta do token novo e o guarda no cofre de credenciais. "Desconectar"
    /// durante a leitura da conta cancela: o token não é gravado (ERR-02 cancelado).
    async fn guardar(&self, token: Segredo) -> Result<StatusGitHub, ErroGitHub> {
        let usuario = self
            .host
            .usuario(&token)
            .await
            .map_err(|e| ErroGitHub::do_host(&e, &[&token]))?;
        if self.cancelado.load(Ordering::SeqCst) {
            return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Cancelado));
        }
        self.cofre
            .gravar(&self.config.chave, &token)
            .map_err(|e| ErroGitHub::do_cofre(&e, &[&token]))?;
        Ok(StatusGitHub::conectado(usuario))
    }

    /// Espera `intervalo` antes da próxima consulta; falha se o Humano cancelou
    /// (ERR-02 cancelado) ou se o código vence antes disso (ERR-02 expirado).
    async fn esperar(&self, intervalo: Duration, limite: Instant) -> Result<(), ErroGitHub> {
        let alvo = Instant::now() + intervalo;
        if alvo >= limite {
            // A consulta cairia depois do vencimento: o código expira sem ela.
            tokio::select! {
                _ = tokio::time::sleep_until(limite) => {}
                _ = self.esperar_cancelamento() => {}
            }
            if self.cancelado.load(Ordering::SeqCst) {
                return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Cancelado));
            }
            return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Expirado));
        }
        tokio::select! {
            _ = tokio::time::sleep_until(alvo) => {}
            _ = self.esperar_cancelamento() => {}
        }
        if self.cancelado.load(Ordering::SeqCst) {
            return Err(ErroGitHub::oauth(MotivoOauthNaoConcluido::Cancelado));
        }
        Ok(())
    }

    /// Termina só quando o cancelamento foi pedido (aviso antigo não conta).
    async fn esperar_cancelamento(&self) {
        loop {
            // Inscreve antes de olhar a marca, para não perder um aviso no meio.
            let aviso = self.aviso.notified();
            tokio::pin!(aviso);
            aviso.as_mut().enable();
            if self.cancelado.load(Ordering::SeqCst) {
                return;
            }
            aviso.await;
        }
    }

    /// Cancela o device flow em andamento (ERR-02 cancelado); sem um, não faz nada.
    pub fn cancelar(&self) {
        self.cancelado.store(true, Ordering::SeqCst);
        self.aviso.notify_waiters();
    }

    fn apagar_token(&self) -> Result<(), ErroGitHub> {
        self.cofre
            .apagar(&self.config.chave)
            .map_err(|e| ErroGitHub::do_cofre(&e, &[]))
    }

    /// "Desconectar" (CMP-11): cancela o device flow em andamento e apaga o token.
    /// O token não é revogado no GitHub: isso exige o client secret, que o app não tem.
    pub fn remover(&self) -> Result<StatusGitHub, ErroGitHub> {
        self.cancelar();
        self.apagar_token()?;
        Ok(StatusGitHub::desconectado())
    }
}

impl From<ErroGitHub> for ErroIntegracao {
    fn from(erro: ErroGitHub) -> Self {
        match erro {
            ErroGitHub::Catalogo(erro) => erro,
            ErroGitHub::FalhaInesperada { motivo } => {
                ErroIntegracao::inesperada(TipoIntegracao::GitHub, motivo)
            }
        }
    }
}

/// Converte para o contrato do trait: ERR-02 e ERR-53 saem como erro, com o motivo.
/// Os escopos ficam em [`GitHub::checar`] e [`GitHub::conectar_com`].
fn para_o_trait(
    resultado: Result<StatusGitHub, ErroGitHub>,
) -> Result<EstadoIntegracao, ErroIntegracao> {
    resultado.map(|status| status.estado).map_err(Into::into)
}

#[async_trait]
impl Integracao for GitHub {
    fn tipo(&self) -> TipoIntegracao {
        TipoIntegracao::GitHub
    }

    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        para_o_trait(self.checar().await)
    }

    async fn conectar(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        para_o_trait(self.conectar_com(observador).await)
    }

    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        para_o_trait(self.remover())
    }

    /// O token guardado, para a máscara do núcleo (RN-04). Sem token ou com o cofre de
    /// credenciais em erro, nenhum.
    fn segredos_guardados(&self) -> Vec<Segredo> {
        self.token().ok().flatten().into_iter().collect()
    }
}
