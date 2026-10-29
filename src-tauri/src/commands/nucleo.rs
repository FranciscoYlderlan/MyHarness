//! A interface que os comandos chamam ([`Nucleo`]) e o corpo dela ([`NucleoMyHarness`]).
//!
//! Os comandos não sabem quem implementa o núcleo: o app guarda um `Arc<dyn Nucleo>`
//! como estado do Tauri. As integrações (UC-01) já são reais ([`ServicoIntegracoes`]);
//! o resto ainda é provisório: listas vazias, consumo indisponível (ERR-41) e ERR-55
//! `FALHA_INTERNA` ("ainda não implementado") em toda escrita. Nada é inventado.
//!
//! **Conexão em andamento** (UC-01 FA-02, AC-157; decisão
//! `geral-decisao-2026-11-26-conectar-repetido-reaproveita`): o núcleo guarda uma conexão
//! por integração. "Conectar" pedido de novo com ela em andamento (dois cliques, a tela
//! recarregada) não abre outro fluxo: recebe as instruções já mandadas, as próximas e o
//! mesmo resultado. "Desconectar" durante a conexão a cancela, e o `conectar` devolve o
//! estado que o núcleo guarda (`desconectado`; no GitHub, ERR-02 cancelado). Os dois
//! casos vão ao registro de operações do núcleo.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use my_harness_core::application::integracao::{
    InstrucaoConexao as InstrucaoCore, ObservadorConexao,
};
use my_harness_core::domain::integracao::{
    ErroIntegracao, EstadoIntegracao, MotivoOauthNaoConcluido, StatusIntegracao as StatusCore,
    TipoIntegracao as TipoCore,
};
use my_harness_core::domain::segredo::TextoMascarado;
use tauri::ipc::Channel;
use tokio::sync::{Notify, watch};

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

/// Quem recebe as operações de integração que o núcleo registra (conexão repetida
/// reaproveitada, conexão cancelada pelo "Desconectar"). Padrão: a saída de erro.
pub type RegistroDeOperacoes = Arc<dyn Fn(&str) + Send + Sync>;

/// O núcleo do app: integrações reais e o resto provisório.
pub struct NucleoMyHarness {
    integracoes: ServicoIntegracoes,
    registro: Arc<dyn RegistroNucleo>,
    conexoes: Mutex<HashMap<TipoIntegracao, Arc<ConexaoEmAndamento>>>,
    operacoes: RegistroDeOperacoes,
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
            conexoes: Mutex::new(HashMap::new()),
            operacoes: Arc::new(|texto| eprintln!("[myharness] {texto}")),
        }
    }

    /// Define quem recebe as operações de integração registradas (os testes guardam).
    pub fn com_registro_de_operacoes(mut self, operacoes: RegistroDeOperacoes) -> Self {
        self.operacoes = operacoes;
        self
    }

    fn conexoes(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<TipoIntegracao, Arc<ConexaoEmAndamento>>> {
        self.conexoes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Conduz a conexão nova `conexao` até o fim (ou o cancelamento) e publica o
    /// resultado para quem pediu "Conectar" de novo no meio.
    async fn conduzir(
        &self,
        tipo: TipoIntegracao,
        conexao: Arc<ConexaoEmAndamento>,
    ) -> Resultado<Integracao> {
        let fim = FimDaConexao {
            conexoes: &self.conexoes,
            tipo,
            conexao: conexao.clone(),
        };
        let observador = Arc::new(ObservadorConexoes(conexao.clone()));
        let resultado = tokio::select! {
            biased;
            // Cancelada: a conexão do serviço para aqui, sem avisar o estado dela; o
            // "Desconectar" que cancelou avisa o estado guardado.
            _ = conexao.esperar_cancelamento() => estado_cancelado(tipo),
            resultado = self.integracoes.conectar(tipo.into(), observador) => {
                if conexao.cancelada() {
                    estado_cancelado(tipo)
                } else {
                    resultado.map(Into::into)
                }
            }
        };
        fim.concluir(resultado.clone());
        resultado
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

/// Uma conexão em andamento (UC-01 FA-02): as instruções já mandadas, os canais de
/// quem pediu "Conectar", o pedido de cancelamento e o resultado, quando sai.
struct ConexaoEmAndamento {
    ouvintes: Mutex<Ouvintes>,
    cancelada: AtomicBool,
    aviso_cancelamento: Notify,
    resultado: watch::Sender<Option<Resultado<Integracao>>>,
}

#[derive(Default)]
struct Ouvintes {
    instrucoes: Vec<InstrucaoConexao>,
    canais: Vec<Channel<InstrucaoConexao>>,
}

impl ConexaoEmAndamento {
    fn nova(canal: Channel<InstrucaoConexao>) -> Self {
        Self {
            ouvintes: Mutex::new(Ouvintes {
                instrucoes: Vec::new(),
                canais: vec![canal],
            }),
            cancelada: AtomicBool::new(false),
            aviso_cancelamento: Notify::new(),
            resultado: watch::channel(None).0,
        }
    }

    fn ouvintes(&self) -> std::sync::MutexGuard<'_, Ouvintes> {
        self.ouvintes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Mais um pedido de "Conectar": recebe as instruções já mandadas e as próximas.
    fn juntar(&self, canal: Channel<InstrucaoConexao>) {
        let mut ouvintes = self.ouvintes();
        for instrucao in &ouvintes.instrucoes {
            // Tela fechada: a instrução se perde; a conexão segue.
            let _ = canal.send(instrucao.clone());
        }
        ouvintes.canais.push(canal);
    }

    fn mandar(&self, instrucao: InstrucaoConexao) {
        let mut ouvintes = self.ouvintes();
        for canal in &ouvintes.canais {
            let _ = canal.send(instrucao.clone());
        }
        ouvintes.instrucoes.push(instrucao);
    }

    fn cancelar(&self) {
        self.cancelada.store(true, Ordering::SeqCst);
        self.aviso_cancelamento.notify_waiters();
    }

    fn cancelada(&self) -> bool {
        self.cancelada.load(Ordering::SeqCst)
    }

    /// Termina só quando o cancelamento foi pedido.
    async fn esperar_cancelamento(&self) {
        loop {
            // Inscreve antes de olhar a marca, para não perder um aviso no meio.
            let aviso = self.aviso_cancelamento.notified();
            tokio::pin!(aviso);
            aviso.as_mut().enable();
            if self.cancelada() {
                return;
            }
            aviso.await;
        }
    }

    /// O resultado da conexão, quando ela termina.
    async fn esperar_resultado(&self, tipo: TipoIntegracao) -> Resultado<Integracao> {
        let mut receptor = self.resultado.subscribe();
        let publicado = receptor
            .wait_for(Option::is_some)
            .await
            .ok()
            .and_then(|valor| valor.clone());
        // O remetente vive com a conexão, que quem espera segura: sem resultado não
        // acontece; se acontecer, é ERR-53, e não pânico.
        publicado.unwrap_or_else(|| {
            Err(ErroIntegracao::inesperada(
                tipo.into(),
                "a conexão em andamento parou sem resultado",
            )
            .into())
        })
    }
}

/// Fim de uma conexão: sai da guarda e publica o resultado. Se a conexão é largada no
/// meio (o pedido foi abandonado), quem esperava recebe ERR-53, e não fica preso.
struct FimDaConexao<'a> {
    conexoes: &'a Mutex<HashMap<TipoIntegracao, Arc<ConexaoEmAndamento>>>,
    tipo: TipoIntegracao,
    conexao: Arc<ConexaoEmAndamento>,
}

impl FimDaConexao<'_> {
    fn concluir(self, resultado: Resultado<Integracao>) {
        self.sair_da_guarda();
        self.conexao.resultado.send_replace(Some(resultado));
    }

    fn sair_da_guarda(&self) {
        let mut conexoes = self.conexoes.lock().unwrap_or_else(|e| e.into_inner());
        if conexoes
            .get(&self.tipo)
            .is_some_and(|c| Arc::ptr_eq(c, &self.conexao))
        {
            conexoes.remove(&self.tipo);
        }
    }
}

impl Drop for FimDaConexao<'_> {
    fn drop(&mut self) {
        if self.conexao.resultado.borrow().is_some() {
            return;
        }
        self.sair_da_guarda();
        let erro = ErroIntegracao::inesperada(self.tipo.into(), "a conexão parou no meio");
        self.conexao.resultado.send_replace(Some(Err(erro.into())));
    }
}

/// Manda as instruções da conexão a todos que pediram "Conectar".
struct ObservadorConexoes(Arc<ConexaoEmAndamento>);

impl ObservadorConexao for ObservadorConexoes {
    fn instrucao(&self, instrucao: InstrucaoCore) {
        self.0.mandar(instrucao.into());
    }
}

/// O que o `conectar` cancelado pelo "Desconectar" devolve: o estado que o núcleo guarda
/// (`desconectado`; no GitHub, o ERR-02 cancelado, cujo estado final é `desconectado`).
fn estado_cancelado(tipo: TipoIntegracao) -> Resultado<Integracao> {
    match tipo {
        TipoIntegracao::GitHub => {
            Err(ErroIntegracao::GithubOauthNaoConcluido(MotivoOauthNaoConcluido::Cancelado).into())
        }
        _ => Ok(EstadoIntegracao::novo(tipo.into(), StatusCore::Desconectado).into()),
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
        let (conexao, nova) = {
            let mut conexoes = self.conexoes();
            match conexoes.get(&tipo) {
                Some(conexao) => {
                    conexao.juntar(instrucoes);
                    (conexao.clone(), false)
                }
                None => {
                    let conexao = Arc::new(ConexaoEmAndamento::nova(instrucoes));
                    conexoes.insert(tipo, conexao.clone());
                    (conexao, true)
                }
            }
        };
        if nova {
            return self.conduzir(tipo, conexao).await;
        }
        let tipo_core: TipoCore = tipo.into();
        (self.operacoes)(&format!(
            "conectar {tipo_core} repetido: reaproveita a conexão em andamento (UC-01 FA-02)"
        ));
        conexao.esperar_resultado(tipo).await
    }
    async fn desconectar_integracao(&self, tipo: TipoIntegracao) -> Resultado<Integracao> {
        let em_andamento = self.conexoes().get(&tipo).cloned();
        if let Some(conexao) = em_andamento {
            let tipo_core: TipoCore = tipo.into();
            (self.operacoes)(&format!(
                "desconectar {tipo_core} durante a conexão: cancela a conexão em andamento"
            ));
            conexao.cancelar();
            // A conexão para antes do "Desconectar": o estado dela não passa por cima.
            let _ = conexao.esperar_resultado(tipo).await;
        }
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
