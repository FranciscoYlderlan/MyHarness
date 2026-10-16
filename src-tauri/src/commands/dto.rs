//! Tipos que cruzam a ponte entre a tela e o núcleo (DTOs), derivados dos ENT de
//! `Anexos/contrato/02-entidades.md` (no cofre do projeto até a aceitação final).
//!
//! Só formato: nenhuma regra de negócio mora aqui. Os nomes dos campos seguem os do
//! contrato; os valores das enumerações vão em `snake_case` ASCII (ex.: `"sem_login"`
//! para "sem login"), e a tela mostra o texto do contrato. Os ids são opacos (texto)
//! para a tela: quem guarda (SQLite, T4) escolhe o formato real.
//!
//! Data e hora: sempre [`DataHoraUtc`], texto ISO 8601 em UTC com `Z`
//! (`geral-decisao-2026-11-19-data-e-hora-em-utc`); a conversão para a hora local é só
//! da tela.

use std::fmt;

use my_harness_core::domain::integracao::{
    EstadoIntegracao, StatusIntegracao as StatusIntegracaoCore,
    TipoIntegracao as TipoIntegracaoCore,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::erro::ErroCatalogo;

// ---------------------------------------------------------------------------
// Data e hora
// ---------------------------------------------------------------------------

/// Data e hora em ISO 8601 UTC com `Z` (ex.: `2026-11-19T18:20:00Z`), com fração de
/// segundo opcional (até 9 casas). Qualquer outro formato é recusado na entrada e na
/// saída, então a tela nunca recebe hora local nem deslocamento (`-03:00`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(type = String)]
pub struct DataHoraUtc(String);

/// O texto não está em ISO 8601 UTC com `Z`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("data e hora fora do formato ISO 8601 UTC com Z: {0:?}")]
pub struct DataHoraInvalida(pub String);

impl DataHoraUtc {
    /// Confere o formato `AAAA-MM-DDTHH:MM:SS[.fração]Z` e as faixas de cada campo.
    pub fn new(texto: impl Into<String>) -> Result<Self, DataHoraInvalida> {
        let texto = texto.into();
        if data_hora_valida(&texto) {
            Ok(Self(texto))
        } else {
            Err(DataHoraInvalida(texto))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DataHoraUtc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for DataHoraUtc {
    type Error = DataHoraInvalida;
    fn try_from(texto: String) -> Result<Self, Self::Error> {
        Self::new(texto)
    }
}

impl From<DataHoraUtc> for String {
    fn from(data: DataHoraUtc) -> Self {
        data.0
    }
}

fn data_hora_valida(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() < 20 || b[b.len() - 1] != b'Z' {
        return false;
    }
    let digitos = |de: usize, ate: usize| b[de..ate].iter().all(u8::is_ascii_digit);
    let numero = |de: usize, ate: usize| t[de..ate].parse::<u32>().unwrap_or(u32::MAX);
    let forma = digitos(0, 4)
        && b[4] == b'-'
        && digitos(5, 7)
        && b[7] == b'-'
        && digitos(8, 10)
        && b[10] == b'T'
        && digitos(11, 13)
        && b[13] == b':'
        && digitos(14, 16)
        && b[16] == b':'
        && digitos(17, 19);
    if !forma {
        return false;
    }
    let fracao = &b[19..b.len() - 1];
    let fracao_ok = fracao.is_empty()
        || (fracao[0] == b'.'
            && (2..=10).contains(&fracao.len())
            && fracao[1..].iter().all(u8::is_ascii_digit));
    if !fracao_ok {
        return false;
    }
    let (ano, mes, dia) = (numero(0, 4), numero(5, 7), numero(8, 10));
    let bissexto = (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0;
    let dias_no_mes = match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if bissexto => 29,
        2 => 28,
        _ => return false,
    };
    (1..=dias_no_mes).contains(&dia)
        && numero(11, 13) <= 23
        && numero(14, 16) <= 59
        && numero(17, 19) <= 59
}

// ---------------------------------------------------------------------------
// Ids (opacos para a tela)
// ---------------------------------------------------------------------------

macro_rules! id {
    ($(#[$doc:meta])* $nome:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
        #[serde(transparent)]
        pub struct $nome(pub String);
    };
}

id!(
    /// Id de um Projeto (ENT-01).
    ProjetoId
);
id!(
    /// Id de um Cofre (ENT-02).
    CofreId
);
id!(
    /// Id de um Chat (ENT-03).
    ChatId
);
id!(
    /// Id de uma Mensagem (ENT-04).
    MensagemId
);
id!(
    /// Id de um PlanoDelegação (ENT-05).
    PlanoId
);
id!(
    /// Id de uma Tarefa (ENT-06).
    TarefaId
);
id!(
    /// Id de uma Execução (ENT-07).
    ExecucaoId
);
id!(
    /// Id de um Relatório (ENT-09).
    RelatorioId
);
id!(
    /// Id de um Achado (ENT-10).
    AchadoId
);
id!(
    /// Id de uma Leitura (ENT-11).
    LeituraId
);
id!(
    /// Id de um Log (ENT-12).
    LogId
);
id!(
    /// Id de uma PastaContexto (ENT-26).
    PastaContextoId
);

// ---------------------------------------------------------------------------
// ENT-01 Projeto
// ---------------------------------------------------------------------------

/// ENT-01 Projeto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Projeto {
    pub id: ProjetoId,
    /// 1 a 80 caracteres, único (ERR-45).
    pub nome: String,
    /// Caminho absoluto de uma pasta com `.git` (ERR-46); obrigatoriedade `[a confirmar]`.
    pub repositorio: Option<String>,
    /// Tokens; mínimo 1 (ERR-52), recomendado 10 mil a 1 milhão (aviso ERR-47).
    pub limite_contexto_padrao: u32,
}

/// Dados para criar um projeto (o id vem do núcleo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NovoProjeto {
    pub nome: String,
    pub repositorio: Option<String>,
    /// Sem valor, o núcleo usa o padrão (100 mil, S7).
    pub limite_contexto_padrao: Option<u32>,
}

// ---------------------------------------------------------------------------
// ENT-02 Cofre
// ---------------------------------------------------------------------------

/// `funcao` de ENT-02.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FuncaoCofre {
    Memoria,
    Consulta,
}

/// `status` de ENT-02.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusCofre {
    Conectado,
    Desconectado,
}

/// ENT-02 Cofre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Cofre {
    pub id: CofreId,
    pub projeto: ProjetoId,
    /// Nome do cofre no Obsidian.
    pub nome: String,
    pub funcao: FuncaoCofre,
    pub ativo: bool,
    pub status: StatusCofre,
}

// ---------------------------------------------------------------------------
// ENT-03 Chat e ENT-04 Mensagem
// ---------------------------------------------------------------------------

/// `status` de ENT-03.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusChat {
    Ativo,
    Pausado,
    Encerrado,
    /// "só leitura" no contrato.
    SoLeitura,
}

/// ENT-03 Chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Chat {
    pub id: ChatId,
    pub projeto: ProjetoId,
    pub limite_tokens: u32,
    pub tokens_usados: u32,
    /// Só chat encerrado por passagem de turno.
    pub chat_anterior: Option<ChatId>,
    pub status: StatusChat,
    /// Vezes que o Humano seguiu no chat depois do aviso de limite (RN-23).
    pub continuacoes: u32,
}

/// `autor` de ENT-04.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AutorMensagem {
    Humano,
    Orquestrador,
    Sistema,
}

/// ENT-04 Mensagem. O `texto` já chega mascarado (RN-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Mensagem {
    pub id: MensagemId,
    pub chat: ChatId,
    pub autor: AutorMensagem,
    pub texto: String,
    /// Obrigatório quando `autor = orquestrador`.
    pub uso_tokens: Option<u32>,
}

/// Chat reaberto com a conversa (CMP-09, UC-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ChatAberto {
    pub chat: Chat,
    pub mensagens: Vec<Mensagem>,
}

// ---------------------------------------------------------------------------
// ENT-05 PlanoDelegação e ENT-06 Tarefa
// ---------------------------------------------------------------------------

/// `status` de ENT-05.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusPlano {
    Proposto,
    Aprovado,
    Recusado,
}

/// ENT-05 PlanoDelegação, com as tarefas (ENT-06) que a TEL-03 mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PlanoDelegacao {
    pub id: PlanoId,
    pub chat: ChatId,
    pub versao: u32,
    pub status: StatusPlano,
    /// Só quando `status = aprovado`.
    pub aprovado_em: Option<DataHoraUtc>,
    /// 0 a 2 por pedido do Humano (ERR-49).
    pub devolucoes: u32,
    pub tarefas: Vec<Tarefa>,
}

/// Papel de um agente (`nome` de ENT-15; `papel` de ENT-06 e ENT-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NomePapel {
    Orquestrador,
    Funcionalidade,
    Teste,
    Adversarial,
    Pesquisa,
}

/// ENT-06 Tarefa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Tarefa {
    pub id: TarefaId,
    /// O plano da tarefa (ENT-05).
    pub plano: PlanoId,
    /// 1 a 500 caracteres (ERR-49); vazio ou longo só com `incompleta = true`.
    pub faz: String,
    /// Caminhos relativos ao repositório.
    pub toca_em: Vec<String>,
    pub depende_de: Vec<TarefaId>,
    pub paralelo: bool,
    /// Vazio só com `incompleta = true` (ERR-49).
    pub pronto_quando: String,
    /// Vazio só com `incompleta = true` (ERR-49).
    pub prova: String,
    /// Marcada na 3ª devolução do plano (ERR-49); "Aprovar" fica bloqueado enquanto
    /// houver tarefa com `incompleta = true`. Padrão: `false`.
    pub incompleta: bool,
    /// Na tarefa, nunca `orquestrador`.
    pub papel: NomePapel,
    /// Sem modelo, vale o `modelo_padrao` do papel (RN-07).
    pub modelo: Option<String>,
    /// 0 a 2 (ERR-21).
    pub voltas: u32,
    pub implementada_por: Option<ExecucaoId>,
    pub testada_por: Option<ExecucaoId>,
}

// ---------------------------------------------------------------------------
// ENT-07 Execução
// ---------------------------------------------------------------------------

/// `status` de ENT-07.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusExecucao {
    /// "na fila" no contrato.
    NaFila,
    Rodando,
    /// No fio, `concluido` (ASCII, como todo enum da IPC) = "concluído" do contrato (M-38).
    Concluido,
    Falhou,
    Interrompida,
}

/// `motivo_fila` de ENT-07.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MotivoFila {
    Teto,
    TocaEm,
    Recurso,
}

/// ENT-07 Execução (o que a tela de execução mostra, inclusive os comandos).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Execucao {
    pub id: ExecucaoId,
    pub chat: ChatId,
    /// Vazio só para o orquestrador.
    pub tarefa: Option<TarefaId>,
    pub papel: NomePapel,
    pub modelo: String,
    pub prompt_versao: u32,
    /// Obrigatórios para `funcionalidade` (ERR-16).
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub status: StatusExecucao,
    /// Só quando `status = na_fila`.
    pub motivo_fila: Option<MotivoFila>,
    /// Comandos executados pelo agente, mascarados (RN-04); push, merge e push --force
    /// nunca entram, porque o núcleo os nega antes (RN-17, ERR-14).
    pub comandos: Vec<String>,
}

// ---------------------------------------------------------------------------
// ENT-08 PromptPapel
// ---------------------------------------------------------------------------

/// `fluxo` de ENT-08.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FluxoPrompt {
    Orquestrador,
    Funcionalidade,
    Teste,
    Adversarial,
    PassagemDeTurno,
}

/// ENT-08 PromptPapel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PromptPapel {
    pub fluxo: FluxoPrompt,
    /// 1 a 80 palavras (ERR-34, ERR-35).
    pub texto: String,
    pub versao: u32,
    pub palavras: u32,
    pub tokens: u32,
    pub vigente: bool,
}

// ---------------------------------------------------------------------------
// ENT-09 Relatório e ENT-24 Critério
// ---------------------------------------------------------------------------

/// `resultado` de ENT-09.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResultadoRelatorio {
    /// No fio, `concluido` (ASCII, como todo enum da IPC) = "concluído" do contrato (M-38).
    Concluido,
    Falhou,
    Bloqueado,
}

/// `status` de ENT-24, em ASCII no fio; a tela mostra o texto do contrato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusCriterio {
    /// ✔ no contrato; só com `prova` (ERR-20).
    Passou,
    /// ✘ no contrato (inclusive ✘ "sem prova").
    Falhou,
    /// "não verificado, presumido" no contrato.
    NaoVerificadoPresumido,
}

/// ENT-24 Critério.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Criterio {
    pub status: StatusCriterio,
    /// Saída de teste, captura ou resposta; obrigatória para `passou` (ERR-20).
    pub prova: Option<String>,
}

/// ENT-09 Relatório. Sem comando nem evento que o leve por ora: só o tipo do contrato.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Relatorio {
    pub id: RelatorioId,
    /// A execução que devolveu o relatório (ENT-07).
    pub execucao: ExecucaoId,
    pub resultado: ResultadoRelatorio,
    /// Até 1,5 mil tokens no total (RNF-10).
    pub o_que_fez: String,
    pub arquivos: Vec<String>,
    pub prova: String,
    /// Obrigatório quando `execucao.papel = teste`.
    pub criterios: Vec<Criterio>,
    pub duvidas: Vec<String>,
    pub candidatos_a_nota: Vec<String>,
    /// Vazio até o registro; preenchido antes da próxima delegação (RN-03, ERR-15).
    pub registrado_em: Option<DataHoraUtc>,
}

// ---------------------------------------------------------------------------
// ENT-10 Achado
// ---------------------------------------------------------------------------

/// `tipo` de ENT-10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TipoAchado {
    Bug,
    Sugestao,
}

/// `decisao` de ENT-10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DecisaoAchado {
    Corrigir,
    Limite,
    Opcional,
}

/// `status` de ENT-10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusAchado {
    Aberto,
    Fechado,
}

/// ENT-10 Achado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Achado {
    pub id: AchadoId,
    /// A execução que achou (ENT-07).
    pub execucao: ExecucaoId,
    pub tipo: TipoAchado,
    pub passos: Vec<String>,
    pub esperado: String,
    pub obtido: String,
    /// Caminho da imagem; obrigatório quando `execucao.papel = adversarial` (ERR-43 se não
    /// abrir).
    pub captura: Option<String>,
    /// Bug: sempre `corrigir`. Sugestão: obrigatória ao fechar a rodada (ERR-28).
    pub decisao: Option<DecisaoAchado>,
    pub status: StatusAchado,
}

// ---------------------------------------------------------------------------
// ENT-11 Leitura e ENT-26 PastaContexto
// ---------------------------------------------------------------------------

/// `origem` de ENT-11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrigemLeitura {
    Cli,
    /// Leitura sem CLI, que a tela marca "sem CLI" (RN-02).
    Arquivo,
}

/// ENT-11 Leitura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Leitura {
    pub id: LeituraId,
    pub execucao: ExecucaoId,
    pub caminho: String,
    /// Obrigatório quando `origem = cli` (RN-30).
    pub cofre: Option<CofreId>,
    pub origem: OrigemLeitura,
    pub hora: DataHoraUtc,
}

/// ENT-26 PastaContexto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PastaContexto {
    pub id: PastaContextoId,
    pub projeto: ProjetoId,
    /// Caminho absoluto de pasta no disco (ERR-32).
    pub caminho: String,
}

// ---------------------------------------------------------------------------
// ENT-12 Log
// ---------------------------------------------------------------------------

/// ENT-12 Log. A saída já chega mascarada (RN-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Log {
    pub id: LogId,
    pub execucao: ExecucaoId,
    pub comando: String,
    pub inicio: DataHoraUtc,
    /// Vazio enquanto roda.
    pub fim: Option<DataHoraUtc>,
    /// Vazio enquanto roda.
    pub codigo_saida: Option<i32>,
    pub saida_mascarada: Option<String>,
}

// ---------------------------------------------------------------------------
// ENT-13 Integração
// ---------------------------------------------------------------------------

/// `tipo` de ENT-13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TipoIntegracao {
    ClaudeCli,
    ObsidianCli,
    #[serde(rename = "github")]
    GitHub,
}

/// `status` de ENT-13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusIntegracao {
    Conectado,
    Desconectado,
    SemLogin,
    NaoInstalado,
}

/// ENT-13 Integração, sem a credencial (RN-04: ela nunca sai do cofre de credenciais).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Integracao {
    pub tipo: TipoIntegracao,
    pub status: StatusIntegracao,
    pub conta: Option<String>,
    pub versao: Option<String>,
    /// Notas `.md` do cofre checado; só Obsidian CLI `conectado` (AC-128, M-46).
    pub notas_indexadas: Option<u32>,
    /// O ERR do catálogo que explica o status, quando ele vem de uma falha (M-46), com
    /// os valores já mascarados (RN-04).
    pub erro: Option<ErroCatalogo>,
}

/// Instrução para o Humano durante a conexão (UC-01 passo 2), pelo canal do comando.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum InstrucaoConexao {
    /// Device flow do GitHub: digitar `codigo` em `endereco`.
    CodigoDoDispositivo { codigo: String, endereco: String },
    /// Texto livre (ex.: andamento do `/login` do Claude CLI).
    Mensagem { texto: String },
}

impl From<TipoIntegracaoCore> for TipoIntegracao {
    fn from(t: TipoIntegracaoCore) -> Self {
        match t {
            TipoIntegracaoCore::ClaudeCli => Self::ClaudeCli,
            TipoIntegracaoCore::ObsidianCli => Self::ObsidianCli,
            TipoIntegracaoCore::GitHub => Self::GitHub,
        }
    }
}

impl From<TipoIntegracao> for TipoIntegracaoCore {
    fn from(t: TipoIntegracao) -> Self {
        match t {
            TipoIntegracao::ClaudeCli => Self::ClaudeCli,
            TipoIntegracao::ObsidianCli => Self::ObsidianCli,
            TipoIntegracao::GitHub => Self::GitHub,
        }
    }
}

impl From<StatusIntegracaoCore> for StatusIntegracao {
    fn from(s: StatusIntegracaoCore) -> Self {
        match s {
            StatusIntegracaoCore::Conectado => Self::Conectado,
            StatusIntegracaoCore::Desconectado => Self::Desconectado,
            StatusIntegracaoCore::SemLogin => Self::SemLogin,
            StatusIntegracaoCore::NaoInstalado => Self::NaoInstalado,
        }
    }
}

impl From<EstadoIntegracao> for Integracao {
    fn from(e: EstadoIntegracao) -> Self {
        Self {
            tipo: e.tipo.into(),
            status: e.status.into(),
            conta: e.conta,
            versao: e.versao,
            // Acima de u32 não cabe na tela sem perder precisão; satura (cofre real: dezenas).
            notas_indexadas: e
                .notas_indexadas
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
            erro: e.erro.as_ref().map(ErroCatalogo::from),
        }
    }
}

impl From<my_harness_core::application::integracao::InstrucaoConexao> for InstrucaoConexao {
    fn from(i: my_harness_core::application::integracao::InstrucaoConexao) -> Self {
        use my_harness_core::application::integracao::InstrucaoConexao as Core;
        match i {
            Core::CodigoDoDispositivo { codigo, endereco } => {
                Self::CodigoDoDispositivo { codigo, endereco }
            }
            Core::Mensagem(texto) => Self::Mensagem { texto },
        }
    }
}

// ---------------------------------------------------------------------------
// ENT-14 Consumo
// ---------------------------------------------------------------------------

/// `origem` de ENT-14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrigemConsumo {
    /// Do `rate_limit_event`.
    Evento,
    /// Dos registros locais de sessão da CLI; a tela marca "estimado" (D7).
    Estimado,
}

/// `janela_status` e `semanal_status` de ENT-14: os valores do `rate_limit_event`
/// e `desconhecido` para qualquer outro (aviso ERR-50).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusConsumo {
    Allowed,
    AllowedWarning,
    Rejected,
    Desconhecido,
}

/// ENT-14 Consumo (a última leitura). `excedente` fica fora da tela por ora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Consumo {
    /// Percentual inteiro, 0 a 100.
    pub semanal_pct: u8,
    /// Percentual inteiro, 0 a 100.
    pub janela_pct: u8,
    /// Reinício da janela de 5 h.
    pub proximo_reinicio: DataHoraUtc,
    /// Reinício da semana de 7 dias.
    pub semanal_reinicio: DataHoraUtc,
    pub lida_em: DataHoraUtc,
    pub origem: OrigemConsumo,
    pub janela_status: StatusConsumo,
    pub semanal_status: StatusConsumo,
}

// ---------------------------------------------------------------------------
// ENT-15 Papel
// ---------------------------------------------------------------------------

/// ENT-15 Papel: o que a TEL-04 mostra e edita (as ferramentas e o cartão de briefing
/// ficam no núcleo, RN-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Papel {
    pub nome: NomePapel,
    pub modelo_padrao: String,
}

// ---------------------------------------------------------------------------
// Pendências da caixa de aval (CMP-10)
// ---------------------------------------------------------------------------

/// Algo esperando o Humano (CMP-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Pendencia {
    /// Plano em `proposto` (RF-08, UC-03).
    PlanoProposto { plano: PlanoId },
    /// Dúvida de subagente (RN-12).
    DuvidaDeSubagente { execucao: ExecucaoId, texto: String },
    /// 3ª reprovação de uma tarefa (RN-19, ERR-21).
    TetoDeVoltas { tarefa: TarefaId },
    /// Achados de uma rodada esperando decisão (RN-22, UC-07).
    AchadosParaDecidir { quantidade: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_hora_aceita_so_utc_com_z() {
        for ok in [
            "2026-11-19T18:20:00Z",
            "2026-11-19T18:20:00.5Z",
            "2026-11-19T18:20:00.123456789Z",
            "2024-02-29T00:00:00Z",
            "2026-12-31T23:59:59Z",
        ] {
            assert!(DataHoraUtc::new(ok).is_ok(), "{ok} devia passar");
        }
        for ruim in [
            "",
            "2026-11-19",
            "2026-11-19T18:20:00",
            "2026-11-19T18:20:00-03:00",
            "2026-11-19T18:20:00+00:00",
            "2026-11-19 18:20:00Z",
            "2026-11-19T18:20Z",
            "2026-11-19T18:20:00.Z",
            "2026-11-19T18:20:00.1234567890Z",
            "2026-11-19T18:20:00z",
            "2026-13-01T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-11-19T24:00:00Z",
            "2026-11-19T18:60:00Z",
            "2026-11-19T18:20:60Z",
            "2026-11-00T00:00:00Z",
            "２026-11-19T18:20:00Z",
        ] {
            assert!(DataHoraUtc::new(ruim).is_err(), "{ruim} devia falhar");
        }
    }

    #[test]
    fn data_hora_recusada_tambem_na_desserializacao() {
        assert!(serde_json::from_str::<DataHoraUtc>("\"2026-11-19T18:20:00-03:00\"").is_err());
        let ok: DataHoraUtc = serde_json::from_str("\"2026-11-19T18:20:00Z\"").unwrap();
        assert_eq!(
            serde_json::to_string(&ok).unwrap(),
            "\"2026-11-19T18:20:00Z\""
        );
    }

    #[test]
    fn enumeracoes_vao_em_snake_case() {
        let j = |v: &dyn erased::Ser| v.json();
        assert_eq!(j(&StatusIntegracao::SemLogin), "\"sem_login\"");
        assert_eq!(j(&StatusIntegracao::NaoInstalado), "\"nao_instalado\"");
        assert_eq!(j(&TipoIntegracao::GitHub), "\"github\"");
        assert_eq!(j(&TipoIntegracao::ClaudeCli), "\"claude_cli\"");
        assert_eq!(j(&StatusChat::SoLeitura), "\"so_leitura\"");
        assert_eq!(j(&StatusExecucao::NaFila), "\"na_fila\"");
        assert_eq!(j(&StatusExecucao::Concluido), "\"concluido\"");
        assert_eq!(j(&ResultadoRelatorio::Concluido), "\"concluido\"");
        assert_eq!(
            j(&StatusCriterio::NaoVerificadoPresumido),
            "\"nao_verificado_presumido\""
        );
        assert_eq!(j(&FluxoPrompt::PassagemDeTurno), "\"passagem_de_turno\"");
        assert_eq!(j(&StatusConsumo::AllowedWarning), "\"allowed_warning\"");
    }

    #[test]
    fn integracao_vem_do_estado_do_nucleo() {
        let estado = EstadoIntegracao::conectado(
            TipoIntegracaoCore::GitHub,
            Some("conta".into()),
            Some("1.0".into()),
        );
        let dto = Integracao::from(estado);
        assert_eq!(dto.tipo, TipoIntegracao::GitHub);
        assert_eq!(dto.status, StatusIntegracao::Conectado);
        assert_eq!((dto.notas_indexadas, dto.erro), (None, None));
        for t in TipoIntegracaoCore::TODAS {
            assert_eq!(TipoIntegracaoCore::from(TipoIntegracao::from(t)), t);
        }
    }

    // M-46: o número de notas e o erro do catálogo chegam à tela pelo DTO.
    #[test]
    fn integracao_leva_notas_indexadas_e_erro() {
        use my_harness_core::domain::integracao::ErroIntegracao;

        let obsidian = EstadoIntegracao::conectado(TipoIntegracaoCore::ObsidianCli, None, None)
            .com_notas_indexadas(Some(75));
        let dto = Integracao::from(obsidian);
        assert_eq!(dto.notas_indexadas, Some(75));
        assert_eq!(dto.erro, None);

        let falha = ErroIntegracao::inesperada(
            TipoIntegracaoCore::GitHub,
            "erro de rede com ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        );
        let dto = Integracao::from(EstadoIntegracao::a_partir_do_erro(&falha));
        assert_eq!(dto.status, StatusIntegracao::Desconectado);
        let erro = dto.erro.expect("erro do catálogo");
        assert_eq!(erro.id, "ERR-53");
        assert_eq!(erro.parametros["integracao"], "GitHub");
        assert_eq!(erro.parametros["motivo"], "erro de rede com ••••");
        let json = serde_json::to_value(Integracao::from(EstadoIntegracao::a_partir_do_erro(
            &ErroIntegracao::ClaudeCliNaoInstalado,
        )))
        .unwrap();
        assert_eq!(json["status"], "nao_instalado");
        assert_eq!(json["notas_indexadas"], serde_json::Value::Null);
        assert_eq!(json["erro"]["codigo"], "CLAUDE_CLI_NAO_INSTALADO");

        let muitas = EstadoIntegracao::conectado(TipoIntegracaoCore::ObsidianCli, None, None)
            .com_notas_indexadas(Some(u64::MAX));
        assert_eq!(Integracao::from(muitas).notas_indexadas, Some(u32::MAX));
    }

    mod erased {
        pub trait Ser {
            fn json(&self) -> String;
        }
        impl<T: serde::Serialize> Ser for T {
            fn json(&self) -> String {
                serde_json::to_string(self).unwrap()
            }
        }
    }
}
