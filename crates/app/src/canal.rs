//! Canal ordenado do núcleo para a tela, sem o Tauri.
//!
//! O núcleo manda as instruções de conexão e a saída ao vivo de uma execução por um
//! [`Canal`]. No app, o `src-tauri` cria o canal a partir do `tauri::ipc::Channel` do
//! comando; nos testes, o canal guarda o que recebe.

use std::fmt;
use std::sync::Arc;

/// A tela não recebe mais mensagens por este canal (ex.: janela fechada).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("o canal para a tela está fechado")]
pub struct CanalFechado;

type Envio<T> = dyn Fn(T) -> Result<(), CanalFechado> + Send + Sync;

/// Canal para a tela: cada `enviar` entrega uma mensagem, na ordem das chamadas.
pub struct Canal<T> {
    enviar: Arc<Envio<T>>,
}

impl<T> Canal<T> {
    /// Canal que entrega cada mensagem à função `enviar`.
    pub fn new(enviar: impl Fn(T) -> Result<(), CanalFechado> + Send + Sync + 'static) -> Self {
        Self {
            enviar: Arc::new(enviar),
        }
    }

    /// Entrega uma mensagem à tela.
    pub fn send(&self, mensagem: T) -> Result<(), CanalFechado> {
        (self.enviar)(mensagem)
    }
}

impl<T> Clone for Canal<T> {
    fn clone(&self) -> Self {
        Self {
            enviar: self.enviar.clone(),
        }
    }
}

impl<T> fmt::Debug for Canal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Canal")
    }
}
