//! Leitor dos eventos do `claude -p --output-format stream-json` (decisão
//! `geral-decisao-2026-11-19-agente-por-claude-p`; plano B3 da onda 0).
//!
//! A CLI escreve um JSON por linha. O [`LeitorEventosCli`] lê uma linha por vez e devolve
//! os [`EventoCli`] que interessam ao núcleo:
//!
//! | Linha da CLI | Evento |
//! |---|---|
//! | `system`/`init` | [`EventoCli::Init`] |
//! | `stream_event` com `text_delta` (só com `--include-partial-messages`) | [`EventoCli::Texto`] (um pedaço) |
//! | `assistant`, bloco `text` | [`EventoCli::Texto`] (o bloco inteiro, se não veio em pedaços) |
//! | `assistant`, bloco `tool_use` | [`EventoCli::FerramentaInicio`] |
//! | `user`, bloco `tool_result` | [`EventoCli::FerramentaFim`] |
//! | `rate_limit_event` | [`EventoCli::RateLimit`] |
//! | `assistant` com `usage` (uma vez por mensagem) | [`EventoCli::Uso`] |
//! | `result` | [`EventoCli::Resultado`] |
//!
//! **Tolerante:** linha vazia, tipo conhecido sem interesse (`message_stop`, `ping`…) e
//! campo a mais não geram evento nem erro. Linha que não é JSON, sem `type` ou com tipo
//! desconhecido volta como [`LinhaIgnorada`], para o núcleo registrar no Log; a leitura
//! segue na linha seguinte. Campo esperado ausente vira valor padrão ou `None`.
//!
//! Os textos saem como a CLI mandou, **sem máscara**: quem leva o evento para fora do
//! núcleo (tela, Log) aplica a máscara de [`super::segredo`] (RN-04).
//!
//! Fontes do formato: protótipo P2 (2026-11-19, `rate_limit_event` com `unifiedWindows`) e
//! a tabela de campos citada em `geral-plano-arquitetura-e-stack-integracoes-e-testes`.

use std::collections::HashSet;

use serde_json::{Map, Value};

/// Um evento da CLI que interessa ao núcleo.
#[derive(Debug, Clone, PartialEq)]
pub enum EventoCli {
    /// Começo da sessão: modelo, sessão e versão da CLI.
    Init(Init),
    /// Pedaço de texto da resposta, na ordem em que chegou.
    Texto {
        /// O texto do pedaço.
        pedaco: String,
        /// `parent_tool_use_id`: texto de um subagente (ferramenta `Task`); `None` no agente.
        dentro_de: Option<String>,
    },
    /// O agente chamou uma ferramenta.
    FerramentaInicio(FerramentaInicio),
    /// A ferramenta devolveu o resultado.
    FerramentaFim(FerramentaFim),
    /// Situação da janela de 5 h e da semana (RF-18, ENT-14).
    RateLimit(RateLimit),
    /// Tokens de uma mensagem do modelo (RF-17), uma vez por mensagem.
    Uso(Uso),
    /// Fim da execução, com o texto final e o uso total.
    Resultado(Resultado),
}

/// Dados do `system`/`init`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Init {
    /// `session_id`: vai para o `--resume` do próximo prompt do chat.
    pub sessao: String,
    /// `model` em uso.
    pub modelo: Option<String>,
    /// `permissionMode` em uso (o núcleo pede `dontAsk`).
    pub modo_permissao: Option<String>,
    /// `claude_code_version`: entra no Log junto do ERR-50.
    pub versao_cli: Option<String>,
    /// `cwd` do processo.
    pub pasta: Option<String>,
    /// `tools` disponíveis, na ordem da CLI.
    pub ferramentas: Vec<String>,
    /// `mcp_servers`: nome e situação (`connected`, `failed`…).
    pub servidores_mcp: Vec<ServidorMcp>,
}

/// Um servidor MCP do `init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServidorMcp {
    /// `name`.
    pub nome: String,
    /// `status`.
    pub status: String,
}

/// Bloco `tool_use` de uma mensagem do modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct FerramentaInicio {
    /// `id` do uso (`toolu_…`), o mesmo do [`FerramentaFim::id`].
    pub id: String,
    /// `name` da ferramenta (`Bash`, `Read`, `mcp__…`).
    pub nome: String,
    /// `input` como a CLI mandou.
    pub entrada: Value,
    /// `parent_tool_use_id`: uso de um subagente; `None` no agente.
    pub dentro_de: Option<String>,
}

/// Bloco `tool_result` de uma mensagem do usuário (a resposta da ferramenta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FerramentaFim {
    /// `tool_use_id`.
    pub id: String,
    /// `content` em texto; com lista de blocos, os textos unidos por `\n`.
    pub saida: String,
    /// `is_error`.
    pub erro: bool,
    /// `N` de um `Exit code N` no começo da saída (ferramenta `Bash` ou `PowerShell`).
    pub codigo_saida: Option<i32>,
    /// `parent_tool_use_id`: resultado de um subagente; `None` no agente.
    pub dentro_de: Option<String>,
}

impl FerramentaFim {
    /// Código de saída para o Log: o `Exit code N`; sem ele, 1 com `is_error` e 0 sem.
    pub fn codigo_efetivo(&self) -> i32 {
        self.codigo_saida.unwrap_or(i32::from(self.erro))
    }
}

/// Situação de uma janela do `rate_limit_event`, pelos valores do ENT-14.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusJanela {
    /// `allowed`.
    Permitido,
    /// `allowed_warning`.
    PermitidoComAviso,
    /// `rejected`.
    Recusado,
    /// Valor fora da lista, guardado como veio (o núcleo grava `desconhecido` e dá o ERR-50).
    Desconhecido(String),
}

impl StatusJanela {
    fn de(texto: &str) -> Self {
        match texto {
            "allowed" => Self::Permitido,
            "allowed_warning" => Self::PermitidoComAviso,
            "rejected" => Self::Recusado,
            outro => Self::Desconhecido(outro.to_owned()),
        }
    }
}

/// Uma janela de consumo (5 h ou 7 dias).
#[derive(Debug, Clone, PartialEq)]
pub struct Janela {
    /// `utilization`, de 0 a 1 (0,71 = 71%).
    pub utilizacao: Option<f64>,
    /// `resetsAt`, em segundos desde 1970 (UTC).
    pub reinicia_em: Option<i64>,
    /// `status` da janela; sem ele, o `status` geral quando o `rateLimitType` é esta janela.
    pub status: Option<StatusJanela>,
}

impl Janela {
    /// Utilização em pontos percentuais (0 a 100), arredondada.
    pub fn percentual(&self) -> Option<u8> {
        self.utilizacao
            .map(|u| (u * 100.0).round().clamp(0.0, 100.0) as u8)
    }
}

/// Dados do `rate_limit_event` (`rate_limit_info`).
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimit {
    /// `unifiedWindows.five_hour` (ou o evento com `rateLimitType` = `five_hour`).
    pub cinco_horas: Option<Janela>,
    /// `unifiedWindows.seven_day` (ou o evento com `rateLimitType` = `seven_day`).
    pub sete_dias: Option<Janela>,
    /// `status` geral do evento.
    pub status: Option<StatusJanela>,
    /// `overageStatus`, como veio (ENT-14, campo `excedente`).
    pub excedente: Option<String>,
}

/// Tokens de uma mensagem (`usage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContagemTokens {
    /// `input_tokens`.
    pub entrada: u64,
    /// `output_tokens`.
    pub saida: u64,
    /// `cache_creation_input_tokens`.
    pub cache_criacao: u64,
    /// `cache_read_input_tokens`.
    pub cache_leitura: u64,
}

impl ContagemTokens {
    /// Soma dos quatro campos: o que a mensagem ocupou do contexto e gerou.
    pub fn total(&self) -> u64 {
        self.entrada + self.saida + self.cache_criacao + self.cache_leitura
    }

    fn de(usage: &Value) -> Self {
        let n = |chave: &str| usage.get(chave).and_then(Value::as_u64).unwrap_or(0);
        Self {
            entrada: n("input_tokens"),
            saida: n("output_tokens"),
            cache_criacao: n("cache_creation_input_tokens"),
            cache_leitura: n("cache_read_input_tokens"),
        }
    }
}

/// Uso de uma mensagem do modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uso {
    /// `message.id` (`msg_…`); a CLI repete a mesma mensagem por bloco, o leitor emite uma vez.
    pub mensagem: Option<String>,
    /// Os tokens.
    pub tokens: ContagemTokens,
    /// `parent_tool_use_id`: mensagem de um subagente; `None` no agente.
    pub dentro_de: Option<String>,
}

/// Dados do `result`.
#[derive(Debug, Clone, PartialEq)]
pub struct Resultado {
    /// `subtype`: `success`, `error_max_turns`, `error_during_execution`…
    pub subtipo: String,
    /// `is_error`.
    pub erro: bool,
    /// `result`: o texto final (ou a mensagem de erro).
    pub texto: Option<String>,
    /// `session_id`.
    pub sessao: Option<String>,
    /// `num_turns`.
    pub turnos: Option<u64>,
    /// `duration_ms`.
    pub duracao_ms: Option<u64>,
    /// `total_cost_usd`.
    pub custo_usd: Option<f64>,
    /// `usage` da execução inteira.
    pub tokens: Option<ContagemTokens>,
    /// Quantos itens vieram em `permission_denials` (ferramentas negadas).
    pub negacoes: usize,
}

impl Resultado {
    /// Sucesso: `subtype` = `success` e `is_error` falso.
    pub fn sucesso(&self) -> bool {
        self.subtipo == "success" && !self.erro
    }
}

/// Por que uma linha não virou evento. A leitura segue na linha seguinte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinhaIgnorada {
    /// Não é um objeto JSON (o começo da linha, até 200 caracteres).
    NaoJson {
        /// Começo da linha.
        inicio: String,
    },
    /// Objeto JSON sem `type` em texto.
    SemTipo,
    /// `type` (ou `system.subtype`) que o leitor não conhece: talvez versão nova da CLI.
    TipoDesconhecido {
        /// O tipo, como `assistant` ou `system/compact_boundary`.
        tipo: String,
    },
}

/// Lê o `stream-json` linha a linha. Guarda o mínimo de estado para não repetir texto
/// (pedaços e mensagem inteira) nem uso (a mesma mensagem em várias linhas).
#[derive(Debug, Default)]
pub struct LeitorEventosCli {
    /// Mensagem aberta pelo último `message_start`, por `parent_tool_use_id`.
    mensagem_em_pedacos: Vec<(Option<String>, String)>,
    /// Mensagens cujo texto já saiu em pedaços.
    com_pedacos: HashSet<String>,
    /// Mensagens cujo uso já saiu.
    com_uso: HashSet<String>,
}

/// Tipos do `stream_event` que não trazem nada além do que a mensagem inteira traz.
const TIPOS_SEM_INTERESSE: [&str; 5] = [
    "message_stop",
    "content_block_start",
    "content_block_stop",
    "message_delta",
    "ping",
];

impl LeitorEventosCli {
    /// Leitor novo, para uma execução.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lê uma linha (com ou sem o `\n`). `Ok(vec![])` para linha vazia ou sem interesse.
    pub fn ler_linha(&mut self, linha: &str) -> Result<Vec<EventoCli>, LinhaIgnorada> {
        let linha = linha.trim();
        if linha.is_empty() {
            return Ok(Vec::new());
        }
        let valor: Value = serde_json::from_str(linha).map_err(|_| LinhaIgnorada::NaoJson {
            inicio: linha.chars().take(200).collect(),
        })?;
        let objeto = valor.as_object().ok_or_else(|| LinhaIgnorada::NaoJson {
            inicio: linha.chars().take(200).collect(),
        })?;
        let tipo = texto(objeto, "type").ok_or(LinhaIgnorada::SemTipo)?;
        let dentro_de = texto(objeto, "parent_tool_use_id");
        match tipo.as_str() {
            "system" => match texto(objeto, "subtype").as_deref() {
                Some("init") => Ok(vec![EventoCli::Init(ler_init(objeto))]),
                outro => Err(LinhaIgnorada::TipoDesconhecido {
                    tipo: format!("system/{}", outro.unwrap_or("")),
                }),
            },
            "stream_event" => self.ler_stream_event(objeto, dentro_de),
            "assistant" => Ok(self.ler_assistant(objeto, dentro_de)),
            "user" => Ok(ler_user(objeto, dentro_de)),
            "rate_limit_event" => Ok(vec![EventoCli::RateLimit(ler_rate_limit(objeto))]),
            "result" => Ok(vec![EventoCli::Resultado(ler_resultado(objeto))]),
            _ => Err(LinhaIgnorada::TipoDesconhecido { tipo }),
        }
    }

    fn ler_stream_event(
        &mut self,
        objeto: &Map<String, Value>,
        dentro_de: Option<String>,
    ) -> Result<Vec<EventoCli>, LinhaIgnorada> {
        let evento = objeto.get("event").and_then(Value::as_object);
        let Some(evento) = evento else {
            return Ok(Vec::new());
        };
        let tipo = texto(evento, "type").unwrap_or_default();
        match tipo.as_str() {
            "message_start" => {
                if let Some(id) = evento
                    .get("message")
                    .and_then(|m| m.get("id"))
                    .and_then(Value::as_str)
                {
                    self.mensagem_em_pedacos
                        .retain(|(pai, _)| *pai != dentro_de);
                    self.mensagem_em_pedacos.push((dentro_de, id.to_owned()));
                }
                Ok(Vec::new())
            }
            "content_block_delta" => {
                let delta = evento.get("delta");
                let e_texto =
                    delta.and_then(|d| d.get("type")).and_then(Value::as_str) == Some("text_delta");
                let pedaco = delta.and_then(|d| d.get("text")).and_then(Value::as_str);
                match (e_texto, pedaco) {
                    (true, Some(pedaco)) if !pedaco.is_empty() => {
                        if let Some((_, id)) = self
                            .mensagem_em_pedacos
                            .iter()
                            .find(|(p, _)| *p == dentro_de)
                        {
                            self.com_pedacos.insert(id.clone());
                        }
                        Ok(vec![EventoCli::Texto {
                            pedaco: pedaco.to_owned(),
                            dentro_de,
                        }])
                    }
                    // `input_json_delta`, `thinking_delta`…: a mensagem inteira traz o resto.
                    _ => Ok(Vec::new()),
                }
            }
            t if TIPOS_SEM_INTERESSE.contains(&t) => Ok(Vec::new()),
            _ => Err(LinhaIgnorada::TipoDesconhecido {
                tipo: format!("stream_event/{tipo}"),
            }),
        }
    }

    fn ler_assistant(
        &mut self,
        objeto: &Map<String, Value>,
        dentro_de: Option<String>,
    ) -> Vec<EventoCli> {
        let Some(mensagem) = objeto.get("message").and_then(Value::as_object) else {
            return Vec::new();
        };
        let id = texto(mensagem, "id");
        let ja_em_pedacos = id.as_ref().is_some_and(|i| self.com_pedacos.contains(i));
        let mut eventos = Vec::new();
        for bloco in blocos(mensagem.get("content")) {
            match bloco.get("type").and_then(Value::as_str) {
                Some("text") if !ja_em_pedacos => {
                    if let Some(t) = bloco.get("text").and_then(Value::as_str)
                        && !t.is_empty()
                    {
                        eventos.push(EventoCli::Texto {
                            pedaco: t.to_owned(),
                            dentro_de: dentro_de.clone(),
                        });
                    }
                }
                Some("tool_use") => eventos.push(EventoCli::FerramentaInicio(FerramentaInicio {
                    id: texto_valor(bloco, "id").unwrap_or_default(),
                    nome: texto_valor(bloco, "name").unwrap_or_default(),
                    entrada: bloco.get("input").cloned().unwrap_or(Value::Null),
                    dentro_de: dentro_de.clone(),
                })),
                _ => {}
            }
        }
        if let Some(usage) = mensagem.get("usage").filter(|u| u.is_object()) {
            let nova = match &id {
                Some(i) => self.com_uso.insert(i.clone()),
                None => true,
            };
            if nova {
                eventos.push(EventoCli::Uso(Uso {
                    mensagem: id,
                    tokens: ContagemTokens::de(usage),
                    dentro_de,
                }));
            }
        }
        eventos
    }
}

fn texto(objeto: &Map<String, Value>, chave: &str) -> Option<String> {
    objeto.get(chave).and_then(Value::as_str).map(str::to_owned)
}

fn texto_valor(valor: &Value, chave: &str) -> Option<String> {
    valor.get(chave).and_then(Value::as_str).map(str::to_owned)
}

fn blocos(conteudo: Option<&Value>) -> impl Iterator<Item = &Value> {
    conteudo
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| b.is_object())
}

fn ler_init(objeto: &Map<String, Value>) -> Init {
    let ferramentas = objeto
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let servidores_mcp = objeto
        .get("mcp_servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| {
            Some(ServidorMcp {
                nome: texto_valor(s, "name")?,
                status: texto_valor(s, "status").unwrap_or_default(),
            })
        })
        .collect();
    Init {
        sessao: texto(objeto, "session_id").unwrap_or_default(),
        modelo: texto(objeto, "model"),
        modo_permissao: texto(objeto, "permissionMode"),
        versao_cli: texto(objeto, "claude_code_version"),
        pasta: texto(objeto, "cwd"),
        ferramentas,
        servidores_mcp,
    }
}

fn ler_user(objeto: &Map<String, Value>, dentro_de: Option<String>) -> Vec<EventoCli> {
    let conteudo = objeto.get("message").and_then(|m| m.get("content"));
    blocos(conteudo)
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .map(|b| {
            let saida = saida_da_ferramenta(b.get("content"));
            EventoCli::FerramentaFim(FerramentaFim {
                id: texto_valor(b, "tool_use_id").unwrap_or_default(),
                erro: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                codigo_saida: codigo_de_saida(&saida),
                saida,
                dentro_de: dentro_de.clone(),
            })
        })
        .collect()
}

/// `content` de um `tool_result`: texto, ou lista de blocos `{type: text, text}`.
fn saida_da_ferramenta(conteudo: Option<&Value>) -> String {
    match conteudo {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(lista)) => lista
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// `N` de um `Exit code N` no começo da saída (espaços antes são aceitos).
fn codigo_de_saida(saida: &str) -> Option<i32> {
    let resto = saida.trim_start().strip_prefix("Exit code ")?;
    let fim = resto
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
        .map_or(resto.len(), |(i, _)| i);
    resto[..fim].parse().ok()
}

fn ler_status(valor: Option<&Value>) -> Option<StatusJanela> {
    valor.and_then(Value::as_str).map(StatusJanela::de)
}

fn ler_janela(valor: &Value) -> Janela {
    Janela {
        utilizacao: valor.get("utilization").and_then(Value::as_f64),
        reinicia_em: valor.get("resetsAt").and_then(Value::as_i64),
        status: ler_status(valor.get("status")),
    }
}

fn ler_rate_limit(objeto: &Map<String, Value>) -> RateLimit {
    let vazio = Value::Null;
    let info = objeto.get("rate_limit_info").unwrap_or(&vazio);
    let status = ler_status(info.get("status"));
    let tipo = info.get("rateLimitType").and_then(Value::as_str);
    let janelas = info.get("unifiedWindows");
    let janela = |nome: &str| -> Option<Janela> {
        let mut j = match janelas.and_then(|w| w.get(nome)) {
            Some(v) if v.is_object() => ler_janela(v),
            // Formato de uma janela por evento: os campos ficam no próprio `rate_limit_info`.
            _ if tipo == Some(nome) => ler_janela(info),
            _ => return None,
        };
        if j.status.is_none() && tipo == Some(nome) {
            j.status = status.clone();
        }
        Some(j)
    };
    RateLimit {
        cinco_horas: janela("five_hour"),
        sete_dias: janela("seven_day"),
        excedente: info
            .get("overageStatus")
            .and_then(Value::as_str)
            .map(str::to_owned),
        status,
    }
}

fn ler_resultado(objeto: &Map<String, Value>) -> Resultado {
    let n = |chave: &str| objeto.get(chave).and_then(Value::as_u64);
    Resultado {
        subtipo: texto(objeto, "subtype").unwrap_or_default(),
        erro: objeto
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        texto: texto(objeto, "result"),
        sessao: texto(objeto, "session_id"),
        turnos: n("num_turns"),
        duracao_ms: n("duration_ms"),
        custo_usd: objeto.get("total_cost_usd").and_then(Value::as_f64),
        tokens: objeto
            .get("usage")
            .filter(|u| u.is_object())
            .map(ContagemTokens::de),
        negacoes: objeto
            .get("permission_denials")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Amostras no formato da CLI real (`claude -p --verbose --output-format stream-json
    /// --include-partial-messages`), com os campos que a CLI manda e o leitor não usa.
    const INIT: &str = r#"{"type":"system","subtype":"init","cwd":"/repo","session_id":"9b1c0e1e-2f4c-4b55-9d0b-3f1c2a7e8d11","tools":["Task","Bash","Read","Edit","mcp__obsidian-cli__obsidian_read"],"mcp_servers":[{"name":"obsidian-cli","status":"connected"},{"name":"quebrado","status":"failed"}],"model":"claude-sonnet-4-5-20250929","permissionMode":"dontAsk","slash_commands":["compact"],"apiKeySource":"none","claude_code_version":"2.1.328","output_style":"default","agents":[],"skills":[],"plugins":[],"uuid":"a1"}"#;
    const MESSAGE_START: &str = r#"{"type":"stream_event","event":{"type":"message_start","message":{"model":"claude-sonnet-4-5-20250929","id":"msg_01A","type":"message","role":"assistant","content":[],"stop_reason":null,"usage":{"input_tokens":3,"output_tokens":1}}},"session_id":"s","parent_tool_use_id":null,"uuid":"a2"}"#;
    const BLOCK_START: &str = r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"s","parent_tool_use_id":null,"uuid":"a3"}"#;
    const DELTA_1: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Vou ler "}},"session_id":"s","parent_tool_use_id":null,"uuid":"a4"}"#;
    const DELTA_2: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"o arquivo."}},"session_id":"s","parent_tool_use_id":null,"uuid":"a5"}"#;
    const DELTA_JSON: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_"}},"session_id":"s","parent_tool_use_id":null,"uuid":"a6"}"#;
    const MESSAGE_DELTA: &str = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":42}},"session_id":"s","parent_tool_use_id":null,"uuid":"a7"}"#;
    const ASSISTANT_TEXTO: &str = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-5-20250929","id":"msg_01A","type":"message","role":"assistant","content":[{"type":"text","text":"Vou ler o arquivo."}],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":3,"cache_creation_input_tokens":2048,"cache_read_input_tokens":10240,"output_tokens":42,"service_tier":"standard"}},"parent_tool_use_id":null,"session_id":"s","uuid":"a8"}"#;
    const ASSISTANT_TOOL: &str = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-5-20250929","id":"msg_01A","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_01X","name":"Bash","input":{"command":"cargo test","description":"Run tests"}}],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":3,"cache_creation_input_tokens":2048,"cache_read_input_tokens":10240,"output_tokens":42,"service_tier":"standard"}},"parent_tool_use_id":null,"session_id":"s","uuid":"a9"}"#;
    const TOOL_RESULT_ERRO: &str = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_01X","type":"tool_result","content":"Exit code 101\nerror: test failed","is_error":true}]},"parent_tool_use_id":null,"session_id":"s","uuid":"b1","tool_use_result":"Error: Exit code 101\nerror: test failed"}"#;
    const TOOL_RESULT_LISTA: &str = r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_02Y","type":"tool_result","content":[{"type":"text","text":"linha 1"},{"type":"text","text":"linha 2"}]}]},"parent_tool_use_id":"toolu_00T","session_id":"s","uuid":"b2"}"#;
    const RATE_LIMIT: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1790578200,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false,"unifiedWindows":{"five_hour":{"utilization":0.71,"resetsAt":1790578200,"status":"allowed"},"seven_day":{"utilization":0.11,"resetsAt":1791036000,"status":"allowed_warning"}}},"uuid":"b3","session_id":"s"}"#;
    const RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":12345,"duration_api_ms":12000,"num_turns":2,"result":"Pronto.","session_id":"s","total_cost_usd":0.0123,"usage":{"input_tokens":150,"cache_creation_input_tokens":2048,"cache_read_input_tokens":10240,"output_tokens":70,"server_tool_use":{"web_search_requests":0},"service_tier":"standard"},"modelUsage":{},"permission_denials":[],"uuid":"b4"}"#;

    fn ler(linhas: &[&str]) -> Vec<EventoCli> {
        let mut leitor = LeitorEventosCli::new();
        linhas
            .iter()
            .flat_map(|l| leitor.ler_linha(l).expect("linha aceita"))
            .collect()
    }

    fn um(mut eventos: Vec<EventoCli>) -> EventoCli {
        assert_eq!(eventos.len(), 1, "{eventos:?}");
        eventos.remove(0)
    }

    #[test]
    fn init_traz_sessao_modelo_versao_e_mcp() {
        let EventoCli::Init(init) = um(ler(&[INIT])) else {
            panic!("esperava um Init");
        };
        assert_eq!(init.sessao, "9b1c0e1e-2f4c-4b55-9d0b-3f1c2a7e8d11");
        assert_eq!(init.modelo.as_deref(), Some("claude-sonnet-4-5-20250929"));
        assert_eq!(init.modo_permissao.as_deref(), Some("dontAsk"));
        assert_eq!(init.versao_cli.as_deref(), Some("2.1.328"));
        assert_eq!(init.pasta.as_deref(), Some("/repo"));
        assert_eq!(init.ferramentas.len(), 5);
        assert_eq!(
            init.servidores_mcp[1],
            ServidorMcp {
                nome: "quebrado".into(),
                status: "failed".into()
            }
        );
    }

    #[test]
    fn texto_em_pedacos_nao_se_repete_na_mensagem_inteira() {
        let eventos = ler(&[
            MESSAGE_START,
            BLOCK_START,
            DELTA_1,
            DELTA_2,
            DELTA_JSON,
            MESSAGE_DELTA,
            ASSISTANT_TEXTO,
            ASSISTANT_TOOL,
        ]);
        let pedacos: Vec<&str> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoCli::Texto { pedaco, .. } => Some(pedaco.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(pedacos, ["Vou ler ", "o arquivo."]);
        // A CLI manda a mesma mensagem duas vezes (um bloco por linha): um Uso só.
        let usos: Vec<&Uso> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoCli::Uso(u) => Some(u),
                _ => None,
            })
            .collect();
        assert_eq!(usos.len(), 1);
        assert_eq!(usos[0].mensagem.as_deref(), Some("msg_01A"));
        assert_eq!(
            usos[0].tokens,
            ContagemTokens {
                entrada: 3,
                saida: 42,
                cache_criacao: 2048,
                cache_leitura: 10240
            }
        );
        assert_eq!(usos[0].tokens.total(), 12333);
        assert!(matches!(
            eventos.last(),
            Some(EventoCli::FerramentaInicio(_))
        ));
    }

    #[test]
    fn sem_pedacos_o_texto_sai_da_mensagem_inteira() {
        let eventos = ler(&[ASSISTANT_TEXTO]);
        assert_eq!(
            eventos[0],
            EventoCli::Texto {
                pedaco: "Vou ler o arquivo.".into(),
                dentro_de: None
            }
        );
        assert!(matches!(eventos[1], EventoCli::Uso(_)));
        assert_eq!(eventos.len(), 2);
    }

    #[test]
    fn ferramenta_inicio_e_fim_com_codigo_de_saida() {
        let eventos = ler(&[ASSISTANT_TOOL, TOOL_RESULT_ERRO]);
        let EventoCli::FerramentaInicio(inicio) = &eventos[0] else {
            panic!("{eventos:?}");
        };
        assert_eq!(inicio.id, "toolu_01X");
        assert_eq!(inicio.nome, "Bash");
        assert_eq!(inicio.entrada["command"], "cargo test");
        assert_eq!(inicio.dentro_de, None);
        let EventoCli::FerramentaFim(fim) = &eventos[2] else {
            panic!("{eventos:?}");
        };
        assert_eq!(fim.id, "toolu_01X");
        assert!(fim.erro);
        assert_eq!(fim.codigo_saida, Some(101));
        assert_eq!(fim.codigo_efetivo(), 101);
        assert_eq!(fim.saida, "Exit code 101\nerror: test failed");
    }

    #[test]
    fn resultado_de_ferramenta_em_lista_e_de_subagente() {
        let EventoCli::FerramentaFim(fim) = um(ler(&[TOOL_RESULT_LISTA])) else {
            panic!("esperava um FerramentaFim");
        };
        assert_eq!(fim.saida, "linha 1\nlinha 2");
        assert!(!fim.erro);
        assert_eq!(fim.codigo_saida, None);
        assert_eq!(fim.codigo_efetivo(), 0);
        assert_eq!(fim.dentro_de.as_deref(), Some("toolu_00T"));
    }

    #[test]
    fn codigo_efetivo_sem_exit_code() {
        let fim = |erro, saida: &str| FerramentaFim {
            id: "t".into(),
            saida: saida.into(),
            erro,
            codigo_saida: codigo_de_saida(saida),
            dentro_de: None,
        };
        assert_eq!(fim(true, "Permission denied").codigo_efetivo(), 1);
        assert_eq!(fim(false, "  Exit code 0\nok").codigo_efetivo(), 0);
        assert_eq!(fim(true, "Exit code -1").codigo_saida, Some(-1));
        assert_eq!(fim(true, "Exit code x").codigo_saida, None);
        assert_eq!(fim(true, "saída\nExit code 2").codigo_saida, None);
    }

    #[test]
    fn rate_limit_com_as_duas_janelas() {
        let EventoCli::RateLimit(r) = um(ler(&[RATE_LIMIT])) else {
            panic!("esperava um RateLimit");
        };
        let cinco = r.cinco_horas.unwrap();
        assert_eq!(cinco.utilizacao, Some(0.71));
        assert_eq!(cinco.percentual(), Some(71));
        assert_eq!(cinco.reinicia_em, Some(1790578200));
        assert_eq!(cinco.status, Some(StatusJanela::Permitido));
        let sete = r.sete_dias.unwrap();
        assert_eq!(sete.percentual(), Some(11));
        assert_eq!(sete.status, Some(StatusJanela::PermitidoComAviso));
        assert_eq!(r.status, Some(StatusJanela::Permitido));
        assert_eq!(r.excedente.as_deref(), Some("rejected"));
    }

    #[test]
    fn rate_limit_de_uma_janela_e_status_novo() {
        let linha = json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "throttled_soon",
                "resetsAt": 1791036000,
                "rateLimitType": "seven_day",
                "utilization": 0.995
            }
        })
        .to_string();
        let EventoCli::RateLimit(r) = um(ler(&[&linha])) else {
            panic!("esperava um RateLimit");
        };
        assert_eq!(r.cinco_horas, None);
        let sete = r.sete_dias.unwrap();
        assert_eq!(sete.percentual(), Some(100));
        assert_eq!(sete.reinicia_em, Some(1791036000));
        assert_eq!(
            sete.status,
            Some(StatusJanela::Desconhecido("throttled_soon".into()))
        );
        assert_eq!(r.excedente, None);

        // Janela do `unifiedWindows` sem `status`: herda o geral só se for o `rateLimitType`.
        let linha = json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "rejected", "rateLimitType": "five_hour",
                "unifiedWindows": {
                    "five_hour": {"utilization": 1.0, "resetsAt": 1},
                    "seven_day": {"utilization": 0.2, "resetsAt": 2}
                }
            }
        })
        .to_string();
        let EventoCli::RateLimit(r) = um(ler(&[&linha])) else {
            panic!("esperava um RateLimit");
        };
        assert_eq!(r.cinco_horas.unwrap().status, Some(StatusJanela::Recusado));
        assert_eq!(r.sete_dias.unwrap().status, None);
    }

    #[test]
    fn resultado_de_sucesso_e_de_erro() {
        let EventoCli::Resultado(r) = um(ler(&[RESULT])) else {
            panic!("esperava um Resultado");
        };
        assert!(r.sucesso());
        assert_eq!(r.texto.as_deref(), Some("Pronto."));
        assert_eq!(r.sessao.as_deref(), Some("s"));
        assert_eq!(r.turnos, Some(2));
        assert_eq!(r.duracao_ms, Some(12345));
        assert_eq!(r.custo_usd, Some(0.0123));
        assert_eq!(r.tokens.unwrap().total(), 150 + 70 + 2048 + 10240);
        assert_eq!(r.negacoes, 0);

        let linha = json!({
            "type": "result", "subtype": "error_during_execution", "is_error": true,
            "num_turns": 1, "session_id": "s",
            "permission_denials": [{"tool_name": "Bash", "tool_use_id": "t", "tool_input": {}}]
        })
        .to_string();
        let EventoCli::Resultado(r) = um(ler(&[&linha])) else {
            panic!("esperava um Resultado");
        };
        assert!(!r.sucesso());
        assert_eq!(r.subtipo, "error_during_execution");
        assert_eq!(r.negacoes, 1);
        assert_eq!(r.tokens, None);
        // `success` com `is_error` (ex.: limite de uso) não é sucesso.
        let linha = r#"{"type":"result","subtype":"success","is_error":true,"result":"limite"}"#;
        let EventoCli::Resultado(r) = um(ler(&[linha])) else {
            panic!("esperava um Resultado");
        };
        assert!(!r.sucesso());
    }

    #[test]
    fn linhas_desconhecidas_nao_param_a_leitura() {
        let mut leitor = LeitorEventosCli::new();
        assert_eq!(leitor.ler_linha(""), Ok(vec![]));
        assert_eq!(leitor.ler_linha("  \r\n"), Ok(vec![]));
        assert_eq!(
            leitor.ler_linha("Warning: algo no stdout"),
            Err(LinhaIgnorada::NaoJson {
                inicio: "Warning: algo no stdout".into()
            })
        );
        assert!(matches!(
            leitor.ler_linha("[1,2]"),
            Err(LinhaIgnorada::NaoJson { .. })
        ));
        assert!(matches!(
            leitor.ler_linha(&"x".repeat(1000)),
            Err(LinhaIgnorada::NaoJson { inicio }) if inicio.len() == 200
        ));
        assert_eq!(leitor.ler_linha(r#"{"a":1}"#), Err(LinhaIgnorada::SemTipo));
        assert_eq!(
            leitor.ler_linha(r#"{"type":"tool_progress","x":1}"#),
            Err(LinhaIgnorada::TipoDesconhecido {
                tipo: "tool_progress".into()
            })
        );
        assert_eq!(
            leitor.ler_linha(r#"{"type":"system","subtype":"compact_boundary"}"#),
            Err(LinhaIgnorada::TipoDesconhecido {
                tipo: "system/compact_boundary".into()
            })
        );
        assert_eq!(
            leitor.ler_linha(r#"{"type":"stream_event","event":{"type":"novo"}}"#),
            Err(LinhaIgnorada::TipoDesconhecido {
                tipo: "stream_event/novo".into()
            })
        );
        // Campos ausentes ou de outro tipo: valor padrão, sem pânico.
        assert_eq!(leitor.ler_linha(r#"{"type":"assistant"}"#), Ok(vec![]));
        assert_eq!(
            leitor.ler_linha(r#"{"type":"user","message":{"content":"texto do humano"}}"#),
            Ok(vec![])
        );
        assert_eq!(
            leitor
                .ler_linha(r#"{"type":"assistant","message":{"content":[1,{"type":"tool_use"}]}}"#),
            Ok(vec![EventoCli::FerramentaInicio(FerramentaInicio {
                id: String::new(),
                nome: String::new(),
                entrada: Value::Null,
                dentro_de: None
            })])
        );
        let EventoCli::Init(init) = um(leitor
            .ler_linha(r#"{"type":"system","subtype":"init","tools":"x"}"#)
            .unwrap())
        else {
            panic!();
        };
        assert_eq!(init, Init::default());
        // Depois das linhas ruins, a leitura segue normal.
        assert!(matches!(
            leitor.ler_linha(RESULT).unwrap()[..],
            [EventoCli::Resultado(_)]
        ));
    }

    #[test]
    fn pedacos_de_subagente_nao_misturam_com_os_do_agente() {
        let sub = |linha: &str| {
            linha.replace(
                r#""parent_tool_use_id":null"#,
                r#""parent_tool_use_id":"toolu_T""#,
            )
        };
        let start_sub = sub(&MESSAGE_START.replace("msg_01A", "msg_SUB"));
        let delta_sub = sub(DELTA_1);
        let texto_sub = sub(&ASSISTANT_TEXTO.replace("msg_01A", "msg_SUB"));
        let eventos = ler(&[
            MESSAGE_START,
            &start_sub,
            &delta_sub,
            ASSISTANT_TEXTO,
            &texto_sub,
        ]);
        let textos: Vec<(&str, Option<&str>)> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoCli::Texto { pedaco, dentro_de } => {
                    Some((pedaco.as_str(), dentro_de.as_deref()))
                }
                _ => None,
            })
            .collect();
        // O agente não teve pedaço: o texto dele sai da mensagem inteira; o do subagente, não.
        assert_eq!(
            textos,
            [("Vou ler ", Some("toolu_T")), ("Vou ler o arquivo.", None)]
        );
    }
}
