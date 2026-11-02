//! Montagem da chamada ao `claude -p` de um agente (decisão
//! `geral-decisao-2026-11-19-agente-por-claude-p`; arquitetura, seção 4).
//!
//! Só monta: argumentos (cada um como item separado, nada de string para shell) e ambiente.
//! Abrir o processo e ler o fluxo JSON é do `AgentRunner` da infraestrutura, que deve
//! limpar o ambiente herdado e usar só [`ChamadaClaude::ambiente`].
//!
//! As quatro camadas de contenção viram argumentos assim:
//! 1. ferramentas por papel: `--tools` (nomes das ferramentas internas do papel),
//!    `--allowedTools` (as do papel, com regra) e `--disallowedTools` (nome sozinho);
//! 2. negar por padrão: `--permission-mode dontAsk`;
//! 3. portão: `--settings` com o gancho `PreToolUse` que chama o `harness-gate`;
//! 4. ambiente sem token do GitHub ([`ambiente_sem_token_do_github`]).
//!
//! Também: `--setting-sources ""` (nenhuma configuração pessoal), `--mcp-config` com
//! `--strict-mcp-config` quando houver MCP, `--add-dir` por pasta extra e
//! `--append-system-prompt` com o prompt do papel (RN-25). O prompt da tarefa vem por
//! último, depois de `--`, para nunca ser lido como opção.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use crate::domain::execucao::{
    AmbienteMascarado, CampoCaminho, ErroMontagem, EspecificacaoExecucao, MotivoCaminho,
    ambiente_sem_token_do_github, ferramenta_valida, nome_da_ferramenta,
};
use crate::domain::segredo::TextoMascarado;

/// Nome do executável da CLI, quando o runner não recebe outro caminho.
pub const PROGRAMA_PADRAO: &str = "claude";

/// Chamada pronta: argumentos, na ordem, e o ambiente completo do processo.
/// `Debug` mascara os argumentos e esconde os valores do ambiente (RN-04).
#[derive(Clone, PartialEq, Eq)]
pub struct ChamadaClaude {
    argumentos: Vec<String>,
    ambiente: Vec<(OsString, OsString)>,
}

impl ChamadaClaude {
    /// Argumentos da CLI, sem o nome do programa.
    pub fn argumentos(&self) -> &[String] {
        &self.argumentos
    }

    /// Ambiente completo do processo, já sem token do GitHub. Quem abre o processo limpa o
    /// ambiente herdado antes (`env_clear`) e passa só este.
    pub fn ambiente(&self) -> &[(OsString, OsString)] {
        &self.ambiente
    }

    /// Argumentos e ambiente, por valor.
    pub fn em_partes(self) -> (Vec<String>, Vec<(OsString, OsString)>) {
        (self.argumentos, self.ambiente)
    }
}

impl fmt::Debug for ChamadaClaude {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let argumentos: Vec<_> = self
            .argumentos
            .iter()
            .map(|a| TextoMascarado::new(a))
            .collect();
        f.debug_struct("ChamadaClaude")
            .field("argumentos", &argumentos)
            .field("ambiente", &AmbienteMascarado(&self.ambiente))
            .finish()
    }
}

/// Monta a chamada de `especificacao`, com o ambiente do processo pai (ex.:
/// `std::env::vars_os()`) já sem token do GitHub. Valor vazio ou inválido vira
/// [`ErroMontagem`].
pub fn montar_chamada<I>(
    especificacao: &EspecificacaoExecucao,
    ambiente_do_pai: I,
) -> Result<ChamadaClaude, ErroMontagem>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let e = especificacao;
    texto_obrigatorio(&e.papel, "papel", ErroMontagem::PapelVazio)?;
    texto_obrigatorio(
        &e.prompt_do_papel,
        "prompt do papel",
        ErroMontagem::PromptDoPapelVazio,
    )?;
    texto_obrigatorio(
        &e.prompt_da_tarefa,
        "prompt da tarefa",
        ErroMontagem::PromptDaTarefaVazio,
    )?;
    validar_modelo(&e.modelo)?;
    validar_ferramentas(&e.ferramentas_permitidas, &e.ferramentas_proibidas)?;
    let pastas = validar_pastas(&e.pastas_extras)?;
    let portao = caminho(&e.portao, CampoCaminho::Portao)?;
    let mcp = e
        .mcp_config
        .as_deref()
        .map(|c| caminho(c, CampoCaminho::McpConfig))
        .transpose()?;

    let mut a: Vec<String> = Vec::new();

    a.push("-p".into());
    a.push("--verbose".into());
    par(&mut a, "--output-format", "stream-json");
    par(&mut a, "--model", &e.modelo);
    par(&mut a, "--permission-mode", "dontAsk");
    par(&mut a, "--setting-sources", "");
    par(&mut a, "--settings", &configuracao_do_portao(portao));
    par(
        &mut a,
        "--tools",
        &ferramentas_internas(&e.ferramentas_permitidas),
    );
    lista(&mut a, "--allowedTools", &e.ferramentas_permitidas);
    lista(&mut a, "--disallowedTools", &e.ferramentas_proibidas);
    lista(&mut a, "--add-dir", &pastas);
    if let Some(mcp) = mcp {
        par(&mut a, "--mcp-config", mcp);
        a.push("--strict-mcp-config".into());
    }
    par(&mut a, "--append-system-prompt", &e.prompt_do_papel);
    a.push("--".into());
    a.push(e.prompt_da_tarefa.clone());

    Ok(ChamadaClaude {
        argumentos: a,
        ambiente: ambiente_sem_token_do_github(ambiente_do_pai),
    })
}

/// `--opcao valor`, como dois itens.
fn par(a: &mut Vec<String>, opcao: &str, valor: &str) {
    a.push(opcao.to_owned());
    a.push(valor.to_owned());
}

/// `--opcao item1 item2 …`, cada item separado; nada quando a lista é vazia.
fn lista<S: AsRef<str>>(a: &mut Vec<String>, opcao: &str, itens: &[S]) {
    if !itens.is_empty() {
        a.push(opcao.to_owned());
        a.extend(itens.iter().map(|i| i.as_ref().to_owned()));
    }
}

/// JSON do `--settings`: gancho `PreToolUse` para toda ferramenta, chamando o portão.
/// O caminho vai entre aspas duplas, porque a CLI roda o gancho por um shell; os
/// caracteres que o shell interpretaria já foram recusados em [`caminho`].
fn configuracao_do_portao(portao: &str) -> String {
    serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{ "type": "command", "command": format!("\"{portao}\"") }]
            }]
        }
    })
    .to_string()
}

/// Valor de `--tools`: nomes das ferramentas internas permitidas, sem regra, sem repetição
/// e na ordem em que aparecem; as de MCP (`mcp__…`) ficam só no `--allowedTools`.
/// Lista vazia (`""`) desliga todas as ferramentas internas.
fn ferramentas_internas(permitidas: &[String]) -> String {
    let mut vistas = HashSet::new();
    permitidas
        .iter()
        .map(|f| nome_da_ferramenta(f))
        .filter(|n| !n.starts_with("mcp__") && vistas.insert(*n))
        .collect::<Vec<_>>()
        .join(",")
}

fn texto_obrigatorio(
    texto: &str,
    campo: &'static str,
    vazio: ErroMontagem,
) -> Result<(), ErroMontagem> {
    if texto.trim().is_empty() {
        Err(vazio)
    } else if texto.contains('\0') {
        Err(ErroMontagem::CaractereNulo { campo })
    } else {
        Ok(())
    }
}

fn validar_modelo(modelo: &str) -> Result<(), ErroMontagem> {
    if modelo.trim().is_empty() {
        return Err(ErroMontagem::ModeloVazio);
    }
    let invalido =
        modelo.starts_with('-') || modelo.chars().any(|c| c.is_whitespace() || c.is_control());
    if invalido {
        Err(ErroMontagem::ModeloInvalido {
            modelo: TextoMascarado::new(modelo),
        })
    } else {
        Ok(())
    }
}

fn validar_ferramentas(permitidas: &[String], proibidas: &[String]) -> Result<(), ErroMontagem> {
    let mascarada = |f: &str| TextoMascarado::new(f);
    for lista in [permitidas, proibidas] {
        let mut vistas = HashSet::new();
        for f in lista {
            if !ferramenta_valida(f) {
                return Err(ErroMontagem::FerramentaInvalida {
                    ferramenta: mascarada(f),
                });
            }
            if !vistas.insert(f.as_str()) {
                return Err(ErroMontagem::FerramentaRepetida {
                    ferramenta: mascarada(f),
                });
            }
        }
    }
    if let Some(f) = proibidas.iter().find(|f| f.contains('(')) {
        return Err(ErroMontagem::ProibidaComRegra {
            ferramenta: mascarada(f),
        });
    }
    let nomes_proibidos: HashSet<&str> = proibidas.iter().map(String::as_str).collect();
    if let Some(f) = permitidas
        .iter()
        .map(|f| nome_da_ferramenta(f))
        .find(|n| nomes_proibidos.contains(n))
    {
        return Err(ErroMontagem::PermitidaEProibida {
            ferramenta: mascarada(f),
        });
    }
    Ok(())
}

fn validar_pastas(pastas: &[std::path::PathBuf]) -> Result<Vec<&str>, ErroMontagem> {
    let mut vistas = HashSet::new();
    let mut saida = Vec::with_capacity(pastas.len());
    for p in pastas {
        let texto = caminho(p, CampoCaminho::PastaExtra)?;
        if !vistas.insert(p) {
            return Err(ErroMontagem::PastaRepetida);
        }
        saida.push(texto);
    }
    Ok(saida)
}

/// Caminho aceito: não vazio, UTF-8, absoluto e sem caractere de controle. O portão, que
/// vai para o shell do gancho, também não aceita `"`, `` ` ``, `$`, `%` nem `!`.
fn caminho(p: &Path, campo: CampoCaminho) -> Result<&str, ErroMontagem> {
    let erro = |motivo| ErroMontagem::CaminhoInvalido { campo, motivo };
    if p.as_os_str().is_empty() {
        return Err(erro(MotivoCaminho::Vazio));
    }
    let texto = p.to_str().ok_or(erro(MotivoCaminho::NaoUtf8))?;
    let proibido = |c: char| {
        c.is_control()
            || (campo == CampoCaminho::Portao && matches!(c, '"' | '`' | '$' | '%' | '!'))
    };
    if texto.chars().any(proibido) {
        return Err(erro(MotivoCaminho::CaractereProibido));
    }
    if !p.is_absolute() {
        return Err(erro(MotivoCaminho::Relativo));
    }
    Ok(texto)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn abs(nome: &str) -> PathBuf {
        std::env::temp_dir().join(nome)
    }

    fn spec() -> EspecificacaoExecucao {
        EspecificacaoExecucao {
            papel: "implementador".into(),
            prompt_do_papel: "Você é o implementador.".into(),
            prompt_da_tarefa: "Implemente a T3.".into(),
            modelo: "sonnet".into(),
            ferramentas_permitidas: vec![
                "Read".into(),
                "Edit".into(),
                "Bash(cargo test:*)".into(),
                "Bash(git diff:*)".into(),
                "mcp__obsidian-cli__obsidian_read".into(),
            ],
            ferramentas_proibidas: vec!["WebFetch".into(), "WebSearch".into()],
            pastas_extras: vec![abs("extra-a"), abs("extra-b")],
            portao: abs("harness-gate"),
            mcp_config: None,
        }
    }

    fn sem_ambiente() -> Vec<(OsString, OsString)> {
        Vec::new()
    }

    fn montar(e: &EspecificacaoExecucao) -> Result<ChamadaClaude, ErroMontagem> {
        montar_chamada(e, sem_ambiente())
    }

    /// Valor da opção que vem logo depois de `opcao`.
    fn valor<'a>(c: &'a ChamadaClaude, opcao: &str) -> &'a str {
        let i = c.argumentos().iter().position(|a| a == opcao).unwrap();
        &c.argumentos()[i + 1]
    }

    #[test]
    fn monta_todos_os_argumentos_na_ordem() {
        let e = spec();
        let c = montar(&e).unwrap();
        let a = abs("extra-a").to_str().unwrap().to_owned();
        let b = abs("extra-b").to_str().unwrap().to_owned();
        let settings = configuracao_do_portao(abs("harness-gate").to_str().unwrap());
        let esperado: Vec<String> = [
            "-p",
            "--verbose",
            "--output-format",
            "stream-json",
            "--model",
            "sonnet",
            "--permission-mode",
            "dontAsk",
            "--setting-sources",
            "",
            "--settings",
            &settings,
            "--tools",
            "Read,Edit,Bash",
            "--allowedTools",
            "Read",
            "Edit",
            "Bash(cargo test:*)",
            "Bash(git diff:*)",
            "mcp__obsidian-cli__obsidian_read",
            "--disallowedTools",
            "WebFetch",
            "WebSearch",
            "--add-dir",
            &a,
            &b,
            "--append-system-prompt",
            "Você é o implementador.",
            "--",
            "Implemente a T3.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(c.argumentos(), esperado);
    }

    #[test]
    fn settings_carrega_o_gancho_pre_tool_use() {
        let c = montar(&spec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(valor(&c, "--settings")).unwrap();
        let gancho = &json["hooks"]["PreToolUse"][0];
        assert_eq!(gancho["matcher"], "*");
        assert_eq!(gancho["hooks"][0]["type"], "command");
        let portao = abs("harness-gate");
        assert_eq!(
            gancho["hooks"][0]["command"],
            format!("\"{}\"", portao.to_str().unwrap())
        );
    }

    #[test]
    fn mcp_vem_com_strict() {
        let mut e = spec();
        e.mcp_config = Some(abs("mcp.json"));
        let c = montar(&e).unwrap();
        assert_eq!(valor(&c, "--mcp-config"), abs("mcp.json").to_str().unwrap());
        let i = c
            .argumentos()
            .iter()
            .position(|a| a == "--mcp-config")
            .unwrap();
        assert_eq!(c.argumentos()[i + 2], "--strict-mcp-config");
        assert!(
            !montar(&spec())
                .unwrap()
                .argumentos()
                .iter()
                .any(|a| a.contains("mcp-config"))
        );
    }

    #[test]
    fn listas_vazias_somem_e_tools_vazio_desliga_as_internas() {
        let mut e = spec();
        e.ferramentas_permitidas.clear();
        e.ferramentas_proibidas.clear();
        e.pastas_extras.clear();
        let c = montar(&e).unwrap();
        assert_eq!(valor(&c, "--tools"), "");
        for opcao in ["--allowedTools", "--disallowedTools", "--add-dir"] {
            assert!(!c.argumentos().iter().any(|a| a == opcao), "{opcao}");
        }
    }

    #[test]
    fn prompt_que_parece_opcao_fica_depois_do_separador() {
        let mut e = spec();
        e.prompt_da_tarefa = "--dangerously-skip-permissions".into();
        let c = montar(&e).unwrap();
        let n = c.argumentos().len();
        assert_eq!(c.argumentos()[n - 2], "--");
        assert_eq!(c.argumentos()[n - 1], "--dangerously-skip-permissions");
    }

    #[test]
    fn ambiente_sai_sem_token_do_github() {
        let pai = vec![
            (OsString::from("PATH"), OsString::from("/bin")),
            (OsString::from("GITHUB_TOKEN"), OsString::from("ghp_x")),
            (OsString::from("GH_TOKEN"), OsString::from("gho_x")),
        ];
        let c = montar_chamada(&spec(), pai).unwrap();
        assert_eq!(
            c.ambiente(),
            [(OsString::from("PATH"), OsString::from("/bin"))]
        );
    }

    #[test]
    fn textos_vazios_viram_erro() {
        type Mudar = fn(&mut EspecificacaoExecucao);
        let casos: [(Mudar, ErroMontagem); 5] = [
            (|e| e.papel = " ".into(), ErroMontagem::PapelVazio),
            (
                |e| e.prompt_do_papel.clear(),
                ErroMontagem::PromptDoPapelVazio,
            ),
            (
                |e| e.prompt_da_tarefa = "\n\t".into(),
                ErroMontagem::PromptDaTarefaVazio,
            ),
            (|e| e.modelo.clear(), ErroMontagem::ModeloVazio),
            (
                |e| e.prompt_da_tarefa = "a\0b".into(),
                ErroMontagem::CaractereNulo {
                    campo: "prompt da tarefa",
                },
            ),
        ];
        for (mudar, erro) in casos {
            let mut e = spec();
            mudar(&mut e);
            assert_eq!(montar(&e).unwrap_err(), erro);
        }
    }

    #[test]
    fn modelo_invalido() {
        for m in ["--model", "son net", "sonnet\n", "-x"] {
            let mut e = spec();
            e.modelo = m.into();
            assert!(
                matches!(montar(&e), Err(ErroMontagem::ModeloInvalido { .. })),
                "{m:?}"
            );
        }
    }

    #[test]
    fn ferramentas_invalidas_repetidas_e_em_conflito() {
        let mut e = spec();
        e.ferramentas_permitidas.push("Read,Write".into());
        assert!(matches!(
            montar(&e),
            Err(ErroMontagem::FerramentaInvalida { .. })
        ));

        let mut e = spec();
        e.ferramentas_proibidas.push("".into());
        assert!(matches!(
            montar(&e),
            Err(ErroMontagem::FerramentaInvalida { .. })
        ));

        let mut e = spec();
        e.ferramentas_permitidas.push("Read".into());
        assert!(matches!(
            montar(&e),
            Err(ErroMontagem::FerramentaRepetida { .. })
        ));

        let mut e = spec();
        e.ferramentas_proibidas.push("Bash(rm:*)".into());
        assert!(matches!(
            montar(&e),
            Err(ErroMontagem::ProibidaComRegra { .. })
        ));

        for proibida in ["Read", "Bash", "mcp__obsidian-cli__obsidian_read"] {
            let mut e = spec();
            e.ferramentas_proibidas.push(proibida.into());
            assert_eq!(
                montar(&e).unwrap_err(),
                ErroMontagem::PermitidaEProibida {
                    ferramenta: TextoMascarado::new(proibida)
                },
                "{proibida}"
            );
        }
    }

    #[test]
    fn caminhos_invalidos() {
        let erro = |campo, motivo| ErroMontagem::CaminhoInvalido { campo, motivo };

        let mut e = spec();
        e.pastas_extras.push(PathBuf::new());
        assert_eq!(
            montar(&e).unwrap_err(),
            erro(CampoCaminho::PastaExtra, MotivoCaminho::Vazio)
        );

        let mut e = spec();
        e.pastas_extras.push(PathBuf::from("relativa"));
        assert_eq!(
            montar(&e).unwrap_err(),
            erro(CampoCaminho::PastaExtra, MotivoCaminho::Relativo)
        );

        let mut e = spec();
        e.pastas_extras.push(abs("extra-a"));
        assert_eq!(montar(&e).unwrap_err(), ErroMontagem::PastaRepetida);

        let mut e = spec();
        e.portao = PathBuf::from("harness-gate");
        assert_eq!(
            montar(&e).unwrap_err(),
            erro(CampoCaminho::Portao, MotivoCaminho::Relativo)
        );

        for nome in ["ga\"te", "ga$te", "ga`te", "ga%te", "ga!te", "ga\nte"] {
            let mut e = spec();
            e.portao = abs(nome);
            assert_eq!(
                montar(&e).unwrap_err(),
                erro(CampoCaminho::Portao, MotivoCaminho::CaractereProibido),
                "{nome:?}"
            );
        }

        let mut e = spec();
        e.mcp_config = Some(PathBuf::new());
        assert_eq!(
            montar(&e).unwrap_err(),
            erro(CampoCaminho::McpConfig, MotivoCaminho::Vazio)
        );
    }

    #[cfg(unix)]
    #[test]
    fn caminho_nao_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let mut e = spec();
        e.pastas_extras = vec![PathBuf::from(OsString::from_vec(b"/tmp/\xff".to_vec()))];
        assert_eq!(
            montar(&e).unwrap_err(),
            ErroMontagem::CaminhoInvalido {
                campo: CampoCaminho::PastaExtra,
                motivo: MotivoCaminho::NaoUtf8
            }
        );
    }

    #[test]
    fn debug_da_chamada_mascara() {
        let mut e = spec();
        e.prompt_da_tarefa = "clone com ghp_abc123".into();
        let pai = vec![(OsString::from("ANTHROPIC_API_KEY"), OsString::from("chave"))];
        let texto = format!("{:?}", montar_chamada(&e, pai).unwrap());
        assert!(texto.contains("clone com ••••"), "{texto}");
        assert!(texto.contains("\"ANTHROPIC_API_KEY\": ••••"), "{texto}");
        assert!(
            !texto.contains("ghp_abc123") && !texto.contains("chave\""),
            "{texto}"
        );
    }
}
