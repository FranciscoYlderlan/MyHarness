//! Eventos do núcleo para a tela.
//!
//! - **Eventos globais** (`tauri_specta::Event`): mudanças de estado que qualquer painel
//!   pode ouvir (integrações, consumo, chat, execuções, logs, leituras, pendências).
//! - **Canal por execução** ([`EventoExecucao`] num `tauri::ipc::Channel`): a saída ao
//!   vivo de um agente, em ordem (arquitetura, seção 1; RNF-01).
//!
//! A tela valida cada payload com zod (`src/lib/ipc/`) antes de usar.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use super::dto::{
    Chat, ChatId, Consumo, Execucao, Integracao, Leitura, Log, Mensagem, Pendencia, PlanoId,
    StatusExecucao,
};
use super::erro::ErroCatalogo;

/// Uma integração mudou de status (CMP-03, CMP-11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct IntegracaoMudou {
    pub integracao: Integracao,
}

/// Leitura nova de consumo (CMP-01), ou o aviso de status desconhecido (ERR-50).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct ConsumoAtualizado {
    pub consumo: Consumo,
    pub avisos: Vec<ErroCatalogo>,
}

/// O chat mudou (tokens, status, continuações; CMP-07). `avisos` traz o ERR-48 a cada
/// resposta acima do limite (RN-23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct ChatMudou {
    pub chat: Chat,
    pub avisos: Vec<ErroCatalogo>,
}

/// Uma execução foi criada ou mudou de status (CMP-04, CMP-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct ExecucaoMudou {
    pub execucao: Execucao,
}

/// Comando novo ou atualizado no log (CMP-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct LogRegistrado {
    pub log: Log,
}

/// Leitura nova de um agente (CMP-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct LeituraRegistrada {
    pub leitura: Leitura,
}

/// A caixa de aval mudou (CMP-10): a lista completa do chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
pub struct PendenciasMudaram {
    pub chat: ChatId,
    pub pendencias: Vec<Pendencia>,
}

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
