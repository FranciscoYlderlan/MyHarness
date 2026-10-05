//! Cliente MCP (Model Context Protocol) pelo transporte HTTP, só com o que o núcleo usa:
//! `initialize`, `tools/list` e `tools/call`.
//!
//! O MyHarness acessa o Obsidian só por um servidor MCP da Obsidian CLI
//! (decisão `geral-decisao-2026-11-19-acesso-ao-obsidian`), alcançado em
//! `OBSIDIAN_MCP_URL`. Cada [`SessaoMcp`] abre uma sessão nova (`initialize` +
//! `notifications/initialized`) e guarda o `Mcp-Session-Id` que o servidor devolver.
//! Toda chamada tem tempo limite: tempo esgotado, erro de rede e resposta que não dá
//! para ler viram [`ErroMcp`], que a integração traduz para o ERR-53.

mod http;

#[cfg(test)]
pub(crate) mod servidor_falso;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use http::{Endereco, RespostaHttp};

/// Versão do protocolo MCP pedida no `initialize`.
pub const VERSAO_DO_PROTOCOLO: &str = "2025-06-18";

/// Falha ao falar com um servidor MCP. As mensagens podem trazer texto do servidor:
/// quem mostra ao Humano passa pela máscara (RN-04).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroMcp {
    /// Endereço fora de `http://host[:porta]/caminho`.
    #[error("endereço do servidor MCP inválido: {0}")]
    EnderecoInvalido(String),
    /// Conexão recusada, nome que não resolve, conexão caída.
    #[error("servidor MCP não responde: {0}")]
    Rede(String),
    /// O servidor não respondeu dentro do tempo limite.
    #[error("tempo esgotado ({0} ms) esperando o servidor MCP")]
    TempoEsgotado(u128),
    /// Status HTTP fora de 2xx.
    #[error("servidor MCP respondeu HTTP {status}: {corpo}")]
    Http {
        /// Status HTTP.
        status: u16,
        /// Começo do corpo da resposta.
        corpo: String,
    },
    /// Corpo que não é JSON-RPC válido ou sem o campo esperado.
    #[error("resposta do servidor MCP que não dá para ler: {0}")]
    RespostaInvalida(String),
    /// O servidor devolveu um erro JSON-RPC.
    #[error("servidor MCP recusou `{metodo}`: {mensagem}")]
    JsonRpc {
        /// Método chamado.
        metodo: String,
        /// Mensagem do erro.
        mensagem: String,
    },
}

/// Resultado de `tools/call`: o texto de todos os itens `text` do `content`, juntados
/// por quebra de linha, e o `isError` do servidor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoFerramenta {
    /// Texto devolvido pela ferramenta (sem máscara: quem mostra mascara).
    pub texto: String,
    /// `isError` do resultado.
    pub erro: bool,
}

/// Dados do servidor devolvidos no `initialize`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InfoServidor {
    /// `serverInfo.name`.
    pub nome: Option<String>,
    /// `serverInfo.version`.
    pub versao: Option<String>,
}

/// Sessão aberta com um servidor MCP por HTTP.
#[derive(Debug)]
pub struct SessaoMcp {
    endereco: Endereco,
    tempo_limite: Duration,
    sessao: Option<String>,
    proximo_id: AtomicU64,
    info: InfoServidor,
}

impl SessaoMcp {
    /// Abre a sessão: `initialize` e `notifications/initialized`, cada um com `tempo_limite`.
    pub async fn abrir(url: &str, tempo_limite: Duration) -> Result<Self, ErroMcp> {
        let mut sessao = Self {
            endereco: Endereco::ler(url)?,
            tempo_limite,
            sessao: None,
            proximo_id: AtomicU64::new(1),
            info: InfoServidor::default(),
        };
        let (resultado, id_da_sessao) = sessao
            .pedir(
                "initialize",
                json!({
                    "protocolVersion": VERSAO_DO_PROTOCOLO,
                    "capabilities": {},
                    "clientInfo": { "name": "MyHarness", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        sessao.sessao = id_da_sessao;
        let texto = |campo: &str| {
            resultado
                .pointer(&format!("/serverInfo/{campo}"))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        sessao.info = InfoServidor {
            nome: texto("name"),
            versao: texto("version"),
        };
        sessao
            .notificar("notifications/initialized", json!({}))
            .await?;
        Ok(sessao)
    }

    /// Dados do servidor (`serverInfo`).
    pub fn info(&self) -> &InfoServidor {
        &self.info
    }

    /// Nomes das ferramentas que o servidor expõe (`tools/list`).
    pub async fn listar_ferramentas(&self) -> Result<Vec<String>, ErroMcp> {
        let (resultado, _) = self.pedir("tools/list", json!({})).await?;
        let ferramentas = resultado
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| ErroMcp::RespostaInvalida("`tools/list` sem `tools`".into()))?;
        Ok(ferramentas
            .iter()
            .filter_map(|f| f.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect())
    }

    /// Chama uma ferramenta (`tools/call`).
    pub async fn chamar(
        &self,
        ferramenta: &str,
        argumentos: Value,
    ) -> Result<ResultadoFerramenta, ErroMcp> {
        let (resultado, _) = self
            .pedir(
                "tools/call",
                json!({ "name": ferramenta, "arguments": argumentos }),
            )
            .await?;
        let conteudo = resultado
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| ErroMcp::RespostaInvalida("`tools/call` sem `content`".into()))?;
        let texto = conteudo
            .iter()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        let erro = resultado
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(ResultadoFerramenta { texto, erro })
    }

    /// Pedido JSON-RPC com `id`; devolve `result` e o `Mcp-Session-Id` da resposta.
    async fn pedir(
        &self,
        metodo: &str,
        parametros: Value,
    ) -> Result<(Value, Option<String>), ErroMcp> {
        let id = self.proximo_id.fetch_add(1, Ordering::Relaxed);
        let mensagem =
            json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": parametros });
        let parada = move |corpo: &[u8]| achar_resposta_sse(corpo, id).is_some();
        let resposta = self.enviar(&mensagem, &parada).await?;
        let corpo = if resposta.e_sse() {
            achar_resposta_sse(&resposta.corpo, id).ok_or_else(|| {
                ErroMcp::RespostaInvalida(format!("fluxo SSE sem a resposta de `{metodo}`"))
            })?
        } else {
            serde_json::from_slice::<Value>(&resposta.corpo)
                .map_err(|e| ErroMcp::RespostaInvalida(format!("`{metodo}`: {e}")))?
        };
        if let Some(erro) = corpo.get("error") {
            let mensagem = erro
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("sem mensagem")
                .to_string();
            return Err(ErroMcp::JsonRpc {
                metodo: metodo.to_string(),
                mensagem,
            });
        }
        let resultado = corpo
            .get("result")
            .cloned()
            .ok_or_else(|| ErroMcp::RespostaInvalida(format!("`{metodo}` sem `result`")))?;
        let id_da_sessao = resposta.cabecalho("mcp-session-id").map(str::to_string);
        Ok((resultado, id_da_sessao))
    }

    /// Notificação JSON-RPC (sem `id`): basta o servidor aceitar (2xx).
    async fn notificar(&self, metodo: &str, parametros: Value) -> Result<(), ErroMcp> {
        let mensagem = json!({ "jsonrpc": "2.0", "method": metodo, "params": parametros });
        self.enviar(&mensagem, &|_| true).await.map(|_| ())
    }

    async fn enviar(
        &self,
        mensagem: &Value,
        parada_sse: http::Parada<'_>,
    ) -> Result<RespostaHttp, ErroMcp> {
        let corpo = mensagem.to_string();
        let mut cabecalhos = vec![("MCP-Protocol-Version", VERSAO_DO_PROTOCOLO)];
        if let Some(sessao) = &self.sessao {
            cabecalhos.push(("Mcp-Session-Id", sessao.as_str()));
        }
        let resposta = tokio::time::timeout(
            self.tempo_limite,
            http::post(&self.endereco, &cabecalhos, corpo.as_bytes(), parada_sse),
        )
        .await
        .map_err(|_| ErroMcp::TempoEsgotado(self.tempo_limite.as_millis()))??;
        if !(200..300).contains(&resposta.status) {
            let corpo: String = String::from_utf8_lossy(&resposta.corpo)
                .chars()
                .take(300)
                .collect();
            return Err(ErroMcp::Http {
                status: resposta.status,
                corpo,
            });
        }
        Ok(resposta)
    }
}

/// Procura, num corpo SSE, o evento `data:` com a mensagem JSON-RPC de `id`.
fn achar_resposta_sse(corpo: &[u8], id: u64) -> Option<Value> {
    let texto = String::from_utf8_lossy(corpo).replace("\r\n", "\n");
    // Só eventos completos (terminados por linha em branco).
    let completos = &texto[..texto.rfind("\n\n")?];
    completos.split("\n\n").find_map(|evento| {
        let dados: Vec<&str> = evento
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.strip_prefix(' ').unwrap_or(l))
            .collect();
        if dados.is_empty() {
            return None;
        }
        let valor: Value = serde_json::from_str(&dados.join("\n")).ok()?;
        (valor.get("id").and_then(Value::as_u64) == Some(id)).then_some(valor)
    })
}

#[cfg(test)]
mod tests {
    use super::servidor_falso::{Formato, ServidorFalso};
    use super::*;

    const LIMITE: Duration = Duration::from_secs(2);

    #[test]
    fn acha_a_resposta_certa_no_sse() {
        let corpo = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"x\"}\n\n\
                      data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n\n";
        assert!(achar_resposta_sse(corpo, 7).is_some());
        assert!(achar_resposta_sse(corpo, 8).is_none());
        // Evento incompleto ainda não conta.
        assert!(achar_resposta_sse(b"data: {\"id\":1,\"result\":{}}\n", 1).is_none());
    }

    #[tokio::test]
    async fn sessao_por_json_por_sse_e_chunked() {
        for formato in [Formato::Json, Formato::Sse, Formato::SseChunked] {
            let servidor = ServidorFalso::iniciar().await;
            servidor.config(|c| c.formato = formato);
            let sessao = SessaoMcp::abrir(&servidor.url(), LIMITE).await.unwrap();
            assert_eq!(sessao.info().nome.as_deref(), Some("obsidian-cli-falso"));
            let ferramentas = sessao.listar_ferramentas().await.unwrap();
            assert!(
                ferramentas.contains(&"obsidian_search".to_string()),
                "{formato:?}"
            );
            let r = sessao
                .chamar("obsidian_search", json!({"vault": "v", "query": "q"}))
                .await
                .unwrap();
            assert!(!r.erro, "{formato:?}");
            // O id de sessão do initialize volta nos pedidos seguintes.
            assert!(servidor.pedidos_com_sessao() >= 2, "{formato:?}");
        }
    }

    #[tokio::test]
    async fn servidor_fora_do_ar_e_rede() {
        let url = ServidorFalso::url_sem_ninguem().await;
        let inicio = std::time::Instant::now();
        let erro = SessaoMcp::abrir(&url, ServidorFalso::LIMITE_FORA_DO_AR)
            .await
            .unwrap_err();
        let tempo = inicio.elapsed();
        assert!(matches!(erro, ErroMcp::Rede(_)), "{erro:?} em {tempo:?}");
    }

    #[tokio::test]
    async fn servidor_mudo_esgota_o_tempo() {
        let servidor = ServidorFalso::iniciar().await;
        servidor.config(|c| c.atraso = Duration::from_millis(500));
        let erro = SessaoMcp::abrir(&servidor.url(), Duration::from_millis(100))
            .await
            .unwrap_err();
        assert_eq!(erro, ErroMcp::TempoEsgotado(100));
    }

    #[tokio::test]
    async fn erro_http_e_corpo_ilegivel() {
        let servidor = ServidorFalso::iniciar().await;
        servidor.config(|c| c.status_http = 500);
        let erro = SessaoMcp::abrir(&servidor.url(), LIMITE).await.unwrap_err();
        assert!(
            matches!(erro, ErroMcp::Http { status: 500, .. }),
            "{erro:?}"
        );

        let servidor = ServidorFalso::iniciar().await;
        servidor.config(|c| c.formato = Formato::Lixo);
        let erro = SessaoMcp::abrir(&servidor.url(), LIMITE).await.unwrap_err();
        assert!(matches!(erro, ErroMcp::RespostaInvalida(_)), "{erro:?}");
    }

    #[tokio::test]
    async fn erro_json_rpc() {
        let servidor = ServidorFalso::iniciar().await;
        servidor.config(|c| c.erro_json_rpc_em = Some("tools/list".into()));
        let sessao = SessaoMcp::abrir(&servidor.url(), LIMITE).await.unwrap();
        let erro = sessao.listar_ferramentas().await.unwrap_err();
        assert!(matches!(erro, ErroMcp::JsonRpc { .. }), "{erro:?}");
    }
}
