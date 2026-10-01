//! Dual write (RN-31): plano, tarefa, relatório, achado and the approval of a decisão are written
//! to SQLite and to the memory vault in the same core operation, with the same content. If one
//! side fails, the other is undone and nothing is written (ERR-51 `GRAVACAO_DIVERGENTE`).
//!
//! Order of one operation:
//! 1. the caller opens a transaction and runs its SQLite statements on it (a failing statement
//!    drops the transaction, so the vault is never touched);
//! 2. [`commit_with_vault`] writes the vault side; if that fails, the transaction is rolled back;
//! 3. the transaction is committed; if the commit fails, the vault write is undone.
//!
//! ```ignore
//! let mut tx = db.pool().begin().await.map_err(DualWriteError::Sqlite)?;
//! sqlx::query("INSERT INTO tarefa (...) VALUES (...)")
//!     .execute(&mut *tx)
//!     .await
//!     .map_err(DualWriteError::Sqlite)?;
//! dual_write::commit_with_vault(tx, &mut nota_do_plano).await?;
//! ```

use std::future::Future;

use sqlx::{Sqlite, Transaction};

/// Tables whose rows are also written to the memory vault (ENT-05, ENT-06, ENT-09, ENT-10, ENT-18).
pub const DUAL_WRITE_TABLES: [&str; 5] = [
    "plano_delegacao",
    "tarefa",
    "relatorio",
    "achado",
    "decisao",
];

/// Error raised by the vault side of a dual write.
pub type VaultError = Box<dyn std::error::Error + Send + Sync>;

/// The vault half of a dual write: writes the note content and can undo that write.
pub trait VaultSide {
    /// What [`VaultSide::undo`] needs to remove or restore what was written.
    type Receipt: Send;

    fn write(&mut self) -> impl Future<Output = Result<Self::Receipt, VaultError>> + Send;

    fn undo(
        &mut self,
        receipt: Self::Receipt,
    ) -> impl Future<Output = Result<(), VaultError>> + Send;
}

/// ERR-51 `GRAVACAO_DIVERGENTE`, by the side that failed.
#[derive(Debug, thiserror::Error)]
pub enum DualWriteError {
    /// The SQLite side failed before the vault was written: nothing was written.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: SQLite write failed, nothing was written: {0}")]
    Sqlite(#[source] sqlx::Error),
    /// The vault side failed: the SQLite transaction was rolled back, nothing was written.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: vault write failed, SQLite rolled back: {0}")]
    Vault(#[source] VaultError),
    /// The SQLite commit failed after the vault write, and the vault write was undone.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: SQLite commit failed, vault write undone: {0}")]
    Commit(#[source] sqlx::Error),
    /// The SQLite commit failed and undoing the vault write failed too: the vault holds a note
    /// that SQLite does not. The caller must log it (ENT-12) for the check of AC-148.
    #[error(
        "ERR-51 GRAVACAO_DIVERGENTE: SQLite commit failed ({commit}) and vault undo failed ({undo})"
    )]
    UndoFailed {
        commit: sqlx::Error,
        undo: VaultError,
    },
}

impl DualWriteError {
    /// Catalog id (05-erros).
    pub const ID: &'static str = "ERR-51";
    /// Catalog code (05-erros).
    pub const CODE: &'static str = "GRAVACAO_DIVERGENTE";
    /// Message shown to the Humano (05-erros).
    pub const MESSAGE: &'static str =
        "Não deu para gravar no banco e no cofre ao mesmo tempo; nada foi gravado.";
}

/// Writes the vault side and commits `tx`, undoing whichever side succeeded when the other fails.
pub async fn commit_with_vault<V: VaultSide>(
    tx: Transaction<'_, Sqlite>,
    vault: &mut V,
) -> Result<V::Receipt, DualWriteError> {
    let receipt = match vault.write().await {
        Ok(receipt) => receipt,
        Err(err) => {
            // The transaction was never committed: a failed rollback still leaves nothing written,
            // because dropping the connection rolls it back as well.
            let _ = tx.rollback().await;
            return Err(DualWriteError::Vault(err));
        }
    };

    match tx.commit().await {
        Ok(()) => Ok(receipt),
        Err(commit) => match vault.undo(receipt).await {
            Ok(()) => Err(DualWriteError::Commit(commit)),
            Err(undo) => Err(DualWriteError::UndoFailed { commit, undo }),
        },
    }
}
