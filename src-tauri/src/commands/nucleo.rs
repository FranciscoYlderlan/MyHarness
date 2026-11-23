//! A interface que os comandos chamam ([`Nucleo`]) e o corpo provisório dela
//! ([`NucleoProvisorio`]), sem banco e sem regra de negócio.
//!
//! Os comandos não sabem quem implementa o núcleo: o app guarda um `Arc<dyn Nucleo>`
//! como estado do Tauri. As etapas seguintes trocam o provisório pelo núcleo de verdade
//! (casos de uso de `my-harness-core` + `infra/`) sem mudar o contrato.

use async_trait::async_trait;
use tauri::ipc::Channel;

use super::dto::*;
use super::erro::{CodigoErro, ComAvisos, ErroIpc, MotivoInterno};
use super::eventos::EventoExecucao;

/// Resultado de todo comando.
pub type Resultado<T> = Result<T, ErroIpc>;

/// Operações que a tela pede ao núcleo (uma por comando do contrato IPC).
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

/// Corpo provisório: listas vazias, integrações sem verificação (`desconectado`),
/// consumo indisponível (ERR-41) e `nao_implementado` em toda escrita. Nada é inventado.
#[derive(Debug, Default, Clone, Copy)]
pub struct NucleoProvisorio;

fn nao_implementado<T>() -> Resultado<T> {
    Err(ErroIpc::Interno {
        motivo: MotivoInterno::NaoImplementado,
    })
}

fn nao_verificada(tipo: TipoIntegracao) -> Integracao {
    Integracao {
        tipo,
        status: StatusIntegracao::Desconectado,
        conta: None,
        versao: None,
    }
}

#[async_trait]
impl Nucleo for NucleoProvisorio {
    async fn listar_integracoes(&self) -> Resultado<Vec<Integracao>> {
        Ok(my_harness_core::domain::integracao::TipoIntegracao::TODAS
            .into_iter()
            .map(|t| nao_verificada(t.into()))
            .collect())
    }
    async fn testar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao> {
        Ok(nao_verificada(tipo))
    }
    async fn conectar_integracao(
        &self,
        _tipo: TipoIntegracao,
        _instrucoes: Channel<InstrucaoConexao>,
    ) -> Resultado<Integracao> {
        nao_implementado()
    }
    async fn desconectar_integracao(&self, _tipo: TipoIntegracao) -> Resultado<Integracao> {
        nao_implementado()
    }

    async fn ler_consumo(&self) -> Resultado<Consumo> {
        Err(CodigoErro::ConsumoIndisponivel.into())
    }

    async fn listar_projetos(&self) -> Resultado<Vec<Projeto>> {
        Ok(Vec::new())
    }
    async fn criar_projeto(&self, _novo: NovoProjeto) -> Resultado<ComAvisos<Projeto>> {
        nao_implementado()
    }
    async fn listar_chats(&self, _projeto: ProjetoId) -> Resultado<Vec<Chat>> {
        Ok(Vec::new())
    }
    async fn abrir_chat(&self, _chat: ChatId) -> Resultado<ChatAberto> {
        nao_implementado()
    }

    async fn novo_chat(&self, _projeto: ProjetoId) -> Resultado<Chat> {
        nao_implementado()
    }
    async fn definir_limite_chat(
        &self,
        _chat: ChatId,
        _limite_tokens: u32,
    ) -> Resultado<ComAvisos<Chat>> {
        nao_implementado()
    }
    async fn enviar_prompt(
        &self,
        _chat: ChatId,
        _texto: String,
        _modelo: Option<String>,
        _saida: Channel<EventoExecucao>,
    ) -> Resultado<Execucao> {
        nao_implementado()
    }
    async fn aceitar_passagem(&self, _chat: ChatId) -> Resultado<Chat> {
        nao_implementado()
    }
    async fn continuar_no_chat(&self, _chat: ChatId) -> Resultado<Chat> {
        nao_implementado()
    }

    async fn listar_execucoes(&self, _chat: ChatId) -> Resultado<Vec<Execucao>> {
        Ok(Vec::new())
    }
    async fn acompanhar_execucao(
        &self,
        _execucao: ExecucaoId,
        _saida: Channel<EventoExecucao>,
    ) -> Resultado<()> {
        nao_implementado()
    }

    async fn ler_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        nao_implementado()
    }
    async fn trocar_modelo_tarefa(&self, _tarefa: TarefaId, _modelo: String) -> Resultado<Tarefa> {
        nao_implementado()
    }
    async fn aprovar_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        nao_implementado()
    }
    async fn pedir_alteracao_plano(
        &self,
        _plano: PlanoId,
        _comentario: String,
    ) -> Resultado<PlanoDelegacao> {
        nao_implementado()
    }
    async fn recusar_plano(&self, _plano: PlanoId) -> Resultado<PlanoDelegacao> {
        nao_implementado()
    }

    async fn listar_papeis(&self) -> Resultado<Vec<Papel>> {
        Ok(Vec::new())
    }
    async fn listar_modelos_da_cli(&self) -> Resultado<Vec<String>> {
        Ok(Vec::new())
    }
    async fn salvar_modelos_padrao(&self, _papeis: Vec<Papel>) -> Resultado<Vec<Papel>> {
        nao_implementado()
    }
    async fn restaurar_modelos_padrao(&self) -> Resultado<Vec<Papel>> {
        nao_implementado()
    }

    async fn listar_achados(&self, _chat: ChatId) -> Resultado<Vec<Achado>> {
        Ok(Vec::new())
    }
    async fn decidir_achado(
        &self,
        _achado: AchadoId,
        _decisao: DecisaoAchado,
    ) -> Resultado<Achado> {
        nao_implementado()
    }
    async fn confirmar_decisoes(&self, _chat: ChatId) -> Resultado<()> {
        nao_implementado()
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
        nao_implementado()
    }
    async fn restaurar_prompt_padrao(&self, _fluxo: FluxoPrompt) -> Resultado<PromptPapel> {
        nao_implementado()
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
        nao_implementado()
    }
    async fn definir_funcao_cofre(
        &self,
        _cofre: CofreId,
        _funcao: FuncaoCofre,
    ) -> Resultado<Vec<Cofre>> {
        nao_implementado()
    }
    async fn definir_cofre_ativo(&self, _cofre: CofreId, _ativo: bool) -> Resultado<Cofre> {
        nao_implementado()
    }
    async fn testar_cofre(&self, _cofre: CofreId) -> Resultado<Cofre> {
        nao_implementado()
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
        nao_implementado()
    }
    async fn remover_pasta_contexto(&self, _pasta: PastaContextoId) -> Resultado<()> {
        nao_implementado()
    }

    async fn listar_logs(&self, _chat: ChatId) -> Resultado<Vec<Log>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::erro::ErroCatalogo;

    #[tokio::test]
    async fn provisorio_nao_inventa_dados() {
        let n: std::sync::Arc<dyn Nucleo> = std::sync::Arc::new(NucleoProvisorio);
        let integracoes = n.listar_integracoes().await.unwrap();
        assert_eq!(
            integracoes.iter().map(|i| i.tipo).collect::<Vec<_>>(),
            [
                TipoIntegracao::ClaudeCli,
                TipoIntegracao::ObsidianCli,
                TipoIntegracao::GitHub
            ]
        );
        assert!(
            integracoes
                .iter()
                .all(|i| i.status == StatusIntegracao::Desconectado)
        );
        assert_eq!(
            n.ler_consumo().await.unwrap_err(),
            ErroIpc::Catalogo {
                erro: ErroCatalogo::new(CodigoErro::ConsumoIndisponivel)
            }
        );
        assert!(n.listar_projetos().await.unwrap().is_empty());
        let nao_implementado = ErroIpc::Interno {
            motivo: MotivoInterno::NaoImplementado,
        };
        assert_eq!(
            n.aprovar_plano(PlanoId("p1".into())).await.unwrap_err(),
            nao_implementado
        );
        assert_eq!(
            n.desconectar_integracao(TipoIntegracao::GitHub)
                .await
                .unwrap_err(),
            nao_implementado
        );
    }
}
