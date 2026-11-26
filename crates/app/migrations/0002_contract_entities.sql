-- Contract entities (Anexos/contrato/02-entidades.md, ENT-01 to ENT-26, with M-23 onwards).
--
-- Conventions:
-- * One STRICT table per entity that lives in SQLite. ENT-15 Papel lives in the core code and
--   ENT-25 Diario only in the vault, so they have no table. ENT-13 credencial never comes here (RN-04).
-- * Table and column names are the contract field names; enum values are the contract values, verbatim.
-- * Every CHECK is named `<table>_<field>_<rule>`, so the error says what is wrong.
-- * "data e hora": ISO 8601 text in UTC with `Z` (`2026-11-19T18:20:00Z`, milliseconds allowed).
-- * "obrigatorio" text: NOT NULL and not blank (spaces, tabs and line breaks only count as blank).
-- * "booleano": INTEGER 0 or 1. "lista": JSON array; items are checked by triggers.
-- * Dual write (RN-31, ERR-51): plano_delegacao, tarefa, relatorio, achado and decisao are also
--   written to the vault in the same core operation (see `infra::db::dual_write`).

-- ENT-01 Projeto
CREATE TABLE projeto (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL UNIQUE
        CONSTRAINT projeto_nome_tamanho CHECK (length(nome) BETWEEN 1 AND 80)
        CONSTRAINT projeto_nome_vazio CHECK (length(trim(nome, ' ' || char(9, 10, 13))) > 0),
    repositorio TEXT
        CONSTRAINT projeto_repositorio_absoluto CHECK (
            repositorio GLOB '[A-Za-z]:[\/]*' OR repositorio GLOB '\\?*' OR repositorio GLOB '/*'
        ),
    limite_contexto_padrao INTEGER NOT NULL DEFAULT 100000
        CONSTRAINT projeto_limite_contexto_padrao_minimo CHECK (limite_contexto_padrao >= 1)
) STRICT;

-- ENT-02 Cofre
CREATE TABLE cofre (
    id INTEGER PRIMARY KEY,
    projeto INTEGER NOT NULL REFERENCES projeto (id),
    nome TEXT NOT NULL
        CONSTRAINT cofre_nome_vazio CHECK (length(trim(nome, ' ' || char(9, 10, 13))) > 0),
    funcao TEXT NOT NULL
        CONSTRAINT cofre_funcao_valores CHECK (funcao IN ('memoria', 'consulta')),
    ativo INTEGER NOT NULL
        CONSTRAINT cofre_ativo_booleano CHECK (ativo IN (0, 1)),
    status TEXT NOT NULL
        CONSTRAINT cofre_status_valores CHECK (status IN ('conectado', 'desconectado'))
) STRICT;

-- RN-29: one memory vault per project (at most one here; "at least one" is a core rule, ERR-38).
CREATE UNIQUE INDEX cofre_uma_memoria_por_projeto ON cofre (projeto) WHERE funcao = 'memoria';

-- ENT-03 Chat
CREATE TABLE chat (
    id INTEGER PRIMARY KEY,
    projeto INTEGER NOT NULL REFERENCES projeto (id),
    limite_tokens INTEGER NOT NULL
        CONSTRAINT chat_limite_tokens_minimo CHECK (limite_tokens >= 1),
    tokens_usados INTEGER NOT NULL DEFAULT 0
        CONSTRAINT chat_tokens_usados_minimo CHECK (tokens_usados >= 0),
    chat_anterior INTEGER REFERENCES chat (id)
        CONSTRAINT chat_chat_anterior_proprio CHECK (chat_anterior IS NOT id),
    status TEXT NOT NULL
        CONSTRAINT chat_status_valores CHECK (status IN ('ativo', 'pausado', 'encerrado', 'só leitura')),
    continuacoes INTEGER NOT NULL DEFAULT 0
        CONSTRAINT chat_continuacoes_minimo CHECK (continuacoes >= 0)
) STRICT;

-- chat_anterior: only a chat closed by a shift handover (status encerrado, UC-08).
CREATE TRIGGER chat_chat_anterior_encerrado_insert BEFORE INSERT ON chat
WHEN NEW.chat_anterior IS NOT NULL
    AND (SELECT status FROM chat WHERE id = NEW.chat_anterior) IS NOT 'encerrado'
BEGIN
    SELECT RAISE(ABORT, 'chat_chat_anterior_encerrado');
END;

CREATE TRIGGER chat_chat_anterior_encerrado_update BEFORE UPDATE OF chat_anterior ON chat
WHEN NEW.chat_anterior IS NOT NULL
    AND (SELECT status FROM chat WHERE id = NEW.chat_anterior) IS NOT 'encerrado'
BEGIN
    SELECT RAISE(ABORT, 'chat_chat_anterior_encerrado');
END;

-- ENT-04 Mensagem
CREATE TABLE mensagem (
    id INTEGER PRIMARY KEY,
    chat INTEGER NOT NULL REFERENCES chat (id),
    autor TEXT NOT NULL
        CONSTRAINT mensagem_autor_valores CHECK (autor IN ('Humano', 'orquestrador', 'sistema')),
    texto TEXT NOT NULL
        CONSTRAINT mensagem_texto_vazio CHECK (length(trim(texto, ' ' || char(9, 10, 13))) > 0),
    uso_tokens INTEGER
        CONSTRAINT mensagem_uso_tokens_minimo CHECK (uso_tokens >= 0),
    CONSTRAINT mensagem_uso_tokens_orquestrador CHECK (autor <> 'orquestrador' OR uso_tokens IS NOT NULL)
) STRICT;

-- ENT-05 PlanoDelegacao (dual write)
CREATE TABLE plano_delegacao (
    id INTEGER PRIMARY KEY,
    chat INTEGER NOT NULL REFERENCES chat (id),
    versao INTEGER NOT NULL
        CONSTRAINT plano_delegacao_versao_minimo CHECK (versao >= 1),
    status TEXT NOT NULL
        CONSTRAINT plano_delegacao_status_valores CHECK (status IN ('proposto', 'aprovado', 'recusado')),
    aprovado_em TEXT
        CONSTRAINT plano_delegacao_aprovado_em_utc CHECK (
            aprovado_em IS strftime('%Y-%m-%dT%H:%M:%SZ', aprovado_em)
            OR aprovado_em IS strftime('%Y-%m-%dT%H:%M:%fZ', aprovado_em)
        ),
    devolucoes INTEGER NOT NULL DEFAULT 0
        CONSTRAINT plano_delegacao_devolucoes_faixa CHECK (devolucoes BETWEEN 0 AND 2),
    CONSTRAINT plano_delegacao_aprovado_em_so_se_aprovado CHECK (
        (status = 'aprovado') = (aprovado_em IS NOT NULL)
    )
) STRICT;

-- ENT-06 Tarefa (dual write)
CREATE TABLE tarefa (
    id INTEGER PRIMARY KEY,
    faz TEXT NOT NULL
        CONSTRAINT tarefa_faz_tamanho CHECK (length(faz) BETWEEN 1 AND 500)
        CONSTRAINT tarefa_faz_vazio CHECK (length(trim(faz, ' ' || char(9, 10, 13))) > 0),
    toca_em TEXT
        CONSTRAINT tarefa_toca_em_lista CHECK (json_valid(toca_em) AND json_type(toca_em) = 'array'),
    paralelo INTEGER NOT NULL
        CONSTRAINT tarefa_paralelo_booleano CHECK (paralelo IN (0, 1)),
    pronto_quando TEXT NOT NULL
        CONSTRAINT tarefa_pronto_quando_vazio CHECK (length(trim(pronto_quando, ' ' || char(9, 10, 13))) > 0),
    prova TEXT NOT NULL
        CONSTRAINT tarefa_prova_vazio CHECK (length(trim(prova, ' ' || char(9, 10, 13))) > 0),
    papel TEXT NOT NULL
        CONSTRAINT tarefa_papel_valores CHECK (papel IN ('funcionalidade', 'teste', 'adversarial', 'pesquisa')),
    modelo TEXT
        CONSTRAINT tarefa_modelo_vazio CHECK (length(trim(modelo, ' ' || char(9, 10, 13))) > 0),
    voltas INTEGER NOT NULL DEFAULT 0
        CONSTRAINT tarefa_voltas_faixa CHECK (voltas BETWEEN 0 AND 2),
    implementada_por INTEGER REFERENCES execucao (id),
    testada_por INTEGER REFERENCES execucao (id),
    CONSTRAINT tarefa_toca_em_funcionalidade CHECK (
        papel <> 'funcionalidade' OR (toca_em IS NOT NULL AND json_array_length(toca_em) >= 1)
    ),
    CONSTRAINT tarefa_testada_por_diferente CHECK (testada_por IS NOT implementada_por OR testada_por IS NULL)
) STRICT;

-- toca_em: each item is a non-blank path relative to the repository.
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

-- ENT-06 depende_de: list of references to other tasks.
CREATE TABLE tarefa_dependencia (
    tarefa INTEGER NOT NULL REFERENCES tarefa (id),
    depende_de INTEGER NOT NULL REFERENCES tarefa (id),
    PRIMARY KEY (tarefa, depende_de),
    CONSTRAINT tarefa_dependencia_propria CHECK (tarefa <> depende_de)
) STRICT;

-- ENT-07 Execucao
CREATE TABLE execucao (
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
            status IN ('na fila', 'rodando', 'concluido', 'falhou', 'interrompida')
        ),
    motivo_fila TEXT
        CONSTRAINT execucao_motivo_fila_valores CHECK (motivo_fila IN ('teto', 'toca_em', 'recurso')),
    CONSTRAINT execucao_tarefa_obrigatoria CHECK (papel = 'orquestrador' OR tarefa IS NOT NULL),
    CONSTRAINT execucao_worktree_funcionalidade CHECK (
        papel <> 'funcionalidade' OR (worktree IS NOT NULL AND branch IS NOT NULL)
    ),
    CONSTRAINT execucao_motivo_fila_na_fila CHECK (status <> 'na fila' OR motivo_fila IS NOT NULL)
) STRICT;

-- RN-28: prompt_versao does not change during the run.
CREATE TRIGGER execucao_prompt_versao_imutavel BEFORE UPDATE OF prompt_versao ON execucao
WHEN NEW.prompt_versao IS NOT OLD.prompt_versao
BEGIN
    SELECT RAISE(ABORT, 'execucao_prompt_versao_imutavel');
END;

-- ENT-08 PromptPapel
CREATE TABLE prompt_papel (
    id INTEGER PRIMARY KEY,
    fluxo TEXT NOT NULL
        CONSTRAINT prompt_papel_fluxo_valores CHECK (
            fluxo IN ('orquestrador', 'funcionalidade', 'teste', 'adversarial', 'passagem de turno')
        ),
    texto TEXT NOT NULL
        CONSTRAINT prompt_papel_texto_vazio CHECK (length(trim(texto, ' ' || char(9, 10, 13))) > 0),
    versao INTEGER NOT NULL
        CONSTRAINT prompt_papel_versao_minimo CHECK (versao >= 1),
    palavras INTEGER NOT NULL
        CONSTRAINT prompt_papel_palavras_faixa CHECK (palavras BETWEEN 1 AND 80),
    tokens INTEGER NOT NULL
        CONSTRAINT prompt_papel_tokens_minimo CHECK (tokens >= 0),
    vigente INTEGER NOT NULL
        CONSTRAINT prompt_papel_vigente_booleano CHECK (vigente IN (0, 1)),
    UNIQUE (fluxo, versao)
) STRICT;

-- One current version per flow.
CREATE UNIQUE INDEX prompt_papel_uma_vigente_por_fluxo ON prompt_papel (fluxo) WHERE vigente = 1;

-- RF-31: saving creates a new version; the previous one stays in the history, unchanged.
CREATE TRIGGER prompt_papel_versao_imutavel BEFORE UPDATE OF fluxo, texto, versao, palavras, tokens ON prompt_papel
WHEN NEW.fluxo IS NOT OLD.fluxo
    OR NEW.texto IS NOT OLD.texto
    OR NEW.versao IS NOT OLD.versao
    OR NEW.palavras IS NOT OLD.palavras
    OR NEW.tokens IS NOT OLD.tokens
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_versao_imutavel');
END;

CREATE TRIGGER prompt_papel_historico_delete BEFORE DELETE ON prompt_papel
BEGIN
    SELECT RAISE(ABORT, 'prompt_papel_versao_imutavel');
END;

-- ENT-09 Relatorio (dual write)
CREATE TABLE relatorio (
    id INTEGER PRIMARY KEY,
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

-- arquivos, duvidas, candidatos_a_nota: every item is non-blank text.
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

-- ENT-10 Achado (dual write)
CREATE TABLE achado (
    id INTEGER PRIMARY KEY,
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

-- passos: every step is non-blank text.
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

-- ENT-11 Leitura
CREATE TABLE leitura (
    id INTEGER PRIMARY KEY,
    execucao INTEGER NOT NULL REFERENCES execucao (id),
    caminho TEXT NOT NULL
        CONSTRAINT leitura_caminho_vazio CHECK (length(trim(caminho, ' ' || char(9, 10, 13))) > 0),
    cofre INTEGER REFERENCES cofre (id),
    origem TEXT NOT NULL
        CONSTRAINT leitura_origem_valores CHECK (origem IN ('cli', 'arquivo')),
    hora TEXT NOT NULL
        CONSTRAINT leitura_hora_utc CHECK (
            hora IS strftime('%Y-%m-%dT%H:%M:%SZ', hora) OR hora IS strftime('%Y-%m-%dT%H:%M:%fZ', hora)
        ),
    CONSTRAINT leitura_cofre_cli CHECK (origem <> 'cli' OR cofre IS NOT NULL)
) STRICT;

-- ENT-12 Log
CREATE TABLE log (
    id INTEGER PRIMARY KEY,
    execucao INTEGER NOT NULL REFERENCES execucao (id),
    comando TEXT NOT NULL
        CONSTRAINT log_comando_vazio CHECK (length(trim(comando, ' ' || char(9, 10, 13))) > 0),
    inicio TEXT NOT NULL
        CONSTRAINT log_inicio_utc CHECK (
            inicio IS strftime('%Y-%m-%dT%H:%M:%SZ', inicio) OR inicio IS strftime('%Y-%m-%dT%H:%M:%fZ', inicio)
        ),
    fim TEXT
        CONSTRAINT log_fim_utc CHECK (
            fim IS strftime('%Y-%m-%dT%H:%M:%SZ', fim) OR fim IS strftime('%Y-%m-%dT%H:%M:%fZ', fim)
        ),
    codigo_saida INTEGER,
    saida_mascarada TEXT,
    CONSTRAINT log_fim_depois_do_inicio CHECK (fim IS NULL OR julianday(fim) >= julianday(inicio))
) STRICT;

-- ENT-13 Integracao (status only; the credential lives in the Windows Credential Manager, D12)
CREATE TABLE integracao (
    id INTEGER PRIMARY KEY,
    tipo TEXT NOT NULL
        CONSTRAINT integracao_tipo_valores CHECK (tipo IN ('Claude CLI', 'Obsidian CLI', 'GitHub')),
    status TEXT NOT NULL
        CONSTRAINT integracao_status_valores CHECK (
            status IN ('conectado', 'desconectado', 'sem login', 'não instalado')
        ),
    conta TEXT,
    versao TEXT
) STRICT;

-- ENT-14 Consumo: only the last reading is kept (D7), so the table holds one row.
CREATE TABLE consumo (
    id INTEGER PRIMARY KEY
        CONSTRAINT consumo_ultima_leitura CHECK (id = 1),
    semanal_pct REAL NOT NULL
        CONSTRAINT consumo_semanal_pct_faixa CHECK (semanal_pct BETWEEN 0 AND 100),
    janela_pct REAL NOT NULL
        CONSTRAINT consumo_janela_pct_faixa CHECK (janela_pct BETWEEN 0 AND 100),
    proximo_reinicio TEXT NOT NULL
        CONSTRAINT consumo_proximo_reinicio_utc CHECK (
            proximo_reinicio IS strftime('%Y-%m-%dT%H:%M:%SZ', proximo_reinicio)
            OR proximo_reinicio IS strftime('%Y-%m-%dT%H:%M:%fZ', proximo_reinicio)
        ),
    semanal_reinicio TEXT NOT NULL
        CONSTRAINT consumo_semanal_reinicio_utc CHECK (
            semanal_reinicio IS strftime('%Y-%m-%dT%H:%M:%SZ', semanal_reinicio)
            OR semanal_reinicio IS strftime('%Y-%m-%dT%H:%M:%fZ', semanal_reinicio)
        ),
    lida_em TEXT NOT NULL
        CONSTRAINT consumo_lida_em_utc CHECK (
            lida_em IS strftime('%Y-%m-%dT%H:%M:%SZ', lida_em) OR lida_em IS strftime('%Y-%m-%dT%H:%M:%fZ', lida_em)
        ),
    origem TEXT NOT NULL
        CONSTRAINT consumo_origem_valores CHECK (origem IN ('evento', 'estimado')),
    janela_status TEXT NOT NULL
        CONSTRAINT consumo_janela_status_valores CHECK (
            janela_status IN ('allowed', 'allowed_warning', 'rejected', 'desconhecido')
        ),
    semanal_status TEXT NOT NULL
        CONSTRAINT consumo_semanal_status_valores CHECK (
            semanal_status IN ('allowed', 'allowed_warning', 'rejected', 'desconhecido')
        ),
    excedente TEXT
) STRICT;

-- ENT-16 Recurso: one row per resource, so one run at a time uses it (RN-15).
CREATE TABLE recurso (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL UNIQUE
        CONSTRAINT recurso_nome_valores CHECK (nome IN ('navegador do Claude in Chrome', 'banco de teste')),
    em_uso_por INTEGER REFERENCES execucao (id)
) STRICT;

-- ENT-17 Escrita
CREATE TABLE escrita (
    id INTEGER PRIMARY KEY,
    caminho TEXT NOT NULL
        CONSTRAINT escrita_caminho_vazio CHECK (length(trim(caminho, ' ' || char(9, 10, 13))) > 0),
    motivo TEXT NOT NULL
        CONSTRAINT escrita_motivo_vazio CHECK (length(trim(motivo, ' ' || char(9, 10, 13))) > 0),
    hora TEXT NOT NULL
        CONSTRAINT escrita_hora_utc CHECK (
            hora IS strftime('%Y-%m-%dT%H:%M:%SZ', hora) OR hora IS strftime('%Y-%m-%dT%H:%M:%fZ', hora)
        )
) STRICT;

-- ENT-18 Decisao: the note lives in the vault; SQLite keeps the approval and the record time (dual write).
CREATE TABLE decisao (
    id INTEGER PRIMARY KEY,
    aval_id TEXT NOT NULL
        CONSTRAINT decisao_aval_id_vazio CHECK (length(trim(aval_id, ' ' || char(9, 10, 13))) > 0),
    registrada_em TEXT
        CONSTRAINT decisao_registrada_em_utc CHECK (
            registrada_em IS strftime('%Y-%m-%dT%H:%M:%SZ', registrada_em)
            OR registrada_em IS strftime('%Y-%m-%dT%H:%M:%fZ', registrada_em)
        )
) STRICT;

-- ENT-19 Briefing
CREATE TABLE briefing (
    id INTEGER PRIMARY KEY,
    conteudo TEXT NOT NULL
        CONSTRAINT briefing_conteudo_vazio CHECK (length(trim(conteudo, ' ' || char(9, 10, 13))) > 0)
) STRICT;

-- ENT-20 Rodada ("em andamento" is still [a confirmar] in the contract)
CREATE TABLE rodada (
    id INTEGER PRIMARY KEY,
    status TEXT NOT NULL
        CONSTRAINT rodada_status_valores CHECK (
            status IN ('em andamento', 'aprovada', 'falhou', 'aguardando decisão')
        )
) STRICT;

-- ENT-21 Funcionalidade
CREATE TABLE funcionalidade (
    id INTEGER PRIMARY KEY,
    status TEXT NOT NULL
        CONSTRAINT funcionalidade_status_valores CHECK (status IN ('em-verificacao', 'concluida', 'bloqueada'))
) STRICT;

-- ENT-22 Configuracao: app settings, one row, created with the defaults (teto_escrita 5, S8).
CREATE TABLE configuracao (
    id INTEGER PRIMARY KEY
        CONSTRAINT configuracao_unica CHECK (id = 1),
    teto_escrita INTEGER NOT NULL DEFAULT 5
        CONSTRAINT configuracao_teto_escrita_minimo CHECK (teto_escrita >= 1)
) STRICT;

INSERT INTO configuracao (id) VALUES (1);

-- ENT-23 PR
CREATE TABLE pr (
    id INTEGER PRIMARY KEY,
    status TEXT NOT NULL
        CONSTRAINT pr_status_valores CHECK (status IN ('aberto', 'merged'))
) STRICT;

-- ENT-24 Criterio: item of Relatorio.criterios (ENT-09).
CREATE TABLE criterio (
    id INTEGER PRIMARY KEY,
    relatorio INTEGER NOT NULL REFERENCES relatorio (id),
    status TEXT NOT NULL
        CONSTRAINT criterio_status_valores CHECK (status IN ('✔', '✘', 'não verificado, presumido')),
    prova TEXT
        CONSTRAINT criterio_prova_vazio CHECK (length(trim(prova, ' ' || char(9, 10, 13))) > 0),
    CONSTRAINT criterio_prova_para_ok CHECK (status <> '✔' OR prova IS NOT NULL)
) STRICT;

-- ENT-26 PastaContexto
CREATE TABLE pasta_contexto (
    id INTEGER PRIMARY KEY,
    projeto INTEGER NOT NULL REFERENCES projeto (id),
    caminho TEXT NOT NULL
        CONSTRAINT pasta_contexto_caminho_absoluto CHECK (
            caminho GLOB '[A-Za-z]:[\/]*' OR caminho GLOB '\\?*' OR caminho GLOB '/*'
        )
) STRICT;
