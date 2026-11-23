-- no-transaction
-- Contract changes M-37 to M-39 and fixes from the T4 verification (Anexos/contrato/02-entidades.md):
-- * M-37: required links tarefa.plano, relatorio.execucao, achado.execucao and escrita.cofre;
--   achado.captura required when execucao.papel = adversarial; escrita only in the memory vault (ERR-39).
-- * M-38: execucao.status `concluido` becomes `concluído`.
-- * M-39: tarefa.incompleta; with incompleta = 1, faz, pronto_quando and prova may be empty;
--   a plan with an incomplete task cannot be approved (ERR-49).
-- * ENT-07 comandos (RN-17): commands the agent ran, never push or merge (ERR-14). Relatorio.criterios
--   required for papel teste is a core rule, not a database one.
-- * RF-31 and RN-28: `INSERT OR REPLACE` and `UPDATE OR REPLACE` delete the old row without
--   firing the DELETE or UPDATE triggers, so prompt versions and execucao rows refuse any write
--   that would replace an existing row.
--
-- SQLite cannot change a column's constraints in place, so tarefa, execucao, relatorio, achado and
-- escrita are rebuilt (new table, copy, drop, rename). Foreign keys must be off for the rebuild and
-- `PRAGMA foreign_keys` is a no-op inside a transaction, hence `no-transaction` with an explicit
-- BEGIN/COMMIT; `migracao_0003_fk` makes the commit fail if any reference is left dangling.
-- Rows of the rebuilt tables have no value for the new required links: a database that already
-- holds tarefa, relatorio, achado or escrita rows stops here with a NOT NULL error and nothing changes.

PRAGMA foreign_keys = OFF;

BEGIN;

-- ENT-06 Tarefa (dual write)
CREATE TABLE tarefa_nova (
    id INTEGER PRIMARY KEY,
    plano INTEGER NOT NULL REFERENCES plano_delegacao (id),
    faz TEXT NOT NULL DEFAULT '',
    toca_em TEXT
        CONSTRAINT tarefa_toca_em_lista CHECK (json_valid(toca_em) AND json_type(toca_em) = 'array'),
    paralelo INTEGER NOT NULL
        CONSTRAINT tarefa_paralelo_booleano CHECK (paralelo IN (0, 1)),
    pronto_quando TEXT NOT NULL DEFAULT '',
    prova TEXT NOT NULL DEFAULT '',
    incompleta INTEGER NOT NULL DEFAULT 0
        CONSTRAINT tarefa_incompleta_booleano CHECK (incompleta IN (0, 1)),
    papel TEXT NOT NULL
        CONSTRAINT tarefa_papel_valores CHECK (papel IN ('funcionalidade', 'teste', 'adversarial', 'pesquisa')),
    modelo TEXT
        CONSTRAINT tarefa_modelo_vazio CHECK (length(trim(modelo, ' ' || char(9, 10, 13))) > 0),
    voltas INTEGER NOT NULL DEFAULT 0
        CONSTRAINT tarefa_voltas_faixa CHECK (voltas BETWEEN 0 AND 2),
    implementada_por INTEGER REFERENCES execucao (id),
    testada_por INTEGER REFERENCES execucao (id),
    -- ERR-49: only a task marked incomplete may have these fields empty (or faz above 500).
    CONSTRAINT tarefa_faz_tamanho CHECK (incompleta = 1 OR length(faz) BETWEEN 1 AND 500),
    CONSTRAINT tarefa_faz_vazio CHECK (incompleta = 1 OR length(trim(faz, ' ' || char(9, 10, 13))) > 0),
    CONSTRAINT tarefa_pronto_quando_vazio CHECK (
        incompleta = 1 OR length(trim(pronto_quando, ' ' || char(9, 10, 13))) > 0
    ),
    CONSTRAINT tarefa_prova_vazio CHECK (incompleta = 1 OR length(trim(prova, ' ' || char(9, 10, 13))) > 0),
    CONSTRAINT tarefa_toca_em_funcionalidade CHECK (
        papel <> 'funcionalidade' OR (toca_em IS NOT NULL AND json_array_length(toca_em) >= 1)
    ),
    CONSTRAINT tarefa_testada_por_diferente CHECK (testada_por IS NOT implementada_por OR testada_por IS NULL)
) STRICT;

INSERT INTO tarefa_nova (
    id, plano, faz, toca_em, paralelo, pronto_quando, prova, papel, modelo, voltas,
    implementada_por, testada_por
)
SELECT
    id, NULL, faz, toca_em, paralelo, pronto_quando, prova, papel, modelo, voltas,
    implementada_por, testada_por
FROM tarefa;

DROP TABLE tarefa;
ALTER TABLE tarefa_nova RENAME TO tarefa;

-- ENT-07 Execucao
CREATE TABLE execucao_nova (
    id INTEGER PRIMARY KEY,
    chat INTEGER NOT NULL REFERENCES chat (id),
    tarefa INTEGER REFERENCES tarefa (id),
    papel TEXT NOT NULL
        CONSTRAINT execucao_papel_valores CHECK (
            papel IN ('orquestrador', 'funcionalidade', 'teste', 'adversarial', 'pesquisa')
        ),
    modelo TEXT NOT NULL
        CONSTRAINT execucao_modelo_vazio CHECK (length(trim(modelo, ' ' || char(9, 10, 13))) > 0),
    prompt_versao INTEGER NOT NULL
        CONSTRAINT execucao_prompt_versao_minimo CHECK (prompt_versao >= 1),
    worktree TEXT
        CONSTRAINT execucao_worktree_vazio CHECK (length(trim(worktree, ' ' || char(9, 10, 13))) > 0),
    branch TEXT
        CONSTRAINT execucao_branch_formato CHECK (
            branch GLOB '[a-z]*/?*-?*' AND branch NOT GLOB '*[ ' || char(9, 10, 13) || ']*'
        ),
    status TEXT NOT NULL
        CONSTRAINT execucao_status_valores CHECK (
            status IN ('na fila', 'rodando', 'concluído', 'falhou', 'interrompida')
        ),
    motivo_fila TEXT
        CONSTRAINT execucao_motivo_fila_valores CHECK (motivo_fila IN ('teto', 'toca_em', 'recurso')),
    comandos TEXT
        CONSTRAINT execucao_comandos_lista CHECK (json_valid(comandos) AND json_type(comandos) = 'array'),
    CONSTRAINT execucao_tarefa_obrigatoria CHECK (papel = 'orquestrador' OR tarefa IS NOT NULL),
    CONSTRAINT execucao_worktree_funcionalidade CHECK (
        papel <> 'funcionalidade' OR (worktree IS NOT NULL AND branch IS NOT NULL)
    ),
    CONSTRAINT execucao_motivo_fila_na_fila CHECK (status <> 'na fila' OR motivo_fila IS NOT NULL)
) STRICT;

INSERT INTO execucao_nova (
    id, chat, tarefa, papel, modelo, prompt_versao, worktree, branch, status, motivo_fila
)
SELECT
    id, chat, tarefa, papel, modelo, prompt_versao, worktree, branch,
    CASE status WHEN 'concluido' THEN 'concluído' ELSE status END,
    motivo_fila
FROM execucao;

DROP TABLE execucao;
ALTER TABLE execucao_nova RENAME TO execucao;

-- ENT-09 Relatorio (dual write)
CREATE TABLE relatorio_novo (
    id INTEGER PRIMARY KEY,
    execucao INTEGER NOT NULL REFERENCES execucao (id),
    resultado TEXT NOT NULL
        CONSTRAINT relatorio_resultado_valores CHECK (resultado IN ('concluído', 'falhou', 'bloqueado')),
    o_que_fez TEXT NOT NULL
        CONSTRAINT relatorio_o_que_fez_vazio CHECK (length(trim(o_que_fez, ' ' || char(9, 10, 13))) > 0),
    arquivos TEXT
        CONSTRAINT relatorio_arquivos_lista CHECK (json_valid(arquivos) AND json_type(arquivos) = 'array'),
    prova TEXT NOT NULL
        CONSTRAINT relatorio_prova_vazio CHECK (length(trim(prova, ' ' || char(9, 10, 13))) > 0),
    duvidas TEXT
        CONSTRAINT relatorio_duvidas_lista CHECK (json_valid(duvidas) AND json_type(duvidas) = 'array'),
    candidatos_a_nota TEXT
        CONSTRAINT relatorio_candidatos_a_nota_lista CHECK (
            json_valid(candidatos_a_nota) AND json_type(candidatos_a_nota) = 'array'
        ),
    registrado_em TEXT
        CONSTRAINT relatorio_registrado_em_utc CHECK (
            registrado_em IS strftime('%Y-%m-%dT%H:%M:%SZ', registrado_em)
            OR registrado_em IS strftime('%Y-%m-%dT%H:%M:%fZ', registrado_em)
        )
) STRICT;

INSERT INTO relatorio_novo (
    id, execucao, resultado, o_que_fez, arquivos, prova, duvidas, candidatos_a_nota, registrado_em
)
SELECT id, NULL, resultado, o_que_fez, arquivos, prova, duvidas, candidatos_a_nota, registrado_em
FROM relatorio;

DROP TABLE relatorio;
ALTER TABLE relatorio_novo RENAME TO relatorio;

-- ENT-10 Achado (dual write)
CREATE TABLE achado_novo (
    id INTEGER PRIMARY KEY,
    execucao INTEGER NOT NULL REFERENCES execucao (id),
    tipo TEXT NOT NULL
        CONSTRAINT achado_tipo_valores CHECK (tipo IN ('bug', 'sugestao')),
    passos TEXT NOT NULL
        CONSTRAINT achado_passos_lista CHECK (
            json_valid(passos) AND json_type(passos) = 'array' AND json_array_length(passos) >= 1
        ),
    esperado TEXT NOT NULL
        CONSTRAINT achado_esperado_vazio CHECK (length(trim(esperado, ' ' || char(9, 10, 13))) > 0),
    obtido TEXT NOT NULL
        CONSTRAINT achado_obtido_vazio CHECK (length(trim(obtido, ' ' || char(9, 10, 13))) > 0),
    captura TEXT
        CONSTRAINT achado_captura_vazio CHECK (length(trim(captura, ' ' || char(9, 10, 13))) > 0),
    decisao TEXT
        CONSTRAINT achado_decisao_valores CHECK (decisao IN ('corrigir', 'limite', 'opcional')),
    status TEXT NOT NULL
        CONSTRAINT achado_status_valores CHECK (status IN ('aberto', 'fechado')),
    CONSTRAINT achado_bug_corrigir CHECK (tipo <> 'bug' OR decisao IS 'corrigir')
) STRICT;

INSERT INTO achado_novo (id, execucao, tipo, passos, esperado, obtido, captura, decisao, status)
SELECT id, NULL, tipo, passos, esperado, obtido, captura, decisao, status
FROM achado;

DROP TABLE achado;
ALTER TABLE achado_novo RENAME TO achado;

-- ENT-17 Escrita
CREATE TABLE escrita_nova (
    id INTEGER PRIMARY KEY,
    cofre INTEGER NOT NULL REFERENCES cofre (id),
    caminho TEXT NOT NULL
        CONSTRAINT escrita_caminho_vazio CHECK (length(trim(caminho, ' ' || char(9, 10, 13))) > 0),
    motivo TEXT NOT NULL
        CONSTRAINT escrita_motivo_vazio CHECK (length(trim(motivo, ' ' || char(9, 10, 13))) > 0),
    hora TEXT NOT NULL
        CONSTRAINT escrita_hora_utc CHECK (
            hora IS strftime('%Y-%m-%dT%H:%M:%SZ', hora) OR hora IS strftime('%Y-%m-%dT%H:%M:%fZ', hora)
        )
) STRICT;

INSERT INTO escrita_nova (id, cofre, caminho, motivo, hora)
SELECT id, NULL, caminho, motivo, hora
FROM escrita;

DROP TABLE escrita;
ALTER TABLE escrita_nova RENAME TO escrita;

-- Triggers of the rebuilt tables (dropped with them), unchanged from 0002.

CREATE TRIGGER tarefa_toca_em_relativo_insert BEFORE INSERT ON tarefa
WHEN json_valid(NEW.toca_em) AND json_type(NEW.toca_em) = 'array' AND EXISTS (
    SELECT 1 FROM json_each(NEW.toca_em)
    WHERE type <> 'text'
        OR length(trim(value, ' ' || char(9, 10, 13))) = 0
        OR value GLOB '[A-Za-z]:*'
        OR value GLOB '[\/]*'
)
BEGIN
    SELECT RAISE(ABORT, 'tarefa_toca_em_relativo');
END;

CREATE TRIGGER tarefa_toca_em_relativo_update BEFORE UPDATE OF toca_em ON tarefa
WHEN json_valid(NEW.toca_em) AND json_type(NEW.toca_em) = 'array' AND EXISTS (
    SELECT 1 FROM json_each(NEW.toca_em)
    WHERE type <> 'text'
        OR length(trim(value, ' ' || char(9, 10, 13))) = 0
        OR value GLOB '[A-Za-z]:*'
        OR value GLOB '[\/]*'
)
BEGIN
    SELECT RAISE(ABORT, 'tarefa_toca_em_relativo');
END;

CREATE TRIGGER execucao_prompt_versao_imutavel BEFORE UPDATE OF prompt_versao ON execucao
WHEN NEW.prompt_versao IS NOT OLD.prompt_versao
BEGIN
    SELECT RAISE(ABORT, 'execucao_prompt_versao_imutavel');
END;

CREATE TRIGGER relatorio_listas_texto_insert BEFORE INSERT ON relatorio
WHEN EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.arquivos) THEN NEW.arquivos END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
) OR EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.duvidas) THEN NEW.duvidas END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
) OR EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.candidatos_a_nota) THEN NEW.candidatos_a_nota END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'relatorio_listas_texto');
END;

CREATE TRIGGER relatorio_listas_texto_update BEFORE UPDATE OF arquivos, duvidas, candidatos_a_nota ON relatorio
WHEN EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.arquivos) THEN NEW.arquivos END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
) OR EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.duvidas) THEN NEW.duvidas END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
) OR EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.candidatos_a_nota) THEN NEW.candidatos_a_nota END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'relatorio_listas_texto');
END;

CREATE TRIGGER achado_passos_texto_insert BEFORE INSERT ON achado
WHEN EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.passos) THEN NEW.passos END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'achado_passos_texto');
END;

CREATE TRIGGER achado_passos_texto_update BEFORE UPDATE OF passos ON achado
WHEN EXISTS (
    SELECT 1 FROM json_each(CASE WHEN json_valid(NEW.passos) THEN NEW.passos END)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'achado_passos_texto');
END;

-- M-39 / ERR-49: "Aprovar" is blocked while the plan has a task with incompleta = 1, and an
-- approved plan never gets an incomplete task.
CREATE TRIGGER plano_delegacao_aprovado_sem_incompleta BEFORE UPDATE OF status ON plano_delegacao
WHEN NEW.status = 'aprovado'
    AND EXISTS (SELECT 1 FROM tarefa WHERE plano = NEW.id AND incompleta = 1)
BEGIN
    SELECT RAISE(ABORT, 'plano_delegacao_aprovado_sem_incompleta');
END;

CREATE TRIGGER tarefa_incompleta_plano_aprovado_insert BEFORE INSERT ON tarefa
WHEN NEW.incompleta = 1
    AND (SELECT status FROM plano_delegacao WHERE id = NEW.plano) IS 'aprovado'
BEGIN
    SELECT RAISE(ABORT, 'plano_delegacao_aprovado_sem_incompleta');
END;

CREATE TRIGGER tarefa_incompleta_plano_aprovado_update BEFORE UPDATE OF incompleta, plano ON tarefa
WHEN NEW.incompleta = 1
    AND (SELECT status FROM plano_delegacao WHERE id = NEW.plano) IS 'aprovado'
BEGIN
    SELECT RAISE(ABORT, 'plano_delegacao_aprovado_sem_incompleta');
END;

-- RN-17 / ERR-14: comandos lists the commands the agent ran (the core denies push, merge and
-- push --force before they run): non-blank command lines, none of them a git push or merge
-- (`push --force` included) nor `gh pr merge`. Command lines are compared in lower case, with
-- tabs, line breaks and repeated spaces as one space, and the program allowed with a path or `.exe`.
-- Only the subcommand right after the program is matched (`git -C x push` is not): a commit message
-- with the word "push" or "merge" must still be recorded. The core gate is the real barrier (RN-27).
CREATE TRIGGER execucao_comandos_texto_insert BEFORE INSERT ON execucao
WHEN json_valid(NEW.comandos) AND json_type(NEW.comandos) = 'array' AND EXISTS (
    SELECT 1 FROM json_each(NEW.comandos)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'execucao_comandos_texto');
END;

CREATE TRIGGER execucao_comandos_texto_update BEFORE UPDATE OF comandos ON execucao
WHEN json_valid(NEW.comandos) AND json_type(NEW.comandos) = 'array' AND EXISTS (
    SELECT 1 FROM json_each(NEW.comandos)
    WHERE type <> 'text' OR length(trim(value, ' ' || char(9, 10, 13))) = 0
)
BEGIN
    SELECT RAISE(ABORT, 'execucao_comandos_texto');
END;

CREATE TRIGGER execucao_comandos_git_negado_insert BEFORE INSERT ON execucao
WHEN json_valid(NEW.comandos) AND json_type(NEW.comandos) = 'array' AND EXISTS (
    SELECT 1
    FROM (
        SELECT ' ' || replace(replace(replace(
            lower(replace(replace(replace(value, char(9), ' '), char(10), ' '), char(13), ' ')),
            '  ', ' '), '  ', ' '), '  ', ' ') || ' ' AS linha
        FROM json_each(NEW.comandos)
        WHERE type = 'text'
    )
    WHERE linha GLOB '*[ /\]git push *'
        OR linha GLOB '*[ /\]git merge *'
        OR linha GLOB '*[ /\]git.exe push *'
        OR linha GLOB '*[ /\]git.exe merge *'
        OR linha GLOB '*[ /\]gh pr merge *'
        OR linha GLOB '*[ /\]gh.exe pr merge *'
)
BEGIN
    SELECT RAISE(ABORT, 'execucao_comandos_git_negado');
END;

CREATE TRIGGER execucao_comandos_git_negado_update BEFORE UPDATE OF comandos ON execucao
WHEN json_valid(NEW.comandos) AND json_type(NEW.comandos) = 'array' AND EXISTS (
    SELECT 1
    FROM (
        SELECT ' ' || replace(replace(replace(
            lower(replace(replace(replace(value, char(9), ' '), char(10), ' '), char(13), ' ')),
            '  ', ' '), '  ', ' '), '  ', ' ') || ' ' AS linha
        FROM json_each(NEW.comandos)
        WHERE type = 'text'
    )
    WHERE linha GLOB '*[ /\]git push *'
        OR linha GLOB '*[ /\]git merge *'
        OR linha GLOB '*[ /\]git.exe push *'
        OR linha GLOB '*[ /\]git.exe merge *'
        OR linha GLOB '*[ /\]gh pr merge *'
        OR linha GLOB '*[ /\]gh.exe pr merge *'
)
BEGIN
    SELECT RAISE(ABORT, 'execucao_comandos_git_negado');
END;

-- RF-31 / RN-28: `INSERT OR REPLACE` (and an upsert) on an existing row and `UPDATE OR REPLACE`
-- of the key delete rows without firing the DELETE trigger. Any insert that collides with an
-- existing row and any change of id is refused, so no conflict clause can replace a row.
CREATE TRIGGER execucao_sem_substituicao_insert BEFORE INSERT ON execucao
WHEN EXISTS (SELECT 1 FROM execucao WHERE id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'execucao_sem_substituicao');
END;

CREATE TRIGGER execucao_sem_substituicao_update BEFORE UPDATE OF id ON execucao
WHEN NEW.id IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'execucao_sem_substituicao');
END;

DROP TRIGGER prompt_papel_versao_imutavel;

CREATE TRIGGER prompt_papel_versao_imutavel BEFORE UPDATE OF id, fluxo, texto, versao, palavras, tokens ON prompt_papel
WHEN NEW.id IS NOT OLD.id
    OR NEW.fluxo IS NOT OLD.fluxo
    OR NEW.texto IS NOT OLD.texto
    OR NEW.versao IS NOT OLD.versao
    OR NEW.palavras IS NOT OLD.palavras
    OR NEW.tokens IS NOT OLD.tokens
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_versao_imutavel');
END;

CREATE TRIGGER prompt_papel_sem_substituicao_insert BEFORE INSERT ON prompt_papel
WHEN EXISTS (
    SELECT 1 FROM prompt_papel WHERE id = NEW.id OR (fluxo = NEW.fluxo AND versao = NEW.versao)
)
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_versao_imutavel');
END;

-- The partial unique index on vigente would let `OR REPLACE` delete the current version.
CREATE TRIGGER prompt_papel_uma_vigente_insert BEFORE INSERT ON prompt_papel
WHEN NEW.vigente = 1
    AND EXISTS (SELECT 1 FROM prompt_papel WHERE fluxo = NEW.fluxo AND vigente = 1)
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_uma_vigente_por_fluxo');
END;

CREATE TRIGGER prompt_papel_uma_vigente_update BEFORE UPDATE OF vigente ON prompt_papel
WHEN NEW.vigente = 1
    AND EXISTS (SELECT 1 FROM prompt_papel WHERE fluxo = NEW.fluxo AND vigente = 1 AND id IS NOT OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_uma_vigente_por_fluxo');
END;

-- M-37: achado.captura required when the run is adversarial.
CREATE TRIGGER achado_captura_adversarial_insert BEFORE INSERT ON achado
WHEN NEW.captura IS NULL
    AND (SELECT papel FROM execucao WHERE id = NEW.execucao) IS 'adversarial'
BEGIN
    SELECT RAISE(ABORT, 'achado_captura_adversarial');
END;

CREATE TRIGGER achado_captura_adversarial_update BEFORE UPDATE OF captura, execucao ON achado
WHEN NEW.captura IS NULL
    AND (SELECT papel FROM execucao WHERE id = NEW.execucao) IS 'adversarial'
BEGIN
    SELECT RAISE(ABORT, 'achado_captura_adversarial');
END;

-- M-37 / RN-29 / ERR-39: the orchestrator writes only to the memory vault.
CREATE TRIGGER escrita_cofre_memoria_insert BEFORE INSERT ON escrita
WHEN (SELECT funcao FROM cofre WHERE id = NEW.cofre) IS NOT 'memoria'
    AND EXISTS (SELECT 1 FROM cofre WHERE id = NEW.cofre)
BEGIN
    SELECT RAISE(ABORT, 'escrita_cofre_memoria');
END;

CREATE TRIGGER escrita_cofre_memoria_update BEFORE UPDATE OF cofre ON escrita
WHEN (SELECT funcao FROM cofre WHERE id = NEW.cofre) IS NOT 'memoria'
    AND EXISTS (SELECT 1 FROM cofre WHERE id = NEW.cofre)
BEGIN
    SELECT RAISE(ABORT, 'escrita_cofre_memoria');
END;

-- No reference left dangling by the rebuild: a violation fails this CHECK and the commit never runs.
CREATE TEMP TABLE migracao_0003_fk (
    violacoes INTEGER NOT NULL CONSTRAINT migracao_0003_fk_sem_violacao CHECK (violacoes = 0)
);
INSERT INTO migracao_0003_fk (violacoes) SELECT COUNT(*) FROM pragma_foreign_key_check;
DROP TABLE migracao_0003_fk;

COMMIT;

PRAGMA foreign_keys = ON;
