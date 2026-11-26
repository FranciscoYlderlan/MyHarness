//! Schema of the contract entities (`0002_contract_entities.sql`, 02-entidades): every restriction
//! accepts the valid value and rejects the invalid one with its own constraint.

use my_harness_app::infra::db::Database;
use my_harness_app::infra::db::dual_write::DUAL_WRITE_TABLES;
use sqlx::{AssertSqlSafe, SqlitePool};

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

/// Database with one row in each parent table: projeto 1, cofre 1, chat 1 (ativo), chat 2
/// (encerrado), tarefa 1, execucao 1 and 2, relatorio 1.
async fn seeded() -> Fixture {
    let f = open().await;
    for sql in [
        "INSERT INTO projeto (id, nome) VALUES (1, 'MyHarness')",
        "INSERT INTO cofre (id, projeto, nome, funcao, ativo, status) VALUES (1, 1, 'myHarness-vault', 'memoria', 1, 'conectado')",
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (1, 1, 100000, 'ativo')",
        "INSERT INTO chat (id, projeto, limite_tokens, status) VALUES (2, 1, 100000, 'encerrado')",
        "INSERT INTO plano_delegacao (id, chat, versao, status) VALUES (1, 1, 1, 'proposto')",
        "INSERT INTO tarefa (id, plano, faz, toca_em, paralelo, pronto_quando, prova, papel) VALUES (1, 1, 'Esquema', '[\"src-tauri/migrations/\"]', 1, 'testes passam', 'cargo test', 'funcionalidade')",
        "INSERT INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, worktree, branch, status) VALUES (1, 1, 1, 'funcionalidade', 'opus', 1, '/w/t4', 'feat/RF-01-esquema', 'rodando')",
        "INSERT INTO execucao (id, chat, tarefa, papel, modelo, prompt_versao, status) VALUES (2, 1, 1, 'teste', 'opus', 1, 'rodando')",
        "INSERT INTO relatorio (id, execucao, resultado, o_que_fez, prova) VALUES (1, 1, 'concluído', 'migração', 'cargo test')",
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

const VALID_UTC: [&str; 3] = [
    "2026-11-19T18:20:00Z",
    "2026-11-19T18:20:00.123Z",
    "2026-02-28T00:00:00Z",
];

const INVALID_UTC: [&str; 9] = [
    "2026-11-19T18:20:00",
    "2026-11-19 18:20:00Z",
    "2026-11-19T15:20:00-03:00",
    "2026-11-19T18:20:00+00:00",
    "2026-11-19T18:20:00z",
    "2026-02-30T10:00:00Z",
    "2026-11-19",
    "19/11/2026 18:20",
    "",
];

/// `template` has `{}` where the date goes; each valid date runs with its own `{id}`.
async fn check_utc(pool: &SqlitePool, template: &str, constraint: &str, first_id: i64) {
    for (i, value) in VALID_UTC.iter().enumerate() {
        let sql = template
            .replace("{id}", &(first_id + i as i64).to_string())
            .replace("{}", value);
        ok(pool, &sql).await;
    }
    for value in INVALID_UTC {
        let sql = template
            .replace("{id}", &(first_id + 100).to_string())
            .replace("{}", value);
        rejects(pool, &sql, constraint).await;
    }
}

#[tokio::test]
async fn every_sqlite_entity_has_a_strict_table() {
    let f = open().await;
    let tables = [
        "projeto",
        "cofre",
        "chat",
        "mensagem",
        "plano_delegacao",
        "tarefa",
        "tarefa_dependencia",
        "execucao",
        "prompt_papel",
        "relatorio",
        "achado",
        "leitura",
        "log",
        "integracao",
        "consumo",
        "recurso",
        "escrita",
        "decisao",
        "briefing",
        "rodada",
        "funcionalidade",
        "configuracao",
        "pr",
        "criterio",
        "pasta_contexto",
    ];
    for table in tables {
        let (strict,): (i64,) =
            sqlx::query_as("SELECT strict FROM pragma_table_list WHERE name = ?")
                .bind(table)
                .fetch_one(f.pool())
                .await
                .unwrap_or_else(|e| panic!("table {table} missing: {e}"));
        assert_eq!(strict, 1, "{table} is not STRICT");
    }
    for table in DUAL_WRITE_TABLES {
        assert!(tables.contains(&table), "dual write table {table} missing");
    }
    // ENT-15 Papel lives in the core code and ENT-25 Diario only in the vault.
    for table in ["papel", "diario"] {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pragma_table_list WHERE name = ?")
            .bind(table)
            .fetch_one(f.pool())
            .await
            .expect("table list");
        assert_eq!(n, 0, "{table} should not be in SQLite");
    }
}

#[tokio::test]
async fn foreign_keys_are_enforced() {
    let f = seeded().await;
    let p = f.pool();
    let fk = "FOREIGN KEY constraint failed";
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (9, 'x', 'consulta', 1, 'conectado')", fk).await;
    rejects(
        p,
        "INSERT INTO chat (projeto, limite_tokens, status) VALUES (9, 1, 'ativo')",
        fk,
    )
    .await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (9, 'Humano', 'oi')",
        fk,
    )
    .await;
    rejects(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (9, 1, 'proposto')",
        fk,
    )
    .await;
    rejects(p, "UPDATE tarefa SET implementada_por = 9 WHERE id = 1", fk).await;
    rejects(p, "UPDATE tarefa SET testada_por = 9 WHERE id = 1", fk).await;
    rejects(
        p,
        "INSERT INTO tarefa_dependencia (tarefa, depende_de) VALUES (1, 9)",
        fk,
    )
    .await;
    rejects(p, "INSERT INTO execucao (chat, papel, modelo, prompt_versao, status) VALUES (9, 'orquestrador', 'opus', 1, 'rodando')", fk).await;
    rejects(p, "INSERT INTO execucao (chat, tarefa, papel, modelo, prompt_versao, status) VALUES (1, 9, 'teste', 'opus', 1, 'rodando')", fk).await;
    rejects(p, "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (9, 'a.md', 'arquivo', '2026-11-19T18:20:00Z')", fk).await;
    rejects(p, "INSERT INTO leitura (execucao, caminho, cofre, origem, hora) VALUES (1, 'a.md', 9, 'cli', '2026-11-19T18:20:00Z')", fk).await;
    rejects(
        p,
        "INSERT INTO log (execucao, comando, inicio) VALUES (9, 'ls', '2026-11-19T18:20:00Z')",
        fk,
    )
    .await;
    rejects(
        p,
        "INSERT INTO recurso (nome, em_uso_por) VALUES ('banco de teste', 9)",
        fk,
    )
    .await;
    rejects(
        p,
        "INSERT INTO criterio (relatorio, status, prova) VALUES (9, '✔', 'ok')",
        fk,
    )
    .await;
    rejects(
        p,
        "INSERT INTO pasta_contexto (projeto, caminho) VALUES (9, 'C:\\x')",
        fk,
    )
    .await;
    rejects(p, "DELETE FROM projeto WHERE id = 1", fk).await;
}

// ENT-01 Projeto
#[tokio::test]
async fn projeto() {
    let f = open().await;
    let p = f.pool();
    ok(p, "INSERT INTO projeto (id, nome) VALUES (1, 'A')").await;
    let (limite,): (i64,) =
        sqlx::query_as("SELECT limite_contexto_padrao FROM projeto WHERE id = 1")
            .fetch_one(p)
            .await
            .expect("projeto");
    assert_eq!(limite, 100_000, "default S7");

    let nome_80 = "n".repeat(80);
    let nome_81 = "n".repeat(81);
    ok(
        p,
        &format!("INSERT INTO projeto (nome) VALUES ('{nome_80}')"),
    )
    .await;
    ok(
        p,
        "INSERT INTO projeto (nome) VALUES ('Projeto com acento é ok')",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO projeto (nome) VALUES ('{nome_81}')"),
        "projeto_nome_tamanho",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome) VALUES ('')",
        "projeto_nome_tamanho",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome) VALUES ('   ')",
        "projeto_nome_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome) VALUES (NULL)",
        "NOT NULL constraint failed: projeto.nome",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome) VALUES ('A')",
        "UNIQUE constraint failed: projeto.nome",
    )
    .await;

    for path in [
        "C:\\repos\\my-harness",
        "c:/repos/x",
        "\\\\server\\share\\repo",
        "/home/y/repo",
    ] {
        ok(
            p,
            &format!("INSERT INTO projeto (nome, repositorio) VALUES ('r {path}', '{path}')"),
        )
        .await;
    }
    ok(
        p,
        "INSERT INTO projeto (nome, repositorio) VALUES ('sem repo', NULL)",
    )
    .await;
    for path in ["repos\\x", "./x", "C:repo", "", "\\x"] {
        rejects(
            p,
            &format!("INSERT INTO projeto (nome, repositorio) VALUES ('bad {path}', '{path}')"),
            "projeto_repositorio_absoluto",
        )
        .await;
    }

    ok(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('um token', 1)",
    )
    .await;
    ok(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('fora da faixa', 5000000)",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('zero', 0)",
        "projeto_limite_contexto_padrao_minimo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('neg', -1)",
        "projeto_limite_contexto_padrao_minimo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('txt', 'cem mil')",
        "cannot store TEXT value in INTEGER column",
    )
    .await;
    rejects(
        p,
        "INSERT INTO projeto (nome, limite_contexto_padrao) VALUES ('nulo', NULL)",
        "NOT NULL constraint failed: projeto.limite_contexto_padrao",
    )
    .await;
}

// ENT-02 Cofre
#[tokio::test]
async fn cofre() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'Vault', 'consulta', 0, 'desconectado')").await;
    ok(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'Outro', 'consulta', 1, 'conectado')").await;
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'Dois', 'memoria', 1, 'conectado')", "UNIQUE constraint failed: cofre.projeto").await;
    ok(p, "INSERT INTO projeto (id, nome) VALUES (2, 'B')").await;
    ok(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (2, 'b-vault', 'memoria', 1, 'conectado')").await;
    // Swap: the old memory becomes consulta first, then the new one becomes memoria (UC-14 FA-01).
    ok(p, "UPDATE cofre SET funcao = 'consulta' WHERE id = 1").await;
    ok(
        p,
        "UPDATE cofre SET funcao = 'memoria' WHERE nome = 'Vault'",
    )
    .await;

    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'x', 'leitura', 1, 'conectado')", "cofre_funcao_valores").await;
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'x', 'consulta', 2, 'conectado')", "cofre_ativo_booleano").await;
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'x', 'consulta', 'sim', 'conectado')", "cannot store TEXT value in INTEGER column").await;
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, 'x', 'consulta', 1, 'online')", "cofre_status_valores").await;
    rejects(p, "INSERT INTO cofre (projeto, nome, funcao, ativo, status) VALUES (1, ' ', 'consulta', 1, 'conectado')", "cofre_nome_vazio").await;
    rejects(
        p,
        "INSERT INTO cofre (nome, funcao, ativo, status) VALUES ('x', 'consulta', 1, 'conectado')",
        "NOT NULL constraint failed: cofre.projeto",
    )
    .await;
}

// ENT-03 Chat
#[tokio::test]
async fn chat() {
    let f = seeded().await;
    let p = f.pool();
    for status in ["ativo", "pausado", "encerrado", "só leitura"] {
        ok(
            p,
            &format!("INSERT INTO chat (projeto, limite_tokens, status) VALUES (1, 1, '{status}')"),
        )
        .await;
    }
    rejects(
        p,
        "INSERT INTO chat (projeto, limite_tokens, status) VALUES (1, 1, 'so leitura')",
        "chat_status_valores",
    )
    .await;
    rejects(
        p,
        "INSERT INTO chat (projeto, limite_tokens, status) VALUES (1, 0, 'ativo')",
        "chat_limite_tokens_minimo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO chat (projeto, status) VALUES (1, 'ativo')",
        "NOT NULL constraint failed: chat.limite_tokens",
    )
    .await;
    rejects(p, "INSERT INTO chat (projeto, limite_tokens, tokens_usados, status) VALUES (1, 1, -1, 'ativo')", "chat_tokens_usados_minimo").await;
    rejects(p, "INSERT INTO chat (projeto, limite_tokens, continuacoes, status) VALUES (1, 1, -1, 'ativo')", "chat_continuacoes_minimo").await;

    let (usados, cont): (i64, i64) =
        sqlx::query_as("SELECT tokens_usados, continuacoes FROM chat WHERE id = 1")
            .fetch_one(p)
            .await
            .expect("chat");
    assert_eq!((usados, cont), (0, 0));

    ok(p, "INSERT INTO chat (projeto, limite_tokens, status, chat_anterior) VALUES (1, 1, 'ativo', 2)").await;
    rejects(p, "INSERT INTO chat (projeto, limite_tokens, status, chat_anterior) VALUES (1, 1, 'ativo', 1)", "chat_chat_anterior_encerrado").await;
    rejects(
        p,
        "UPDATE chat SET chat_anterior = 1 WHERE id = 2",
        "chat_chat_anterior_encerrado",
    )
    .await;
    rejects(p, "INSERT INTO chat (id, projeto, limite_tokens, status, chat_anterior) VALUES (50, 1, 1, 'encerrado', 50)", "chat_chat_anterior").await;
}

// ENT-04 Mensagem
#[tokio::test]
async fn mensagem() {
    let f = seeded().await;
    let p = f.pool();
    ok(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'Humano', 'oi')",
    )
    .await;
    ok(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'sistema', 'aviso')",
    )
    .await;
    ok(p, "INSERT INTO mensagem (chat, autor, texto, uso_tokens) VALUES (1, 'orquestrador', 'plano', 0)").await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'orquestrador', 'plano')",
        "mensagem_uso_tokens_orquestrador",
    )
    .await;
    rejects(p, "INSERT INTO mensagem (chat, autor, texto, uso_tokens) VALUES (1, 'orquestrador', 'plano', -1)", "mensagem_uso_tokens_minimo").await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'humano', 'oi')",
        "mensagem_autor_valores",
    )
    .await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'Humano', '')",
        "mensagem_texto_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor, texto) VALUES (1, 'Humano', char(10, 9, 32))",
        "mensagem_texto_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO mensagem (chat, autor) VALUES (1, 'Humano')",
        "NOT NULL constraint failed: mensagem.texto",
    )
    .await;
}

// ENT-05 PlanoDelegacao
#[tokio::test]
async fn plano_delegacao() {
    let f = seeded().await;
    let p = f.pool();
    ok(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (1, 1, 'proposto')",
    )
    .await;
    ok(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (1, 2, 'recusado')",
    )
    .await;
    ok(p, "INSERT INTO plano_delegacao (chat, versao, status, devolucoes) VALUES (1, 3, 'proposto', 2)").await;
    rejects(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (1, 0, 'proposto')",
        "plano_delegacao_versao_minimo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (1, 1, 'em análise')",
        "plano_delegacao_status_valores",
    )
    .await;
    rejects(p, "INSERT INTO plano_delegacao (chat, versao, status, devolucoes) VALUES (1, 1, 'proposto', 3)", "plano_delegacao_devolucoes_faixa").await;
    rejects(p, "INSERT INTO plano_delegacao (chat, versao, status, devolucoes) VALUES (1, 1, 'proposto', -1)", "plano_delegacao_devolucoes_faixa").await;
    rejects(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status) VALUES (1, 1, 'aprovado')",
        "plano_delegacao_aprovado_em_so_se_aprovado",
    )
    .await;
    rejects(p, "INSERT INTO plano_delegacao (chat, versao, status, aprovado_em) VALUES (1, 1, 'proposto', '2026-11-19T18:20:00Z')", "plano_delegacao_aprovado_em_so_se_aprovado").await;
    check_utc(
        p,
        "INSERT INTO plano_delegacao (chat, versao, status, aprovado_em) VALUES (1, 1, 'aprovado', '{}')",
        "plano_delegacao_aprovado_em_utc",
        0,
    )
    .await;
    let (dev,): (i64,) = sqlx::query_as(
        "SELECT devolucoes FROM plano_delegacao WHERE versao = 1 AND status = 'proposto'",
    )
    .fetch_one(p)
    .await
    .expect("plano");
    assert_eq!(dev, 0, "devolucoes starts at 0");
}

// ENT-06 Tarefa
#[tokio::test]
async fn tarefa() {
    let f = seeded().await;
    let p = f.pool();
    let cols = "plano, faz, toca_em, paralelo, pronto_quando, prova, papel";
    let faz_500 = "f".repeat(500);
    let faz_501 = "f".repeat(501);
    ok(p, &format!("INSERT INTO tarefa ({cols}) VALUES (1, '{faz_500}', '[\"a.rs\"]', 0, 'x', 'y', 'funcionalidade')")).await;
    ok(
        p,
        &format!("INSERT INTO tarefa ({cols}) VALUES (1, 'ler', NULL, 0, 'x', 'y', 'pesquisa')"),
    )
    .await;
    ok(
        p,
        &format!("INSERT INTO tarefa ({cols}) VALUES (1, 'testar', '[]', 1, 'x', 'y', 'teste')"),
    )
    .await;
    ok(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'usar', NULL, 1, 'x', 'y', 'adversarial')"
        ),
    )
    .await;
    rejects(p, &format!("INSERT INTO tarefa ({cols}) VALUES (1, '{faz_501}', '[\"a\"]', 0, 'x', 'y', 'funcionalidade')"), "tarefa_faz_tamanho").await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, '', '[\"a\"]', 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_faz_tamanho",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, '  ', '[\"a\"]', 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_faz_vazio",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[\"a\"]', 0, ' ', 'y', 'funcionalidade')"
        ),
        "tarefa_pronto_quando_vazio",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[\"a\"]', 0, 'x', '', 'funcionalidade')"
        ),
        "tarefa_prova_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO tarefa (plano, faz, paralelo, prova, papel) VALUES (1, 'f', 0, 'y', 'teste')",
        "tarefa_pronto_quando_vazio",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[\"a\"]', 2, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_paralelo_booleano",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[\"a\"]', 0, 'x', 'y', 'orquestrador')"
        ),
        "tarefa_papel_valores",
    )
    .await;

    // toca_em: required (non-empty) for funcionalidade; JSON list of relative paths.
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', NULL, 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_toca_em_funcionalidade",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[]', 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_toca_em_funcionalidade",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', 'a.rs', 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_toca_em_lista",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO tarefa ({cols}) VALUES (1, 'f', '{{\"a\":1}}', 0, 'x', 'y', 'funcionalidade')"
        ),
        "tarefa_toca_em_lista",
    )
    .await;
    for item in [
        "\"/etc/x\"",
        "\"C:\\\\x\"",
        "\"\\\\srv\\\\x\"",
        "\" \"",
        "1",
    ] {
        rejects(
            p,
            &format!("INSERT INTO tarefa ({cols}) VALUES (1, 'f', '[\"ok.rs\", {item}]', 0, 'x', 'y', 'funcionalidade')"),
            "tarefa_toca_em_relativo",
        )
        .await;
    }
    rejects(
        p,
        "UPDATE tarefa SET toca_em = '[\"/abs\"]' WHERE id = 1",
        "tarefa_toca_em_relativo",
    )
    .await;

    // modelo optional, not blank.
    ok(
        p,
        "UPDATE tarefa SET modelo = 'claude-opus-5-5' WHERE id = 1",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET modelo = '' WHERE id = 1",
        "tarefa_modelo_vazio",
    )
    .await;

    // voltas 0 to 2, starts at 0.
    ok(p, "UPDATE tarefa SET voltas = 2 WHERE id = 1").await;
    rejects(
        p,
        "UPDATE tarefa SET voltas = 3 WHERE id = 1",
        "tarefa_voltas_faixa",
    )
    .await;
    rejects(
        p,
        "UPDATE tarefa SET voltas = -1 WHERE id = 1",
        "tarefa_voltas_faixa",
    )
    .await;

    // testada_por differs from implementada_por (RN-09, ERR-19).
    ok(p, "UPDATE tarefa SET implementada_por = 1 WHERE id = 1").await;
    rejects(
        p,
        "UPDATE tarefa SET testada_por = 1 WHERE id = 1",
        "tarefa_testada_por_diferente",
    )
    .await;
    ok(p, "UPDATE tarefa SET testada_por = 2 WHERE id = 1").await;

    // depende_de
    ok(
        p,
        "INSERT INTO tarefa_dependencia (tarefa, depende_de) VALUES (2, 1)",
    )
    .await;
    rejects(
        p,
        "INSERT INTO tarefa_dependencia (tarefa, depende_de) VALUES (2, 1)",
        "UNIQUE constraint failed",
    )
    .await;
    rejects(
        p,
        "INSERT INTO tarefa_dependencia (tarefa, depende_de) VALUES (1, 1)",
        "tarefa_dependencia_propria",
    )
    .await;
}

// ENT-07 Execucao
#[tokio::test]
async fn execucao() {
    let f = seeded().await;
    let p = f.pool();
    let cols = "chat, tarefa, papel, modelo, prompt_versao, worktree, branch, status, motivo_fila";
    ok(p, "INSERT INTO execucao (chat, papel, modelo, prompt_versao, status) VALUES (1, 'orquestrador', 'opus', 1, 'rodando')").await;
    for status in ["rodando", "concluído", "falhou", "interrompida"] {
        ok(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'funcionalidade', 'opus', 1, 'C:\\w', 'fix/BUG-3-x', '{status}', NULL)")).await;
    }
    for motivo in ["teto", "toca_em", "recurso"] {
        ok(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'pesquisa', 'opus', 1, NULL, NULL, 'na fila', '{motivo}')")).await;
    }
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'pesquisa', 'opus', 1, NULL, NULL, 'na fila', NULL)"), "execucao_motivo_fila_na_fila").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'pesquisa', 'opus', 1, NULL, NULL, 'na fila', 'cansaco')"), "execucao_motivo_fila_valores").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'pesquisa', 'opus', 1, NULL, NULL, 'concluido', NULL)"), "execucao_status_valores").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'revisor', 'opus', 1, NULL, NULL, 'rodando', NULL)"), "execucao_papel_valores").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, NULL, 'teste', 'opus', 1, NULL, NULL, 'rodando', NULL)"), "execucao_tarefa_obrigatoria").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'teste', ' ', 1, NULL, NULL, 'rodando', NULL)"), "execucao_modelo_vazio").await;
    rejects(p, "INSERT INTO execucao (chat, tarefa, papel, prompt_versao, status) VALUES (1, 1, 'teste', 1, 'rodando')", "NOT NULL constraint failed: execucao.modelo").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'teste', 'opus', 0, NULL, NULL, 'rodando', NULL)"), "execucao_prompt_versao_minimo").await;
    rejects(p, "INSERT INTO execucao (chat, tarefa, papel, modelo, status) VALUES (1, 1, 'teste', 'opus', 'rodando')", "NOT NULL constraint failed: execucao.prompt_versao").await;

    // funcionalidade: own worktree and branch `tipo/ID-descricao` (RN-16, ERR-16).
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'funcionalidade', 'opus', 1, NULL, 'feat/RF-01-x', 'rodando', NULL)"), "execucao_worktree_funcionalidade").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'funcionalidade', 'opus', 1, '/w', NULL, 'rodando', NULL)"), "execucao_worktree_funcionalidade").await;
    rejects(p, &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'funcionalidade', 'opus', 1, '', 'feat/RF-01-x', 'rodando', NULL)"), "execucao_worktree_vazio").await;
    for branch in [
        "main",
        "feat/esquema",
        "feat/RF-01 esquema",
        "/RF-01-x",
        "Feat/RF-01-x",
        "feat/-x",
    ] {
        rejects(
            p,
            &format!("INSERT INTO execucao ({cols}) VALUES (1, 1, 'funcionalidade', 'opus', 1, '/w', '{branch}', 'rodando', NULL)"),
            "execucao_branch_formato",
        )
        .await;
    }

    // prompt_versao does not change during the run (RN-28).
    ok(p, "UPDATE execucao SET status = 'concluído' WHERE id = 1").await;
    ok(p, "UPDATE execucao SET prompt_versao = 1 WHERE id = 1").await;
    rejects(
        p,
        "UPDATE execucao SET prompt_versao = 2 WHERE id = 1",
        "execucao_prompt_versao_imutavel",
    )
    .await;
}

// ENT-08 PromptPapel
#[tokio::test]
async fn prompt_papel() {
    let f = open().await;
    let p = f.pool();
    let cols = "fluxo, texto, versao, palavras, tokens, vigente";
    for fluxo in [
        "orquestrador",
        "funcionalidade",
        "teste",
        "adversarial",
        "passagem de turno",
    ] {
        ok(p, &format!("INSERT INTO prompt_papel ({cols}) VALUES ('{fluxo}', 'Você é o papel.', 1, 4, 6, 1)")).await;
    }
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('pesquisa', 't', 1, 1, 1, 0)"),
        "prompt_papel_fluxo_valores",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 'outro', 2, 1, 1, 1)"),
        "prompt_papel_uma_vigente_por_fluxo",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 'outro', 1, 1, 1, 0)"),
        "prompt_papel_versao_imutavel",
    )
    .await;
    ok(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 'oitenta', 2, 80, 150, 0)"),
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 'muito', 3, 81, 1, 0)"),
        "prompt_papel_palavras_faixa",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 'nada', 3, 0, 1, 0)"),
        "prompt_papel_palavras_faixa",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', ' ', 3, 1, 1, 0)"),
        "prompt_papel_texto_vazio",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 't', 0, 1, 1, 0)"),
        "prompt_papel_versao_minimo",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 't', 3, 1, -1, 0)"),
        "prompt_papel_tokens_minimo",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO prompt_papel ({cols}) VALUES ('teste', 't', 3, 1, 1, 2)"),
        "prompt_papel_vigente_booleano",
    )
    .await;

    // Switching the current version is allowed; editing or deleting a version is not (RF-31).
    ok(
        p,
        "UPDATE prompt_papel SET vigente = 0 WHERE fluxo = 'teste' AND versao = 1",
    )
    .await;
    ok(
        p,
        "UPDATE prompt_papel SET vigente = 1 WHERE fluxo = 'teste' AND versao = 2",
    )
    .await;
    rejects(
        p,
        "UPDATE prompt_papel SET texto = 'editado' WHERE fluxo = 'teste' AND versao = 1",
        "prompt_papel_versao_imutavel",
    )
    .await;
    rejects(
        p,
        "UPDATE prompt_papel SET palavras = 5 WHERE fluxo = 'teste' AND versao = 1",
        "prompt_papel_versao_imutavel",
    )
    .await;
    rejects(
        p,
        "DELETE FROM prompt_papel WHERE fluxo = 'teste' AND versao = 1",
        "prompt_papel_versao_imutavel",
    )
    .await;
}

// ENT-09 Relatorio and ENT-24 Criterio
#[tokio::test]
async fn relatorio_e_criterio() {
    let f = seeded().await;
    let p = f.pool();
    let cols = "execucao, resultado, o_que_fez, prova";
    for resultado in ["concluído", "falhou", "bloqueado"] {
        ok(
            p,
            &format!("INSERT INTO relatorio ({cols}) VALUES (1, '{resultado}', 'fez', 'prova')"),
        )
        .await;
    }
    rejects(
        p,
        &format!("INSERT INTO relatorio ({cols}) VALUES (1, 'concluido', 'fez', 'prova')"),
        "relatorio_resultado_valores",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO relatorio ({cols}) VALUES (1, 'falhou', '', 'prova')"),
        "relatorio_o_que_fez_vazio",
    )
    .await;
    rejects(
        p,
        &format!("INSERT INTO relatorio ({cols}) VALUES (1, 'falhou', 'fez', ' ')"),
        "relatorio_prova_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO relatorio (execucao, resultado, o_que_fez) VALUES (1, 'falhou', 'fez')",
        "NOT NULL constraint failed: relatorio.prova",
    )
    .await;

    ok(p, "UPDATE relatorio SET arquivos = '[\"a.rs\"]', duvidas = '[]', candidatos_a_nota = '[\"nota\"]' WHERE id = 1").await;
    rejects(
        p,
        "UPDATE relatorio SET arquivos = 'a.rs' WHERE id = 1",
        "relatorio_arquivos_lista",
    )
    .await;
    rejects(
        p,
        "UPDATE relatorio SET duvidas = '\"uma\"' WHERE id = 1",
        "relatorio_duvidas_lista",
    )
    .await;
    rejects(
        p,
        "UPDATE relatorio SET candidatos_a_nota = '{}' WHERE id = 1",
        "relatorio_candidatos_a_nota_lista",
    )
    .await;
    rejects(
        p,
        "UPDATE relatorio SET duvidas = '[\"ok\", \"\"]' WHERE id = 1",
        "relatorio_listas_texto",
    )
    .await;
    rejects(
        p,
        "UPDATE relatorio SET arquivos = '[1]' WHERE id = 1",
        "relatorio_listas_texto",
    )
    .await;
    rejects(p, &format!("INSERT INTO relatorio ({cols}, candidatos_a_nota) VALUES (1, 'falhou', 'fez', 'p', '[null]')"), "relatorio_listas_texto").await;
    check_utc(
        p,
        "INSERT INTO relatorio (execucao, resultado, o_que_fez, prova, registrado_em) VALUES (1, 'falhou', 'fez', 'p', '{}')",
        "relatorio_registrado_em_utc",
        0,
    )
    .await;

    ok(
        p,
        "INSERT INTO criterio (relatorio, status, prova) VALUES (1, '✔', 'cargo test: 12 passed')",
    )
    .await;
    ok(
        p,
        "INSERT INTO criterio (relatorio, status) VALUES (1, '✘')",
    )
    .await;
    ok(
        p,
        "INSERT INTO criterio (relatorio, status) VALUES (1, 'não verificado, presumido')",
    )
    .await;
    rejects(
        p,
        "INSERT INTO criterio (relatorio, status) VALUES (1, '✔')",
        "criterio_prova_para_ok",
    )
    .await;
    rejects(
        p,
        "INSERT INTO criterio (relatorio, status, prova) VALUES (1, '✔', ' ')",
        "criterio_prova_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO criterio (relatorio, status) VALUES (1, 'ok')",
        "criterio_status_valores",
    )
    .await;
    rejects(
        p,
        "INSERT INTO criterio (status) VALUES ('✘')",
        "NOT NULL constraint failed: criterio.relatorio",
    )
    .await;
}

// ENT-10 Achado
#[tokio::test]
async fn achado() {
    let f = seeded().await;
    let p = f.pool();
    let cols = "execucao, tipo, passos, esperado, obtido, decisao, status";
    ok(p, &format!("INSERT INTO achado ({cols}) VALUES (1, 'bug', '[\"abrir\"]', 'e', 'o', 'corrigir', 'aberto')")).await;
    for decisao in ["NULL", "'corrigir'", "'limite'", "'opcional'"] {
        ok(p, &format!("INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\", \"b\"]', 'e', 'o', {decisao}, 'fechado')")).await;
    }
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'bug', '[\"a\"]', 'e', 'o', NULL, 'aberto')"
        ),
        "achado_bug_corrigir",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'bug', '[\"a\"]', 'e', 'o', 'opcional', 'aberto')"
        ),
        "achado_bug_corrigir",
    )
    .await;
    rejects(
        p,
        "UPDATE achado SET decisao = 'limite' WHERE tipo = 'bug'",
        "achado_bug_corrigir",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'melhoria', '[\"a\"]', 'e', 'o', NULL, 'aberto')"
        ),
        "achado_tipo_valores",
    )
    .await;
    rejects(p, "INSERT INTO achado (execucao, passos, esperado, obtido, status) VALUES (1, '[\"a\"]', 'e', 'o', 'aberto')", "NOT NULL constraint failed: achado.tipo").await;
    rejects(p, &format!("INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\"]', 'e', 'o', 'talvez', 'aberto')"), "achado_decisao_valores").await;
    rejects(p, &format!("INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\"]', 'e', 'o', NULL, 'resolvido')"), "achado_status_valores").await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[]', 'e', 'o', NULL, 'aberto')"
        ),
        "achado_passos_lista",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '1. abrir', 'e', 'o', NULL, 'aberto')"
        ),
        "achado_passos_lista",
    )
    .await;
    rejects(p, &format!("INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\", \" \"]', 'e', 'o', NULL, 'aberto')"), "achado_passos_texto").await;
    rejects(
        p,
        "UPDATE achado SET passos = '[2]' WHERE id = 1",
        "achado_passos_texto",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\"]', '', 'o', NULL, 'aberto')"
        ),
        "achado_esperado_vazio",
    )
    .await;
    rejects(
        p,
        &format!(
            "INSERT INTO achado ({cols}) VALUES (1, 'sugestao', '[\"a\"]', 'e', ' ', NULL, 'aberto')"
        ),
        "achado_obtido_vazio",
    )
    .await;
    ok(
        p,
        "UPDATE achado SET captura = 'Anexos/captura-1.png' WHERE id = 1",
    )
    .await;
    rejects(
        p,
        "UPDATE achado SET captura = '' WHERE id = 1",
        "achado_captura_vazio",
    )
    .await;
}

// ENT-11 Leitura
#[tokio::test]
async fn leitura() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "INSERT INTO leitura (execucao, caminho, cofre, origem, hora) VALUES (1, 'a.md', 1, 'cli', '2026-11-19T18:20:00Z')").await;
    ok(p, "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (1, 'a.md', 'arquivo', '2026-11-19T18:20:00Z')").await;
    rejects(p, "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (1, 'a.md', 'cli', '2026-11-19T18:20:00Z')", "leitura_cofre_cli").await;
    rejects(p, "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (1, 'a.md', 'grep', '2026-11-19T18:20:00Z')", "leitura_origem_valores").await;
    rejects(p, "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (1, ' ', 'arquivo', '2026-11-19T18:20:00Z')", "leitura_caminho_vazio").await;
    rejects(
        p,
        "INSERT INTO leitura (execucao, caminho, origem) VALUES (1, 'a.md', 'arquivo')",
        "NOT NULL constraint failed: leitura.hora",
    )
    .await;
    check_utc(
        p,
        "INSERT INTO leitura (execucao, caminho, origem, hora) VALUES (1, 'a.md', 'arquivo', '{}')",
        "leitura_hora_utc",
        0,
    )
    .await;
}

// ENT-12 Log
#[tokio::test]
async fn log() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "INSERT INTO log (execucao, comando, inicio) VALUES (1, 'cargo test', '2026-11-19T18:20:00Z')").await;
    ok(p, "INSERT INTO log (execucao, comando, inicio, fim, codigo_saida, saida_mascarada) VALUES (1, 'ls', '2026-11-19T18:20:00Z', '2026-11-19T18:20:00Z', 0, 'ok ••••')").await;
    ok(p, "INSERT INTO log (execucao, comando, inicio, fim) VALUES (1, 'ls', '2026-11-19T18:20:00.500Z', '2026-11-19T18:20:01Z')").await;
    rejects(p, "INSERT INTO log (execucao, comando, inicio, fim) VALUES (1, 'ls', '2026-11-19T18:20:00Z', '2026-11-19T18:19:59Z')", "log_fim_depois_do_inicio").await;
    rejects(p, "INSERT INTO log (execucao, comando, inicio, fim) VALUES (1, 'ls', '2026-11-19T18:20:00.500Z', '2026-11-19T18:20:00Z')", "log_fim_depois_do_inicio").await;
    rejects(
        p,
        "INSERT INTO log (execucao, comando, inicio) VALUES (1, '', '2026-11-19T18:20:00Z')",
        "log_comando_vazio",
    )
    .await;
    rejects(p, "INSERT INTO log (execucao, comando, inicio, codigo_saida) VALUES (1, 'ls', '2026-11-19T18:20:00Z', 'zero')", "cannot store TEXT value in INTEGER column").await;
    check_utc(
        p,
        "INSERT INTO log (execucao, comando, inicio) VALUES (1, 'ls', '{}')",
        "log_inicio_utc",
        0,
    )
    .await;
    check_utc(
        p,
        "INSERT INTO log (execucao, comando, inicio, fim) VALUES (1, 'ls', '2000-01-01T00:00:00Z', '{}')",
        "log_fim_utc",
        0,
    )
    .await;
}

// ENT-13 Integracao
#[tokio::test]
async fn integracao() {
    let f = open().await;
    let p = f.pool();
    for tipo in ["Claude CLI", "Obsidian CLI", "GitHub"] {
        for status in ["conectado", "desconectado", "sem login", "não instalado"] {
            ok(
                p,
                &format!("INSERT INTO integracao (tipo, status) VALUES ('{tipo}', '{status}')"),
            )
            .await;
        }
    }
    ok(p, "INSERT INTO integracao (tipo, status, conta, versao) VALUES ('GitHub', 'conectado', 'ylderlan', '2.62')").await;
    rejects(
        p,
        "INSERT INTO integracao (tipo, status) VALUES ('Git', 'conectado')",
        "integracao_tipo_valores",
    )
    .await;
    rejects(
        p,
        "INSERT INTO integracao (tipo, status) VALUES ('GitHub', 'nao instalado')",
        "integracao_status_valores",
    )
    .await;

    // The credential never goes to SQLite (RN-04, RNF-05).
    let columns: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM pragma_table_info('integracao')")
            .fetch_all(p)
            .await
            .expect("columns");
    let names: Vec<_> = columns.into_iter().map(|(n,)| n).collect();
    assert_eq!(names, ["id", "tipo", "status", "conta", "versao"]);
}

// ENT-14 Consumo
#[tokio::test]
async fn consumo() {
    let f = open().await;
    let p = f.pool();
    let cols = "semanal_pct, janela_pct, proximo_reinicio, semanal_reinicio, lida_em, origem, janela_status, semanal_status";
    let dates = "'2026-11-19T23:00:00Z', '2026-11-24T00:00:00Z', '2026-11-19T18:20:00Z'";
    ok(p, &format!("INSERT INTO consumo (id, {cols}) VALUES (1, 0, 100, {dates}, 'evento', 'allowed', 'allowed_warning')")).await;
    // Only the last reading: a new reading replaces the row.
    ok(p, &format!("INSERT OR REPLACE INTO consumo (id, {cols}, excedente) VALUES (1, 42.5, 7, {dates}, 'estimado', 'rejected', 'desconhecido', 'allowed')")).await;
    rejects(p, &format!("INSERT INTO consumo (id, {cols}) VALUES (2, 0, 0, {dates}, 'evento', 'allowed', 'allowed')"), "consumo_ultima_leitura").await;
    ok(p, "DELETE FROM consumo").await;
    let insert = |sem: &str, jan: &str, origem: &str, js: &str, ss: &str| {
        format!(
            "INSERT INTO consumo ({cols}) VALUES ({sem}, {jan}, {dates}, '{origem}', '{js}', '{ss}')"
        )
    };
    rejects(
        p,
        &insert("101", "0", "evento", "allowed", "allowed"),
        "consumo_semanal_pct_faixa",
    )
    .await;
    rejects(
        p,
        &insert("-0.1", "0", "evento", "allowed", "allowed"),
        "consumo_semanal_pct_faixa",
    )
    .await;
    rejects(
        p,
        &insert("0", "100.5", "evento", "allowed", "allowed"),
        "consumo_janela_pct_faixa",
    )
    .await;
    rejects(
        p,
        &insert("NULL", "0", "evento", "allowed", "allowed"),
        "NOT NULL constraint failed: consumo.semanal_pct",
    )
    .await;
    rejects(
        p,
        &insert("0", "0", "cli", "allowed", "allowed"),
        "consumo_origem_valores",
    )
    .await;
    rejects(
        p,
        &insert("0", "0", "evento", "warning", "allowed"),
        "consumo_janela_status_valores",
    )
    .await;
    rejects(
        p,
        &insert("0", "0", "evento", "allowed", "Allowed"),
        "consumo_semanal_status_valores",
    )
    .await;
}

/// `check_utc` on a one-row table: each valid value replaces the row.
#[tokio::test]
async fn consumo_dates() {
    let f = open().await;
    let p = f.pool();
    let cols = "id, semanal_pct, janela_pct, proximo_reinicio, semanal_reinicio, lida_em, origem, janela_status, semanal_status";
    for (col, constraint) in [
        ("proximo_reinicio", "consumo_proximo_reinicio_utc"),
        ("semanal_reinicio", "consumo_semanal_reinicio_utc"),
        ("lida_em", "consumo_lida_em_utc"),
    ] {
        let date = |c: &str, v: &str| {
            if c == col {
                format!("'{v}'")
            } else {
                "'2026-11-19T18:20:00Z'".into()
            }
        };
        for v in VALID_UTC {
            let sql = format!(
                "INSERT OR REPLACE INTO consumo ({cols}) VALUES (1, 0, 0, {}, {}, {}, 'evento', 'allowed', 'allowed')",
                date("proximo_reinicio", v),
                date("semanal_reinicio", v),
                date("lida_em", v)
            );
            ok(p, &sql).await;
        }
        for v in INVALID_UTC {
            let sql = format!(
                "INSERT OR REPLACE INTO consumo ({cols}) VALUES (1, 0, 0, {}, {}, {}, 'evento', 'allowed', 'allowed')",
                date("proximo_reinicio", v),
                date("semanal_reinicio", v),
                date("lida_em", v)
            );
            rejects(p, &sql, constraint).await;
        }
    }
}

// ENT-16 Recurso
#[tokio::test]
async fn recurso() {
    let f = seeded().await;
    let p = f.pool();
    ok(
        p,
        "INSERT INTO recurso (nome) VALUES ('navegador do Claude in Chrome')",
    )
    .await;
    ok(
        p,
        "INSERT INTO recurso (nome, em_uso_por) VALUES ('banco de teste', 1)",
    )
    .await;
    rejects(
        p,
        "INSERT INTO recurso (nome) VALUES ('banco de teste')",
        "UNIQUE constraint failed: recurso.nome",
    )
    .await;
    rejects(
        p,
        "INSERT INTO recurso (nome) VALUES ('impressora')",
        "recurso_nome_valores",
    )
    .await;
    ok(
        p,
        "UPDATE recurso SET em_uso_por = NULL WHERE nome = 'banco de teste'",
    )
    .await;
}

// ENT-17 Escrita, ENT-18 Decisao, ENT-19 Briefing
#[tokio::test]
async fn escrita_decisao_briefing() {
    let f = seeded().await;
    let p = f.pool();
    ok(p, "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (1, 'orquestra/diarios/d.md', 'registro', '2026-11-19T18:20:00Z')").await;
    rejects(
        p,
        "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (1, '', 'm', '2026-11-19T18:20:00Z')",
        "escrita_caminho_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (1, 'c', ' ', '2026-11-19T18:20:00Z')",
        "escrita_motivo_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO escrita (cofre, caminho, hora) VALUES (1, 'c', '2026-11-19T18:20:00Z')",
        "NOT NULL constraint failed: escrita.motivo",
    )
    .await;
    check_utc(
        p,
        "INSERT INTO escrita (cofre, caminho, motivo, hora) VALUES (1, 'c', 'm', '{}')",
        "escrita_hora_utc",
        0,
    )
    .await;

    ok(p, "INSERT INTO decisao (aval_id) VALUES ('aval-1')").await;
    rejects(
        p,
        "INSERT INTO decisao (aval_id) VALUES (' ')",
        "decisao_aval_id_vazio",
    )
    .await;
    rejects(
        p,
        "INSERT INTO decisao (registrada_em) VALUES ('2026-11-19T18:20:00Z')",
        "NOT NULL constraint failed: decisao.aval_id",
    )
    .await;
    check_utc(
        p,
        "INSERT INTO decisao (aval_id, registrada_em) VALUES ('a', '{}')",
        "decisao_registrada_em_utc",
        0,
    )
    .await;

    ok(
        p,
        "INSERT INTO briefing (conteudo) VALUES ('objetivo, endereço e acesso')",
    )
    .await;
    rejects(
        p,
        "INSERT INTO briefing (conteudo) VALUES ('')",
        "briefing_conteudo_vazio",
    )
    .await;
}

// ENT-20 Rodada, ENT-21 Funcionalidade, ENT-22 Configuracao, ENT-23 PR
#[tokio::test]
async fn rodada_funcionalidade_configuracao_pr() {
    let f = open().await;
    let p = f.pool();
    for status in ["em andamento", "aprovada", "falhou", "aguardando decisão"] {
        ok(
            p,
            &format!("INSERT INTO rodada (status) VALUES ('{status}')"),
        )
        .await;
    }
    rejects(
        p,
        "INSERT INTO rodada (status) VALUES ('aprovado')",
        "rodada_status_valores",
    )
    .await;

    for status in ["em-verificacao", "concluida", "bloqueada"] {
        ok(
            p,
            &format!("INSERT INTO funcionalidade (status) VALUES ('{status}')"),
        )
        .await;
    }
    rejects(
        p,
        "INSERT INTO funcionalidade (status) VALUES ('em verificação')",
        "funcionalidade_status_valores",
    )
    .await;

    let (id, teto): (i64, i64) = sqlx::query_as("SELECT id, teto_escrita FROM configuracao")
        .fetch_one(p)
        .await
        .expect("one configuracao row");
    assert_eq!((id, teto), (1, 5), "default S8");
    ok(p, "UPDATE configuracao SET teto_escrita = 1").await;
    rejects(
        p,
        "UPDATE configuracao SET teto_escrita = 0",
        "configuracao_teto_escrita_minimo",
    )
    .await;
    rejects(
        p,
        "INSERT INTO configuracao (id) VALUES (2)",
        "configuracao_unica",
    )
    .await;

    ok(p, "INSERT INTO pr (status) VALUES ('aberto')").await;
    ok(p, "INSERT INTO pr (status) VALUES ('merged')").await;
    rejects(
        p,
        "INSERT INTO pr (status) VALUES ('fechado')",
        "pr_status_valores",
    )
    .await;
}

// ENT-26 PastaContexto
#[tokio::test]
async fn pasta_contexto() {
    let f = seeded().await;
    let p = f.pool();
    for path in [
        "C:\\Users\\y\\docs",
        "D:/dados",
        "\\\\srv\\share",
        "/home/y/docs",
    ] {
        ok(
            p,
            &format!("INSERT INTO pasta_contexto (projeto, caminho) VALUES (1, '{path}')"),
        )
        .await;
    }
    for path in ["docs", "..\\docs", "", "C:docs"] {
        rejects(
            p,
            &format!("INSERT INTO pasta_contexto (projeto, caminho) VALUES (1, '{path}')"),
            "pasta_contexto_caminho_absoluto",
        )
        .await;
    }
    rejects(
        p,
        "INSERT INTO pasta_contexto (caminho) VALUES ('C:\\x')",
        "NOT NULL constraint failed: pasta_contexto.projeto",
    )
    .await;
}
