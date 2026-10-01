//! Dual write (RN-31, ERR-51; AC-147): if one side fails, nothing is written on either side.

use std::sync::{Arc, Mutex};

use my_harness_lib::infra::db::Database;
use my_harness_lib::infra::db::dual_write::{self, DualWriteError, VaultError, VaultSide};
use sqlx::AssertSqlSafe;

/// In-memory vault: `notes` holds what was written; `fail_write`/`fail_undo` force failures.
#[derive(Default, Clone)]
struct FakeVault {
    notes: Arc<Mutex<Vec<String>>>,
    note: String,
    fail_write: bool,
    fail_undo: bool,
}

impl VaultSide for FakeVault {
    type Receipt = String;

    async fn write(&mut self) -> Result<String, VaultError> {
        if self.fail_write {
            return Err("cofre sem permissão de escrita".into());
        }
        self.notes.lock().unwrap().push(self.note.clone());
        Ok(self.note.clone())
    }

    async fn undo(&mut self, receipt: String) -> Result<(), VaultError> {
        if self.fail_undo {
            return Err("não deu para desfazer".into());
        }
        self.notes.lock().unwrap().retain(|n| *n != receipt);
        Ok(())
    }
}

async fn open() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Database::open(dir.path()).await.expect("open database");
    (dir, db)
}

const INSERT_TAREFA: &str = "INSERT INTO tarefa (faz, toca_em, paralelo, pronto_quando, prova, papel) \
     VALUES ('Esquema', '[\"src-tauri/migrations/\"]', 0, 'testes passam', 'cargo test', 'funcionalidade')";

async fn count(db: &Database, table: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(db.pool())
        .await
        .expect("count");
    n
}

#[tokio::test]
async fn both_sides_written() {
    let (_dir, db) = open().await;
    let mut vault = FakeVault {
        note: "fundacao-plano#T4".into(),
        ..Default::default()
    };

    let mut tx = db.pool().begin().await.expect("begin");
    sqlx::query(INSERT_TAREFA)
        .execute(&mut *tx)
        .await
        .expect("insert");
    let receipt = dual_write::commit_with_vault(tx, &mut vault)
        .await
        .expect("dual write");

    assert_eq!(receipt, "fundacao-plano#T4");
    assert_eq!(count(&db, "tarefa").await, 1);
    assert_eq!(*vault.notes.lock().unwrap(), ["fundacao-plano#T4"]);
}

#[tokio::test]
async fn vault_failure_rolls_back_sqlite() {
    let (_dir, db) = open().await;
    let mut vault = FakeVault {
        note: "n".into(),
        fail_write: true,
        ..Default::default()
    };

    let mut tx = db.pool().begin().await.expect("begin");
    sqlx::query(INSERT_TAREFA)
        .execute(&mut *tx)
        .await
        .expect("insert");
    let err = dual_write::commit_with_vault(tx, &mut vault)
        .await
        .expect_err("must fail");

    assert!(matches!(err, DualWriteError::Vault(_)), "{err}");
    assert!(
        err.to_string().starts_with("ERR-51 GRAVACAO_DIVERGENTE"),
        "{err}"
    );
    assert_eq!(count(&db, "tarefa").await, 0, "SQLite side undone");
    assert!(vault.notes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sqlite_failure_never_touches_the_vault() {
    let (_dir, db) = open().await;
    let vault = FakeVault::default();

    let mut tx = db.pool().begin().await.expect("begin");
    // `faz` empty breaks tarefa_faz_tamanho: the caller stops before the vault side.
    let err = sqlx::query(
        "INSERT INTO tarefa (faz, toca_em, paralelo, pronto_quando, prova, papel) \
         VALUES ('', '[\"a\"]', 0, 'x', 'y', 'funcionalidade')",
    )
    .execute(&mut *tx)
    .await
    .map_err(DualWriteError::Sqlite)
    .expect_err("must fail");
    drop(tx);

    assert!(err.to_string().contains("tarefa_faz_tamanho"), "{err}");
    assert_eq!(count(&db, "tarefa").await, 0);
    assert!(vault.notes.lock().unwrap().is_empty());
}

/// A deferred foreign key makes the COMMIT itself fail, after the vault was written.
async fn failing_commit(db: &Database) -> sqlx::Transaction<'static, sqlx::Sqlite> {
    sqlx::query(
        "CREATE TABLE teste_commit (ref INTEGER REFERENCES tarefa (id) DEFERRABLE INITIALLY DEFERRED)",
    )
    .execute(db.pool())
    .await
    .expect("test table");
    let mut tx = db.pool().begin().await.expect("begin");
    sqlx::query(INSERT_TAREFA)
        .execute(&mut *tx)
        .await
        .expect("insert");
    sqlx::query("INSERT INTO teste_commit (ref) VALUES (999)")
        .execute(&mut *tx)
        .await
        .expect("deferred");
    tx
}

#[tokio::test]
async fn commit_failure_undoes_the_vault() {
    let (_dir, db) = open().await;
    let mut vault = FakeVault {
        note: "n".into(),
        ..Default::default()
    };

    let tx = failing_commit(&db).await;
    let err = dual_write::commit_with_vault(tx, &mut vault)
        .await
        .expect_err("must fail");

    assert!(matches!(err, DualWriteError::Commit(_)), "{err}");
    assert_eq!(count(&db, "tarefa").await, 0);
    assert!(vault.notes.lock().unwrap().is_empty(), "vault side undone");
}

#[tokio::test]
async fn commit_and_undo_failure_is_reported() {
    let (_dir, db) = open().await;
    let mut vault = FakeVault {
        note: "n".into(),
        fail_undo: true,
        ..Default::default()
    };

    let tx = failing_commit(&db).await;
    let err = dual_write::commit_with_vault(tx, &mut vault)
        .await
        .expect_err("must fail");

    assert!(matches!(err, DualWriteError::UndoFailed { .. }), "{err}");
    assert_eq!(count(&db, "tarefa").await, 0);
    assert_eq!(
        *vault.notes.lock().unwrap(),
        ["n"],
        "divergence the caller must log"
    );
}

#[test]
fn catalog_entry() {
    assert_eq!(DualWriteError::ID, "ERR-51");
    assert_eq!(DualWriteError::CODE, "GRAVACAO_DIVERGENTE");
    assert_eq!(
        DualWriteError::MESSAGE,
        "Não deu para gravar no banco e no cofre ao mesmo tempo; nada foi gravado."
    );
}
