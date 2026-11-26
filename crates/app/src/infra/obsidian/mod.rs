//! Integração com a Obsidian CLI (RF-02, ENT-13, UC-01), pelo servidor MCP dela (D10).
//!
//! Checar o status é: abrir uma sessão MCP no servidor da Obsidian CLI (o servidor
//! responde), listar as ferramentas e chamar uma delas no cofre do projeto (o Obsidian
//! está aberto e o cofre é acessível). Se o servidor expõe [`FERRAMENTA_CONTAGEM`]
//! (equivalente a `obsidian files ext=md total`), a mesma chamada devolve o número de
//! notas indexadas (AC-128), que vai no próprio estado da integração (M-46); senão a sonda é um [`FERRAMENTA_BUSCA`] com `limit: 1` e o
//! número fica `None`.
//!
//! Como a CLI responde (nota "Obsidian CLI via MCP não carrega", no Vault):
//! - Obsidian fechado: `The CLI is unable to find Obsidian. Please make sure Obsidian is
//!   running and try again.` → ERR-03, com a mensagem da própria CLI mascarada (AC-05,
//!   AC-89, AC-154).
//! - Obsidian recém-aberto: `Command "read" not found. Did you mean: …` → ainda carregando;
//!   espera e repete ([`ConfigObsidian::tentativas_carregando`]); se não passar, ERR-53.
//! - Cofre que a CLI não conhece: `Vault not found.` → ERR-53, citando o cofre.
//! - Servidor fora do ar, tempo esgotado, resposta ilegível → ERR-53 (AC-153).
//!
//! A integração não tem credencial: "Desconectar" só marca a integração como
//! desconectada até o próximo "Conectar", sem falar com o servidor.

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::application::integracao::{Integracao, ObservadorConexao};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, StatusIntegracao, TipoIntegracao,
};
use my_harness_core::domain::segredo::TextoMascarado;
use serde_json::json;

use super::mcp::{ErroMcp, SessaoMcp};

/// Variável de ambiente com o endereço do servidor MCP da Obsidian CLI (`.env.example`).
pub const VARIAVEL_URL: &str = "OBSIDIAN_MCP_URL";

/// Ferramenta de busca do servidor (`obsidian search`); usada como sonda sem contagem.
pub const FERRAMENTA_BUSCA: &str = "obsidian_search";

/// Ferramenta de contagem de arquivos (`obsidian files ext=md total`), quando o servidor
/// a expõe. Argumentos: `vault`, `ext: "md"`, `total: true`; resposta: só o número.
pub const FERRAMENTA_CONTAGEM: &str = "obsidian_files";

/// Como achar e chamar o servidor MCP da Obsidian CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigObsidian {
    /// Endereço `http://` do servidor MCP.
    pub url: String,
    /// Nome do cofre do projeto no Obsidian (ENT-02 `nome`).
    pub cofre: String,
    /// Tempo limite de cada pedido ao servidor.
    pub tempo_limite: Duration,
    /// Quantas vezes repetir a sonda enquanto a CLI responde `Command "…" not found`.
    pub tentativas_carregando: u32,
    /// Espera entre essas tentativas.
    pub espera_carregando: Duration,
}

impl ConfigObsidian {
    /// Configuração com os tempos padrão (5 s por pedido; 3 tentativas a cada 2 s).
    pub fn nova(url: impl Into<String>, cofre: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            cofre: cofre.into(),
            tempo_limite: Duration::from_secs(5),
            tentativas_carregando: 3,
            espera_carregando: Duration::from_secs(2),
        }
    }

    /// Endereço vindo de [`VARIAVEL_URL`]; `None` se a variável não existe ou está vazia.
    pub fn do_ambiente(cofre: impl Into<String>) -> Option<Self> {
        let url = std::env::var(VARIAVEL_URL).ok()?;
        let url = url.trim();
        (!url.is_empty()).then(|| Self::nova(url, cofre))
    }
}

/// Resultado de uma checagem bem-sucedida (RF-02: status e número de notas indexadas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusObsidian {
    /// Estado de ENT-13 (`conectado`).
    pub estado: EstadoIntegracao,
    /// Cofre checado.
    pub cofre: String,
    /// Notas indexadas no cofre; `None` se o servidor não expõe [`FERRAMENTA_CONTAGEM`].
    pub notas_indexadas: Option<u64>,
}

/// Erro de checar ou conectar a Obsidian CLI: o ERR-03 ou o ERR-53 do catálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroObsidian {
    /// ERR-03 (`ErroIntegracao::ObsidianFechado`), com a mensagem da CLI mascarada.
    Catalogo(ErroIntegracao),
    /// ERR-53 `INTEGRACAO_FALHA_INESPERADA`: tempo esgotado, saída ilegível, rede,
    /// cofre não encontrado. O motivo já vem mascarado (RN-04).
    FalhaInesperada {
        /// Motivo, mascarado.
        motivo: TextoMascarado,
    },
}

impl ErroObsidian {
    fn inesperada(motivo: impl AsRef<str>) -> Self {
        ErroObsidian::FalhaInesperada {
            motivo: TextoMascarado::new(motivo.as_ref()),
        }
    }

    /// ID no catálogo (`ERR-03` ou `ERR-53`).
    pub fn id(&self) -> &'static str {
        match self {
            ErroObsidian::Catalogo(e) => e.id(),
            ErroObsidian::FalhaInesperada { .. } => "ERR-53",
        }
    }

    /// Código no servidor, como no catálogo.
    pub fn codigo(&self) -> &'static str {
        match self {
            ErroObsidian::Catalogo(e) => e.codigo(),
            ErroObsidian::FalhaInesperada { .. } => "INTEGRACAO_FALHA_INESPERADA",
        }
    }

    /// Integração do erro (sempre a Obsidian CLI).
    pub fn integracao(&self) -> TipoIntegracao {
        TipoIntegracao::ObsidianCli
    }

    /// Estado final: Obsidian CLI `desconectado` (ERR-03 e ERR-53), com o erro (M-46).
    pub fn estado_final(&self) -> EstadoIntegracao {
        EstadoIntegracao::a_partir_do_erro(&self.clone().into())
    }

    /// Motivo do ERR-53 (`<motivo>` da mensagem), mascarado.
    pub fn motivo(&self) -> Option<&TextoMascarado> {
        match self {
            ErroObsidian::FalhaInesperada { motivo } => Some(motivo),
            ErroObsidian::Catalogo(_) => None,
        }
    }
}

impl fmt::Display for ErroObsidian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroObsidian::Catalogo(e) => e.fmt(f),
            ErroObsidian::FalhaInesperada { motivo } => write!(
                f,
                "Falha inesperada com {}: {motivo}. Tente de novo.",
                TipoIntegracao::ObsidianCli
            ),
        }
    }
}

impl std::error::Error for ErroObsidian {}

impl From<ErroMcp> for ErroObsidian {
    fn from(erro: ErroMcp) -> Self {
        ErroObsidian::inesperada(erro.to_string())
    }
}

/// O que a resposta da CLI diz.
#[derive(Debug, PartialEq, Eq)]
enum Leitura {
    Ok,
    ObsidianFechado,
    Carregando,
    CofreNaoEncontrado,
    Erro,
}

fn ler_resposta(texto: &str, erro: bool) -> Leitura {
    let t = texto.to_lowercase();
    if t.contains("unable to find obsidian") || t.contains("make sure obsidian is running") {
        Leitura::ObsidianFechado
    } else if t.contains("command \"") && t.contains("\" not found") {
        Leitura::Carregando
    } else if t.contains("vault not found") {
        Leitura::CofreNaoEncontrado
    } else if erro {
        Leitura::Erro
    } else {
        Leitura::Ok
    }
}

/// Integração com a Obsidian CLI pelo servidor MCP dela.
#[derive(Debug)]
pub struct ObsidianCli {
    config: ConfigObsidian,
    desconectada: AtomicBool,
}

impl ObsidianCli {
    /// Integração para o cofre e o servidor de `config`.
    pub fn new(config: ConfigObsidian) -> Self {
        Self {
            config,
            desconectada: AtomicBool::new(false),
        }
    }

    /// Configuração em uso.
    pub fn config(&self) -> &ConfigObsidian {
        &self.config
    }

    /// Checa servidor, Obsidian e cofre e conta as notas, com o erro completo
    /// (ERR-03 ou ERR-53). Fala com o servidor mesmo depois de "Desconectar".
    pub async fn checar(&self) -> Result<StatusObsidian, ErroObsidian> {
        let sessao = SessaoMcp::abrir(&self.config.url, self.config.tempo_limite).await?;
        let ferramentas = sessao.listar_ferramentas().await?;
        let tem = |nome: &str| ferramentas.iter().any(|f| f == nome);
        let contar = tem(FERRAMENTA_CONTAGEM);
        if !contar && !tem(FERRAMENTA_BUSCA) {
            return Err(ErroObsidian::inesperada(format!(
                "o servidor MCP não expõe `{FERRAMENTA_BUSCA}` nem `{FERRAMENTA_CONTAGEM}`"
            )));
        }
        let cofre = &self.config.cofre;
        let (ferramenta, argumentos) = if contar {
            (
                FERRAMENTA_CONTAGEM,
                json!({ "vault": cofre, "ext": "md", "total": true }),
            )
        } else {
            (
                FERRAMENTA_BUSCA,
                json!({ "vault": cofre, "query": cofre, "limit": 1 }),
            )
        };

        let mut tentativa = 0;
        let resposta = loop {
            let resposta = sessao.chamar(ferramenta, argumentos.clone()).await?;
            match ler_resposta(&resposta.texto, resposta.erro) {
                Leitura::Ok => break resposta,
                Leitura::ObsidianFechado => {
                    return Err(ErroObsidian::Catalogo(ErroIntegracao::ObsidianFechado {
                        mensagem_da_cli: TextoMascarado::new(resposta.texto.trim()),
                    }));
                }
                Leitura::Carregando if tentativa < self.config.tentativas_carregando => {
                    tentativa += 1;
                    tokio::time::sleep(self.config.espera_carregando).await;
                }
                Leitura::Carregando => {
                    return Err(ErroObsidian::inesperada(format!(
                        "o Obsidian ainda está carregando os comandos da CLI ({})",
                        resposta.texto.trim()
                    )));
                }
                Leitura::CofreNaoEncontrado => {
                    return Err(ErroObsidian::inesperada(format!(
                        "cofre {cofre} não encontrado pela Obsidian CLI ({})",
                        resposta.texto.trim()
                    )));
                }
                Leitura::Erro => {
                    return Err(ErroObsidian::inesperada(format!(
                        "a Obsidian CLI devolveu erro em `{ferramenta}`: {}",
                        resposta.texto.trim()
                    )));
                }
            }
        };

        let notas_indexadas = if contar {
            let texto = resposta.texto.trim();
            Some(texto.parse::<u64>().map_err(|_| {
                ErroObsidian::inesperada(format!("contagem de notas que não dá para ler: {texto}"))
            })?)
        } else {
            None
        };
        Ok(StatusObsidian {
            estado: EstadoIntegracao::conectado(TipoIntegracao::ObsidianCli, None, None)
                .com_notas_indexadas(notas_indexadas),
            cofre: cofre.clone(),
            notas_indexadas,
        })
    }

    /// "Conectar" (UC-01 passo 2, D10): tira a marca de desconectada e testa o servidor.
    pub async fn testar(&self) -> Result<StatusObsidian, ErroObsidian> {
        self.desconectada.store(false, Ordering::SeqCst);
        self.checar().await
    }

    /// `true` depois de "Desconectar" e até o próximo "Conectar".
    pub fn esta_desconectada(&self) -> bool {
        self.desconectada.load(Ordering::SeqCst)
    }

    fn desconectado() -> EstadoIntegracao {
        EstadoIntegracao::novo(TipoIntegracao::ObsidianCli, StatusIntegracao::Desconectado)
    }
}

impl From<ErroObsidian> for ErroIntegracao {
    fn from(erro: ErroObsidian) -> Self {
        match erro {
            ErroObsidian::Catalogo(erro) => erro,
            ErroObsidian::FalhaInesperada { motivo } => {
                ErroIntegracao::inesperada(TipoIntegracao::ObsidianCli, motivo)
            }
        }
    }
}

/// Converte para o contrato do trait: o estado já leva o número de notas (AC-128);
/// ERR-03 e ERR-53 saem como erro, com o motivo.
fn para_o_trait(
    resultado: Result<StatusObsidian, ErroObsidian>,
) -> Result<EstadoIntegracao, ErroIntegracao> {
    resultado.map(|status| status.estado).map_err(Into::into)
}

#[async_trait]
impl Integracao for ObsidianCli {
    fn tipo(&self) -> TipoIntegracao {
        TipoIntegracao::ObsidianCli
    }

    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        if self.esta_desconectada() {
            return Ok(Self::desconectado());
        }
        para_o_trait(self.checar().await)
    }

    async fn conectar(
        &self,
        _observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        // Sem login nem código: conectar é testar o servidor MCP (D10).
        para_o_trait(self.testar().await)
    }

    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
        self.desconectada.store(true, Ordering::SeqCst);
        Ok(Self::desconectado())
    }
}
