use my_harness_lib::infra::db::{DB_FILE_NAME, Database};

#[tokio::test]
async fn open_creates_wal_database_and_runs_migrations() {
    let dir = tempfile::tempdir().expect("temp dir");

    let db = Database::open(dir.path()).await.expect("open database");
    assert_eq!(db.path(), dir.path().join(DB_FILE_NAME));
    assert!(db.path().exists());

    let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(db.pool())
        .await
        .expect("journal_mode");
    assert_eq!(mode.to_lowercase(), "wal");

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schema_info")
        .fetch_one(db.pool())
        .await
        .expect("schema_info exists");
    assert_eq!(count, 1);

    db.close().await;

    // Reopening is idempotent: migrations already applied are skipped.
    let again = Database::open(dir.path()).await.expect("reopen database");
    again.close().await;
}
