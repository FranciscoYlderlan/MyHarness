//! Tests of `0003_contract_links_and_history.sql` (M-37 to M-39, RN-17, RF-31, RN-28) and of the
//! ERR-54 path of the dual write (M-40, AC-155).

use std::path::Path;
use std::sync::{Arc, Mutex};

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, SqlitePool};

use super::Database;
use super::dual_write::{self, DualWriteError, LogTarget, VaultError, VaultSide};

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
}

impl Fixture {
    fn pool(&self) -> &SqlitePool {
        self.db.pool()
    }
}

async fn open() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Database::open(dir.path()).await.expect("open database");
    Fixture { _dir: dir, db }
}

/// projeto 1, cofre 1 (memoria), cofre 2 (consulta), chat 1 (ativo), chat 2 (encerrado),
/// plano 1 (proposto), plano 2 (aprovado), tarefa 1 (plano 1), execucao 1 (funcionalidade),
/// 2 (adversarial) and 3 (orquestrador), prompt_papel teste v1 (vigente).
async fn seeded() -> Fixture {
    let f = open().await;
    for sql in [
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
        "INSERT INTO cofre (id, projeto, nome, funcao, ativo, status) VALUES (1, 1, 'myHarness-vault', 'memoria', 1, 'conectado')",
        "INSERT INTO cofre (id, projeto, nome, funcao, ativo, status) VALUES (2, 1, 'Vault', 'consulta', 1, 'conectado')",
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (1, 1, 100000, 'ativo')",
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (2, 1, 100000, 'encerrado')",
        "INSERT INTO plano_delegacao (id, chat, versao, status) VALUES (1, 1, 1, 'proposto')",
        "INSERT INTO plano_delegacao (id, chat, versao, status, aprovado_em) VALUES (2, 1, 2, 'aprovado', '2026-11-20T09:31:00Z')",
        "INSERT INTO tarefa (id, plano, faz, toca_em, paralelo, pronto_quando, prova, papel) VALUES (1, 1, 'Esquema', '[\"crates/app/migrations/\"]', 1, 'testes passam', 'cargo test', 'funcionalidade')",
        "INSERT INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, worktree, branch, status) VALUES (1, 1, 1, 'funcionalidade', 'opus', 1, '/w/t4', 'feat/RF-01-esquema', 'rodando')",
        "INSERT INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, status) VALUES (2, 1, 1, 'adversarial', 'opus', 1, 'rodando')",
        "INSERT INTO execucao (id, chat, papel, modelo, prompt_versao, status) VALUES (3, 1, 'orquestrador', 'opus', 1, 'rodando')",
        "INSERT INTO prompt_papel (id, fluxo, texto, versao, palavras, tokens, vigente) VALUES (1, 'teste', 'Você testa.', 1, 2, 4, 1)",
    ] {
        ok(f.pool(), sql).await;
    }
    f
}

// Test-only SQL built from literals in this file.
async fn ok(pool: &SqlitePool, sql: &str) {
    sqlx::query(AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("should accept: {sql}\n{e}"));
}

async fn rejects(pool: &SqlitePool, sql: &str, reason: &str) {
    let err = sqlx::query(AssertSqlSafe(sql))
        .execute(pool)
        .await
        .err()
        .unwrap_or_else(|| panic!("should reject ({reason}): {sql}"));
    assert!(
        err.to_string().contains(reason),
        "rejected for another reason, expected `{reason}`: {sql}\n{err}"
    );
}

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}\n{e}"));
    n
}

async fn text(pool: &SqlitePool, sql: &str) -> String {
    let (t,): (String,) = sqlx::query_as(AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}\n{e}"));
    t
}

// ---------------------------------------------------------------------------------------------
// RF-31 and RN-28: no conflict clause replaces a prompt version or a run.

#[tokio::test]
async fn insert_or_replace_cannot_overwrite_a_run() {
    let f = seeded().await;
    let p = f.pool();
    let replace = "INSERT OR REPLACE INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, worktree, branch, status) \
                   VALUES (1, 1, 1, 'funcionalidade', 'opus', 2, '/w/t4', 'feat/RF-01-esquema', 'rodando')";
    rejects(p, replace, "execucao_sem_substituicao").await;
    rejects(
        p,
        &replace.replace("INSERT OR REPLACE", "REPLACE"),
        "execucao_sem_substituicao",
    )
    .await;
    rejects(
        p,
        "INSERT INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, status) VALUES (1, 1, 1, 'teste', 'opus', 2, 'rodando') \
         ON CONFLICT (id) DO UPDATE SET prompt_versao = excluded.prompt_versao",
        "execucao_sem_substituicao",
    )
    .await;
    // UPDATE OR REPLACE of the key would delete run 2 without any DELETE trigger.
    rejects(
        p,
        "UPDATE OR REPLACE execucao SET id = 2 WHERE id = 3",
        "execucao_sem_substituicao",
    )
    .await;
    rejects(
        p,
        "UPDATE OR REPLACE execucao SET prompt_versao = 2 WHERE id = 1",
        "execucao_prompt_versao_imutavel",
    )
    .await;
    assert_eq!(
        count(p, "SELECT prompt_versao FROM execucao WHERE id = 1").await,
        1,
        "prompt_versao kept (RN-28)"
    );
    assert_eq!(count(p, "SELECT COUNT(*) FROM execucao").await, 3);
    // A new run with a new id is still a plain insert.
    ok(p, "INSERT OR REPLACE INTO execucao (id, chat, papel, modelo, prompt_versao, status) VALUES (9, 1, 'orquestrador', 'opus', 1, 'rodando')").await;
}

#[tokio::test]
async fn insert_or_replace_cannot_overwrite_a_prompt_version() {
    let f = seeded().await;
    let p = f.pool();
    let cols = "id, fluxo, texto, versao, palavras, tokens, vigente";
    // Same id.
    rejects(
        p,
        &format!("INSERT OR REPLACE INTO prompt_papel ({cols}) VALUES (1, 'teste', 'Trocado.', 1, 1, 2, 0)"),
        "prompt_papel_versao_imutavel",
    )
    .await;
    // Same (fluxo, versao), new id.
    rejects(
        p,
        &format!("INSERT OR REPLACE INTO prompt_papel ({cols}) VALUES (5, 'teste', 'Trocado.', 1, 1, 2, 0)"),
        "prompt_papel_versao_imutavel",
    )
    .await;
    // New version marked current while v1 is current: the partial unique index would delete v1.
    rejects(
        p,
        &format!(
            "INSERT OR REPLACE INTO prompt_papel ({cols}) VALUES (6, 'teste', 'Nova.', 2, 1, 2, 1)"
        ),
        "prompt_papel_uma_vigente_por_fluxo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO prompt_papel (fluxo, texto, versao, palavras, tokens, vigente) VALUES ('teste', 'Trocado.', 1, 1, 2, 0) \
         ON CONFLICT DO UPDATE SET texto = excluded.texto",
        "prompt_papel_versao_imutavel",
    )
    .await;

    // The proper way: v2 not current, then switch.
    ok(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES (2, 'teste', 'Nova.', 2, 1, 2, 0)"),
    )
    .await;
    // UPDATE OR REPLACE vigente = 1 on v2 would delete the current v1.
    rejects(
        p,
        "UPDATE OR REPLACE prompt_papel SET vigente = 1 WHERE id = 2",
        "prompt_papel_uma_vigente_por_fluxo",
    )
    .await;
    // UPDATE OR REPLACE of the id would delete v1.
    rejects(
        p,
        "UPDATE OR REPLACE prompt_papel SET id = 1 WHERE id = 2",
        "prompt_papel_versao_imutavel",
    )
    .await;
    ok(p, "UPDATE prompt_papel SET vigente = 0 WHERE id = 1").await;
    ok(p, "UPDATE prompt_papel SET vigente = 1 WHERE id = 2").await;
    ok(p, "UPDATE prompt_papel SET vigente = 1 WHERE id = 2").await;

    assert_eq!(
        text(p, "SELECT texto FROM prompt_papel WHERE id = 1").await,
        "Você testa.",
        "history kept (RF-31)"
    );
    assert_eq!(count(p, "SELECT COUNT(*) FROM prompt_papel").await, 2);
}

// ---------------------------------------------------------------------------------------------
// ENT-03: the CHECK `chat_chat_anterior_proprio` itself, not the trigger before it.

#[tokio::test]
async fn chat_cannot_follow_itself_check() {
    let f = seeded().await;
    let p = f.pool();
    // Chat 2 is encerrado, so `chat_chat_anterior_encerrado_update` lets it through and the
    // CHECK is the constraint that refuses it.
    let err = sqlx::query("UPDATE chat SET chat_anterior = 2 WHERE id = 2")
        .execute(p)
        .await
        .expect_err("a chat cannot follow itself");
    let msg = err.to_string();
    assert!(
        msg.contains("CHECK constraint failed: chat_chat_anterior_proprio"),
        "{msg}"
    );
    assert!(!msg.contains("chat_chat_anterior_encerrado"), "{msg}");
    // The same row may point to another closed chat.
    ok(
        p,
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (3, 1, 1, 'encerrado')",
    )
    .await;
    ok(p, "UPDATE chat SET chat_anterior = 3 WHERE id = 2").await;
}

// ---------------------------------------------------------------------------------------------
// ENT-07 comandos (RN-17, ERR-14).

#[tokio::test]
async fn execucao_comandos() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "UPDATE execucao SET comandos = '[\"git status\", \"git commit -m \\\"push merge docs\\\"\", \"git merge-base main HEAD\", \"cargo test\"]' WHERE id = 1").await;
    ok(p, "UPDATE execucao SET comandos = '[]' WHERE id = 1").await;
    ok(p, "UPDATE execucao SET comandos = NULL WHERE id = 1").await;
    rejects(
        p,
        "UPDATE execucao SET comandos = 'git status' WHERE id = 1",
        "execucao_comandos_lista",
    )
    .await;
    rejects(
        p,
        "UPDATE execucao SET comandos = '{\"a\": 1}' WHERE id = 1",
        "execucao_comandos_lista",
    )
    .await;
    rejects(
        p,
        "UPDATE execucao SET comandos = '[\"ok\", \" \"]' WHERE id = 1",
        "execucao_comandos_texto",
    )
    .await;
    rejects(
        p,
        "UPDATE execucao SET comandos = '[1]' WHERE id = 1",
        "execucao_comandos_texto",
    )
    .await;
    for cmd in [
        "git push",
        "git push origin feat/x",
        "git push --force",
        "git push -f origin main",
        "git merge main",
        "GIT PUSH",
        "git  push",
        "git\\tpush",
        "cd /w && git push",
        "/usr/bin/git push",
        "C:\\\\Program Files\\\\Git\\\\cmd\\\\git.exe push",
        "gh pr merge 3",
        "git status; git merge feat/x",
    ] {
        rejects(
            p,
            &format!("UPDATE execucao SET comandos = '[\"git status\", \"{cmd}\"]' WHERE id = 1"),
            "execucao_comandos_git_negado",
        )
        .await;
    }
    rejects(
        p,
        "INSERT INTO execucao (chat, papel, modelo, prompt_versao, status, comandos) VALUES (1, 'orquestrador', 'opus', 1, 'rodando', '[\"git push\"]')",
        "execucao_comandos_git_negado",
    )
    .await;
}

// ---------------------------------------------------------------------------------------------
// M-37: required links.

#[tokio::test]
async fn required_links() {
    let f = seeded().await;
    let p = f.pool();
    let fk = "FOREIGN KEY constraint failed";

    // Tarefa.plano
    rejects(p, "INSERT INTO tarefa (faz, paralelo, pronto_quando, prova, papel) VALUES ('f', 0, 'x', 'y', 'teste')", "NOT NULL constraint failed: tarefa.plano").await;
    rejects(p, "INSERT INTO tarefa (plano, faz, paralelo, pronto_quando, prova, papel) VALUES (9, 'f', 0, 'x', 'y', 'teste')", fk).await;
    rejects(p, "UPDATE tarefa SET plano = 9 WHERE id = 1", fk).await;

    // Relatorio.execucao
    rejects(
        p,
        "INSERT INTO relatorio (resultado, o_que_fez, prova) VALUES ('falhou', 'f', 'p')",
        "NOT NULL constraint failed: relatorio.execucao",
    )
    .await;
    rejects(p, "INSERT INTO relatorio (execucao, resultado, o_que_fez, prova) VALUES (9, 'falhou', 'f', 'p')", fk).await;
    ok(p, "INSERT INTO relatorio (id, execucao, resultado, o_que_fez, prova) VALUES (1, 1, 'concluído', 'f', 'p')").await;
    ok(
        p,
        "INSERT INTO criterio (relatorio, status, prova) VALUES (1, '✔', 'cargo test')",
    )
    .await;

    // Achado.execucao, and captura required for an adversarial run.
    let cols = "tipo, passos, esperado, obtido, decisao, status";
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES ('bug', '[\"a\"]', 'e', 'o', 'corrigir', 'aberto')"
        ),
        "NOT NULL constraint failed: achado.execucao",
    )
    .await;
    rejects(p, &format!("INSERT INTO achado (execucao, {cols}) VALUES (9, 'bug', '[\"a\"]', 'e', 'o', 'corrigir', 'aberto')"), fk).await;
    ok(p, &format!("INSERT INTO achado (id, execucao, {cols}) VALUES (1, 1, 'bug', '[\"a\"]', 'e', 'o', 'corrigir', 'aberto')")).await;
    rejects(p, &format!("INSERT INTO achado (execucao, {cols}) VALUES (2, 'bug', '[\"a\"]', 'e', 'o', 'corrigir', 'aberto')"), "achado_captura_adversarial").await;
    ok(p, &format!("INSERT INTO achado (id, execucao, captura, {cols}) VALUES (2, 2, 'Anexos/c.png', 'bug', '[\"a\"]', 'e', 'o', 'corrigir', 'aberto')")).await;
    rejects(
        p,
        "UPDATE achado SET captura = NULL WHERE id = 2",
        "achado_captura_adversarial",
    )
    .await;
    rejects(
        p,
        "UPDATE achado SET execucao = 2 WHERE id = 1",
        "achado_captura_adversarial",
    )
    .await;

    // Escrita.cofre, only the memory vault (ERR-39).
    let hora = "'2026-11-20T09:31:00Z'";
    rejects(
        p,
        &format!("INSERT INTO escrita (caminho, motivo, hora) VALUES ('d.md', 'm', {hora})"),
        "NOT NULL constraint failed: escrita.cofre",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (9, 'd.md', 'm', {hora})"
        ),
        fk,
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (2, 'd.md', 'm', {hora})"
        ),
        "escrita_cofre_memoria",
    )
    .await;
    ok(p, &format!("INSERT INTO escrita (id, cofre, caminho, motivo, hora) VALUES (1, 1, 'd.md', 'm', {hora})")).await;
    rejects(
        p,
        "UPDATE escrita SET cofre = 2 WHERE id = 1",
        "escrita_cofre_memoria",
    )
    .await;

    // Parents in use cannot go away.
    rejects(p, "DELETE FROM plano_delegacao WHERE id = 1", fk).await;
    rejects(p, "DELETE FROM execucao WHERE id = 2", fk).await;
    rejects(p, "DELETE FROM cofre WHERE id = 1", fk).await;
}

// ---------------------------------------------------------------------------------------------
// M-38: execucao.status `concluído`.

#[tokio::test]
async fn execucao_concluido_with_accent() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "UPDATE execucao SET status = 'concluído' WHERE id = 1").await;
    rejects(
        p,
        "UPDATE execucao SET status = 'concluido' WHERE id = 1",
        "execucao_status_valores",
    )
    .await;
}

// ---------------------------------------------------------------------------------------------
// M-39: tarefa.incompleta (ERR-49).

#[tokio::test]
async fn tarefa_incompleta() {
    let f = seeded().await;
    let p = f.pool();
    assert_eq!(
        count(p, "SELECT incompleta FROM tarefa WHERE id = 1").await,
        0,
        "default não"
    );

    let faz_501 = "f".repeat(501);
    ok(
        p,
        "INSERT INTO tarefa (id, plano, incompleta, paralelo, papel) VALUES (2, 1, 1, 0, 'teste')",
    )
    .await;
    ok(p, &format!("INSERT INTO tarefa (id, plano, incompleta, faz, pronto_quando, prova, paralelo, papel) VALUES (3, 1, 1, '{faz_501}', ' ', '', 0, 'pesquisa')")).await;
    rejects(
        p,
        "INSERT INTO tarefa (plano, incompleta, paralelo, papel) VALUES (1, 2, 0, 'teste')",
        "tarefa_incompleta_booleano",
    )
    .await;

    // With incompleta = 0 the ERR-49 rules hold again.
    rejects(
        p,
        "UPDATE tarefa SET incompleta = 0 WHERE id = 2",
        "tarefa_faz_tamanho",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET incompleta = 0, faz = 'f', pronto_quando = 'x' WHERE id = 2",
        "tarefa_prova_vazio",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET incompleta = 0, faz = 'f', prova = 'y' WHERE id = 2",
        "tarefa_pronto_quando_vazio",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET incompleta = 0, pronto_quando = 'x', prova = 'y' WHERE id = 3",
        "tarefa_faz_tamanho",
    )
    .await;
    rejects(p, "INSERT INTO tarefa (plano, faz, paralelo, pronto_quando, papel) VALUES (1, 'f', 0, 'x', 'teste')", "tarefa_prova_vazio").await;
    // toca_em is still required for funcionalidade.
    rejects(p, "INSERT INTO tarefa (plano, incompleta, paralelo, papel) VALUES (1, 1, 0, 'funcionalidade')", "tarefa_toca_em_funcionalidade").await;

    // "Aprovar" blocked while the plan has an incomplete task.
    rejects(
        p,
        "UPDATE plano_delegacao SET status = 'aprovado', aprovado_em = '2026-11-20T09:31:00Z' WHERE id = 1",
        "plano_delegacao_aprovado_sem_incompleta",
    )
    .await;
    // An approved plan never gets an incomplete task.
    rejects(
        p,
        "INSERT INTO tarefa (plano, incompleta, paralelo, papel) VALUES (2, 1, 0, 'teste')",
        "plano_delegacao_aprovado_sem_incompleta",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET plano = 2 WHERE id = 2",
        "plano_delegacao_aprovado_sem_incompleta",
    )
    .await;
    ok(p, "INSERT INTO tarefa (id, plano, faz, paralelo, pronto_quando, prova, papel) VALUES (4, 2, 'f', 0, 'x', 'y', 'teste')").await;
    rejects(
        p,
        "UPDATE tarefa SET incompleta = 1 WHERE id = 4",
        "plano_delegacao_aprovado_sem_incompleta",
    )
    .await;

    // Once filled in, the plan can be approved.
    ok(p, "UPDATE tarefa SET incompleta = 0, faz = 'f', pronto_quando = 'x', prova = 'y' WHERE id = 2").await;
    ok(p, "UPDATE tarefa SET incompleta = 0, faz = 'f', pronto_quando = 'x', prova = 'y' WHERE id = 3").await;
    ok(p, "UPDATE plano_delegacao SET status = 'aprovado', aprovado_em = '2026-11-20T09:31:00Z' WHERE id = 1").await;
}

// ---------------------------------------------------------------------------------------------
// Migration 0003 on a database created by 0001 and 0002.

/// Database with only 0001 and 0002 applied, as a build of the `main` before this change left it.
async fn old_database(dir: &Path) -> SqlitePool {
    let migrations = tempfile::tempdir().expect("migrations dir");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for name in ["0001_schema_info.sql", "0002_contract_entities.sql"] {
        std::fs::copy(src.join(name), migrations.path().join(name)).expect("copy migration");
    }
    let options = SqliteConnectOptions::new()
        .filename(dir.join(super::DB_FILE_NAME))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("old pool");
    Migrator::new(migrations.path())
        .await
        .expect("old migrator")
        .run(&pool)
        .await
        .expect("0001 and 0002");
    pool
}

#[tokio::test]
async fn migration_0003_upgrades_a_0002_database() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = old_database(dir.path()).await;
    for sql in [
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
        "INSERT INTO cofre (id, projeto, nome, funcao, ativo, status) VALUES (1, 1, 'v', 'memoria', 1, 'conectado')",
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (1, 1, 100000, 'ativo')",
        "INSERT INTO execucao (id, chat, papel, modelo, prompt_versao, status) VALUES (1, 1, 'orquestrador', 'opus', 1, 'concluido')",
        "INSERT INTO log (execucao, comando, inicio) VALUES (1, 'ls', '2026-11-20T09:31:00Z')",
    ] {
        ok(&old, sql).await;
    }
    old.close().await;

    let db = Database::open(dir.path()).await.expect("0003 applied");
    let p = db.pool();
    assert_eq!(
        text(p, "SELECT status FROM execucao WHERE id = 1").await,
        "concluído",
        "M-38 carried to existing rows"
    );
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM log").await,
        1,
        "children kept"
    );
    assert_eq!(
        count(p, "SELECT MAX(version) FROM _sqlx_migrations").await,
        3
    );
    for (table, column) in [
        ("tarefa", "plano"),
        ("tarefa", "incompleta"),
        ("execucao", "comandos"),
        ("relatorio", "execucao"),
        ("achado", "execucao"),
        ("escrita", "cofre"),
    ] {
        assert_eq!(
            count(
                p,
                &format!(
                    "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
                )
            )
            .await,
            1,
            "{table}.{column}"
        );
    }
    for table in ["tarefa", "execucao", "relatorio", "achado", "escrita"] {
        assert_eq!(
            count(
                p,
                &format!("SELECT strict FROM pragma_table_list WHERE name = '{table}'")
            )
            .await,
            1,
            "{table} is STRICT"
        );
    }
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM pragma_foreign_key_check").await,
        0
    );

    // Foreign keys are back on in every pooled connection, including the one that migrated.
    let mut conns = Vec::new();
    for _ in 0..4 {
        conns.push(p.acquire().await.expect("connection"));
    }
    for conn in &mut conns {
        let (on,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&mut **conn)
            .await
            .expect("pragma");
        assert_eq!(on, 1);
    }
    drop(conns);
    db.close().await;

    // Reopening skips the applied migrations.
    Database::open(dir.path())
        .await
        .expect("reopen")
        .close()
        .await;
}

#[tokio::test]
async fn migration_0003_stops_without_changes_when_a_task_has_no_plan() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = old_database(dir.path()).await;
    ok(&old, "INSERT INTO tarefa (id, faz, paralelo, pronto_quando, prova, papel) VALUES (1, 'f', 0, 'x', 'y', 'teste')").await;

    let err = super::MIGRATOR
        .run(&old)
        .await
        .expect_err("tarefa without plano");
    assert!(err.to_string().contains("tarefa_nova.plano"), "{err}");
    old.close().await;

    let old = old_database(dir.path()).await;
    assert_eq!(
        count(&old, "SELECT MAX(version) FROM _sqlx_migrations").await,
        2
    );
    assert_eq!(
        count(&old, "SELECT COUNT(*) FROM tarefa").await,
        1,
        "row kept"
    );
    assert_eq!(
        count(
            &old,
            "SELECT COUNT(*) FROM pragma_table_info('tarefa') WHERE name = 'plano'"
        )
        .await,
        0,
        "old table kept"
    );
    assert_eq!(
        count(
            &old,
            "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE '%_nova' OR name LIKE '%_novo'"
        )
        .await,
        0
    );
}

// ---------------------------------------------------------------------------------------------
// Old databases on open: before 0003 and unable to migrate (deleted and created again), and
// already on 0003 as first committed (checksum accepted).

#[tokio::test]
async fn open_deletes_and_recreates_a_0002_database_that_cannot_migrate() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = old_database(dir.path()).await;
    for sql in [
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
        "INSERT INTO tarefa (id, faz, paralelo, pronto_quando, prova, papel) VALUES (1, 'f', 0, 'x', 'y', 'teste')",
    ] {
        ok(&old, sql).await;
    }
    old.close().await;

    let db = Database::open(dir.path())
        .await
        .expect("old database deleted and created again");
    let p = db.pool();
    assert_eq!(
        count(
            p,
            "SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1"
        )
        .await,
        3
    );
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM projeto").await,
        0,
        "new database"
    );
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM tarefa").await,
        0,
        "new database"
    );
    assert_eq!(
        count(
            p,
            "SELECT COUNT(*) FROM pragma_table_info('tarefa') WHERE name = 'plano'"
        )
        .await,
        1,
        "0003 applied"
    );
    let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(p)
        .await
        .expect("journal_mode");
    assert_eq!(mode.to_lowercase(), "wal");
    ok(p, "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')").await;
    db.close().await;

    let db = Database::open(dir.path()).await.expect("reopen");
    assert_eq!(
        count(db.pool(), "SELECT COUNT(*) FROM projeto").await,
        1,
        "the new database is kept on the next open"
    );
    db.close().await;
}

/// Comment on the `comandos` filter in `0003` as first committed (`ff1343f`).
const FIRST_FILTER_COMMENT: &str = "\
-- RN-17 / ERR-14: comandos lists the commands the agent ran (the core denies push, merge and
-- push --force before they run): non-blank command lines, none of them a git push or merge
-- (`push --force` included) nor `gh pr merge`. Command lines are compared in lower case, with
-- tabs, line breaks and repeated spaces as one space, and the program allowed with a path or `.exe`.
-- Only the subcommand right after the program is matched (`git -C x push` is not): a commit message
-- with the word \"push\" or \"merge\" must still be recorded. The core gate is the real barrier (RN-27).
";

const CURRENT_FILTER_COMMENT: &str = "\
-- RN-17 / ERR-14: comandos lists the commands the agent ran, as non-blank command lines. The push
-- and merge check below is best effort, not a barrier: each line is compared in lower case, with
-- tabs and line breaks as spaces and runs of up to 8 spaces as one, and only `git push`,
-- `git merge` (`push --force` included) and `gh pr merge` written with the subcommand right after
-- the program (a path or `.exe` allowed) are refused. Forms such as `git -C x push`,
-- `git --no-pager push`, `\"git\" push`, `cd x&&git push` or 9 or more spaces before `push` get
-- through, and a commit message with the word \"push\" or \"merge\" must still be recorded. The real
-- barrier is the core gate, which denies push, merge and push --force before they run (RN-27).
";

const FIRST_HEADER_END: &str = "\
-- holds tarefa, relatorio, achado or escrita rows stops here with a NOT NULL error and nothing changes.
";

const CURRENT_HEADER_END: &str = "\
-- holds tarefa, relatorio, achado or escrita rows stops here with a NOT NULL error and nothing changes;
-- `Database::open` then deletes that database and creates it again (no version is published yet).
";

/// Text of `0003` as first committed: only comments differ from the current file.
fn first_text_of_0003(current: &str) -> String {
    for (part, name) in [
        (CURRENT_FILTER_COMMENT, "filter comment"),
        (CURRENT_HEADER_END, "header"),
    ] {
        assert_eq!(
            current.matches(part).count(),
            1,
            "{name} in the current 0003"
        );
    }
    let first = current
        .replace(CURRENT_FILTER_COMMENT, FIRST_FILTER_COMMENT)
        .replace(CURRENT_HEADER_END, FIRST_HEADER_END);
    let code = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(code(&first), code(current), "only comments changed in 0003");
    first
}

/// Database with 0001, 0002 and 0003 as first committed, as a build of `ff1343f` left it.
async fn database_on_first_0003(dir: &Path) -> SqlitePool {
    let migrations = tempfile::tempdir().expect("migrations dir");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for name in ["0001_schema_info.sql", "0002_contract_entities.sql"] {
        std::fs::copy(src.join(name), migrations.path().join(name)).expect("copy migration");
    }
    let name = "0003_contract_links_and_history.sql";
    let current = std::fs::read_to_string(src.join(name)).expect("read 0003");
    std::fs::write(migrations.path().join(name), first_text_of_0003(&current))
        .expect("write first 0003");
    let options = SqliteConnectOptions::new()
        .filename(dir.join(super::DB_FILE_NAME))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("old pool");
    Migrator::new(migrations.path())
        .await
        .expect("old migrator")
        .run(&pool)
        .await
        .expect("0001 to first 0003");
    pool
}

fn current_checksum_of_0003() -> String {
    let m = super::MIGRATOR
        .iter()
        .find(|m| m.version == 3 && !m.migration_type.is_down_migration())
        .expect("0003");
    m.checksum.iter().map(|b| format!("{b:02X}")).collect()
}

#[tokio::test]
async fn open_accepts_a_database_on_0003_as_first_committed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = database_on_first_0003(dir.path()).await;
    assert_eq!(
        text(
            &old,
            "SELECT hex(checksum) FROM _sqlx_migrations WHERE version = 3"
        )
        .await,
        super::MIGRATION_0003_FIRST_CHECKSUM,
        "the constant is the checksum sqlx stored for the first 0003"
    );
    assert_ne!(
        super::MIGRATION_0003_FIRST_CHECKSUM,
        current_checksum_of_0003()
    );
    ok(
        &old,
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
    )
    .await;
    old.close().await;

    let db = Database::open(dir.path())
        .await
        .expect("no checksum error on a database already on 0003");
    let p = db.pool();
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM projeto").await,
        1,
        "data kept"
    );
    assert_eq!(
        text(
            p,
            "SELECT hex(checksum) FROM _sqlx_migrations WHERE version = 3"
        )
        .await,
        current_checksum_of_0003()
    );
    db.close().await;

    let db = Database::open(dir.path()).await.expect("reopen");
    assert_eq!(count(db.pool(), "SELECT COUNT(*) FROM projeto").await, 1);
    db.close().await;
}

#[tokio::test]
async fn open_still_refuses_any_other_checksum_of_0003_and_keeps_the_database() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Database::open(dir.path()).await.expect("open");
    ok(
        db.pool(),
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
    )
    .await;
    ok(
        db.pool(),
        "UPDATE _sqlx_migrations SET checksum = x'00' WHERE version = 3",
    )
    .await;
    db.close().await;

    let err = Database::open(dir.path())
        .await
        .expect_err("unknown checksum of 0003");
    assert!(matches!(err, super::DbError::Migrate(_)), "{err}");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(dir.path().join(super::DB_FILE_NAME)))
        .await
        .expect("raw pool");
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM projeto").await,
        1,
        "not deleted"
    );
    pool.close().await;
}

/// A 0002 database whose stored checksum of 0001 or 0002 is wrong is not deleted: only `0003`
/// stopping on NOT NULL deletes (decision `geral-decisao-2026-11-23-banco-antigo-apagado`).
#[tokio::test]
async fn open_keeps_a_0002_database_with_a_wrong_checksum_of_0001_or_0002() {
    for version in [1_i64, 2] {
        let dir = tempfile::tempdir().expect("temp dir");
        let old = old_database(dir.path()).await;
        for sql in [
            "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
            "INSERT INTO tarefa (id, faz, paralelo, pronto_quando, prova, papel) VALUES (1, 'f', 0, 'x', 'y', 'teste')",
        ] {
            ok(&old, sql).await;
        }
        ok(
            &old,
            &format!("UPDATE _sqlx_migrations SET checksum = x'00' WHERE version = {version}"),
        )
        .await;
        old.close().await;

        let err = Database::open(dir.path())
            .await
            .expect_err("wrong checksum is an error");
        assert!(
            matches!(
                err,
                super::DbError::Migrate(sqlx::migrate::MigrateError::VersionMismatch(v)) if v == version
            ),
            "migration {version}: {err}"
        );

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new().filename(dir.path().join(super::DB_FILE_NAME)),
            )
            .await
            .expect("raw pool");
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM projeto").await,
            1,
            "data kept ({version})"
        );
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM tarefa").await,
            1,
            "data kept ({version})"
        );
        assert_eq!(
            count(&pool, "SELECT MAX(version) FROM _sqlx_migrations").await,
            2,
            "still on 0002 ({version})"
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM _sqlx_migrations WHERE checksum = x'00'"
            )
            .await,
            1,
            "tampered checksum untouched ({version})"
        );
        pool.close().await;
    }
}

/// A database file that cannot be removed comes back as `DbError::Delete` with its path.
#[test]
fn delete_failure_is_db_error_delete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(super::DB_FILE_NAME);
    std::fs::write(&path, b"old").expect("db file");
    // A non-empty directory where the WAL file goes: `remove_file` fails on it.
    let wal = dir.path().join(format!("{}-wal", super::DB_FILE_NAME));
    std::fs::create_dir(&wal).expect("wal dir");
    std::fs::write(wal.join("x"), b"x").expect("wal dir content");

    let err = super::delete_database_files(&path).expect_err("cannot remove a directory");
    match err {
        super::DbError::Delete { path: failed, .. } => assert_eq!(failed, wal),
        other => panic!("expected DbError::Delete, got {other}"),
    }
    assert!(!path.exists(), "the database file itself was removed first");
}

// ---------------------------------------------------------------------------------------------
// M-40 / AC-155: ERR-54 when the SQLite commit and the vault undo both fail.

#[derive(Default, Clone)]
struct FakeVault {
    notes: Arc<Mutex<Vec<String>>>,
    note: String,
    fail_undo: bool,
}

impl VaultSide for FakeVault {
    type Receipt = String;

    fn note_name(&self) -> String {
        self.note.clone()
    }

    async fn write(&mut self) -> Result<String, VaultError> {
        self.notes.lock().unwrap().push(self.note.clone());
        Ok(self.note.clone())
    }

    async fn undo(&mut self, receipt: String) -> Result<(), VaultError> {
        if self.fail_undo {
            return Err("cofre fechado; token ghp_abcdefghijklmnop".into());
        }
        self.notes.lock().unwrap().retain(|n| *n != receipt);
        Ok(())
    }
}

/// A deferred foreign key makes the COMMIT itself fail, after the vault was written.
async fn failing_commit(pool: &SqlitePool) -> sqlx::Transaction<'static, sqlx::Sqlite> {
    ok(
        pool,
        "CREATE TABLE teste_commit (ref INTEGER REFERENCES tarefa (id) DEFERRABLE INITIALLY DEFERRED)",
    )
    .await;
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("INSERT INTO tarefa (plano, faz, toca_em, paralelo, pronto_quando, prova, papel) VALUES (1, 'T4', '[\"a\"]', 0, 'x', 'y', 'funcionalidade')")
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
async fn ac_155_commit_and_undo_failure_is_err_54_and_logged() {
    let f = seeded().await;
    let p = f.pool();
    let tarefas = count(p, "SELECT COUNT(*) FROM tarefa").await;
    let mut vault = FakeVault {
        note: "fundacao-plano".into(),
        fail_undo: true,
        ..Default::default()
    };

    let tx = failing_commit(p).await;
    let err = dual_write::commit_with_vault(
        tx,
        &mut vault,
        LogTarget {
            pool: p,
            execucao: 3,
        },
    )
    .await
    .expect_err("must fail");

    assert!(
        matches!(&err, DualWriteError::HalfUndone { note, log: None, .. } if note == "fundacao-plano"),
        "{err}"
    );
    assert_eq!(err.id(), "ERR-54");
    assert_eq!(err.code(), "GRAVACAO_DESFEITA_PELA_METADE");
    assert_eq!(
        err.message(),
        "A nota fundacao-plano ficou no cofre, mas não no banco. Apague a nota ou grave de novo."
    );
    assert!(
        err.to_string()
            .starts_with("ERR-54 GRAVACAO_DESFEITA_PELA_METADE"),
        "{err}"
    );

    // The note stays in the vault, nothing in SQLite, the case is in the Log.
    assert_eq!(*vault.notes.lock().unwrap(), ["fundacao-plano"]);
    assert_eq!(count(p, "SELECT COUNT(*) FROM tarefa").await, tarefas);
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM log WHERE execucao = 3 AND comando = 'ERR-54 GRAVACAO_DESFEITA_PELA_METADE' AND fim IS NOT NULL").await,
        1
    );
    let saida = text(p, "SELECT saida_mascarada FROM log WHERE execucao = 3").await;
    assert!(saida.contains("fundacao-plano"), "{saida}");
    assert!(saida.contains("cofre fechado"), "{saida}");
    assert!(
        !saida.contains("ghp_abcdefghijklmnop"),
        "masked (RN-04): {saida}"
    );
}

#[tokio::test]
async fn commit_failure_with_undo_is_still_err_51() {
    let f = seeded().await;
    let p = f.pool();
    let mut vault = FakeVault {
        note: "n".into(),
        ..Default::default()
    };

    let tx = failing_commit(p).await;
    let err = dual_write::commit_with_vault(
        tx,
        &mut vault,
        LogTarget {
            pool: p,
            execucao: 3,
        },
    )
    .await
    .expect_err("must fail");

    assert!(matches!(err, DualWriteError::Commit(_)), "{err}");
    assert_eq!(err.id(), "ERR-51");
    assert_eq!(err.code(), "GRAVACAO_DIVERGENTE");
    assert_eq!(
        err.message(),
        "Não deu para gravar no banco e no cofre ao mesmo tempo; nada foi gravado."
    );
    assert!(vault.notes.lock().unwrap().is_empty(), "vault side undone");
    assert_eq!(
        count(p, "SELECT COUNT(*) FROM log").await,
        0,
        "nothing to log"
    );
}

#[tokio::test]
async fn err_54_reports_a_log_failure() {
    let f = seeded().await;
    let p = f.pool();
    let mut vault = FakeVault {
        note: "n".into(),
        fail_undo: true,
        ..Default::default()
    };

    let tx = failing_commit(p).await;
    // Run 99 does not exist: the log row is refused and the error says so.
    let err = dual_write::commit_with_vault(
        tx,
        &mut vault,
        LogTarget {
            pool: p,
            execucao: 99,
        },
    )
    .await
    .expect_err("must fail");

    assert!(
        matches!(&err, DualWriteError::HalfUndone { log: Some(_), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("log write failed"), "{err}");
}
