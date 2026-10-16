//! Integração (ENT-13): tipos do domínio comuns a Claude CLI, Obsidian CLI e GitHub.
//!
//! Aqui ficam só os valores e as regras; a conversa com cada ferramenta mora na
//! infraestrutura, atrás do trait [`crate::application::integracao::Integracao`] (RNF-11).
//! A credencial (token do GitHub) não aparece em nenhum tipo deste módulo: ela vive só
//! no Gerenciador de Credenciais do SO (RN-04, RNF-05).

use std::fmt;

use super::segredo::TextoMascarado;

/// Tipo da integração (campo `tipo` de ENT-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoIntegracao {
    /// Claude CLI, conectado pelo `/login` da própria CLI (D10).
    ClaudeCli,
    /// Obsidian CLI, conectada pelo servidor MCP da Obsidian CLI (D10).
    ObsidianCli,
    /// GitHub, conectado pelo device flow do app OAuth do MyHarness (D10).
    GitHub,
}

impl TipoIntegracao {
    /// As três integrações do contrato, na ordem em que o painel as mostra (UC-01 passo 1).
    pub const TODAS: [TipoIntegracao; 3] = [
        TipoIntegracao::ClaudeCli,
        TipoIntegracao::ObsidianCli,
        TipoIntegracao::GitHub,
    ];

    /// Nome mostrado ao Humano, igual ao valor permitido em ENT-13.
    pub fn nome(self) -> &'static str {
        match self {
            TipoIntegracao::ClaudeCli => "Claude CLI",
            TipoIntegracao::ObsidianCli => "Obsidian CLI",
            TipoIntegracao::GitHub => "GitHub",
        }
    }
}

impl fmt::Display for TipoIntegracao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nome())
    }
}

/// Status da integração (campo `status` de ENT-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusIntegracao {
    /// Pronta para uso; só neste estado a ação dependente executa (RN-08).
    Conectado,
    /// Sem conexão (ex.: OAuth não concluído, ERR-02; Obsidian fechado, ERR-03).
    Desconectado,
    /// Instalada, mas sem login (ex.: login do Claude CLI falhou, ERR-04).
    SemLogin,
    /// Executável não encontrado (ERR-01).
    NaoInstalado,
}

impl StatusIntegracao {
    /// Nome mostrado ao Humano, igual ao valor permitido em ENT-13.
    pub fn nome(self) -> &'static str {
        match self {
            StatusIntegracao::Conectado => "conectado",
            StatusIntegracao::Desconectado => "desconectado",
            StatusIntegracao::SemLogin => "sem login",
            StatusIntegracao::NaoInstalado => "não instalado",
        }
    }

    /// `true` só em [`StatusIntegracao::Conectado`].
    pub fn esta_conectado(self) -> bool {
        self == StatusIntegracao::Conectado
    }
}

impl fmt::Display for StatusIntegracao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nome())
    }
}

/// Retrato de uma integração: os campos de ENT-13 que podem ir ao SQLite e à tela
/// (tipo, status, conta e versão). A credencial fica de fora de propósito (RN-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoIntegracao {
    /// Qual integração.
    pub tipo: TipoIntegracao,
    /// Status atual.
    pub status: StatusIntegracao,
    /// Conta conectada, quando a integração informa (UC-01 FA-01).
    pub conta: Option<String>,
    /// Versão da ferramenta, quando houver (UC-01 FA-01).
    pub versao: Option<String>,
}

impl EstadoIntegracao {
    /// Estado sem conta nem versão.
    pub fn novo(tipo: TipoIntegracao, status: StatusIntegracao) -> Self {
        Self {
            tipo,
            status,
            conta: None,
            versao: None,
        }
    }

    /// Estado `conectado` com conta e versão, quando houver.
    pub fn conectado(tipo: TipoIntegracao, conta: Option<String>, versao: Option<String>) -> Self {
        Self {
            tipo,
            status: StatusIntegracao::Conectado,
            conta,
            versao,
        }
    }

    /// Estado que resulta de um erro de verificação ou conexão (Estado final do ERR).
    /// Erro que não muda o status (ERR-05, ERR-06, ERR-07) devolve `desconectado`,
    /// que é a condição que os dispara.
    pub fn a_partir_do_erro(erro: &ErroIntegracao) -> Self {
        let status = erro
            .status_resultante()
            .unwrap_or(StatusIntegracao::Desconectado);
        Self::novo(erro.integracao(), status)
    }
}

/// Motivo de o device flow do GitHub não concluir (ERR-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoOauthNaoConcluido {
    /// O Humano cancelou.
    Cancelado,
    /// O GitHub devolveu `access_denied`.
    Recusado,
    /// O GitHub devolveu `expired_token`.
    Expirado,
}

/// Erros do catálogo (`05-erros.md`) que uma integração pode devolver.
/// Cada variante é um ERR; [`ErroIntegracao::codigo`] é o código que o núcleo devolve
/// e a tela usa para escolher a mensagem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroIntegracao {
    /// ERR-01: o executável do Claude CLI não é encontrado (UC-01 FE-01).
    #[error("Claude CLI não encontrado")]
    ClaudeCliNaoInstalado,
    /// ERR-02: o device flow do GitHub foi cancelado, recusado ou expirou (UC-01 FE-02).
    #[error("Conexão com o GitHub não concluída")]
    GithubOauthNaoConcluido(MotivoOauthNaoConcluido),
    /// ERR-03: o Obsidian está fechado ao checar o status (UC-01 FE-03).
    /// Guarda a mensagem da própria CLI, que é a mostrada ao Humano, já mascarada (RN-04).
    #[error("{mensagem_da_cli}")]
    ObsidianFechado {
        /// Texto devolvido pela Obsidian CLI, mascarado.
        mensagem_da_cli: TextoMascarado,
    },
    /// ERR-04: o login do Claude CLI falhou; guarda o motivo, já mascarado (RN-04).
    #[error("{motivo}")]
    LoginClaudeCliFalhou {
        /// Motivo da falha, mascarado.
        motivo: TextoMascarado,
    },
    /// ERR-05: ação que depende do Claude CLI com ele desconectado (RN-08).
    #[error("Conecte o Claude CLI para enviar")]
    ClaudeCliDesconectado,
    /// ERR-06: plano ou ação que exige GitHub com ele desconectado (RN-08).
    #[error("Conecte o GitHub para aprovar este plano")]
    GithubDesconectado,
    /// ERR-07: ação que depende da Obsidian CLI com ela desconectada (RN-08).
    #[error("Conecte o Obsidian para registrar")]
    ObsidianDesconectado,
    /// ERR-53: erro genérico das integrações (UC-01 FE-04): qualquer falha ao checar,
    /// conectar ou desconectar que não tem ERR próprio (tempo esgotado, saída ilegível,
    /// rede, cofre não encontrado pela Obsidian CLI). O motivo já vem mascarado (RN-04)
    /// e vai à mensagem e ao Log (ENT-12).
    #[error("Falha inesperada com {integracao}: {motivo}. Tente de novo.")]
    FalhaInesperada {
        /// Integração em que a falha aconteceu.
        integracao: TipoIntegracao,
        /// Motivo real, mascarado.
        motivo: TextoMascarado,
    },
}

impl ErroIntegracao {
    /// ERR-53 de `integracao` com o `motivo` mascarado (RN-04).
    pub fn inesperada(integracao: TipoIntegracao, motivo: impl Into<TextoMascarado>) -> Self {
        ErroIntegracao::FalhaInesperada {
            integracao,
            motivo: motivo.into(),
        }
    }

    /// ID do item no catálogo de erros (ex.: `"ERR-01"`).
    pub fn id(&self) -> &'static str {
        match self {
            ErroIntegracao::ClaudeCliNaoInstalado => "ERR-01",
            ErroIntegracao::GithubOauthNaoConcluido(_) => "ERR-02",
            ErroIntegracao::ObsidianFechado { .. } => "ERR-03",
            ErroIntegracao::LoginClaudeCliFalhou { .. } => "ERR-04",
            ErroIntegracao::ClaudeCliDesconectado => "ERR-05",
            ErroIntegracao::GithubDesconectado => "ERR-06",
            ErroIntegracao::ObsidianDesconectado => "ERR-07",
            ErroIntegracao::FalhaInesperada { .. } => "ERR-53",
        }
    }

    /// Código no servidor, como no catálogo de erros.
    pub fn codigo(&self) -> &'static str {
        match self {
            ErroIntegracao::ClaudeCliNaoInstalado => "CLAUDE_CLI_NAO_INSTALADO",
            ErroIntegracao::GithubOauthNaoConcluido(_) => "GITHUB_OAUTH_NAO_CONCLUIDO",
            ErroIntegracao::ObsidianFechado { .. } => "OBSIDIAN_FECHADO",
            ErroIntegracao::LoginClaudeCliFalhou { .. } => "LOGIN_CLAUDE_CLI_FALHOU",
            ErroIntegracao::ClaudeCliDesconectado => "CLAUDE_CLI_DESCONECTADO",
            ErroIntegracao::GithubDesconectado => "GITHUB_DESCONECTADO",
            ErroIntegracao::ObsidianDesconectado => "OBSIDIAN_DESCONECTADO",
            ErroIntegracao::FalhaInesperada { .. } => "INTEGRACAO_FALHA_INESPERADA",
        }
    }

    /// Integração a que o erro se refere.
    pub fn integracao(&self) -> TipoIntegracao {
        match self {
            ErroIntegracao::ClaudeCliNaoInstalado
            | ErroIntegracao::LoginClaudeCliFalhou { .. }
            | ErroIntegracao::ClaudeCliDesconectado => TipoIntegracao::ClaudeCli,
            ErroIntegracao::GithubOauthNaoConcluido(_) | ErroIntegracao::GithubDesconectado => {
                TipoIntegracao::GitHub
            }
            ErroIntegracao::ObsidianFechado { .. } | ErroIntegracao::ObsidianDesconectado => {
                TipoIntegracao::ObsidianCli
            }
            ErroIntegracao::FalhaInesperada { integracao, .. } => *integracao,
        }
    }

    /// Status em que a integração fica depois do erro (coluna "Estado final").
    /// `None` quando o erro bloqueia a ação sem mudar o status (ERR-05, ERR-06, ERR-07).
    pub fn status_resultante(&self) -> Option<StatusIntegracao> {
        match self {
            ErroIntegracao::ClaudeCliNaoInstalado => Some(StatusIntegracao::NaoInstalado),
            ErroIntegracao::GithubOauthNaoConcluido(_)
            | ErroIntegracao::ObsidianFechado { .. }
            | ErroIntegracao::FalhaInesperada { .. } => Some(StatusIntegracao::Desconectado),
            ErroIntegracao::LoginClaudeCliFalhou { .. } => Some(StatusIntegracao::SemLogin),
            ErroIntegracao::ClaudeCliDesconectado
            | ErroIntegracao::GithubDesconectado
            | ErroIntegracao::ObsidianDesconectado => None,
        }
    }

    /// Erro de "integração desconectada" de cada tipo (ERR-05, ERR-06, ERR-07).
    pub fn desconectada(tipo: TipoIntegracao) -> Self {
        match tipo {
            TipoIntegracao::ClaudeCli => ErroIntegracao::ClaudeCliDesconectado,
            TipoIntegracao::GitHub => ErroIntegracao::GithubDesconectado,
            TipoIntegracao::ObsidianCli => ErroIntegracao::ObsidianDesconectado,
        }
    }
}

/// RN-08: ação que depende de uma integração só executa com ela `conectado`.
/// Fora disso, devolve o ERR que aponta qual integração falta (ERR-05, ERR-06, ERR-07).
pub fn exigir_conectada(estado: &EstadoIntegracao) -> Result<(), ErroIntegracao> {
    if estado.status.esta_conectado() {
        Ok(())
    } else {
        Err(ErroIntegracao::desconectada(estado.tipo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todos_os_erros() -> Vec<ErroIntegracao> {
        vec![
            ErroIntegracao::ClaudeCliNaoInstalado,
            ErroIntegracao::GithubOauthNaoConcluido(MotivoOauthNaoConcluido::Expirado),
            ErroIntegracao::ObsidianFechado {
                mensagem_da_cli: "The CLI is unable to find Obsidian".into(),
            },
            ErroIntegracao::LoginClaudeCliFalhou { motivo: "x".into() },
            ErroIntegracao::ClaudeCliDesconectado,
            ErroIntegracao::GithubDesconectado,
            ErroIntegracao::ObsidianDesconectado,
            ErroIntegracao::inesperada(TipoIntegracao::GitHub, "erro de rede"),
        ]
    }

    #[test]
    fn nomes_seguem_ent_13() {
        let tipos: Vec<_> = TipoIntegracao::TODAS.iter().map(|t| t.nome()).collect();
        assert_eq!(tipos, ["Claude CLI", "Obsidian CLI", "GitHub"]);
        assert_eq!(StatusIntegracao::Conectado.to_string(), "conectado");
        assert_eq!(StatusIntegracao::Desconectado.to_string(), "desconectado");
        assert_eq!(StatusIntegracao::SemLogin.to_string(), "sem login");
        assert_eq!(StatusIntegracao::NaoInstalado.to_string(), "não instalado");
    }

    #[test]
    fn so_conectado_esta_conectado() {
        assert!(StatusIntegracao::Conectado.esta_conectado());
        for s in [
            StatusIntegracao::Desconectado,
            StatusIntegracao::SemLogin,
            StatusIntegracao::NaoInstalado,
        ] {
            assert!(!s.esta_conectado(), "{s}");
        }
    }

    #[test]
    fn ids_e_codigos_seguem_o_catalogo() {
        let pares: Vec<_> = todos_os_erros()
            .iter()
            .map(|e| (e.id(), e.codigo()))
            .collect();
        assert_eq!(
            pares,
            [
                ("ERR-01", "CLAUDE_CLI_NAO_INSTALADO"),
                ("ERR-02", "GITHUB_OAUTH_NAO_CONCLUIDO"),
                ("ERR-03", "OBSIDIAN_FECHADO"),
                ("ERR-04", "LOGIN_CLAUDE_CLI_FALHOU"),
                ("ERR-05", "CLAUDE_CLI_DESCONECTADO"),
                ("ERR-06", "GITHUB_DESCONECTADO"),
                ("ERR-07", "OBSIDIAN_DESCONECTADO"),
                ("ERR-53", "INTEGRACAO_FALHA_INESPERADA"),
            ]
        );
    }

    #[test]
    fn estado_final_de_cada_erro() {
        let finais: Vec<_> = todos_os_erros()
            .iter()
            .map(|e| (e.integracao(), e.status_resultante()))
            .collect();
        assert_eq!(
            finais,
            [
                (
                    TipoIntegracao::ClaudeCli,
                    Some(StatusIntegracao::NaoInstalado)
                ),
                (TipoIntegracao::GitHub, Some(StatusIntegracao::Desconectado)),
                (
                    TipoIntegracao::ObsidianCli,
                    Some(StatusIntegracao::Desconectado)
                ),
                (TipoIntegracao::ClaudeCli, Some(StatusIntegracao::SemLogin)),
                (TipoIntegracao::ClaudeCli, None),
                (TipoIntegracao::GitHub, None),
                (TipoIntegracao::ObsidianCli, None),
                (TipoIntegracao::GitHub, Some(StatusIntegracao::Desconectado)),
            ]
        );
    }

    #[test]
    fn mensagens_seguem_o_catalogo() {
        assert_eq!(
            ErroIntegracao::ClaudeCliNaoInstalado.to_string(),
            "Claude CLI não encontrado"
        );
        assert_eq!(
            ErroIntegracao::GithubOauthNaoConcluido(MotivoOauthNaoConcluido::Cancelado).to_string(),
            "Conexão com o GitHub não concluída"
        );
        assert_eq!(
            ErroIntegracao::ObsidianFechado {
                mensagem_da_cli: "msg da CLI".into()
            }
            .to_string(),
            "msg da CLI"
        );
        assert_eq!(
            ErroIntegracao::ObsidianDesconectado.to_string(),
            "Conecte o Obsidian para registrar"
        );
    }

    #[test]
    fn rn_08_exige_conectado_e_aponta_a_integracao_que_falta() {
        for tipo in TipoIntegracao::TODAS {
            assert_eq!(
                exigir_conectada(&EstadoIntegracao::conectado(tipo, None, None)),
                Ok(())
            );
            for status in [
                StatusIntegracao::Desconectado,
                StatusIntegracao::SemLogin,
                StatusIntegracao::NaoInstalado,
            ] {
                let erro = exigir_conectada(&EstadoIntegracao::novo(tipo, status)).unwrap_err();
                assert_eq!(erro.integracao(), tipo);
                assert_eq!(erro, ErroIntegracao::desconectada(tipo));
            }
        }
        assert_eq!(
            ErroIntegracao::desconectada(TipoIntegracao::ClaudeCli).id(),
            "ERR-05"
        );
        assert_eq!(
            ErroIntegracao::desconectada(TipoIntegracao::GitHub).id(),
            "ERR-06"
        );
        assert_eq!(
            ErroIntegracao::desconectada(TipoIntegracao::ObsidianCli).id(),
            "ERR-07"
        );
    }

    #[test]
    fn texto_livre_da_cli_sai_mascarado_no_display_e_no_debug() {
        let token = "ghp_A1b2C3d4E5f6G7h8I9j0KlMnOpQrStUvWx";
        let erros = [
            ErroIntegracao::ObsidianFechado {
                mensagem_da_cli: format!("falhou com \"{token}\" no vault").into(),
            },
            ErroIntegracao::LoginClaudeCliFalhou {
                motivo: format!("{token} recusado; de novo {}", token.to_uppercase()).into(),
            },
            ErroIntegracao::inesperada(
                TipoIntegracao::ObsidianCli,
                format!("tempo esgotado com {token}"),
            ),
        ];
        for erro in &erros {
            for saida in [erro.to_string(), format!("{erro:?}"), format!("{erro:#?}")] {
                assert!(!saida.to_lowercase().contains("a1b2c3d4"), "{saida}");
                assert!(saida.contains(crate::domain::segredo::MASCARA), "{saida}");
            }
        }
        assert_eq!(erros[0].to_string(), "falhou com \"••••\" no vault");
        assert_eq!(erros[1].to_string(), "•••• recusado; de novo ••••");
        assert_eq!(
            erros[2].to_string(),
            "Falha inesperada com Obsidian CLI: tempo esgotado com ••••. Tente de novo."
        );
        assert_eq!(
            format!("{:?}", erros[1]),
            "LoginClaudeCliFalhou { motivo: \"•••• recusado; de novo ••••\" }"
        );
    }

    #[test]
    fn mensagem_sem_token_sai_igual() {
        let msg = "error: the argument '--token <TOKEN>' requires a value but none was supplied";
        let erro = ErroIntegracao::LoginClaudeCliFalhou { motivo: msg.into() };
        assert_eq!(erro.to_string(), msg);
    }

    #[test]
    fn estado_a_partir_do_erro() {
        let e = EstadoIntegracao::a_partir_do_erro(&ErroIntegracao::ClaudeCliNaoInstalado);
        assert_eq!(
            e,
            EstadoIntegracao::novo(TipoIntegracao::ClaudeCli, StatusIntegracao::NaoInstalado)
        );
        let e = EstadoIntegracao::a_partir_do_erro(&ErroIntegracao::GithubDesconectado);
        assert_eq!(e.status, StatusIntegracao::Desconectado);
    }
}
