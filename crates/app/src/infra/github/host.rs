//! `GitHost`: a conversa com o GitHub que o device flow e o status precisam.
//!
//! Real: [`super::http::GitHubHttp`]. Testes automáticos: [`super::falso::GitHubFalso`]
//! (decisão `geral-decisao-2026-11-20-github-falso`): nenhum teste chama o GitHub real.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::domain::segredo::{Segredo, TextoMascarado};

/// Resposta de `POST github.com/login/device/code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodigoDispositivo {
    /// `device_code`: quem o tem pode pegar o token, então é segredo.
    pub device_code: Segredo,
    /// `user_code`: o código que o Humano digita (vai à tela pela `InstrucaoConexao`).
    pub user_code: String,
    /// `verification_uri`: onde digitar (`https://github.com/login/device`).
    pub verification_uri: String,
    /// `expires_in`: validade do código (15 min no GitHub).
    pub expira_em: Duration,
    /// `interval`: espera mínima entre as consultas do token.
    pub intervalo: Duration,
}

/// Resposta de `POST github.com/login/oauth/access_token` no device flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RespostaToken {
    /// O Humano autorizou: o token (mostrado como `Segredo(••••)` no `Debug`).
    Concedido(Segredo),
    /// `authorization_pending`: o Humano ainda não digitou o código.
    Pendente,
    /// `slow_down`: consultar mais devagar; `interval` novo, quando o GitHub manda.
    Devagar {
        /// Intervalo novo.
        intervalo: Option<Duration>,
    },
    /// `expired_token`: o código expirou (ERR-02).
    Expirado,
    /// `access_denied`: o Humano recusou no GitHub (ERR-02).
    Recusado,
    /// Outro erro do protocolo (ex.: `device_flow_disabled`, `incorrect_client_credentials`).
    Erro {
        /// Campo `error`.
        codigo: String,
        /// Campo `error_description`, mascarado.
        descricao: Option<TextoMascarado>,
    },
}

/// Resposta de `GET api.github.com/user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsuarioGitHub {
    /// `login` da conta (ENT-13 `conta`).
    pub login: String,
    /// Escopos do token, do cabeçalho `X-OAuth-Scopes` (RF-03 "escopos").
    pub escopos: Vec<String>,
}

/// Falha ao falar com o GitHub. Todo texto vem mascarado (RN-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroHost {
    /// Rede fora do ar, tempo esgotado, TLS.
    Rede(TextoMascarado),
    /// Resposta que não dá para usar (status inesperado, corpo ilegível, campo faltando).
    Resposta(TextoMascarado),
    /// `401` em `/user`: o GitHub não aceita mais o token (revogado ou expirado).
    TokenRecusado,
}

impl fmt::Display for ErroHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroHost::Rede(m) => write!(f, "erro de rede com o GitHub ({m})"),
            ErroHost::Resposta(m) => write!(f, "resposta inesperada do GitHub ({m})"),
            ErroHost::TokenRecusado => f.write_str(
                "o GitHub recusou o token guardado (revogado ou expirado); conecte de novo",
            ),
        }
    }
}

impl std::error::Error for ErroHost {}

/// O que o MyHarness pede ao GitHub no RF-03.
#[async_trait]
pub trait GitHost: Send + Sync + fmt::Debug {
    /// Inicia o device flow para `client_id`, pedindo `escopos`.
    async fn pedir_codigo(
        &self,
        client_id: &str,
        escopos: &[String],
    ) -> Result<CodigoDispositivo, ErroHost>;

    /// Consulta uma vez se o Humano já autorizou o `device_code`.
    async fn trocar_codigo(
        &self,
        client_id: &str,
        device_code: &Segredo,
    ) -> Result<RespostaToken, ErroHost>;

    /// Conta e escopos do token.
    async fn usuario(&self, token: &Segredo) -> Result<UsuarioGitHub, ErroHost>;
}
