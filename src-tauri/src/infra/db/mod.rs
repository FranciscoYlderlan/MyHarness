//! SQLite storage: one database file in the app data directory, WAL mode,
//! versioned migrations (`src-tauri/migrations/`) applied on every open.

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
}

/// Handle to the app database, managed as Tauri state.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    path: PathBuf,
}

impl Database {
    /// Opens (or creates) the database in `data_dir`, enables WAL and runs pending migrations.
    pub async fn open(data_dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir).map_err(|source| DbError::CreateDir {
            path: data_dir.to_path_buf(),
            source,
        })?;
        let path = data_dir.join(DB_FILE_NAME);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;

        MIGRATOR.run(&pool).await?;

        Ok(Self { pool, path })
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
