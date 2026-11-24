//! Dual write (RN-31): plano, tarefa, relatório, achado and the approval of a decisão are written
//! to SQLite and to the memory vault in the same core operation, with the same content. If one
//! side fails, the other is undone and nothing is written (ERR-51 `GRAVACAO_DIVERGENTE`). If the
//! SQLite commit fails and undoing the vault note fails too, the note stays in the vault, nothing
//! is in SQLite and the case goes to the Log (ERR-54 `GRAVACAO_DESFEITA_PELA_METADE`, AC-155).
//!
//! Order of one operation:
//! 1. the caller opens a transaction and runs its SQLite statements on it (a failing statement
//!    drops the transaction, so the vault is never touched);
//! 2. [`commit_with_vault`] writes the vault side; if that fails, the transaction is rolled back;
//! 3. the transaction is committed; if the commit fails, the vault write is undone;
//! 4. if the undo fails too, a row goes to `log` (ENT-12) under the run in [`LogTarget`].
//!
//! ```ignore
//! let mut tx = db.pool().begin().await.map_err(DualWriteError::Sqlite)?;
//! sqlx::query("INSERT INTO tarefa (...) VALUES (...)")
//!     .execute(&mut *tx)
//!     .await
//!     .map_err(DualWriteError::Sqlite)?;
//! let log = LogTarget { pool: db.pool(), execucao: id_da_execucao_do_orquestrador };
//! dual_write::commit_with_vault(tx, &mut nota_do_plano, log).await?;
//! ```

use std::future::Future;

use my_harness_core::domain::segredo::mascarar;
use sqlx::{Sqlite, SqlitePool, Transaction};

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

    /// Name of the note this side writes, shown in the ERR-54 message.
    fn note_name(&self) -> String;

    fn write(&mut self) -> impl Future<Output = Result<Self::Receipt, VaultError>> + Send;

    fn undo(
        &mut self,
        receipt: Self::Receipt,
    ) -> impl Future<Output = Result<(), VaultError>> + Send;
}

/// Where the ERR-54 case is logged (ENT-12): the run (ENT-07) that asked for the write.
#[derive(Debug, Clone, Copy)]
pub struct LogTarget<'a> {
    pub pool: &'a SqlitePool,
    pub execucao: i64,
}

/// ERR-51 `GRAVACAO_DIVERGENTE`, by the side that failed, and ERR-54 `GRAVACAO_DESFEITA_PELA_METADE`.
#[derive(Debug, thiserror::Error)]
pub enum DualWriteError {
    /// ERR-51: the SQLite side failed before the vault was written: nothing was written.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: SQLite write failed, nothing was written: {0}")]
    Sqlite(#[source] sqlx::Error),
    /// ERR-51: the vault side failed: the SQLite transaction was rolled back, nothing was written.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: vault write failed, SQLite rolled back: {0}")]
    Vault(#[source] VaultError),
    /// ERR-51: the SQLite commit failed after the vault write, and the vault write was undone.
    #[error("ERR-51 GRAVACAO_DIVERGENTE: SQLite commit failed, vault write undone: {0}")]
    Commit(#[source] sqlx::Error),
    /// ERR-54: the SQLite commit failed and undoing the vault write failed too: the note stays in
    /// the vault, nothing is in SQLite. The case was written to `log`, unless `log` holds the error
    /// that prevented it.
    #[error(
        "ERR-54 GRAVACAO_DESFEITA_PELA_METADE: note {note} stayed in the vault, not in SQLite; \
         SQLite commit failed ({commit}) and vault undo failed ({undo}){}",
        log.as_ref().map(|e| format!("; log write failed ({e})")).unwrap_or_default()
    )]
    HalfUndone {
        note: String,
        // Boxed so the error stays small (`clippy::result_large_err`); `sqlx::Error` is 48 bytes.
        commit: Box<sqlx::Error>,
        undo: VaultError,
        log: Option<Box<sqlx::Error>>,
    },
}

impl DualWriteError {
    /// ERR-51 id, code and message (05-erros).
    pub const ERR_51_ID: &'static str = "ERR-51";
    pub const ERR_51_CODE: &'static str = "GRAVACAO_DIVERGENTE";
    pub const ERR_51_MESSAGE: &'static str =
        "Não deu para gravar no banco e no cofre ao mesmo tempo; nada foi gravado.";

    /// ERR-54 id, code and message (05-erros); `<nome>` is the note name.
    pub const ERR_54_ID: &'static str = "ERR-54";
    pub const ERR_54_CODE: &'static str = "GRAVACAO_DESFEITA_PELA_METADE";
    pub const ERR_54_MESSAGE: &'static str =
        "A nota <nome> ficou no cofre, mas não no banco. Apague a nota ou grave de novo.";

    /// Catalog id (05-erros).
    pub fn id(&self) -> &'static str {
        match self {
            Self::HalfUndone { .. } => Self::ERR_54_ID,
            _ => Self::ERR_51_ID,
        }
    }

    /// Catalog code (05-erros).
    pub fn code(&self) -> &'static str {
        match self {
            Self::HalfUndone { .. } => Self::ERR_54_CODE,
            _ => Self::ERR_51_CODE,
        }
    }

    /// Message shown to the Humano (05-erros).
    pub fn message(&self) -> String {
        match self {
            Self::HalfUndone { note, .. } => Self::ERR_54_MESSAGE.replace("<nome>", note),
            _ => Self::ERR_51_MESSAGE.to_owned(),
        }
    }
}

/// Writes the vault side and commits `tx`, undoing whichever side succeeded when the other fails.
pub async fn commit_with_vault<V: VaultSide>(
    tx: Transaction<'_, Sqlite>,
    vault: &mut V,
    log: LogTarget<'_>,
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

    let commit = match tx.commit().await {
        Ok(()) => return Ok(receipt),
        Err(commit) => commit,
    };
    let undo = match vault.undo(receipt).await {
        Ok(()) => return Err(DualWriteError::Commit(commit)),
        Err(undo) => undo,
    };

    let note = vault.note_name();
    let log = log_half_undone(log, &note, &commit, &undo)
        .await
        .err()
        .map(Box::new);
    Err(DualWriteError::HalfUndone {
        note,
        commit: Box::new(commit),
        undo,
        log,
    })
}

/// ERR-54 row in `log` (ENT-12), with the error texts masked (RN-04).
async fn log_half_undone(
    log: LogTarget<'_>,
    note: &str,
    commit: &sqlx::Error,
    undo: &VaultError,
) -> Result<(), sqlx::Error> {
    let saida = format!("nota: {note}; commit do SQLite: {commit}; desfazer no cofre: {undo}");
    sqlx::query(
        "INSERT INTO log (execucao, comando, inicio, fim, saida_mascarada) \
         VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?)",
    )
    .bind(log.execucao)
    .bind(format!(
        "{} {}",
        DualWriteError::ERR_54_ID,
        DualWriteError::ERR_54_CODE
    ))
    .bind(mascarar(&saida).into_owned())
    .execute(log.pool)
    .await
    .map(|_| ())
}
