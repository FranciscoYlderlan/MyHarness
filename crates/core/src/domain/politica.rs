//! Política de ferramentas por papel, aplicada pelo `harness-gate` a cada tentativa de
//! ferramenta (camada 3 do [`crate::application::chamada_claude`]; RN-27, RNF-06).
//!
//! **Nega por padrão:** só passa a ferramenta listada para o papel e, dentro dela, só a
//! entrada que respeita as regras fixas:
//! - todo papel lê o Obsidian pela Obsidian CLI via MCP; **só o orquestrador escreve**
//!   (`obsidian_create`, `obsidian_append`), e só no cofre de memória (decisão
//!   `geral-decisao-2026-11-27-cofre-do-app`; RN-01, RN-29);
//! - nenhum papel lê os `.md` do cofre direto, salvo a exceção autorizada (RN-02);
//! - nenhum subagente escreve em cofre, em `Anexos/contrato/` ou em `docs/contrato/` (RN-01);
//! - nenhum agente faz push, merge, pull ou usa o `gh` (RN-17); só o implementador commita;
//! - escrita de arquivo só dentro das pastas da execução (worktree e `--add-dir`).
//!
//! O texto do prompt de papel não muda nada disto (RN-27). Tudo aqui é puro: caminhos são
//! comparados pelo texto normalizado, sem ir ao disco.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Prefixo das ferramentas do servidor MCP da Obsidian CLI.
pub const MCP_OBSIDIAN: &str = "mcp__obsidian-cli__";
/// Prefixo das ferramentas do servidor MCP do Claude in Chrome.
pub const MCP_CHROME: &str = "mcp__claude-in-chrome__";

/// Ferramentas de leitura da Obsidian CLI, liberadas a todo papel.
pub const OBSIDIAN_LEITURA: [&str; 6] = [
    "obsidian_search",
    "obsidian_read",
    "obsidian_backlinks",
    "obsidian_help",
    "obsidian_files",
    "obsidian_vaults",
];
/// Ferramentas de escrita da Obsidian CLI, só do orquestrador.
pub const OBSIDIAN_ESCRITA: [&str; 2] = ["obsidian_create", "obsidian_append"];

/// Papel de um agente no MyHarness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Papel {
    /// Sessão principal: lê, planeja e registra no cofre de memória.
    Orquestrador,
    /// Responde uma pergunta com fontes; não altera arquivos.
    Pesquisador,
    /// Entrega uma tarefa na própria worktree e commita.
    Implementador,
    /// Prova cada critério e lê o diff; não corrige.
    Verificador,
    /// Acha problemas da sua especialidade; não corrige.
    Revisor,
    /// Roda os AC dos fluxos principais, na API e na tela.
    VerificadorFluxoPrincipal,
    /// Tenta quebrar a funcionalidade pela tela.
    Adversarial,
}

impl Papel {
    /// Todos os papéis, na ordem do manual.
    pub const TODOS: [Papel; 7] = [
        Papel::Orquestrador,
        Papel::Pesquisador,
        Papel::Implementador,
        Papel::Verificador,
        Papel::Revisor,
        Papel::VerificadorFluxoPrincipal,
        Papel::Adversarial,
    ];

    /// Nome canônico do papel.
    pub fn nome(self) -> &'static str {
        match self {
            Papel::Orquestrador => "orquestrador",
            Papel::Pesquisador => "pesquisador",
            Papel::Implementador => "implementador",
            Papel::Verificador => "verificador",
            Papel::Revisor => "revisor",
            Papel::VerificadorFluxoPrincipal => "verificador-fluxo-principal",
            Papel::Adversarial => "adversarial",
        }
    }

    /// Papel pelo nome canônico ou pelo nome do contrato (ENT-15: `pesquisa`,
    /// `funcionalidade`, `teste`). Nome desconhecido: `None` (e o portão nega tudo).
    pub fn do_nome(nome: &str) -> Option<Papel> {
        let nome = nome.trim().to_ascii_lowercase();
        let papel = match nome.as_str() {
            "pesquisa" => Papel::Pesquisador,
            "funcionalidade" => Papel::Implementador,
            "teste" => Papel::Verificador,
            _ => return Papel::TODOS.into_iter().find(|p| p.nome() == nome),
        };
        Some(papel)
    }

    /// Ferramentas internas da CLI liberadas ao papel. As de MCP seguem
    /// [`Papel::usa_chrome`] e as listas da Obsidian CLI.
    pub fn ferramentas_internas(self) -> &'static [&'static str] {
        match self {
            Papel::Orquestrador => &["Read", "Glob", "Grep", "TodoWrite"],
            Papel::Pesquisador => &["Read", "Glob", "Grep", "TodoWrite", "WebSearch", "WebFetch"],
            Papel::Implementador => &[
                "Read",
                "Glob",
                "Grep",
                "TodoWrite",
                "WebSearch",
                "WebFetch",
                "Write",
                "Edit",
                "MultiEdit",
                "NotebookEdit",
                "Bash",
                "BashOutput",
                "KillShell",
            ],
            Papel::Verificador | Papel::Revisor | Papel::VerificadorFluxoPrincipal => &[
                "Read",
                "Glob",
                "Grep",
                "TodoWrite",
                "Bash",
                "BashOutput",
                "KillShell",
            ],
            Papel::Adversarial => &["TodoWrite"],
        }
    }

    /// O papel usa o navegador do Claude in Chrome.
    pub fn usa_chrome(self) -> bool {
        matches!(self, Papel::VerificadorFluxoPrincipal | Papel::Adversarial)
    }

    /// O papel pode commitar (só o implementador, na própria worktree; RN-16).
    pub fn pode_commitar(self) -> bool {
        self == Papel::Implementador
    }
}

impl fmt::Display for Papel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nome())
    }
}

/// Cofre ligado ao projeto, como o portão o vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cofre {
    /// Nome do cofre no Obsidian (o `vault` das ferramentas da CLI).
    pub nome: String,
    /// Pasta do cofre, absoluta.
    pub caminho: PathBuf,
    /// `true` no cofre de memória (o único em que o orquestrador grava, RN-29).
    pub memoria: bool,
}

/// O que o portão sabe da execução, além do papel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contexto {
    /// Cofres ligados ao projeto.
    pub cofres: Vec<Cofre>,
    /// Pastas em que o agente pode escrever (worktree e `--add-dir`), absolutas.
    pub pastas_de_escrita: Vec<PathBuf>,
    /// Exceção do RN-02 autorizada pelo Humano: leitura direta do `.md` do cofre.
    pub leitura_direta_autorizada: bool,
}

/// Uma tentativa de ferramenta, como chega no gancho `PreToolUse`.
#[derive(Debug, Clone, Copy)]
pub struct Tentativa<'a> {
    /// Nome da ferramenta (`Bash`, `mcp__obsidian-cli__obsidian_read`).
    pub ferramenta: &'a str,
    /// Entrada da ferramenta (`tool_input`).
    pub entrada: &'a Value,
    /// Pasta atual do agente (`cwd`), para resolver caminho relativo.
    pub pasta_atual: Option<&'a Path>,
}

/// Por que a tentativa foi negada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    /// Papel ausente ou desconhecido: nada passa.
    PapelDesconhecido,
    /// Ferramenta que o papel não tem.
    FerramentaForaDoPapel,
    /// Escrita em cofre por quem não é o orquestrador (RN-01).
    EscritaEmCofre,
    /// Escrita em `Anexos/contrato/` ou `docs/contrato/` por quem não é o orquestrador (RN-01, D14).
    EscritaNoContrato,
    /// Escrita do orquestrador num cofre que não é o de memória (RN-29).
    EscritaEmCofreDeConsulta,
    /// Leitura do `.md` do cofre sem a CLI e sem a exceção autorizada (RN-02).
    LeituraDiretaDoCofre,
    /// `git push`, `merge`, `pull`, `gh` ou comando git fora do papel (RN-17).
    ComandoGitNegado,
    /// Escrita fora das pastas da execução.
    EscritaForaDasPastas,
    /// Entrada que o portão não sabe ler (sem caminho, tipo errado).
    EntradaInvalida,
}

impl Motivo {
    /// ERR do contrato (`05-erros.md`), quando há um.
    pub fn erro(self) -> Option<(&'static str, &'static str)> {
        match self {
            Motivo::EscritaEmCofre | Motivo::EscritaNoContrato => {
                Some(("ERR-13", "ESCRITA_NEGADA_PAPEL"))
            }
            Motivo::ComandoGitNegado => Some(("ERR-14", "COMANDO_GIT_NEGADO")),
            Motivo::LeituraDiretaDoCofre => Some(("ERR-18", "LEITURA_DIRETA_BLOQUEADA")),
            Motivo::EscritaEmCofreDeConsulta => Some(("ERR-39", "ESCRITA_EM_COFRE_DE_CONSULTA")),
            Motivo::PapelDesconhecido
            | Motivo::FerramentaForaDoPapel
            | Motivo::EscritaForaDasPastas
            | Motivo::EntradaInvalida => None,
        }
    }

    /// Texto curto da negação.
    pub fn texto(self) -> &'static str {
        match self {
            Motivo::PapelDesconhecido => "papel desconhecido: tudo negado",
            Motivo::FerramentaForaDoPapel => "ferramenta fora do papel",
            Motivo::EscritaEmCofre => "escrita negada: papel sem permissão",
            Motivo::EscritaNoContrato => "escrita negada: papel sem permissão (contrato)",
            Motivo::EscritaEmCofreDeConsulta => "escrita negada: cofre de consulta",
            Motivo::LeituraDiretaDoCofre => "leitura direta do cofre bloqueada: use a Obsidian CLI",
            Motivo::ComandoGitNegado => "comando negado: push, merge e GitHub são do núcleo",
            Motivo::EscritaForaDasPastas => "escrita fora das pastas da execução",
            Motivo::EntradaInvalida => "entrada da ferramenta inválida",
        }
    }
}

impl fmt::Display for Motivo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.erro() {
            Some((id, codigo)) => write!(f, "{id} {codigo}: {}", self.texto()),
            None => f.write_str(self.texto()),
        }
    }
}

/// Resultado do portão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    /// Passa (as outras camadas de permissão ainda valem).
    Permitir,
    /// Negada, com o motivo.
    Negar(Motivo),
}

/// Decide uma tentativa. `papel` `None` (ausente ou desconhecido) nega tudo.
pub fn decidir(papel: Option<Papel>, contexto: &Contexto, tentativa: &Tentativa<'_>) -> Decisao {
    match avaliar(papel, contexto, tentativa) {
        Ok(()) => Decisao::Permitir,
        Err(m) => Decisao::Negar(m),
    }
}

fn avaliar(papel: Option<Papel>, ctx: &Contexto, t: &Tentativa<'_>) -> Result<(), Motivo> {
    let papel = papel.ok_or(Motivo::PapelDesconhecido)?;
    let nome = t.ferramenta;

    if let Some(ferramenta) = nome.strip_prefix(MCP_OBSIDIAN) {
        return obsidian(papel, ctx, ferramenta, t.entrada);
    }
    if nome.starts_with(MCP_CHROME) && nome.len() > MCP_CHROME.len() {
        return if papel.usa_chrome() {
            Ok(())
        } else {
            Err(Motivo::FerramentaForaDoPapel)
        };
    }
    if !papel.ferramentas_internas().contains(&nome) {
        return Err(Motivo::FerramentaForaDoPapel);
    }
    let entrada = t.entrada.as_object().ok_or(Motivo::EntradaInvalida)?;
    let texto = |chave: &str| entrada.get(chave).and_then(Value::as_str);

    match nome {
        "Read" => ler(ctx, texto("file_path").ok_or(Motivo::EntradaInvalida)?, t),
        "Glob" | "Grep" => {
            ler(ctx, texto("path").unwrap_or(""), t)?;
            match texto("pattern") {
                Some(p) if nome == "Glob" && Path::new(p).is_absolute() => ler(ctx, p, t),
                _ => Ok(()),
            }
        }
        "Write" | "Edit" | "MultiEdit" => {
            escrever(ctx, texto("file_path").ok_or(Motivo::EntradaInvalida)?, t)
        }
        "NotebookEdit" => escrever(
            ctx,
            texto("notebook_path").ok_or(Motivo::EntradaInvalida)?,
            t,
        ),
        "Bash" => comando(papel, ctx, texto("command").ok_or(Motivo::EntradaInvalida)?),
        _ => Ok(()),
    }
}

fn obsidian(papel: Papel, ctx: &Contexto, ferramenta: &str, entrada: &Value) -> Result<(), Motivo> {
    if OBSIDIAN_LEITURA.contains(&ferramenta) {
        return Ok(());
    }
    if !OBSIDIAN_ESCRITA.contains(&ferramenta) {
        return Err(Motivo::FerramentaForaDoPapel);
    }
    if papel != Papel::Orquestrador {
        return Err(Motivo::EscritaEmCofre);
    }
    // Sem `vault` explícito, a CLI escolheria o cofre sozinha: nega (RN-29).
    let alvo = entrada.get("vault").and_then(Value::as_str).map(str::trim);
    let memoria = ctx.cofres.iter().find(|c| c.memoria);
    match (alvo, memoria) {
        (Some(alvo), Some(m)) if !alvo.is_empty() && alvo == m.nome => Ok(()),
        _ => Err(Motivo::EscritaEmCofreDeConsulta),
    }
}

fn ler(ctx: &Contexto, caminho: &str, t: &Tentativa<'_>) -> Result<(), Motivo> {
    let alvo = resolver(caminho, t.pasta_atual).ok_or(Motivo::EntradaInvalida)?;
    if dentro_de_cofre(ctx, &alvo) && !ctx.leitura_direta_autorizada {
        return Err(Motivo::LeituraDiretaDoCofre);
    }
    Ok(())
}

fn escrever(ctx: &Contexto, caminho: &str, t: &Tentativa<'_>) -> Result<(), Motivo> {
    if caminho.trim().is_empty() {
        return Err(Motivo::EntradaInvalida);
    }
    let alvo = resolver(caminho, t.pasta_atual).ok_or(Motivo::EntradaInvalida)?;
    if dentro_de_cofre(ctx, &alvo) {
        return Err(Motivo::EscritaEmCofre);
    }
    if e_do_contrato(&alvo) {
        return Err(Motivo::EscritaNoContrato);
    }
    if ctx
        .pastas_de_escrita
        .iter()
        .any(|p| dentro(&alvo, &normalizar(p)))
    {
        Ok(())
    } else {
        Err(Motivo::EscritaForaDasPastas)
    }
}

/// Caminho absoluto e normalizado (sem `.` nem `..`), em minúsculas e com `/`.
/// Relativo sem pasta atual: `None`. Vazio vira a pasta atual.
fn resolver(caminho: &str, pasta_atual: Option<&Path>) -> Option<String> {
    let caminho = Path::new(caminho);
    if caminho.is_absolute() || tem_raiz(caminho) {
        Some(normalizar(caminho))
    } else {
        pasta_atual
            .filter(|p| p.is_absolute() || tem_raiz(p))
            .map(|p| normalizar(&p.join(caminho)))
    }
}

/// `C:\…` e `\\servidor\…` contam como absolutos em qualquer SO (o app roda no Windows).
fn tem_raiz(p: &Path) -> bool {
    let t = p.to_string_lossy();
    let b = t.as_bytes();
    t.starts_with('/')
        || t.starts_with('\\')
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
}

/// Normaliza só pelo texto: separador `/`, minúsculas (o Windows não diferencia), sem
/// `.`, `..` resolvido e sem `/` no fim.
fn normalizar(p: &Path) -> String {
    let texto = p.to_string_lossy().replace('\\', "/").to_lowercase();
    let mut partes: Vec<&str> = Vec::new();
    for parte in texto.split('/') {
        match parte {
            "" | "." => {}
            ".." => {
                partes.pop();
            }
            _ => partes.push(parte),
        }
    }
    format!("/{}", partes.join("/"))
}

fn dentro(alvo: &str, raiz: &str) -> bool {
    raiz != "/" && (alvo == raiz || alvo.starts_with(&format!("{raiz}/")))
}

fn dentro_de_cofre(ctx: &Contexto, alvo: &str) -> bool {
    ctx.cofres
        .iter()
        .any(|c| dentro(alvo, &normalizar(&c.caminho)))
}

fn e_do_contrato(alvo: &str) -> bool {
    let partes: Vec<&str> = alvo.split('/').collect();
    partes
        .windows(2)
        .any(|j| j[1] == "contrato" && (j[0] == "docs" || j[0] == "anexos"))
}

/// Subcomandos git negados a todo agente: o núcleo faz push e PR, o Humano faz o merge.
const GIT_NEGADO: [&str; 6] = [
    "push",
    "merge",
    "pull",
    "send-pack",
    "worktree",
    "request-pull",
];
/// Subcomandos git que mudam o histórico: só o implementador.
const GIT_SO_IMPLEMENTADOR: [&str; 8] = [
    "commit",
    "rebase",
    "cherry-pick",
    "revert",
    "am",
    "tag",
    "reset",
    "stash",
];
/// Opções globais do git que levam valor no item seguinte.
/// O comando já chega em minúsculas: `-C` aparece como `-c`.
const GIT_OPCAO_COM_VALOR: [&str; 5] = [
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
];

fn comando(papel: Papel, ctx: &Contexto, comando: &str) -> Result<(), Motivo> {
    // Aspas saem (`gi"t"` vira `git`); separadores do shell viram espaço, então
    // `sh -c "git push"` e `$(git push)` também aparecem. A `\` fica: num caminho do
    // Windows ela separa pastas; no resto, é ignorada na comparação (`pu\sh` é `push`).
    let limpo: String = comando
        .chars()
        .filter(|c| !matches!(c, '"' | '\''))
        .map(|c| {
            if matches!(
                c,
                ';' | '&' | '|' | '(' | ')' | '<' | '>' | '`' | '{' | '}' | '$'
            ) || c.is_whitespace()
            {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .to_lowercase();
    let itens: Vec<&str> = limpo.split(' ').filter(|i| !i.is_empty()).collect();

    let itens: Vec<String> = itens.iter().map(|i| i.to_string()).collect();
    for (i, item) in itens.iter().enumerate() {
        // Duas leituras do nome do programa: `C:\x\gh.exe` (a `\` separa pastas) e
        // `gi\t` (a `\` só escapa).
        let separado = item.rsplit(['/', '\\']).next().unwrap_or(item).to_owned();
        let escapado = sem_barra_invertida(item)
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        for base in [separado, escapado] {
            let base = base.trim_end_matches(".exe");
            if base == "gh" || base == "hub" {
                return Err(Motivo::ComandoGitNegado);
            }
            if let Some(sub) = base.strip_prefix("git-") {
                git_subcomando(papel, sub)?;
            } else if base == "git" {
                let resto: Vec<String> = itens[i + 1..]
                    .iter()
                    .map(|r| sem_barra_invertida(r))
                    .collect();
                let resto: Vec<&str> = resto.iter().map(String::as_str).collect();
                git(papel, &resto)?;
            }
        }
    }

    // Leitura ou escrita do cofre pelo shell: nunca (RN-01, RN-02).
    let caminhos = limpo.replace('\\', "/");
    let cita_cofre = ctx.cofres.iter().any(|c| {
        let raiz = normalizar(&c.caminho);
        let sem_barra = raiz.trim_start_matches('/');
        raiz != "/" && caminhos.contains(sem_barra)
    });
    if cita_cofre {
        return Err(if papel == Papel::Orquestrador {
            Motivo::LeituraDiretaDoCofre
        } else {
            Motivo::EscritaEmCofre
        });
    }
    if papel != Papel::Orquestrador
        && (caminhos.contains("docs/contrato") || caminhos.contains("anexos/contrato"))
    {
        return Err(Motivo::EscritaNoContrato);
    }
    Ok(())
}

fn sem_barra_invertida(item: &str) -> String {
    item.replace('\\', "")
}

fn git(papel: Papel, resto: &[&str]) -> Result<(), Motivo> {
    let mut i = 0;
    while let Some(item) = resto.get(i) {
        let com_valor = GIT_OPCAO_COM_VALOR.contains(item);
        let valor = if com_valor {
            resto.get(i + 1).copied()
        } else {
            item.strip_prefix("-c").filter(|v| v.contains('='))
        };
        // `git -c alias.x=push x` é push disfarçado.
        if item.starts_with("-c") && valor.is_some_and(|v| v.starts_with("alias.")) {
            return Err(Motivo::ComandoGitNegado);
        }
        if com_valor {
            i += 2;
        } else if item.starts_with('-') {
            i += 1;
        } else {
            git_subcomando(papel, item)?;
            if *item == "config" && resto[i + 1..].iter().any(|a| a.starts_with("alias.")) {
                return Err(Motivo::ComandoGitNegado);
            }
            return Ok(());
        }
    }
    Ok(())
}

fn git_subcomando(papel: Papel, sub: &str) -> Result<(), Motivo> {
    if GIT_NEGADO.contains(&sub) || (!papel.pode_commitar() && GIT_SO_IMPLEMENTADOR.contains(&sub))
    {
        Err(Motivo::ComandoGitNegado)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> Contexto {
        Contexto {
            cofres: vec![
                Cofre {
                    nome: "memoria".into(),
                    caminho: "/home/u/cofres/memoria".into(),
                    memoria: true,
                },
                Cofre {
                    nome: "Vault".into(),
                    caminho: "/home/u/cofres/Vault".into(),
                    memoria: false,
                },
            ],
            pastas_de_escrita: vec!["/home/u/wt".into()],
            leitura_direta_autorizada: false,
        }
    }

    fn d(papel: Papel, ferramenta: &str, entrada: Value) -> Decisao {
        let t = Tentativa {
            ferramenta,
            entrada: &entrada,
            pasta_atual: Some(Path::new("/home/u/wt")),
        };
        decidir(Some(papel), &ctx(), &t)
    }

    fn bash(papel: Papel, c: &str) -> Decisao {
        d(papel, "Bash", json!({ "command": c }))
    }

    #[test]
    fn nomes_dos_papeis_vao_e_voltam() {
        for p in Papel::TODOS {
            assert_eq!(Papel::do_nome(p.nome()), Some(p));
        }
        assert_eq!(
            Papel::do_nome(" Funcionalidade "),
            Some(Papel::Implementador)
        );
        assert_eq!(Papel::do_nome("teste"), Some(Papel::Verificador));
        assert_eq!(Papel::do_nome("pesquisa"), Some(Papel::Pesquisador));
        assert_eq!(Papel::do_nome("root"), None);
        assert_eq!(Papel::do_nome(""), None);
    }

    #[test]
    fn sem_papel_nada_passa() {
        let e = json!({ "file_path": "/home/u/wt/a.rs" });
        let t = Tentativa {
            ferramenta: "Read",
            entrada: &e,
            pasta_atual: None,
        };
        assert_eq!(
            decidir(None, &ctx(), &t),
            Decisao::Negar(Motivo::PapelDesconhecido)
        );
    }

    #[test]
    fn ferramenta_desconhecida_e_negada_a_todos() {
        for p in Papel::TODOS {
            for f in [
                "Task",
                "Skill",
                "SlashCommand",
                "mcp__outro__x",
                "mcp__claude-in-chrome__",
                "read",
            ] {
                assert_eq!(
                    d(p, f, json!({})),
                    Decisao::Negar(Motivo::FerramentaForaDoPapel),
                    "{p} {f}"
                );
            }
            assert_eq!(
                d(p, "mcp__obsidian-cli__obsidian_delete", json!({})),
                Decisao::Negar(Motivo::FerramentaForaDoPapel)
            );
        }
    }

    #[test]
    fn git_disfarcado_e_negado() {
        for c in [
            "git push",
            "GIT PUSH origin main",
            "/usr/bin/git -C /x push",
            "git -c core.x=1 --no-pager push --force",
            "git.exe merge main",
            "gi\"t\" pu\\sh",
            "sh -c 'git push'",
            "echo $(git push)",
            "cargo test && git push",
            "git-push origin",
            "git -c alias.p=push p",
            "git config alias.p push",
            "git pull",
            "gh pr merge 1",
            "C:\\tools\\gh.exe api repos",
            "git worktree add ../x",
        ] {
            assert_eq!(
                bash(Papel::Implementador, c),
                Decisao::Negar(Motivo::ComandoGitNegado),
                "{c}"
            );
        }
        for c in [
            "git status",
            "git -C /home/u/wt diff",
            "git commit -m 'push it'",
            "echo pushing",
        ] {
            assert_eq!(bash(Papel::Implementador, c), Decisao::Permitir, "{c}");
        }
        assert_eq!(
            bash(Papel::Verificador, "git commit -m x"),
            Decisao::Negar(Motivo::ComandoGitNegado)
        );
        assert_eq!(
            bash(Papel::Revisor, "git diff main...HEAD"),
            Decisao::Permitir
        );
    }

    #[test]
    fn caminhos_com_ponto_ponto_e_windows() {
        let e = |p: &str| d(Papel::Implementador, "Write", json!({ "file_path": p }));
        assert_eq!(e("src/a.rs"), Decisao::Permitir);
        assert_eq!(
            e("/home/u/wt/../cofres/memoria/n.md"),
            Decisao::Negar(Motivo::EscritaEmCofre)
        );
        assert_eq!(
            e("../cofres/vault/n.md"),
            Decisao::Negar(Motivo::EscritaEmCofre)
        );
        assert_eq!(
            e("/home/u/wt2/a.rs"),
            Decisao::Negar(Motivo::EscritaForaDasPastas)
        );
        assert_eq!(
            e("docs/contrato/00.md"),
            Decisao::Negar(Motivo::EscritaNoContrato)
        );
        assert_eq!(e(""), Decisao::Negar(Motivo::EntradaInvalida));
        let win = Contexto {
            cofres: vec![Cofre {
                nome: "m".into(),
                caminho: "C:\\Users\\u\\Cofre".into(),
                memoria: true,
            }],
            pastas_de_escrita: vec!["C:\\wt".into()],
            leitura_direta_autorizada: false,
        };
        let entrada = json!({ "file_path": "c:/users/U/cofre/x.md" });
        let t = Tentativa {
            ferramenta: "Edit",
            entrada: &entrada,
            pasta_atual: None,
        };
        assert_eq!(
            decidir(Some(Papel::Implementador), &win, &t),
            Decisao::Negar(Motivo::EscritaEmCofre)
        );
    }

    #[test]
    fn excecao_do_rn02_libera_so_a_leitura() {
        let mut c = ctx();
        c.leitura_direta_autorizada = true;
        let ler = json!({ "file_path": "/home/u/cofres/memoria/n.md" });
        let t = Tentativa {
            ferramenta: "Read",
            entrada: &ler,
            pasta_atual: None,
        };
        assert_eq!(decidir(Some(Papel::Pesquisador), &c, &t), Decisao::Permitir);
        assert_eq!(
            decidir(Some(Papel::Pesquisador), &ctx(), &t),
            Decisao::Negar(Motivo::LeituraDiretaDoCofre)
        );
        let t = Tentativa {
            ferramenta: "Write",
            ..t
        };
        assert_eq!(
            decidir(Some(Papel::Implementador), &c, &t),
            Decisao::Negar(Motivo::EscritaEmCofre)
        );
    }

    #[test]
    fn orquestrador_so_grava_no_cofre_de_memoria() {
        let o = |e: Value| d(Papel::Orquestrador, "mcp__obsidian-cli__obsidian_create", e);
        assert_eq!(
            o(json!({ "vault": "memoria", "path": "a.md" })),
            Decisao::Permitir
        );
        assert_eq!(
            o(json!({ "vault": "Vault", "path": "a.md" })),
            Decisao::Negar(Motivo::EscritaEmCofreDeConsulta)
        );
        assert_eq!(
            o(json!({ "path": "a.md" })),
            Decisao::Negar(Motivo::EscritaEmCofreDeConsulta)
        );
    }

    #[test]
    fn motivos_trazem_o_err_do_contrato() {
        assert_eq!(
            Motivo::EscritaEmCofre.to_string(),
            "ERR-13 ESCRITA_NEGADA_PAPEL: escrita negada: papel sem permissão"
        );
        assert_eq!(
            Motivo::ComandoGitNegado.erro(),
            Some(("ERR-14", "COMANDO_GIT_NEGADO"))
        );
        assert_eq!(Motivo::FerramentaForaDoPapel.erro(), None);
    }
}
