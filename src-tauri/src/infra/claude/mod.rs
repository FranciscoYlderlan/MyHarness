//! Integração com o Claude CLI (RF-01, UC-01, ENT-13, D10).
//!
//! - Acha o executável: o caminho configurado (entrada opcional do RF-01) ou `claude` no
//!   `PATH`. Não achou: ERR-01, `não instalado`.
//! - Lê a versão (`claude --version`) e a autenticação (`claude auth status --json`):
//!   `conectado` com versão e conta, ou `sem login` com versão.
//! - Sem login, `conectar` inicia o login da própria CLI (`claude auth login`, o mesmo do
//!   `/login`) e repassa ao Humano, linha a linha, o que a CLI pede (o link de login).
//!   Falhou: ERR-04, `sem login`.
//! - Tempo esgotado, saída que não dá para ler ou processo que não abre: ERR-53,
//!   `desconectado`.
//!
//! Todo texto que vem da CLI e vai à tela ou a erro passa pela máscara (RN-04, AC-154).

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::application::integracao::{InstrucaoConexao, Integracao, ObservadorConexao};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, StatusIntegracao, TipoIntegracao,
};
use my_harness_core::domain::segredo::TextoMascarado;
use serde::Deserialize;

use crate::infra::process::{self, FalhaProcesso, Saida};

#[cfg(test)]
mod tests;

/// Nome do executável procurado no `PATH`.
pub const EXECUTAVEL: &str = "claude";

/// Primeira instrução mostrada ao Humano quando o login começa.
pub const INSTRUCAO_INICIO_LOGIN: &str =
    "Iniciando o /login do Claude CLI. Conclua o login no navegador que abrir.";

/// Configuração da integração.
#[derive(Debug, Clone)]
pub struct ConfigClaudeCli {
    /// Caminho do executável (entrada opcional do RF-01). `None`: procura `claude` no `PATH`.
    pub executavel: Option<PathBuf>,
    /// Pastas onde procurar quando não há caminho (o valor de `PATH`). `None`: o `PATH`
    /// do processo do MyHarness.
    pub caminho_busca: Option<OsString>,
    /// Tempo-limite de cada checagem (`--version`, `auth status`).
    pub tempo_checagem: Duration,
    /// Tempo-limite do login inteiro (o Humano conclui no navegador).
    pub tempo_login: Duration,
}

impl Default for ConfigClaudeCli {
    fn default() -> Self {
        Self {
            executavel: None,
            caminho_busca: None,
            tempo_checagem: Duration::from_secs(20),
            tempo_login: Duration::from_secs(10 * 60),
        }
    }
}

/// Erro da integração com o Claude CLI: um item do catálogo de integrações
/// (ERR-01, ERR-04) ou a falha inesperada (ERR-53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroClaudeCli {
    /// ERR-01 ou ERR-04.
    Catalogo(ErroIntegracao),
    /// ERR-53 `INTEGRACAO_FALHA_INESPERADA`: tempo esgotado, saída ilegível, processo que
    /// não abre. O motivo já vem mascarado (RN-04).
    FalhaInesperada {
        /// Motivo, mascarado.
        motivo: TextoMascarado,
    },
}

impl ErroClaudeCli {
    /// ID no catálogo (`ERR-01`, `ERR-04`, `ERR-53`).
    pub fn id(&self) -> &'static str {
        match self {
            ErroClaudeCli::Catalogo(erro) => erro.id(),
            ErroClaudeCli::FalhaInesperada { .. } => "ERR-53",
        }
    }

    /// Código no servidor, como no catálogo.
    pub fn codigo(&self) -> &'static str {
        match self {
            ErroClaudeCli::Catalogo(erro) => erro.codigo(),
            ErroClaudeCli::FalhaInesperada { .. } => "INTEGRACAO_FALHA_INESPERADA",
        }
    }

    /// Estado final do catálogo: `não instalado` (ERR-01), `sem login` (ERR-04),
    /// `desconectado` (ERR-53), com o erro no estado (M-46).
    pub fn estado(&self) -> EstadoIntegracao {
        EstadoIntegracao::a_partir_do_erro(&self.clone().into())
    }

    fn inesperada(motivo: impl AsRef<str>) -> Self {
        ErroClaudeCli::FalhaInesperada {
            motivo: TextoMascarado::new(motivo.as_ref()),
        }
    }

    fn login_falhou(motivo: impl AsRef<str>) -> Self {
        ErroClaudeCli::Catalogo(ErroIntegracao::LoginClaudeCliFalhou {
            motivo: TextoMascarado::new(motivo.as_ref()),
        })
    }
}

impl fmt::Display for ErroClaudeCli {
    /// Mensagem do catálogo; ERR-53: "Falha inesperada com Claude CLI: <motivo>. Tente de novo."
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroClaudeCli::Catalogo(erro) => fmt::Display::fmt(erro, f),
            ErroClaudeCli::FalhaInesperada { motivo } => write!(
                f,
                "Falha inesperada com {}: {motivo}. Tente de novo.",
                TipoIntegracao::ClaudeCli
            ),
        }
    }
}

impl std::error::Error for ErroClaudeCli {}

/// Para o trait [`Integracao`]: o ERR-53 sai como [`ErroIntegracao::FalhaInesperada`],
/// com o motivo mascarado.
impl From<ErroClaudeCli> for ErroIntegracao {
    fn from(erro: ErroClaudeCli) -> Self {
        match erro {
            ErroClaudeCli::Catalogo(erro) => erro,
            ErroClaudeCli::FalhaInesperada { motivo } => {
                ErroIntegracao::inesperada(TipoIntegracao::ClaudeCli, motivo)
            }
        }
    }
}

/// Resposta de `claude auth status --json` (só os campos usados).
#[derive(Debug, Deserialize)]
struct StatusAutenticacao {
    #[serde(rename = "loggedIn")]
    logado: bool,
    #[serde(default)]
    email: Option<String>,
}

/// A integração com o Claude CLI.
#[derive(Debug, Clone, Default)]
pub struct ClaudeCli {
    config: ConfigClaudeCli,
}

impl ClaudeCli {
    /// Integração com a configuração dada.
    pub fn new(config: ConfigClaudeCli) -> Self {
        Self { config }
    }

    /// Caminho do executável, ou ERR-01.
    pub fn localizar(&self) -> Result<PathBuf, ErroClaudeCli> {
        let nao_instalado = || ErroClaudeCli::Catalogo(ErroIntegracao::ClaudeCliNaoInstalado);
        match &self.config.executavel {
            Some(caminho) if caminho.is_file() => Ok(caminho.clone()),
            Some(_) => Err(nao_instalado()),
            None => {
                let busca = self
                    .config
                    .caminho_busca
                    .clone()
                    .or_else(|| std::env::var_os("PATH"));
                let extensoes = std::env::var_os("PATHEXT");
                process::localizar_executavel(EXECUTAVEL, busca.as_deref(), extensoes.as_deref())
                    .ok_or_else(nao_instalado)
            }
        }
    }

    /// Checa o status (UC-01 passo 1): `conectado` com versão e conta (FA-01) ou
    /// `sem login` com versão. Erros: ERR-01, ERR-53.
    pub async fn checar(&self) -> Result<EstadoIntegracao, ErroClaudeCli> {
        let executavel = self.localizar()?;
        let versao = self.ler_versao(&executavel).await?;
        let autenticacao = self.ler_autenticacao(&executavel).await?;
        Ok(EstadoIntegracao {
            tipo: TipoIntegracao::ClaudeCli,
            status: if autenticacao.logado {
                StatusIntegracao::Conectado
            } else {
                StatusIntegracao::SemLogin
            },
            // A conta vem da CLI e vai à tela: passa pela máscara (RN-04).
            conta: autenticacao
                .email
                .filter(|_| autenticacao.logado)
                .map(|email| TextoMascarado::new(&email).as_str().to_owned()),
            versao: Some(versao),
            ..EstadoIntegracao::novo(TipoIntegracao::ClaudeCli, StatusIntegracao::SemLogin)
        })
    }

    /// Conecta (UC-01 passos 2 e 3): já logado, devolve o estado sem novo login (FA-01);
    /// sem login, roda o login da CLI, repassa cada linha ao `observador` (mascarada) e
    /// checa de novo. Erros: ERR-01, ERR-04, ERR-53.
    pub async fn fazer_login(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroClaudeCli> {
        let estado = self.checar().await?;
        if estado.status.esta_conectado() {
            return Ok(estado);
        }
        let executavel = self.localizar()?;
        observador.instrucao(InstrucaoConexao::Mensagem(INSTRUCAO_INICIO_LOGIN.into()));
        let saida = process::executar_acompanhando(
            &executavel,
            &["auth", "login"],
            self.config.tempo_login,
            |_, linha| {
                if !linha.trim().is_empty() {
                    let linha = TextoMascarado::new(linha.trim());
                    observador.instrucao(InstrucaoConexao::Mensagem(linha.as_str().to_owned()));
                }
            },
        )
        .await
        .map_err(|falha| match falha {
            FalhaProcesso::NaoEncontrado => {
                ErroClaudeCli::Catalogo(ErroIntegracao::ClaudeCliNaoInstalado)
            }
            FalhaProcesso::TempoEsgotado(_) => {
                ErroClaudeCli::login_falhou("tempo esgotado esperando o login")
            }
            outra => ErroClaudeCli::inesperada(outra.to_string()),
        })?;
        if !saida.sucesso() {
            return Err(ErroClaudeCli::login_falhou(motivo_da_saida(&saida)));
        }
        let estado = self.checar().await?;
        if estado.status.esta_conectado() {
            Ok(estado)
        } else {
            Err(ErroClaudeCli::login_falhou(
                "o login terminou, mas a CLI continua sem login",
            ))
        }
    }

    async fn rodar_checagem(
        &self,
        executavel: &std::path::Path,
        argumentos: &[&str],
    ) -> Result<Saida, ErroClaudeCli> {
        process::executar(executavel, argumentos, self.config.tempo_checagem)
            .await
            .map_err(|falha| match falha {
                FalhaProcesso::NaoEncontrado => {
                    ErroClaudeCli::Catalogo(ErroIntegracao::ClaudeCliNaoInstalado)
                }
                outra => {
                    ErroClaudeCli::inesperada(format!("`claude {}`: {outra}", argumentos.join(" ")))
                }
            })
    }

    async fn ler_versao(&self, executavel: &std::path::Path) -> Result<String, ErroClaudeCli> {
        let saida = self.rodar_checagem(executavel, &["--version"]).await?;
        if !saida.sucesso() {
            return Err(ErroClaudeCli::inesperada(format!(
                "`claude --version` falhou: {}",
                motivo_da_saida(&saida)
            )));
        }
        interpretar_versao(&saida.stdout).ok_or_else(|| {
            ErroClaudeCli::inesperada(format!(
                "versão ilegível: {}",
                primeira_linha(&saida.stdout).unwrap_or("(vazia)")
            ))
        })
    }

    async fn ler_autenticacao(
        &self,
        executavel: &std::path::Path,
    ) -> Result<StatusAutenticacao, ErroClaudeCli> {
        // Sem login, a CLI sai com 1 e ainda escreve o JSON: o código não decide, o JSON sim.
        let saida = self
            .rodar_checagem(executavel, &["auth", "status", "--json"])
            .await?;
        serde_json::from_str::<StatusAutenticacao>(saida.stdout.trim()).map_err(|_| {
            ErroClaudeCli::inesperada(format!(
                "status de login ilegível: {}",
                motivo_da_saida(&saida)
            ))
        })
    }
}

/// `"2.1.330 (Claude Code)"` → `"2.1.330"`. Exige que comece por algarismo e tenha ponto.
fn interpretar_versao(stdout: &str) -> Option<String> {
    let token = primeira_linha(stdout)?.split_whitespace().next()?;
    let token = token.strip_prefix('v').unwrap_or(token);
    let valido = token.starts_with(|c: char| c.is_ascii_digit())
        && token.contains('.')
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    valido.then(|| token.to_owned())
}

fn primeira_linha(texto: &str) -> Option<&str> {
    texto.lines().map(str::trim).find(|l| !l.is_empty())
}

/// Motivo legível de uma saída com falha: a última linha não vazia do stderr, senão a do
/// stdout, senão o código de saída. Sem máscara: quem chama mascara.
fn motivo_da_saida(saida: &Saida) -> String {
    let ultima = |texto: &str| {
        texto
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .map(str::to_owned)
    };
    ultima(&saida.stderr)
        .or_else(|| ultima(&saida.stdout))
        .unwrap_or_else(|| match saida.codigo {
            Some(codigo) => format!("a CLI saiu com código {codigo}"),
            None => "a CLI foi encerrada".to_owned(),
        })
}

#[async_trait]
impl Integracao for ClaudeCli {
    fn tipo(&self) -> TipoIntegracao {
        TipoIntegracao::ClaudeCli
    }

    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        self.checar().await.map_err(Into::into)
    }

    async fn conectar(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        self.fazer_login(observador).await.map_err(Into::into)
    }

    /// Não faz logout da CLI (que tiraria o login do Humano também fora do MyHarness):
    /// só devolve `desconectado`, e o núcleo para de usar a integração.
    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        Ok(EstadoIntegracao::novo(
            TipoIntegracao::ClaudeCli,
            StatusIntegracao::Desconectado,
        ))
    }
}
