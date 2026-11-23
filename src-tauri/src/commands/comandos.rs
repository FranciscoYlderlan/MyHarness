//! Comandos do Tauri: a porta da tela para o núcleo. Cada um só repassa ao
//! [`Nucleo`] guardado no estado do app; nenhuma regra mora aqui.

use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use super::dto::*;
use super::erro::ComAvisos;
use super::eventos::EventoExecucao;
use super::nucleo::{Nucleo, Resultado};

/// O núcleo como estado do Tauri.
pub type NucleoApp = Arc<dyn Nucleo>;

// Integrações (ENT-13; UC-01; CMP-03, CMP-11)
/// Status das três integrações, na ordem do painel (UC-01 passo 1).
#[tauri::command]
#[specta::specta]
pub async fn listar_integracoes(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<Integracao>> {
    nucleo.listar_integracoes().await
}
/// "Testar conexão" (CMP-11). Erros: ERR-01, ERR-03, ERR-53.
#[tauri::command]
#[specta::specta]
pub async fn testar_integracao(
    nucleo: State<'_, NucleoApp>,
    tipo: TipoIntegracao,
) -> Resultado<Integracao> {
    nucleo.testar_integracao(tipo).await
}
/// Conecta (UC-01 passos 2 e 3); o que o Humano precisa ver chega por `instrucoes`. Erros: ERR-01 a ERR-04, ERR-53.
#[tauri::command]
#[specta::specta]
pub async fn conectar_integracao(
    nucleo: State<'_, NucleoApp>,
    tipo: TipoIntegracao,
    instrucoes: Channel<InstrucaoConexao>,
) -> Resultado<Integracao> {
    nucleo.conectar_integracao(tipo, instrucoes).await
}
/// Desconecta (CMP-11). Erro: ERR-53.
#[tauri::command]
#[specta::specta]
pub async fn desconectar_integracao(
    nucleo: State<'_, NucleoApp>,
    tipo: TipoIntegracao,
) -> Resultado<Integracao> {
    nucleo.desconectar_integracao(tipo).await
}

// Consumo (ENT-14; CMP-01)
/// Última leitura de consumo (CMP-01). Erro: ERR-41.
#[tauri::command]
#[specta::specta]
pub async fn ler_consumo(nucleo: State<'_, NucleoApp>) -> Resultado<Consumo> {
    nucleo.ler_consumo().await
}

// Projetos e histórico (ENT-01, ENT-03; CMP-09)
/// Projetos do histórico (CMP-09).
#[tauri::command]
#[specta::specta]
pub async fn listar_projetos(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<Projeto>> {
    nucleo.listar_projetos().await
}
/// Cria um projeto. Erros: ERR-45, ERR-46, ERR-52; aviso: ERR-47.
#[tauri::command]
#[specta::specta]
pub async fn criar_projeto(
    nucleo: State<'_, NucleoApp>,
    novo: NovoProjeto,
) -> Resultado<ComAvisos<Projeto>> {
    nucleo.criar_projeto(novo).await
}
/// Chats de um projeto, com a cadeia de passagens (CMP-09).
#[tauri::command]
#[specta::specta]
pub async fn listar_chats(
    nucleo: State<'_, NucleoApp>,
    projeto: ProjetoId,
) -> Resultado<Vec<Chat>> {
    nucleo.listar_chats(projeto).await
}
/// Reabre um chat com a conversa (UC-12). Erro: ERR-33 (abre só leitura).
#[tauri::command]
#[specta::specta]
pub async fn abrir_chat(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<ChatAberto> {
    nucleo.abrir_chat(chat).await
}

// Chat (ENT-03, ENT-04; CMP-06, CMP-07)
/// "Novo chat" (CMP-07).
#[tauri::command]
#[specta::specta]
pub async fn novo_chat(nucleo: State<'_, NucleoApp>, projeto: ProjetoId) -> Resultado<Chat> {
    nucleo.novo_chat(projeto).await
}
/// Troca o limite do chat (CMP-07). Erro: ERR-52; aviso: ERR-47.
#[tauri::command]
#[specta::specta]
pub async fn definir_limite_chat(
    nucleo: State<'_, NucleoApp>,
    chat: ChatId,
    limite_tokens: u32,
) -> Resultado<ComAvisos<Chat>> {
    nucleo.definir_limite_chat(chat, limite_tokens).await
}
/// Envia o pedido ao orquestrador (CMP-06); a resposta chega por `saida`. Erros: ERR-05, ERR-08, ERR-11, ERR-42.
#[tauri::command]
#[specta::specta]
pub async fn enviar_prompt(
    nucleo: State<'_, NucleoApp>,
    chat: ChatId,
    texto: String,
    modelo: Option<String>,
    saida: Channel<EventoExecucao>,
) -> Resultado<Execucao> {
    nucleo.enviar_prompt(chat, texto, modelo, saida).await
}
/// "Passar o turno" no aviso de limite (RN-23, UC-08). Erros: ERR-07, ERR-29.
#[tauri::command]
#[specta::specta]
pub async fn aceitar_passagem(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<Chat> {
    nucleo.aceitar_passagem(chat).await
}
/// "Continuar neste chat" no aviso de limite (RN-23).
#[tauri::command]
#[specta::specta]
pub async fn continuar_no_chat(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<Chat> {
    nucleo.continuar_no_chat(chat).await
}

// Agentes (ENT-07; CMP-04, CMP-05)
/// Execuções do chat, com fila e status (CMP-05).
#[tauri::command]
#[specta::specta]
pub async fn listar_execucoes(
    nucleo: State<'_, NucleoApp>,
    chat: ChatId,
) -> Resultado<Vec<Execucao>> {
    nucleo.listar_execucoes(chat).await
}
/// Abre o canal da saída ao vivo de uma execução (CMP-04).
#[tauri::command]
#[specta::specta]
pub async fn acompanhar_execucao(
    nucleo: State<'_, NucleoApp>,
    execucao: ExecucaoId,
    saida: Channel<EventoExecucao>,
) -> Resultado<()> {
    nucleo.acompanhar_execucao(execucao, saida).await
}

// Plano de delegação (ENT-05, ENT-06; CMP-12, CMP-13)
/// Plano com as tarefas (TEL-03).
#[tauri::command]
#[specta::specta]
pub async fn ler_plano(nucleo: State<'_, NucleoApp>, plano: PlanoId) -> Resultado<PlanoDelegacao> {
    nucleo.ler_plano(plano).await
}
/// Troca o modelo de uma tarefa (CMP-12). Erro: ERR-30.
#[tauri::command]
#[specta::specta]
pub async fn trocar_modelo_tarefa(
    nucleo: State<'_, NucleoApp>,
    tarefa: TarefaId,
    modelo: String,
) -> Resultado<Tarefa> {
    nucleo.trocar_modelo_tarefa(tarefa, modelo).await
}
/// "Aprovar" (CMP-13). Erros: ERR-05, ERR-06, ERR-07, ERR-49.
#[tauri::command]
#[specta::specta]
pub async fn aprovar_plano(
    nucleo: State<'_, NucleoApp>,
    plano: PlanoId,
) -> Resultado<PlanoDelegacao> {
    nucleo.aprovar_plano(plano).await
}
/// "Pedir alteração" (CMP-13).
#[tauri::command]
#[specta::specta]
pub async fn pedir_alteracao_plano(
    nucleo: State<'_, NucleoApp>,
    plano: PlanoId,
    comentario: String,
) -> Resultado<PlanoDelegacao> {
    nucleo.pedir_alteracao_plano(plano, comentario).await
}
/// "Recusar" (CMP-13).
#[tauri::command]
#[specta::specta]
pub async fn recusar_plano(
    nucleo: State<'_, NucleoApp>,
    plano: PlanoId,
) -> Resultado<PlanoDelegacao> {
    nucleo.recusar_plano(plano).await
}

// Papéis e modelos (ENT-15; CMP-14)
/// Papéis com o modelo padrão (CMP-14).
#[tauri::command]
#[specta::specta]
pub async fn listar_papeis(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<Papel>> {
    nucleo.listar_papeis().await
}
/// Modelos disponíveis na CLI (CMP-14). Erro: ERR-05.
#[tauri::command]
#[specta::specta]
pub async fn listar_modelos_da_cli(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<String>> {
    nucleo.listar_modelos_da_cli().await
}
/// Salva os padrões (CMP-14). Erro: ERR-30.
#[tauri::command]
#[specta::specta]
pub async fn salvar_modelos_padrao(
    nucleo: State<'_, NucleoApp>,
    papeis: Vec<Papel>,
) -> Resultado<Vec<Papel>> {
    nucleo.salvar_modelos_padrao(papeis).await
}
/// Restaura os padrões de fábrica (UC-09 FA-01).
#[tauri::command]
#[specta::specta]
pub async fn restaurar_modelos_padrao(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<Papel>> {
    nucleo.restaurar_modelos_padrao().await
}

// Achados (ENT-10; CMP-15)
/// Achados da rodada do chat (CMP-15).
#[tauri::command]
#[specta::specta]
pub async fn listar_achados(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<Vec<Achado>> {
    nucleo.listar_achados(chat).await
}
/// Decide uma sugestão (CMP-15).
#[tauri::command]
#[specta::specta]
pub async fn decidir_achado(
    nucleo: State<'_, NucleoApp>,
    achado: AchadoId,
    decisao: DecisaoAchado,
) -> Resultado<Achado> {
    nucleo.decidir_achado(achado, decisao).await
}
/// "Confirmar" as decisões e fechar a rodada (UC-07). Erro: ERR-28.
#[tauri::command]
#[specta::specta]
pub async fn confirmar_decisoes(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<()> {
    nucleo.confirmar_decisoes(chat).await
}

// Prompts de papel (ENT-08; CMP-16)
/// Versão vigente de cada fluxo (CMP-16).
#[tauri::command]
#[specta::specta]
pub async fn listar_prompts_papel(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<PromptPapel>> {
    nucleo.listar_prompts_papel().await
}
/// Histórico de versões de um fluxo (CMP-16).
#[tauri::command]
#[specta::specta]
pub async fn listar_versoes_prompt(
    nucleo: State<'_, NucleoApp>,
    fluxo: FluxoPrompt,
) -> Resultado<Vec<PromptPapel>> {
    nucleo.listar_versoes_prompt(fluxo).await
}
/// Salva versão nova (RF-31). Erros: ERR-34, ERR-35.
#[tauri::command]
#[specta::specta]
pub async fn salvar_prompt_papel(
    nucleo: State<'_, NucleoApp>,
    fluxo: FluxoPrompt,
    texto: String,
) -> Resultado<PromptPapel> {
    nucleo.salvar_prompt_papel(fluxo, texto).await
}
/// Restaura o padrão de fábrica do fluxo (CMP-16).
#[tauri::command]
#[specta::specta]
pub async fn restaurar_prompt_padrao(
    nucleo: State<'_, NucleoApp>,
    fluxo: FluxoPrompt,
) -> Resultado<PromptPapel> {
    nucleo.restaurar_prompt_padrao(fluxo).await
}

// Cofres (ENT-02; CMP-17)
/// Cofres ligados ao projeto (CMP-17).
#[tauri::command]
#[specta::specta]
pub async fn listar_cofres(
    nucleo: State<'_, NucleoApp>,
    projeto: ProjetoId,
) -> Resultado<Vec<Cofre>> {
    nucleo.listar_cofres(projeto).await
}
/// Cofres que a Obsidian CLI enxerga (CMP-17). Erro: ERR-07.
#[tauri::command]
#[specta::specta]
pub async fn listar_cofres_da_cli(nucleo: State<'_, NucleoApp>) -> Resultado<Vec<String>> {
    nucleo.listar_cofres_da_cli().await
}
/// Liga um cofre ao projeto (UC-14). Erro: ERR-37.
#[tauri::command]
#[specta::specta]
pub async fn ligar_cofre(
    nucleo: State<'_, NucleoApp>,
    projeto: ProjetoId,
    nome: String,
    funcao: FuncaoCofre,
) -> Resultado<Cofre> {
    nucleo.ligar_cofre(projeto, nome, funcao).await
}
/// Troca a função; trocar a memória devolve os dois cofres alterados (UC-14 FA-01). Erro: ERR-38.
#[tauri::command]
#[specta::specta]
pub async fn definir_funcao_cofre(
    nucleo: State<'_, NucleoApp>,
    cofre: CofreId,
    funcao: FuncaoCofre,
) -> Resultado<Vec<Cofre>> {
    nucleo.definir_funcao_cofre(cofre, funcao).await
}
/// Ativa ou desativa um cofre (UC-14 FA-01).
#[tauri::command]
#[specta::specta]
pub async fn definir_cofre_ativo(
    nucleo: State<'_, NucleoApp>,
    cofre: CofreId,
    ativo: bool,
) -> Resultado<Cofre> {
    nucleo.definir_cofre_ativo(cofre, ativo).await
}
/// "Testar busca" (CMP-17). Erro: ERR-37.
#[tauri::command]
#[specta::specta]
pub async fn testar_cofre(nucleo: State<'_, NucleoApp>, cofre: CofreId) -> Resultado<Cofre> {
    nucleo.testar_cofre(cofre).await
}

// Contexto (ENT-11, ENT-26; CMP-02)
/// O que os agentes do chat leram (CMP-02).
#[tauri::command]
#[specta::specta]
pub async fn listar_leituras(
    nucleo: State<'_, NucleoApp>,
    chat: ChatId,
) -> Resultado<Vec<Leitura>> {
    nucleo.listar_leituras(chat).await
}
/// Pastas em contexto (CMP-02).
#[tauri::command]
#[specta::specta]
pub async fn listar_pastas_contexto(
    nucleo: State<'_, NucleoApp>,
    projeto: ProjetoId,
) -> Resultado<Vec<PastaContexto>> {
    nucleo.listar_pastas_contexto(projeto).await
}
/// Adiciona uma pasta (UC-11). Erro: ERR-32.
#[tauri::command]
#[specta::specta]
pub async fn adicionar_pasta_contexto(
    nucleo: State<'_, NucleoApp>,
    projeto: ProjetoId,
    caminho: String,
) -> Resultado<PastaContexto> {
    nucleo.adicionar_pasta_contexto(projeto, caminho).await
}
/// Remove uma pasta (UC-11 FA-01).
#[tauri::command]
#[specta::specta]
pub async fn remover_pasta_contexto(
    nucleo: State<'_, NucleoApp>,
    pasta: PastaContextoId,
) -> Resultado<()> {
    nucleo.remover_pasta_contexto(pasta).await
}

// Logs (ENT-12; CMP-08)
/// Comandos executados no chat (CMP-08).
#[tauri::command]
#[specta::specta]
pub async fn listar_logs(nucleo: State<'_, NucleoApp>, chat: ChatId) -> Resultado<Vec<Log>> {
    nucleo.listar_logs(chat).await
}

/// Todos os comandos do contrato, para o `tauri_specta::Builder`.
pub fn todos() -> tauri_specta::Commands<tauri::Wry> {
    tauri_specta::collect_commands![
        listar_integracoes,
        testar_integracao,
        conectar_integracao,
        desconectar_integracao,
        ler_consumo,
        listar_projetos,
        criar_projeto,
        listar_chats,
        abrir_chat,
        novo_chat,
        definir_limite_chat,
        enviar_prompt,
        aceitar_passagem,
        continuar_no_chat,
        listar_execucoes,
        acompanhar_execucao,
        ler_plano,
        trocar_modelo_tarefa,
        aprovar_plano,
        pedir_alteracao_plano,
        recusar_plano,
        listar_papeis,
        listar_modelos_da_cli,
        salvar_modelos_padrao,
        restaurar_modelos_padrao,
        listar_achados,
        decidir_achado,
        confirmar_decisoes,
        listar_prompts_papel,
        listar_versoes_prompt,
        salvar_prompt_papel,
        restaurar_prompt_padrao,
        listar_cofres,
        listar_cofres_da_cli,
        ligar_cofre,
        definir_funcao_cofre,
        definir_cofre_ativo,
        testar_cofre,
        listar_leituras,
        listar_pastas_contexto,
        adicionar_pasta_contexto,
        remover_pasta_contexto,
        listar_logs,
    ]
}

/// Nomes dos comandos, na ordem do contrato (para testes).
#[cfg(test)]
pub const NOMES: &[&str] = &[
    "listar_integracoes",
    "testar_integracao",
    "conectar_integracao",
    "desconectar_integracao",
    "ler_consumo",
    "listar_projetos",
    "criar_projeto",
    "listar_chats",
    "abrir_chat",
    "novo_chat",
    "definir_limite_chat",
    "enviar_prompt",
    "aceitar_passagem",
    "continuar_no_chat",
    "listar_execucoes",
    "acompanhar_execucao",
    "ler_plano",
    "trocar_modelo_tarefa",
    "aprovar_plano",
    "pedir_alteracao_plano",
    "recusar_plano",
    "listar_papeis",
    "listar_modelos_da_cli",
    "salvar_modelos_padrao",
    "restaurar_modelos_padrao",
    "listar_achados",
    "decidir_achado",
    "confirmar_decisoes",
    "listar_prompts_papel",
    "listar_versoes_prompt",
    "salvar_prompt_papel",
    "restaurar_prompt_padrao",
    "listar_cofres",
    "listar_cofres_da_cli",
    "ligar_cofre",
    "definir_funcao_cofre",
    "definir_cofre_ativo",
    "testar_cofre",
    "listar_leituras",
    "listar_pastas_contexto",
    "adicionar_pasta_contexto",
    "remover_pasta_contexto",
    "listar_logs",
];
