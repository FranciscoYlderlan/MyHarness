//! Interface comum das integrações (RNF-11) e o passo 1 de UC-01 (checar o status de todas).
//!
//! Cada integração (Claude CLI, Obsidian CLI, GitHub) implementa [`Integracao`] na
//! infraestrutura. As regras e as telas só conhecem este trait, então adicionar uma
//! integração não altera os arquivos delas.

use async_trait::async_trait;

use crate::domain::integracao::{ErroIntegracao, EstadoIntegracao, TipoIntegracao};

/// Instrução que a integração precisa mostrar ao Humano durante a conexão (UC-01 passo 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrucaoConexao {
    /// Device flow do GitHub: o Humano digita `codigo` em `endereco`
    /// (`github.com/login/device`).
    CodigoDoDispositivo {
        /// Código a digitar.
        codigo: String,
        /// Endereço onde digitar o código.
        endereco: String,
    },
    /// Texto livre (ex.: andamento do `/login` do Claude CLI).
    Mensagem(String),
}

/// Quem recebe as instruções da conexão em andamento (a tela, pelo núcleo).
pub trait ObservadorConexao: Send + Sync {
    /// Chamado a cada instrução nova; não bloqueia a conexão.
    fn instrucao(&self, instrucao: InstrucaoConexao);
}

/// Interface comum de toda integração externa (RNF-11, ENT-13, UC-01).
///
/// O trait é compatível com `dyn`: o núcleo guarda as integrações como
/// `Box<dyn Integracao>` ou as recebe por genérico.
#[async_trait]
pub trait Integracao: Send + Sync {
    /// Qual integração é esta (campo `tipo` de ENT-13).
    fn tipo(&self) -> TipoIntegracao;

    /// Checa o status sem pedir nada ao Humano (UC-01 passo 1; "testar conexão" do CMP-11).
    /// Conectada, devolve o estado com conta e versão, quando houver (UC-01 FA-01).
    /// Erros: ERR-01 (Claude CLI não instalado), ERR-03 (Obsidian fechado).
    async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao>;

    /// Conecta a integração (UC-01 passos 2 e 3): `/login` do Claude CLI, device flow do
    /// GitHub ou teste do servidor MCP da Obsidian CLI (D10). O que o Humano precisa ver
    /// (ex.: o código do device flow) vai pelo `observador`.
    /// Já conectada, devolve o estado atual sem novo login (UC-01 FA-01).
    /// Erros: ERR-01, ERR-02, ERR-03, ERR-04.
    async fn conectar(
        &self,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao>;

    /// Desconecta a integração (CMP-11) e devolve o estado novo.
    /// Quem chama garante que nenhuma execução depende dela (invariante de ENT-13).
    async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao>;
}

/// Status de uma integração já traduzido para a tela: o estado e, se houve, o erro
/// que o explica (coluna "Estado final" do catálogo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusChecado {
    /// Estado resultante (tipo, status, conta, versão).
    pub estado: EstadoIntegracao,
    /// Erro do catálogo que levou a esse estado, quando a verificação falhou.
    pub erro: Option<ErroIntegracao>,
}

/// Checa uma integração e converte o erro no estado final dele (UC-01 passo 1, FE-01, FE-03).
pub async fn checar_status<I: Integracao + ?Sized>(integracao: &I) -> StatusChecado {
    match integracao.verificar().await {
        Ok(estado) => StatusChecado { estado, erro: None },
        Err(erro) => StatusChecado {
            estado: EstadoIntegracao::a_partir_do_erro(&erro),
            erro: Some(erro),
        },
    }
}

/// Checa todas as integrações, na ordem recebida (UC-01 passo 1: "mostra o status das três").
pub async fn checar_todas(integracoes: &[Box<dyn Integracao>]) -> Vec<StatusChecado> {
    let mut resultado = Vec::with_capacity(integracoes.len());
    for integracao in integracoes {
        resultado.push(checar_status(integracao.as_ref()).await);
    }
    resultado
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::domain::integracao::{MotivoOauthNaoConcluido, StatusIntegracao, exigir_conectada};

    /// Integração falsa: devolve o que o teste configurou e guarda o estado entre chamadas.
    struct IntegracaoFalsa {
        tipo: TipoIntegracao,
        estado: Mutex<StatusIntegracao>,
        falha_ao_verificar: Option<ErroIntegracao>,
        falha_ao_conectar: Option<ErroIntegracao>,
    }

    impl IntegracaoFalsa {
        fn nova(tipo: TipoIntegracao, status: StatusIntegracao) -> Self {
            Self {
                tipo,
                estado: Mutex::new(status),
                falha_ao_verificar: None,
                falha_ao_conectar: None,
            }
        }

        fn atual(&self) -> EstadoIntegracao {
            let status = *self.estado.lock().unwrap();
            if status.esta_conectado() {
                EstadoIntegracao::conectado(self.tipo, Some("conta".into()), Some("1.0.0".into()))
            } else {
                EstadoIntegracao::novo(self.tipo, status)
            }
        }
    }

    #[async_trait]
    impl Integracao for IntegracaoFalsa {
        fn tipo(&self) -> TipoIntegracao {
            self.tipo
        }

        async fn verificar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
            match &self.falha_ao_verificar {
                Some(erro) => Err(erro.clone()),
                None => Ok(self.atual()),
            }
        }

        async fn conectar(
            &self,
            observador: &dyn ObservadorConexao,
        ) -> Result<EstadoIntegracao, ErroIntegracao> {
            if self.estado.lock().unwrap().esta_conectado() {
                return Ok(self.atual());
            }
            if self.tipo == TipoIntegracao::GitHub {
                observador.instrucao(InstrucaoConexao::CodigoDoDispositivo {
                    codigo: "ABCD-1234".into(),
                    endereco: "https://github.com/login/device".into(),
                });
            }
            if let Some(erro) = &self.falha_ao_conectar {
                if let Some(status) = erro.status_resultante() {
                    *self.estado.lock().unwrap() = status;
                }
                return Err(erro.clone());
            }
            *self.estado.lock().unwrap() = StatusIntegracao::Conectado;
            Ok(self.atual())
        }

        async fn desconectar(&self) -> Result<EstadoIntegracao, ErroIntegracao> {
            *self.estado.lock().unwrap() = StatusIntegracao::Desconectado;
            Ok(self.atual())
        }
    }

    #[derive(Default)]
    struct ObservadorGravador(Mutex<Vec<InstrucaoConexao>>);

    impl ObservadorConexao for ObservadorGravador {
        fn instrucao(&self, instrucao: InstrucaoConexao) {
            self.0.lock().unwrap().push(instrucao);
        }
    }

    /// Uso por genérico: compila para qualquer `Integracao`.
    async fn conectar_e_exigir<I: Integracao>(
        integracao: &I,
        observador: &dyn ObservadorConexao,
    ) -> Result<EstadoIntegracao, ErroIntegracao> {
        let estado = integracao.conectar(observador).await?;
        exigir_conectada(&estado)?;
        Ok(estado)
    }

    #[tokio::test]
    async fn checa_as_tres_por_dyn_com_cada_estado() {
        let mut claude =
            IntegracaoFalsa::nova(TipoIntegracao::ClaudeCli, StatusIntegracao::NaoInstalado);
        claude.falha_ao_verificar = Some(ErroIntegracao::ClaudeCliNaoInstalado);
        let mut obsidian =
            IntegracaoFalsa::nova(TipoIntegracao::ObsidianCli, StatusIntegracao::Desconectado);
        obsidian.falha_ao_verificar = Some(ErroIntegracao::ObsidianFechado {
            mensagem_da_cli: "The CLI is unable to find Obsidian".into(),
        });
        let github = IntegracaoFalsa::nova(TipoIntegracao::GitHub, StatusIntegracao::Conectado);

        let integracoes: Vec<Box<dyn Integracao>> =
            vec![Box::new(claude), Box::new(obsidian), Box::new(github)];
        let status = checar_todas(&integracoes).await;

        assert_eq!(status.len(), 3);
        // UC-01 FE-01
        assert_eq!(status[0].estado.status, StatusIntegracao::NaoInstalado);
        assert_eq!(status[0].erro.as_ref().map(|e| e.id()), Some("ERR-01"));
        // UC-01 FE-03
        assert_eq!(status[1].estado.tipo, TipoIntegracao::ObsidianCli);
        assert_eq!(status[1].estado.status, StatusIntegracao::Desconectado);
        assert_eq!(status[1].erro.as_ref().map(|e| e.id()), Some("ERR-03"));
        // UC-01 FA-01: conectada mostra conta e versão
        assert_eq!(status[2].erro, None);
        assert_eq!(status[2].estado.status, StatusIntegracao::Conectado);
        assert_eq!(status[2].estado.versao.as_deref(), Some("1.0.0"));
        assert_eq!(status[2].estado.conta.as_deref(), Some("conta"));
    }

    #[tokio::test]
    async fn conecta_github_por_generico_mostrando_o_codigo() {
        let github = IntegracaoFalsa::nova(TipoIntegracao::GitHub, StatusIntegracao::Desconectado);
        let observador = ObservadorGravador::default();

        let estado = conectar_e_exigir(&github, &observador).await.unwrap();

        assert_eq!(estado.status, StatusIntegracao::Conectado);
        assert_eq!(
            observador.0.lock().unwrap().as_slice(),
            [InstrucaoConexao::CodigoDoDispositivo {
                codigo: "ABCD-1234".into(),
                endereco: "https://github.com/login/device".into(),
            }]
        );
    }

    #[tokio::test]
    async fn ja_conectada_nao_pede_login_de_novo() {
        let github = IntegracaoFalsa::nova(TipoIntegracao::GitHub, StatusIntegracao::Conectado);
        let observador = ObservadorGravador::default();

        let estado = github.conectar(&observador).await.unwrap();

        assert_eq!(estado.status, StatusIntegracao::Conectado);
        assert!(observador.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn oauth_nao_concluido_deixa_github_desconectado() {
        let mut github =
            IntegracaoFalsa::nova(TipoIntegracao::GitHub, StatusIntegracao::Desconectado);
        github.falha_ao_conectar = Some(ErroIntegracao::GithubOauthNaoConcluido(
            MotivoOauthNaoConcluido::Recusado,
        ));
        let integracao: &dyn Integracao = &github;

        let erro = integracao
            .conectar(&ObservadorGravador::default())
            .await
            .unwrap_err();

        assert_eq!(erro.codigo(), "GITHUB_OAUTH_NAO_CONCLUIDO");
        let checado = checar_status(integracao).await;
        assert_eq!(checado.estado.status, StatusIntegracao::Desconectado);
    }

    #[tokio::test]
    async fn login_do_claude_falhou_fica_sem_login_e_bloqueia_por_rn_08() {
        let mut claude =
            IntegracaoFalsa::nova(TipoIntegracao::ClaudeCli, StatusIntegracao::Desconectado);
        claude.falha_ao_conectar = Some(ErroIntegracao::LoginClaudeCliFalhou {
            motivo: "tempo esgotado".into(),
        });

        let erro = claude
            .conectar(&ObservadorGravador::default())
            .await
            .unwrap_err();
        assert_eq!(erro.id(), "ERR-04");

        let checado = checar_status(&claude).await;
        assert_eq!(checado.estado.status, StatusIntegracao::SemLogin);
        assert_eq!(
            exigir_conectada(&checado.estado),
            Err(ErroIntegracao::ClaudeCliDesconectado)
        );
    }

    #[tokio::test]
    async fn desconectar_volta_para_desconectado() {
        let obsidian =
            IntegracaoFalsa::nova(TipoIntegracao::ObsidianCli, StatusIntegracao::Conectado);
        let integracao: Box<dyn Integracao> = Box::new(obsidian);

        let estado = integracao.desconectar().await.unwrap();

        assert_eq!(integracao.tipo(), TipoIntegracao::ObsidianCli);
        assert_eq!(estado.status, StatusIntegracao::Desconectado);
        assert_eq!(estado.versao, None);
        assert_eq!(
            exigir_conectada(&estado).unwrap_err().codigo(),
            "OBSIDIAN_DESCONECTADO"
        );
    }
}
