//! A interface que os comandos chamam ([`Nucleo`]) e o corpo dela ([`NucleoMyHarness`]).
//!
//! Os comandos não sabem quem implementa o núcleo: o app guarda um `Arc<dyn Nucleo>`
//! como estado do Tauri. As integrações (UC-01) já são reais ([`ServicoIntegracoes`]);
//! o resto ainda é provisório: listas vazias, consumo indisponível (ERR-41) e ERR-55
//! `FALHA_INTERNA` ("ainda não implementado") em toda escrita. Nada é inventado.

use std::sync::Arc;

use async_trait::async_trait;
use my_harness_core::application::integracao::{
    InstrucaoConexao as InstrucaoCore, ObservadorConexao,
};
use my_harness_core::domain::integracao::TipoIntegracao as TipoCore;
use my_harness_core::domain::segredo::TextoMascarado;
use tauri::ipc::Channel;

use super::dto::*;
use super::erro::{CodigoErro, ComAvisos, ErroIpc};
use super::eventos::EventoExecucao;
use super::integracoes::{EntradaRegistro, RegistroNucleo, ServicoIntegracoes};

/// Resultado de todo comando.
pub type Resultado<T> = Result<T, ErroIpc>;

#[async_trait]
pub trait Nucleo: Send + Sync {
    // Integrações (ENT-13; UC-01; CMP-03, CMP-11)
    async fn listar_integracoes(&self) -> Resultado<Vec<Integracao>>;
    async fn testar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao>;
    async fn conectar_integracao(
        &self,
        tipo: TipoIntegracao,
        instrucoes: Channel<InstrucaoConexao>,
    ) -> Resultado<Integracao>;
    async fn desconectar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao>;

    // Consumo (ENT-14; CMP-01)
    async fn ler_consumo(&self) -> Resultado<Consumo>;

    // Projetos e histórico (ENT-01, ENT-03; CMP-09)
    async fn listar_projetos(&self) -> Resultado<Vec<Projeto>>;
    async fn criar_projeto(&self, novo: NovoProjeto) -> Resultado<ComAvisos<Projeto>>;
    async fn listar_chats(&self, projeto: ProjetoId) -> Resultado<Vec<Chat>>;
    async fn abrir_chat(&self, chat: ChatId) -> Resultado<ChatAberto>;

    // Chat (ENT-03, ENT-04; CMP-06, CMP-07)
    async fn novo_chat(&self, projeto: ProjetoId) -> Resultado<Chat>;
    async fn definir_limite_chat(
        &self,
        chat: ChatId,
        limite_tokens: u32,
    ) -> Resultado<ComAvisos<Chat>>;
    async fn enviar_prompt(
        &self,
        chat: ChatId,
        texto: String,
        modelo: Option<String>,
        saida: Channel<EventoExecucao>,
    ) -> Resultado<Execucao>;
    async fn aceitar_passagem(&self, chat: ChatId) -> Resultado<Chat>;
    async fn continuar_no_chat(&self, chat: ChatId) -> Resultado<Chat>;

    // Agentes (ENT-07; CMP-04, CMP-05)
    async fn listar_execucoes(&self, chat: ChatId) -> Resultado<Vec<Execucao>>;
    async fn acompanhar_execucao(
        &self,
        execucao: ExecucaoId,
        saida: Channel<EventoExecucao>,
    ) -> Resultado<()>;

    // Plano de delegação (ENT-05, ENT-06; CMP-12, CMP-13)
    async fn ler_plano(&self, plano: PlanoId) -> Resultado<PlanoDelegacao>;
    async fn trocar_modelo_tarefa(&self, tarefa: TarefaId, modelo: String) -> Resultado<Tarefa>;
    async fn aprovar_plano(&self, plano: PlanoId) -> Resultado<PlanoDelegacao>;
    async fn pedir_alteracao_plano(
        &self,
        plano: PlanoId,
        comentario: String,
    ) -> Resultado<PlanoDelegacao>;
    async fn recusar_plano(&self, plano: PlanoId) -> Resultado<PlanoDelegacao>;

    // Papéis e modelos (ENT-15; CMP-14)
    async fn listar_papeis(&self) -> Resultado<Vec<Papel>>;
    async fn listar_modelos_da_cli(&self) -> Resultado<Vec<String>>;
    async fn salvar_modelos_padrao(&self, papeis: Vec<Papel>) -> Resultado<Vec<Papel>>;
    async fn restaurar_modelos_padrao(&self) -> Resultado<Vec<Papel>>;

    // Achados (ENT-10; CMP-15)
    async fn listar_achados(&self, chat: ChatId) -> Resultado<Vec<Achado>>;
    async fn decidir_achado(&self, achado: AchadoId, decisao: DecisaoAchado) -> Resultado<Achado>;
    async fn confirmar_decisoes(&self, chat: ChatId) -> Resultado<()>;

    // Prompts de papel (ENT-08; CMP-16)
    async fn listar_prompts_papel(&self) -> Resultado<Vec<PromptPapel>>;
    async fn listar_versoes_prompt(&self, fluxo: FluxoPrompt) -> Resultado<Vec<PromptPapel>>;
    async fn salvar_prompt_papel(
        &self,
        fluxo: FluxoPrompt,
        texto: String,
    ) -> Resultado<PromptPapel>;
    async fn restaurar_prompt_padrao(&self, fluxo: FluxoPrompt) -> Resultado<PromptPapel>;

    // Cofres (ENT-02; CMP-17)
    async fn listar_cofres(&self, projeto: ProjetoId) -> Resultado<Vec<Cofre>>;
    async fn listar_cofres_da_cli(&self) -> Resultado<Vec<String>>;
    async fn ligar_cofre(
        &self,
        projeto: ProjetoId,
        nome: String,
        funcao: FuncaoCofre,
    ) -> Resultado<Cofre>;
    async fn definir_funcao_cofre(
        &self,
        cofre: CofreId,
        funcao: FuncaoCofre,
    ) -> Resultado<Vec<Cofre>>;
    async fn definir_cofre_ativo(&self, cofre: CofreId, ativo: bool) -> Resultado<Cofre>;
    async fn testar_cofre(&self, cofre: CofreId) -> Resultado<Cofre>;

    // Contexto (ENT-11, ENT-26; CMP-02)
    async fn listar_leituras(&self, chat: ChatId) -> Resultado<Vec<Leitura>>;
    async fn listar_pastas_contexto(&self, projeto: ProjetoId) -> Resultado<Vec<PastaContexto>>;
    async fn adicionar_pasta_contexto(
        &self,
        projeto: ProjetoId,
        caminho: String,
    ) -> Resultado<PastaContexto>;
    async fn remover_pasta_contexto(&self, pasta: PastaContextoId) -> Resultado<()>;

    // Logs (ENT-12; CMP-08)
    async fn listar_logs(&self, chat: ChatId) -> Resultado<Vec<Log>>;
}

/// O núcleo do app: integrações reais e o resto provisório.
pub struct NucleoMyHarness {
    integracoes: ServicoIntegracoes,
    registro: Arc<dyn RegistroNucleo>,
}

impl std::fmt::Debug for NucleoMyHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NucleoMyHarness")
            .field("integracoes", &self.integracoes)
            .finish_non_exhaustive()
    }
}

impl NucleoMyHarness {
    /// Núcleo com as `integracoes`; ERR-55 vai ao `registro`, como o ERR-53 do serviço.
    pub fn new(integracoes: ServicoIntegracoes, registro: Arc<dyn RegistroNucleo>) -> Self {
        Self {
            integracoes,
            registro,
        }
    }

    /// O serviço das integrações.
    pub fn integracoes(&self) -> &ServicoIntegracoes {
        &self.integracoes
    }

    /// ERR-55 de um comando que existe no contrato e o núcleo ainda não implementa,
    /// com o caso registrado.
    fn nao_implementado<T>(&self, comando: &str) -> Resultado<T> {
        let motivo =
            TextoMascarado::new(&format!("o comando {comando} ainda não foi implementado"));
        self.registro.registrar(EntradaRegistro {
            codigo: CodigoErro::FalhaInterna,
            comando: comando.to_owned(),
            motivo: motivo.clone(),
        });
        Err(ErroIpc::falha_interna(&motivo))
    }
}

/// Manda as instruções da conexão pelo canal do comando `conectar_integracao`.
struct ObservadorCanal(Channel<InstrucaoConexao>);

impl ObservadorConexao for ObservadorCanal {
    fn instrucao(&self, instrucao: InstrucaoCore) {
        // Tela fechada no meio da conexão: a conexão segue; a instrução se perde.
        let _ = self.0.send(instrucao.into());
    }
}

#[async_trait]
impl Nucleo for NucleoMyHarness {
    async fn listar_integracoes(&self) -> Resultado<Vec<Integracao>> {
        Ok(self
            .integracoes
            .listar()
            .await
            .into_iter()
            .map(Into::into)
            .collect())
    }
    async fn testar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao> {
        self.integracoes.testar(tipo.into()).await.map(Into::into)
    }
    async fn conectar_integracao(
        &self,
        tipo: TipoIntegracao,
        instrucoes: Channel<InstrucaoConexao>,
    ) -> Resultado<Integracao> {
        self.integracoes
            .conectar(tipo.into(), Arc::new(ObservadorCanal(instrucoes)))
            .await
            .map(Into::into)
    }
    async fn desconectar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao> {
        self.integracoes
            .desconectar(tipo.into())
            .await
            .map(Into::into)
    }

    async fn ler_consumo(&self) -> Resultado<Consumo> {
        Err(CodigoErro::ConsumoIndisponivel.into())
    }

    async fn listar_projetos(&self) -> Resultado<Vec<Projeto>> {
        Ok(Vec::new())
    }
    async fn criar_projeto(&self, _novo: NovoProjeto) -> Resultado<ComAvisos<Projeto>> {
        self.nao_implementado("criar_projeto")
    }
    async fn listar_chats(&self, _projeto: ProjetoId) -> Resultado<Vec<Chat>> {
        Ok(Vec::new())
    }
    async fn abrir_chat(&self, _chat: ChatId) -> Resultado<ChatAberto> {
        self.nao_implementado("abrir_chat")
    }

    async fn novo_chat(&self, _projeto: ProjetoId) -> Resultado<Chat> {
        self.nao_implementado("novo_chat")
    }
    async fn definir_limite_chat(
        &self,
        _chat: ChatId,
        _limite_tokens: u32,
    ) -> Resultado<ComAvisos<Chat>> {
        self.nao_implementado("definir_limite_chat")
    }
    async fn enviar_prompt(
        &self,
        _chat: ChatId,
        _texto: String,
        _modelo: Option<String>,
        _saida: Channel<EventoExecucao>,
    ) -> Resultado<Execucao> {
        // RN-08 (UC-02 FE-01): sem o Claude CLI conectado, nada é enviado (ERR-05).
        self.integracoes.exigir(TipoCore::ClaudeCli).await?;
        self.nao_implementado("enviar_prompt")
    }
    async fn aceitar_passagem(&self, _chat: ChatId) -> Resultado<Chat> {
        self.nao_implementado("aceitar_passagem")
    }
    async fn continuar_no_chat(&self, _chat: ChatId) -> Resultado<Chat> {
        self.nao_implementado("continuar_no_chat")
    }

    async fn listar_execucoes(&self, _chat: ChatId) -> Resultado<Vec<Execucao>> {
        Ok(Vec::new())
    }
    async fn acompanhar_execucao(
        &self,
        _execucao: ExecucaoId,
        _saida: Channel<EventoExecucao>,
    ) -> Resultado<()> {
        self.nao_implementado("acompanhar_execucao")
    }

    async fn ler_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        self.nao_implementado("ler_plano")
    }
    async fn trocar_modelo_tarefa(&self, _tarefa: TarefaId, _modelo: String) -> Resultado<Tarefa> {
        self.nao_implementado("trocar_modelo_tarefa")
    }
    async fn aprovar_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        self.nao_implementado("aprovar_plano")
    }
    async fn pedir_alteracao_plano(
        &self,
        _plano: PlanoId,
        _comentario: String,
    ) -> Resultado<PlanoDelegacao> {
        self.nao_implementado("pedir_alteracao_plano")
    }
    async fn recusar_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        self.nao_implementado("recusar_plano")
    }

    async fn listar_papeis(&self) -> Resultado<Vec<Papel>> {
        Ok(Vec::new())
    }
    async fn listar_modelos_da_cli(&self) -> Resultado<Vec<String>> {
        Ok(Vec::new())
    }
    async fn salvar_modelos_padrao(&self, _papeis: Vec<Papel>) -> Resultado<Vec<Papel>> {
        self.nao_implementado("salvar_modelos_padrao")
    }
    async fn restaurar_modelos_padrao(&self) -> Resultado<Vec<Papel>> {
        self.nao_implementado("restaurar_modelos_padrao")
    }

    async fn listar_achados(&self, _chat: ChatId) -> Resultado<Vec<Achado>> {
        Ok(Vec::new())
    }
    async fn decidir_achado(
        &self,
        _achado: AchadoId,
        _decisao: DecisaoAchado,
    ) -> Resultado<Achado> {
        self.nao_implementado("decidir_achado")
    }
    async fn confirmar_decisoes(&self, _chat: ChatId) -> Resultado<()> {
        self.nao_implementado("confirmar_decisoes")
    }

    async fn listar_prompts_papel(&self) -> Resultado<Vec<PromptPapel>> {
        Ok(Vec::new())
    }
    async fn listar_versoes_prompt(&self, _fluxo: FluxoPrompt) -> Resultado<Vec<PromptPapel>> {
        Ok(Vec::new())
    }
    async fn salvar_prompt_papel(
        &self,
        _fluxo: FluxoPrompt,
        _texto: String,
    ) -> Resultado<PromptPapel> {
        self.nao_implementado("salvar_prompt_papel")
    }
    async fn restaurar_prompt_padrao(&self, _fluxo: FluxoPrompt) -> Resultado<PromptPapel> {
        self.nao_implementado("restaurar_prompt_padrao")
    }

    async fn listar_cofres(&self, _projeto: ProjetoId) -> Resultado<Vec<Cofre>> {
        Ok(Vec::new())
    }
    async fn listar_cofres_da_cli(&self) -> Resultado<Vec<String>> {
        Ok(Vec::new())
    }
    async fn ligar_cofre(
        &self,
        _projeto: ProjetoId,
        _nome: String,
        _funcao: FuncaoCofre,
    ) -> Resultado<Cofre> {
        // RN-08 (UC-14): ligar um cofre testa uma busca pela Obsidian CLI (ERR-07).
        self.integracoes.exigir(TipoCore::ObsidianCli).await?;
        self.nao_implementado("ligar_cofre")
    }
    async fn definir_funcao_cofre(
        &self,
        _cofre: CofreId,
        _funcao: FuncaoCofre,
    ) -> Resultado<Vec<Cofre>> {
        self.nao_implementado("definir_funcao_cofre")
    }
    async fn definir_cofre_ativo(&self, _cofre: CofreId, _ativo: bool) -> Resultado<Cofre> {
        self.nao_implementado("definir_cofre_ativo")
    }
    async fn testar_cofre(&self, _cofre: CofreId) -> Resultado<Cofre> {
        self.integracoes.exigir(TipoCore::ObsidianCli).await?;
        self.nao_implementado("testar_cofre")
    }

    async fn listar_leituras(&self, _chat: ChatId) -> Resultado<Vec<Leitura>> {
        Ok(Vec::new())
    }
    async fn listar_pastas_contexto(&self, _projeto: ProjetoId) -> Resultado<Vec<PastaContexto>> {
        Ok(Vec::new())
    }
    async fn adicionar_pasta_contexto(
        &self,
        _projeto: ProjetoId,
        _caminho: String,
    ) -> Resultado<PastaContexto> {
        self.nao_implementado("adicionar_pasta_contexto")
    }
    async fn remover_pasta_contexto(&self, _pasta: PastaContextoId) -> Resultado<()> {
        self.nao_implementado("remover_pasta_contexto")
    }

    async fn listar_logs(&self, _chat: ChatId) -> Resultado<Vec<Log>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;
