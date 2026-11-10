//! Tipos de evento do núcleo que não dependem do Tauri.
//!
//! - [`EventoExecucao`]: a saída ao vivo de um agente, em ordem, por um
//!   [`Canal`](crate::canal::Canal) (arquitetura, seção 1; RNF-01).
//!
//! Os eventos globais (`tauri_specta::Event`) moram no `src-tauri`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::dto::{Mensagem, PlanoId, StatusExecucao};
use crate::erro::ErroCatalogo;

/// Saída ao vivo de uma execução, pelo canal aberto em `enviar_prompt` ou
/// `acompanhar_execucao` (CMP-04). Os textos já chegam mascarados (RN-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoExecucao {
    /// Pedaço de texto da resposta, na ordem em que chega.
    Texto { texto: String },
    /// Uso de uma ferramenta pelo agente (só o nome e um resumo curto).
    Ferramenta { nome: String, resumo: String },
    /// Mensagem completa gravada no chat.
    Mensagem { mensagem: Mensagem },
    /// Total de tokens usados pela execução até aqui.
    UsoTokens { tokens_usados: u32 },
    /// O orquestrador propôs um plano (abre o CMP-10).
    PlanoProposto { plano: PlanoId },
    /// A execução mudou de status; `concluido`, `falhou` e `interrompida` fecham o canal.
    Status { status: StatusExecucao },
    /// Erro do catálogo durante a execução (ex.: ERR-17, ERR-08).
    Erro { erro: ErroCatalogo },
}
