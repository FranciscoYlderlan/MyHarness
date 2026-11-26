//! Servidor MCP falso, por HTTP, para os testes: imita o servidor da Obsidian CLI
//! (`obsidian-cli-mcp.js`, no Vault) sem Obsidian nem rede de fora.
//!
//! Responde em JSON, em SSE ou em SSE `chunked`; pode atrasar, devolver HTTP de erro,
//! corpo ilegível ou erro JSON-RPC; e cada `tools/call` é respondido por uma função
//! configurável, que recebe o nome da ferramenta e os argumentos.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Como o corpo da resposta vai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Formato {
    Json,
    Sse,
    SseChunked,
    Lixo,
}

type Resposta = Arc<dyn Fn(&str, &Value) -> (String, bool) + Send + Sync>;

/// O que o servidor faz; mude com [`ServidorFalso::config`].
pub(crate) struct Config {
    pub formato: Formato,
    pub atraso: Duration,
    pub status_http: u16,
    pub erro_json_rpc_em: Option<String>,
    pub ferramentas: Vec<String>,
    pub resposta: Resposta,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            formato: Formato::Json,
            atraso: Duration::ZERO,
            status_http: 200,
            erro_json_rpc_em: None,
            ferramentas: [
                "obsidian_search",
                "obsidian_read",
                "obsidian_backlinks",
                "obsidian_help",
            ]
            .map(String::from)
            .to_vec(),
            resposta: Arc::new(|_, _| ("No matches found.".into(), false)),
        }
    }
}

#[derive(Default)]
struct Estado {
    config: Config,
    pedidos_com_sessao: usize,
    chamadas: Vec<(String, Value)>,
}

/// Servidor rodando numa porta local; para quando o teste termina (o runtime cai).
#[derive(Clone)]
pub(crate) struct ServidorFalso {
    porta: u16,
    estado: Arc<Mutex<Estado>>,
}

const SESSAO: &str = "sessao-falsa-1";

impl ServidorFalso {
    /// Sobe numa porta livre.
    pub async fn iniciar() -> Self {
        Self::iniciar_na_porta(0).await
    }

    /// Sobe na porta dada (0 = qualquer livre).
    pub async fn iniciar_na_porta(porta: u16) -> Self {
        let ouvinte = TcpListener::bind(("127.0.0.1", porta)).await.unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let estado = Arc::new(Mutex::new(Estado::default()));
        let compartilhado = Arc::clone(&estado);
        tokio::spawn(async move {
            while let Ok((conexao, _)) = ouvinte.accept().await {
                let estado = Arc::clone(&compartilhado);
                tokio::spawn(async move { atender(conexao, estado).await });
            }
        });
        Self { porta, estado }
    }

    /// Tempo limite dos testes de servidor fora do ar. No Windows, a conexão a uma porta
    /// fechada de `127.0.0.1` não falha na hora: depois do RST, o TCP envia o SYN de novo
    /// (`TcpMaxConnectRetransmissions`) e a recusa só chega em cerca de 2 s. Um limite
    /// de 2 s esgota antes da recusa; este fica bem acima dela.
    pub const LIMITE_FORA_DO_AR: Duration = Duration::from_secs(10);

    /// Endereço de uma porta onde ninguém escuta (servidor fora do ar).
    pub async fn url_sem_ninguem() -> String {
        format!("http://127.0.0.1:{}/mcp", Self::porta_livre().await)
    }

    /// Uma porta livre agora (o ouvinte é solto em seguida).
    pub async fn porta_livre() -> u16 {
        let ouvinte = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        ouvinte.local_addr().unwrap().port()
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.porta)
    }

    pub fn config(&self, mudar: impl FnOnce(&mut Config)) {
        mudar(&mut self.estado.lock().unwrap().config);
    }

    /// Responde cada `tools/call` com `f(ferramenta, argumentos)` → (texto, isError).
    pub fn responder(&self, f: impl Fn(&str, &Value) -> (String, bool) + Send + Sync + 'static) {
        self.config(|c| c.resposta = Arc::new(f));
    }

    pub fn pedidos_com_sessao(&self) -> usize {
        self.estado.lock().unwrap().pedidos_com_sessao
    }

    /// `tools/call` recebidos: (ferramenta, argumentos).
    pub fn chamadas(&self) -> Vec<(String, Value)> {
        self.estado.lock().unwrap().chamadas.clone()
    }
}

async fn atender(mut conexao: TcpStream, estado: Arc<Mutex<Estado>>) {
    let Some((cabeca, corpo)) = ler_pedido(&mut conexao).await else {
        return;
    };
    let com_sessao = cabeca
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("mcp-session-id:"));
    let pedido: Value = serde_json::from_slice(&corpo).unwrap_or(Value::Null);

    let (atraso, formato, status, mensagem) = {
        let mut e = estado.lock().unwrap();
        if com_sessao {
            e.pedidos_com_sessao += 1;
        }
        let mensagem = responder(&mut e, &pedido);
        (
            e.config.atraso,
            e.config.formato,
            e.config.status_http,
            mensagem,
        )
    };
    tokio::time::sleep(atraso).await;

    let resposta = match (mensagem, status) {
        (_, s) if s != 200 => http(s, "text/plain", None, b"falhou".to_vec()),
        (None, _) => http(202, "text/plain", None, Vec::new()),
        (Some(m), _) => {
            let texto = m.to_string();
            match formato {
                Formato::Json => http(200, "application/json", None, texto.into_bytes()),
                Formato::Lixo => http(200, "application/json", None, b"<html>oi".to_vec()),
                Formato::Sse => {
                    let corpo = format!(": ping\n\nevent: message\ndata: {texto}\n\n");
                    http(200, "text/event-stream", None, corpo.into_bytes())
                }
                Formato::SseChunked => {
                    // Fluxo aberto: o servidor não fecha nem manda o pedaço final.
                    let evento = format!("event: message\ndata: {texto}\n\n");
                    let mut r = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                         Mcp-Session-Id: {SESSAO}\r\nTransfer-Encoding: chunked\r\n\r\n"
                    )
                    .into_bytes();
                    let (a, b) = evento.split_at(evento.len() / 2);
                    for pedaco in [a, b] {
                        r.extend_from_slice(
                            format!("{:x}\r\n{pedaco}\r\n", pedaco.len()).as_bytes(),
                        );
                    }
                    let _ = conexao.write_all(&r).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return;
                }
            }
        }
    };
    let _ = conexao.write_all(&resposta).await;
    let _ = conexao.shutdown().await;
}

fn responder(estado: &mut Estado, pedido: &Value) -> Option<Value> {
    let id = pedido.get("id")?.clone();
    let metodo = pedido.get("method").and_then(Value::as_str).unwrap_or("");
    if estado.config.erro_json_rpc_em.as_deref() == Some(metodo) {
        return Some(json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("Method not found: {metodo}") }
        }));
    }
    let resultado = match metodo {
        "initialize" => json!({
            "protocolVersion": super::VERSAO_DO_PROTOCOLO,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "obsidian-cli-falso", "version": "0.0.1" }
        }),
        "tools/list" => json!({
            "tools": estado.config.ferramentas.iter()
                .map(|n| json!({ "name": n, "inputSchema": { "type": "object" } }))
                .collect::<Vec<_>>()
        }),
        "tools/call" => {
            let nome = pedido
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let argumentos = pedido
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(Value::Null);
            estado.chamadas.push((nome.to_string(), argumentos.clone()));
            let (texto, erro) = (estado.config.resposta)(nome, &argumentos);
            json!({ "content": [{ "type": "text", "text": texto }], "isError": erro })
        }
        _ => json!({}),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": resultado }))
}

fn http(status: u16, tipo: &str, extra: Option<&str>, corpo: Vec<u8>) -> Vec<u8> {
    let mut r = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {tipo}\r\nMcp-Session-Id: {SESSAO}\r\n\
         Content-Length: {}\r\nConnection: close\r\n{}\r\n",
        corpo.len(),
        extra.unwrap_or("")
    )
    .into_bytes();
    r.extend_from_slice(&corpo);
    r
}

async fn ler_pedido(conexao: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut bruto = Vec::new();
    let mut pedaco = [0u8; 4096];
    loop {
        if let Some(fim) = bruto.windows(4).position(|j| j == b"\r\n\r\n") {
            let cabeca = String::from_utf8_lossy(&bruto[..fim]).to_string();
            let tamanho = cabeca
                .lines()
                .find_map(|l| {
                    let (n, v) = l.split_once(':')?;
                    n.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if bruto.len() >= fim + 4 + tamanho {
                return Some((cabeca, bruto[fim + 4..fim + 4 + tamanho].to_vec()));
            }
        }
        let lidos = conexao.read(&mut pedaco).await.ok()?;
        if lidos == 0 {
            return None;
        }
        bruto.extend_from_slice(&pedaco[..lidos]);
    }
}
