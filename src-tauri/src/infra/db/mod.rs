//! SQLite storage: one database file in the app data directory, WAL mode,
//! versioned migrations (`src-tauri/migrations/`) applied on every open.
//!
//! `0002_contract_entities.sql` holds one table per contract entity that lives in SQLite
//! (`Anexos/contrato/02-entidades.md`) and `0003_contract_links_and_history.sql` brings it to
//! M-37 to M-39 (required links, `incompleta`, `concluído`, `comandos`) and refuses `OR REPLACE`
//! on prompt versions and runs; [`dual_write`] writes the ones that also live in the vault.
//!
//! Two rules for databases made by earlier builds (no version is published yet):
//! * a database from before `0003` that the migrations cannot bring up to date is deleted and
//!   created again (decision `geral-decisao-2026-11-23-banco-antigo-apagado`);
//! * a database that applied `0003` before its comment was corrected keeps working: sqlx refuses a
//!   migration whose text changed since it ran, so the stored checksum of that first text is
//!   replaced by the current one before migrating. Only that exact old checksum is replaced; any
//!   other mismatch still fails.

pub mod dual_write;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

/// File name of the database inside the app data directory.
pub const DB_FILE_NAME: &str = "my-harness.sqlite3";

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("could not create data directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not open database: {0}")]
    Open(#[from] sqlx::Error),
    #[error("could not run migrations: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("could not delete old database {path}: {source}")]
    Delete {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Version of `0003_contract_links_and_history.sql`.
const MIGRATION_0003: i64 = 3;

/// SHA-384 (as SQLite `hex()`) of `0003_contract_links_and_history.sql` as first committed
/// (`ff1343f`), before its comment on the `comandos` filter was corrected without changing the SQL.
const MIGRATION_0003_FIRST_CHECKSUM: &str = "A5F352C69001C6813FA36252C642E3827134A0C7581035D03D6A24816B4E51FEFE3EB51D53B74DC22147CD2DB0EB081B";

/// Handle to the app database, managed as Tauri state.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    path: PathBuf,
}

impl Database {
    /// Opens (or creates) the database in `data_dir`, enables WAL and runs pending migrations.
    ///
    /// A database from before `0003` whose migration fails is deleted and created again.
    pub async fn open(data_dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir).map_err(|source| DbError::CreateDir {
            path: data_dir.to_path_buf(),
            source,
        })?;
        let path = data_dir.join(DB_FILE_NAME);

        let pool = connect(&path).await?;
        let applied = last_applied_version(&pool).await?;
        accept_first_checksum_of_0003(&pool).await?;

        match MIGRATOR.run(&pool).await {
            Ok(()) => Ok(Self { pool, path }),
            Err(_) if applied.is_some_and(|v| v < MIGRATION_0003) => {
                pool.close().await;
                delete_database_files(&path)?;
                let pool = connect(&path).await?;
                MIGRATOR.run(&pool).await?;
                Ok(Self { pool, path })
            }
            Err(err) => {
                pool.close().await;
                Err(err.into())
            }
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

async fn connect(path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
}

/// Highest migration applied with success, or `None` for a database without migrations.
async fn last_applied_version(pool: &SqlitePool) -> Result<Option<i64>, sqlx::Error> {
    if !has_migrations_table(pool).await? {
        return Ok(None);
    }
    let (version,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(pool)
            .await?;
    Ok(version)
}

async fn has_migrations_table(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Replaces the checksum of `0003` as first committed by the checksum of its current text.
async fn accept_first_checksum_of_0003(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if !has_migrations_table(pool).await? {
        return Ok(());
    }
    let Some(current) = MIGRATOR
        .iter()
        .find(|m| m.version == MIGRATION_0003 && !m.migration_type.is_down_migration())
    else {
        return Ok(());
    };
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ? AND hex(checksum) = ?")
        .bind(current.checksum.as_ref())
        .bind(MIGRATION_0003)
        .bind(MIGRATION_0003_FIRST_CHECKSUM)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes the database file and its WAL companions; missing files are fine.
fn delete_database_files(path: &Path) -> Result<(), DbError> {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_owned();
    shm.push("-shm");
    for file in [path.to_path_buf(), PathBuf::from(wal), PathBuf::from(shm)] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(DbError::Delete { path: file, source }),
        }
    }
    Ok(())
}
