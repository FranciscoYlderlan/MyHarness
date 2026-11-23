//! Erros e avisos que o núcleo devolve à tela, com o código do catálogo
//! (`Anexos/contrato/05-erros.md`). A tela escolhe a mensagem pelo código; o núcleo só
//! manda os valores `<x>` em `parametros` (já mascarados, RN-04).

use std::collections::BTreeMap;

use my_harness_core::domain::integracao::{ErroIntegracao, MotivoOauthNaoConcluido};
use serde::{Deserialize, Serialize};
use specta::Type;

macro_rules! catalogo {
    ($($variante:ident = $id:literal, $codigo:literal;)*) => {
        /// Código no servidor de cada item do catálogo de erros e avisos (ERR-01 a ERR-54).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
        pub enum CodigoErro {
            $(
                #[doc = concat!($id, ": `", $codigo, "`.")]
                #[serde(rename = $codigo)]
                $variante,
            )*
        }

        impl CodigoErro {
            /// Todos os códigos, na ordem do catálogo.
            pub const TODOS: &'static [CodigoErro] = &[$(CodigoErro::$variante),*];

            /// Id do item no catálogo (ex.: `"ERR-01"`).
            pub fn id(self) -> &'static str {
                match self { $(CodigoErro::$variante => $id,)* }
            }

            /// Código no servidor (ex.: `"CLAUDE_CLI_NAO_INSTALADO"`).
            pub fn codigo(self) -> &'static str {
                match self { $(CodigoErro::$variante => $codigo,)* }
            }

            /// O código a partir do texto do servidor.
            pub fn pelo_codigo(codigo: &str) -> Option<Self> {
                match codigo { $($codigo => Some(CodigoErro::$variante),)* _ => None }
            }
        }
    };
}

catalogo! {
    ClaudeCliNaoInstalado = "ERR-01", "CLAUDE_CLI_NAO_INSTALADO";
    GithubOauthNaoConcluido = "ERR-02", "GITHUB_OAUTH_NAO_CONCLUIDO";
    ObsidianFechado = "ERR-03", "OBSIDIAN_FECHADO";
    LoginClaudeCliFalhou = "ERR-04", "LOGIN_CLAUDE_CLI_FALHOU";
    ClaudeCliDesconectado = "ERR-05", "CLAUDE_CLI_DESCONECTADO";
    GithubDesconectado = "ERR-06", "GITHUB_DESCONECTADO";
    ObsidianDesconectado = "ERR-07", "OBSIDIAN_DESCONECTADO";
    LimiteDoPlanoAtingido = "ERR-08", "LIMITE_DO_PLANO_ATINGIDO";
    PlanoSemAval = "ERR-09", "PLANO_SEM_AVAL";
    DecisaoSemAval = "ERR-10", "DECISAO_SEM_AVAL";
    ModeloNaoDefinido = "ERR-11", "MODELO_NAO_DEFINIDO";
    TarefasTocamMesmosArquivos = "ERR-12", "TAREFAS_TOCAM_MESMOS_ARQUIVOS";
    EscritaNegadaPapel = "ERR-13", "ESCRITA_NEGADA_PAPEL";
    ComandoGitNegado = "ERR-14", "COMANDO_GIT_NEGADO";
    AguardandoRegistro = "ERR-15", "AGUARDANDO_REGISTRO";
    WorktreeAusente = "ERR-16", "WORKTREE_AUSENTE";
    SubagenteCaiu = "ERR-17", "SUBAGENTE_CAIU";
    LeituraDiretaBloqueada = "ERR-18", "LEITURA_DIRETA_BLOQUEADA";
    ImplementadorNaoTesta = "ERR-19", "IMPLEMENTADOR_NAO_TESTA";
    CriterioSemProva = "ERR-20", "CRITERIO_SEM_PROVA";
    TetoDeVoltas = "ERR-21", "TETO_DE_VOLTAS";
    ConclusaoComPendencia = "ERR-22", "CONCLUSAO_COM_PENDENCIA";
    BriefingAdversarialComContexto = "ERR-23", "BRIEFING_ADVERSARIAL_COM_CONTEXTO";
    AdversarialAntesDoFimDaRodada = "ERR-24", "ADVERSARIAL_ANTES_DO_FIM_DA_RODADA";
    RecursoOcupado = "ERR-25", "RECURSO_OCUPADO";
    AplicacaoForaDoAr = "ERR-26", "APLICACAO_FORA_DO_AR";
    AchadoSemTipo = "ERR-27", "ACHADO_SEM_TIPO";
    SugestaoSemDecisao = "ERR-28", "SUGESTAO_SEM_DECISAO";
    RegistroIncompleto = "ERR-29", "REGISTRO_INCOMPLETO";
    ModeloIndisponivel = "ERR-30", "MODELO_INDISPONIVEL";
    ConflitoDeMerge = "ERR-31", "CONFLITO_DE_MERGE";
    PastaInacessivel = "ERR-32", "PASTA_INACESSIVEL";
    CofreDoProjetoNaoEncontrado = "ERR-33", "COFRE_DO_PROJETO_NAO_ENCONTRADO";
    PromptAcimaDoLimite = "ERR-34", "PROMPT_ACIMA_DO_LIMITE";
    PromptVazio = "ERR-35", "PROMPT_VAZIO";
    PromptDePapelAusente = "ERR-36", "PROMPT_DE_PAPEL_AUSENTE";
    CofreNaoResponde = "ERR-37", "COFRE_NAO_RESPONDE";
    CofreMemoriaObrigatorio = "ERR-38", "COFRE_MEMORIA_OBRIGATORIO";
    EscritaEmCofreDeConsulta = "ERR-39", "ESCRITA_EM_COFRE_DE_CONSULTA";
    ResultadoSemCofreDeOrigem = "ERR-40", "RESULTADO_SEM_COFRE_DE_ORIGEM";
    ConsumoIndisponivel = "ERR-41", "CONSUMO_INDISPONIVEL";
    FalhaNoEnvio = "ERR-42", "FALHA_NO_ENVIO";
    CapturaIndisponivel = "ERR-43", "CAPTURA_INDISPONIVEL";
    TetoDeEscritaAtingido = "ERR-44", "TETO_DE_ESCRITA_ATINGIDO";
    ProjetoNomeInvalido = "ERR-45", "PROJETO_NOME_INVALIDO";
    RepositorioInvalido = "ERR-46", "REPOSITORIO_INVALIDO";
    LimiteDeContextoForaDaFaixa = "ERR-47", "LIMITE_DE_CONTEXTO_FORA_DA_FAIXA";
    LimiteDoChatAtingido = "ERR-48", "LIMITE_DO_CHAT_ATINGIDO";
    TarefaIncompleta = "ERR-49", "TAREFA_INCOMPLETA";
    StatusDeConsumoDesconhecido = "ERR-50", "STATUS_DE_CONSUMO_DESCONHECIDO";
    GravacaoDivergente = "ERR-51", "GRAVACAO_DIVERGENTE";
    LimiteDeContextoAbaixoDoMinimo = "ERR-52", "LIMITE_DE_CONTEXTO_ABAIXO_DO_MINIMO";
    IntegracaoFalhaInesperada = "ERR-53", "INTEGRACAO_FALHA_INESPERADA";
    GravacaoDesfeitaPelaMetade = "ERR-54", "GRAVACAO_DESFEITA_PELA_METADE";
}

/// Um item do catálogo com os valores do momento (`<x>` da mensagem).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ErroCatalogo {
    /// Código no servidor; a tela escolhe a mensagem por ele.
    pub codigo: CodigoErro,
    /// Id no catálogo (`ERR-xx`), para log e teste.
    pub id: String,
    /// Valores preenchidos em tempo de execução (ex.: `integracao`, `motivo`), mascarados.
    pub parametros: BTreeMap<String, String>,
}

impl ErroCatalogo {
    pub fn new(codigo: CodigoErro) -> Self {
        Self {
            codigo,
            id: codigo.id().to_owned(),
            parametros: BTreeMap::new(),
        }
    }

    pub fn com(mut self, chave: &str, valor: impl Into<String>) -> Self {
        self.parametros.insert(chave.to_owned(), valor.into());
        self
    }
}

/// Erro de um comando. `catalogo` é a regra (o código diz o quê); `interno` é falha que
/// o catálogo não cobre (ex.: o corpo provisório de um comando ainda sem núcleo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, thiserror::Error)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum ErroIpc {
    #[error("{} {}", erro.id, erro.codigo.codigo())]
    Catalogo { erro: ErroCatalogo },
    #[error("erro interno: {motivo}")]
    Interno { motivo: MotivoInterno },
}

/// Motivo de um [`ErroIpc::Interno`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MotivoInterno {
    /// O comando existe no contrato, mas o núcleo ainda não o implementa.
    NaoImplementado,
}

impl std::fmt::Display for MotivoInterno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MotivoInterno::NaoImplementado => "nao_implementado",
        })
    }
}

impl From<CodigoErro> for ErroIpc {
    fn from(codigo: CodigoErro) -> Self {
        ErroCatalogo::new(codigo).into()
    }
}

impl From<ErroCatalogo> for ErroIpc {
    fn from(erro: ErroCatalogo) -> Self {
        ErroIpc::Catalogo { erro }
    }
}

impl From<&ErroIntegracao> for ErroCatalogo {
    fn from(e: &ErroIntegracao) -> Self {
        let codigo = CodigoErro::pelo_codigo(e.codigo())
            .expect("todo código do ErroIntegracao está no catálogo");
        let erro = ErroCatalogo::new(codigo).com("integracao", e.integracao().nome());
        match e {
            ErroIntegracao::GithubOauthNaoConcluido(motivo) => erro.com(
                "motivo",
                match motivo {
                    MotivoOauthNaoConcluido::Cancelado => "cancelado",
                    MotivoOauthNaoConcluido::Recusado => "access_denied",
                    MotivoOauthNaoConcluido::Expirado => "expired_token",
                },
            ),
            ErroIntegracao::ObsidianFechado { mensagem_da_cli } => {
                erro.com("mensagem_da_cli", mensagem_da_cli.as_str())
            }
            ErroIntegracao::LoginClaudeCliFalhou { motivo } => erro.com("motivo", motivo.as_str()),
            _ => erro,
        }
    }
}

impl From<ErroIntegracao> for ErroIpc {
    fn from(e: ErroIntegracao) -> Self {
        ErroCatalogo::from(&e).into()
    }
}

/// Resultado salvo com avisos (tratamento `avisar`: o valor foi salvo e o Humano decide,
/// ex.: ERR-47 limite fora da faixa recomendada).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ComAvisos<T> {
    pub valor: T,
    pub avisos: Vec<ErroCatalogo>,
}

impl<T> ComAvisos<T> {
    pub fn sem_avisos(valor: T) -> Self {
        Self {
            valor,
            avisos: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use my_harness_core::domain::segredo::TextoMascarado;

    use super::*;

    /// Tabela do `05-erros.md` (id e código no servidor), copiada à mão de propósito:
    /// se o catálogo ou o enum mudarem, este teste mostra a diferença.
    const CATALOGO: &[(&str, &str)] = &[
        ("ERR-01", "CLAUDE_CLI_NAO_INSTALADO"),
        ("ERR-02", "GITHUB_OAUTH_NAO_CONCLUIDO"),
        ("ERR-03", "OBSIDIAN_FECHADO"),
        ("ERR-04", "LOGIN_CLAUDE_CLI_FALHOU"),
        ("ERR-05", "CLAUDE_CLI_DESCONECTADO"),
        ("ERR-06", "GITHUB_DESCONECTADO"),
        ("ERR-07", "OBSIDIAN_DESCONECTADO"),
        ("ERR-08", "LIMITE_DO_PLANO_ATINGIDO"),
        ("ERR-09", "PLANO_SEM_AVAL"),
        ("ERR-10", "DECISAO_SEM_AVAL"),
        ("ERR-11", "MODELO_NAO_DEFINIDO"),
        ("ERR-12", "TAREFAS_TOCAM_MESMOS_ARQUIVOS"),
        ("ERR-13", "ESCRITA_NEGADA_PAPEL"),
        ("ERR-14", "COMANDO_GIT_NEGADO"),
        ("ERR-15", "AGUARDANDO_REGISTRO"),
        ("ERR-16", "WORKTREE_AUSENTE"),
        ("ERR-17", "SUBAGENTE_CAIU"),
        ("ERR-18", "LEITURA_DIRETA_BLOQUEADA"),
        ("ERR-19", "IMPLEMENTADOR_NAO_TESTA"),
        ("ERR-20", "CRITERIO_SEM_PROVA"),
        ("ERR-21", "TETO_DE_VOLTAS"),
        ("ERR-22", "CONCLUSAO_COM_PENDENCIA"),
        ("ERR-23", "BRIEFING_ADVERSARIAL_COM_CONTEXTO"),
        ("ERR-24", "ADVERSARIAL_ANTES_DO_FIM_DA_RODADA"),
        ("ERR-25", "RECURSO_OCUPADO"),
        ("ERR-26", "APLICACAO_FORA_DO_AR"),
        ("ERR-27", "ACHADO_SEM_TIPO"),
        ("ERR-28", "SUGESTAO_SEM_DECISAO"),
        ("ERR-29", "REGISTRO_INCOMPLETO"),
        ("ERR-30", "MODELO_INDISPONIVEL"),
        ("ERR-31", "CONFLITO_DE_MERGE"),
        ("ERR-32", "PASTA_INACESSIVEL"),
        ("ERR-33", "COFRE_DO_PROJETO_NAO_ENCONTRADO"),
        ("ERR-34", "PROMPT_ACIMA_DO_LIMITE"),
        ("ERR-35", "PROMPT_VAZIO"),
        ("ERR-36", "PROMPT_DE_PAPEL_AUSENTE"),
        ("ERR-37", "COFRE_NAO_RESPONDE"),
        ("ERR-38", "COFRE_MEMORIA_OBRIGATORIO"),
        ("ERR-39", "ESCRITA_EM_COFRE_DE_CONSULTA"),
        ("ERR-40", "RESULTADO_SEM_COFRE_DE_ORIGEM"),
        ("ERR-41", "CONSUMO_INDISPONIVEL"),
        ("ERR-42", "FALHA_NO_ENVIO"),
        ("ERR-43", "CAPTURA_INDISPONIVEL"),
        ("ERR-44", "TETO_DE_ESCRITA_ATINGIDO"),
        ("ERR-45", "PROJETO_NOME_INVALIDO"),
        ("ERR-46", "REPOSITORIO_INVALIDO"),
        ("ERR-47", "LIMITE_DE_CONTEXTO_FORA_DA_FAIXA"),
        ("ERR-48", "LIMITE_DO_CHAT_ATINGIDO"),
        ("ERR-49", "TAREFA_INCOMPLETA"),
        ("ERR-50", "STATUS_DE_CONSUMO_DESCONHECIDO"),
        ("ERR-51", "GRAVACAO_DIVERGENTE"),
        ("ERR-52", "LIMITE_DE_CONTEXTO_ABAIXO_DO_MINIMO"),
        ("ERR-53", "INTEGRACAO_FALHA_INESPERADA"),
        ("ERR-54", "GRAVACAO_DESFEITA_PELA_METADE"),
    ];

    #[test]
    fn codigos_seguem_o_catalogo() {
        let nossos: Vec<_> = CodigoErro::TODOS
            .iter()
            .map(|c| (c.id(), c.codigo()))
            .collect();
        assert_eq!(nossos, CATALOGO);
        let ids: HashSet<_> = CodigoErro::TODOS.iter().map(|c| c.id()).collect();
        assert_eq!(ids.len(), CodigoErro::TODOS.len());
    }

    #[test]
    fn codigo_vai_ao_fio_como_no_servidor() {
        for c in CodigoErro::TODOS {
            assert_eq!(
                serde_json::to_string(c).unwrap(),
                format!("\"{}\"", c.codigo())
            );
            assert_eq!(CodigoErro::pelo_codigo(c.codigo()), Some(*c));
        }
    }

    #[test]
    fn erro_de_integracao_vira_o_mesmo_codigo_com_parametros_mascarados() {
        let e = ErroIntegracao::LoginClaudeCliFalhou {
            motivo: TextoMascarado::new("falhou com sk-ant-api03-abcdefghijklmnop"),
        };
        let ErroIpc::Catalogo { erro } = ErroIpc::from(e) else {
            panic!("devia ser do catálogo");
        };
        assert_eq!(erro.codigo, CodigoErro::LoginClaudeCliFalhou);
        assert_eq!(erro.id, "ERR-04");
        assert!(!erro.parametros["motivo"].contains("abcdefghijklmnop"));
        assert_eq!(erro.parametros["integracao"], "Claude CLI");

        for e in [
            ErroIntegracao::ClaudeCliNaoInstalado,
            ErroIntegracao::GithubOauthNaoConcluido(MotivoOauthNaoConcluido::Expirado),
            ErroIntegracao::ClaudeCliDesconectado,
            ErroIntegracao::GithubDesconectado,
            ErroIntegracao::ObsidianDesconectado,
        ] {
            assert_eq!(ErroCatalogo::from(&e).codigo.codigo(), e.codigo());
            assert_eq!(ErroCatalogo::from(&e).id, e.id());
        }
    }

    #[test]
    fn erro_ipc_tem_forma_marcada() {
        let json = serde_json::to_value(ErroIpc::from(CodigoErro::ConsumoIndisponivel)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "tipo": "catalogo",
                "erro": { "codigo": "CONSUMO_INDISPONIVEL", "id": "ERR-41", "parametros": {} }
            })
        );
        let json = serde_json::to_value(ErroIpc::Interno {
            motivo: MotivoInterno::NaoImplementado,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "tipo": "interno", "motivo": "nao_implementado" })
        );
    }
}
