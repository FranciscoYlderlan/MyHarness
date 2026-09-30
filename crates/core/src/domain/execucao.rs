//! Especificação de execução de um agente e as regras da chamada ao `claude -p`
//! (decisão `geral-decisao-2026-11-19-agente-por-claude-p`; arquitetura, seção 4).
//!
//! Aqui ficam só os valores e as regras: o que um agente pode usar e como o ambiente do
//! processo dele é limpo. A montagem dos argumentos mora em
//! [`crate::application::chamada_claude`]; abrir o processo e ler a saída é da
//! infraestrutura (`AgentRunner`).

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use super::segredo::{MASCARA, PREFIXOS, TextoMascarado};

/// O que o núcleo precisa para abrir um agente: papel, modelo, ferramentas, pastas e prompts.
#[derive(Clone, PartialEq, Eq)]
pub struct EspecificacaoExecucao {
    /// Nome do papel (ex.: `implementador`); identifica a execução, não vai para a CLI.
    pub papel: String,
    /// Prompt do papel, anexado ao prompt de sistema (`--append-system-prompt`, RN-25).
    pub prompt_do_papel: String,
    /// Prompt da tarefa (`-p`).
    pub prompt_da_tarefa: String,
    /// Modelo (`--model`), ex.: `sonnet` ou `claude-sonnet-4-5`.
    pub modelo: String,
    /// Ferramentas do papel (camada 1): nome (`Read`) ou regra (`Bash(git diff:*)`).
    pub ferramentas_permitidas: Vec<String>,
    /// Ferramentas que o papel não pode usar (camada 1): só o nome, sem regra.
    pub ferramentas_proibidas: Vec<String>,
    /// Pastas extras que o agente pode ler e escrever (`--add-dir`), absolutas.
    pub pastas_extras: Vec<PathBuf>,
    /// Executável do `harness-gate`, chamado pelo gancho `PreToolUse` (camada 3), absoluto.
    pub portao: PathBuf,
    /// Arquivo de configuração de MCP (`--mcp-config`), absoluto, quando houver.
    pub mcp_config: Option<PathBuf>,
}

/// `Debug` mascarado: prompts e nomes podem trazer texto colado pelo Humano (RN-04).
impl fmt::Debug for EspecificacaoExecucao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = |t: &str| TextoMascarado::new(t);
        let lista = |v: &[String]| v.iter().map(|t| m(t)).collect::<Vec<_>>();
        f.debug_struct("EspecificacaoExecucao")
            .field("papel", &m(&self.papel))
            .field("prompt_do_papel", &m(&self.prompt_do_papel))
            .field("prompt_da_tarefa", &m(&self.prompt_da_tarefa))
            .field("modelo", &m(&self.modelo))
            .field(
                "ferramentas_permitidas",
                &lista(&self.ferramentas_permitidas),
            )
            .field("ferramentas_proibidas", &lista(&self.ferramentas_proibidas))
            .field("pastas_extras", &self.pastas_extras)
            .field("portao", &self.portao)
            .field("mcp_config", &self.mcp_config)
            .finish()
    }
}

/// Campo da especificação que guarda um caminho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoCaminho {
    /// Uma das [`EspecificacaoExecucao::pastas_extras`].
    PastaExtra,
    /// [`EspecificacaoExecucao::portao`].
    Portao,
    /// [`EspecificacaoExecucao::mcp_config`].
    McpConfig,
}

impl fmt::Display for CampoCaminho {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CampoCaminho::PastaExtra => "pasta extra",
            CampoCaminho::Portao => "portão",
            CampoCaminho::McpConfig => "configuração de MCP",
        })
    }
}

/// Por que um caminho foi recusado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoCaminho {
    /// Caminho vazio.
    Vazio,
    /// Caminho relativo: dependeria da pasta em que o processo abre.
    Relativo,
    /// Caminho que não é UTF-8 válido.
    NaoUtf8,
    /// Caractere que o shell do gancho interpretaria (`"`, `` ` ``, `$`, `%`, `!`) ou de controle.
    CaractereProibido,
}

impl fmt::Display for MotivoCaminho {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MotivoCaminho::Vazio => "vazio",
            MotivoCaminho::Relativo => "relativo",
            MotivoCaminho::NaoUtf8 => "não é UTF-8",
            MotivoCaminho::CaractereProibido => "com caractere proibido",
        })
    }
}

/// Especificação recusada: valor vazio ou inválido vira erro, nunca argumento torto.
/// Os textos guardados já estão mascarados (RN-04).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroMontagem {
    /// Nome do papel vazio.
    #[error("papel vazio")]
    PapelVazio,
    /// Prompt do papel vazio.
    #[error("prompt do papel vazio")]
    PromptDoPapelVazio,
    /// Prompt da tarefa vazio.
    #[error("prompt da tarefa vazio")]
    PromptDaTarefaVazio,
    /// Algum texto traz o caractere nulo, que nenhum processo aceita como argumento.
    #[error("{campo} com caractere nulo")]
    CaractereNulo {
        /// Qual campo.
        campo: &'static str,
    },
    /// Modelo vazio.
    #[error("modelo vazio")]
    ModeloVazio,
    /// Modelo com espaço, caractere de controle ou começando por `-`.
    #[error("modelo inválido: {modelo}")]
    ModeloInvalido {
        /// O modelo recebido, mascarado.
        modelo: TextoMascarado,
    },
    /// Ferramenta vazia, começando por `-`, com vírgula, com caractere de controle ou
    /// com parênteses desbalanceados.
    #[error("ferramenta inválida: {ferramenta:?}")]
    FerramentaInvalida {
        /// A ferramenta recebida, mascarada.
        ferramenta: TextoMascarado,
    },
    /// A mesma ferramenta aparece duas vezes na mesma lista.
    #[error("ferramenta repetida: {ferramenta}")]
    FerramentaRepetida {
        /// A ferramenta repetida, mascarada.
        ferramenta: TextoMascarado,
    },
    /// Ferramenta proibida com regra (`Bash(rm:*)`): a proibição é pelo nome sozinho.
    #[error("ferramenta proibida deve ser só o nome: {ferramenta}")]
    ProibidaComRegra {
        /// A ferramenta recebida, mascarada.
        ferramenta: TextoMascarado,
    },
    /// O mesmo nome de ferramenta é permitido e proibido.
    #[error("ferramenta permitida e proibida ao mesmo tempo: {ferramenta}")]
    PermitidaEProibida {
        /// O nome da ferramenta, mascarado.
        ferramenta: TextoMascarado,
    },
    /// Caminho recusado.
    #[error("caminho de {campo} {motivo}")]
    CaminhoInvalido {
        /// Qual campo.
        campo: CampoCaminho,
        /// Por quê.
        motivo: MotivoCaminho,
    },
    /// A mesma pasta extra aparece duas vezes.
    #[error("pasta extra repetida")]
    PastaRepetida,
}

/// Nome da ferramenta, sem a regra entre parênteses: `Bash(git diff:*)` → `Bash`.
pub fn nome_da_ferramenta(ferramenta: &str) -> &str {
    ferramenta
        .split_once('(')
        .map_or(ferramenta, |(nome, _)| nome)
        .trim()
}

/// Ferramenta aceita: não vazia, sem espaço nas pontas, sem `-` no início, sem vírgula,
/// sem caractere de controle, nome sem espaço e, se houver regra, `Nome(regra)` com o `)` no fim.
pub(crate) fn ferramenta_valida(ferramenta: &str) -> bool {
    let nome = nome_da_ferramenta(ferramenta);
    let regra_ok = match ferramenta.split_once('(') {
        None => !ferramenta.contains(')'),
        Some((_, resto)) => resto.ends_with(')') && resto.len() > 1,
    };
    !ferramenta.is_empty()
        && ferramenta.trim() == ferramenta
        && !ferramenta.starts_with('-')
        && !ferramenta.contains(',')
        && !ferramenta.chars().any(char::is_control)
        && !nome.is_empty()
        && !nome.contains(char::is_whitespace)
        && regra_ok
}

/// Ambiente do processo do agente sem token do GitHub (camada 4): o agente nunca fala com o
/// GitHub; quem fala é o núcleo, com o token guardado no cofre do SO (RN-04).
///
/// Sai toda variável cujo nome, sem diferenciar maiúsculas:
/// - tem `TOKEN` e começa por `GH_`, `ACTIONS_` ou contém `GITHUB` (`GITHUB_TOKEN`,
///   `GH_TOKEN`, `GH_ENTERPRISE_TOKEN`, `COPILOT_GITHUB_TOKEN`, `ACTIONS_RUNTIME_TOKEN`…);
/// - é `GITHUB_PAT` ou `GH_PAT`;
///
/// e toda variável cujo valor começa por um prefixo de token do GitHub (`ghp_`, `github_pat_`…).
/// O resto passa na ordem recebida.
pub fn ambiente_sem_token_do_github<I>(ambiente: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    ambiente
        .into_iter()
        .filter(|(nome, valor)| !nome_de_token_do_github(nome) && !valor_de_token_do_github(valor))
        .collect()
}

fn nome_de_token_do_github(nome: &OsStr) -> bool {
    let nome = nome.to_string_lossy().to_ascii_uppercase();
    let com_token = nome.contains("TOKEN")
        && (nome.starts_with("GH_") || nome.starts_with("ACTIONS_") || nome.contains("GITHUB"));
    com_token || nome == "GITHUB_PAT" || nome == "GH_PAT"
}

fn valor_de_token_do_github(valor: &OsStr) -> bool {
    let valor = valor.to_string_lossy();
    let valor = valor.trim_start().as_bytes();
    PREFIXOS
        .iter()
        .filter(|p| p.starts_with("gh") || p.starts_with("github"))
        .any(|p| {
            valor
                .get(..p.len())
                .is_some_and(|v| v.eq_ignore_ascii_case(p.as_bytes()))
        })
}

/// Mostra só os nomes das variáveis; os valores viram [`MASCARA`].
pub(crate) struct AmbienteMascarado<'a>(pub &'a [(OsString, OsString)]);

impl fmt::Debug for AmbienteMascarado<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.0.iter().map(|(nome, _)| (nome, Mascara)))
            .finish()
    }
}

/// Escreve [`MASCARA`] no lugar de um valor.
struct Mascara;

impl fmt::Debug for Mascara {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASCARA)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amb(pares: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pares
            .iter()
            .map(|(n, v)| (OsString::from(n), OsString::from(v)))
            .collect()
    }

    #[test]
    fn nome_sem_a_regra() {
        assert_eq!(nome_da_ferramenta("Read"), "Read");
        assert_eq!(nome_da_ferramenta("Bash(git diff:*)"), "Bash");
        assert_eq!(
            nome_da_ferramenta("mcp__obsidian__read"),
            "mcp__obsidian__read"
        );
    }

    #[test]
    fn ferramentas_aceitas_e_recusadas() {
        for ok in [
            "Read",
            "Bash(git diff:*)",
            "Bash(npm run test:*)",
            "mcp__obsidian-cli__obsidian_read",
            "WebFetch(domain:docs.rs)",
        ] {
            assert!(ferramenta_valida(ok), "{ok:?}");
        }
        for ruim in [
            "",
            " Read",
            "Read ",
            "-Read",
            "--dangerously-skip-permissions",
            "Read,Edit",
            "Read Edit",
            "Re\nad",
            "Bash(",
            "Bash()",
            "Bash(ls",
            "Bash)",
            "(ls)",
            "Bash(ls) x",
        ] {
            assert!(!ferramenta_valida(ruim), "{ruim:?}");
        }
    }

    #[test]
    fn tira_os_tokens_do_github_pelo_nome() {
        let entrada = amb(&[
            ("PATH", "/usr/bin"),
            ("GITHUB_TOKEN", "x"),
            ("GH_TOKEN", "x"),
            ("gh_token", "x"),
            ("GH_ENTERPRISE_TOKEN", "x"),
            ("GITHUB_ENTERPRISE_TOKEN", "x"),
            ("COPILOT_GITHUB_TOKEN", "x"),
            ("ACTIONS_RUNTIME_TOKEN", "x"),
            ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "x"),
            ("GITHUB_PAT", "x"),
            ("GH_PAT", "x"),
            ("HOME", "/home/u"),
            ("GITHUB_PATH", "/tmp/path"),
            ("GH_HOST", "github.com"),
            ("ANTHROPIC_API_KEY", "sk-ant-abc"),
        ]);
        let nomes: Vec<_> = ambiente_sem_token_do_github(entrada)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            nomes,
            [
                "PATH",
                "HOME",
                "GITHUB_PATH",
                "GH_HOST",
                "ANTHROPIC_API_KEY"
            ]
        );
    }

    #[test]
    fn tira_os_tokens_do_github_pelo_valor() {
        let entrada = amb(&[
            ("MEU_SEGREDO", "ghp_abc123"),
            ("OUTRO", "  GitHub_Pat_abc"),
            ("SERVIDOR", "gho_x"),
            ("FICA", "ghost"),
            ("CHAVE", "sk-ant-api03-x"),
        ]);
        let nomes: Vec<_> = ambiente_sem_token_do_github(entrada)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(nomes, ["FICA", "CHAVE"]);
    }

    #[test]
    fn debug_da_especificacao_e_do_ambiente_mascara() {
        let spec = EspecificacaoExecucao {
            papel: "implementador".into(),
            prompt_do_papel: "papel".into(),
            prompt_da_tarefa: "use ghp_abc123 para clonar".into(),
            modelo: "sonnet".into(),
            ferramentas_permitidas: vec!["Read".into()],
            ferramentas_proibidas: vec![],
            pastas_extras: vec![],
            portao: PathBuf::from("/gate"),
            mcp_config: None,
        };
        let texto = format!("{spec:?}");
        assert!(texto.contains("use •••• para clonar"), "{texto}");
        assert!(!texto.contains("ghp_abc123"), "{texto}");

        let ambiente = amb(&[("CHAVE", "sk-ant-x"), ("OUTRA", "valor")]);
        let texto = format!("{:?}", AmbienteMascarado(&ambiente));
        assert_eq!(texto, r#"{"CHAVE": ••••, "OUTRA": ••••}"#);
    }

    #[test]
    fn erros_mostram_o_texto_mascarado() {
        let erro = ErroMontagem::ModeloInvalido {
            modelo: TextoMascarado::new("ghp_abc modelo"),
        };
        assert_eq!(erro.to_string(), "modelo inválido: •••• modelo");
        let erro = ErroMontagem::CaminhoInvalido {
            campo: CampoCaminho::Portao,
            motivo: MotivoCaminho::Relativo,
        };
        assert_eq!(erro.to_string(), "caminho de portão relativo");
    }
}
