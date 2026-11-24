//! GitHub falso atrás do [`GitHost`] (decisão `geral-decisao-2026-11-20-github-falso`):
//! os testes automáticos e os verificadores rodam o device flow sem a conta do Humano.
//!
//! O roteiro diz o que cada consulta do token devolve; terminado o roteiro, a consulta
//! fica em `authorization_pending`. [`GitHubFalso::fora_do_ar`] simula a rede caída e
//! [`GitHubFalso::revogar`] um token que o GitHub não aceita mais.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::domain::segredo::{Segredo, TextoMascarado};
use tokio::time::Instant;

use super::host::{CodigoDispositivo, ErroHost, GitHost, RespostaToken, UsuarioGitHub};

/// Código que o falso mostra ao Humano.
pub const CODIGO_DO_USUARIO: &str = "WDJB-MJHT";
/// Endereço que o falso manda abrir.
pub const ENDERECO: &str = "https://github.com/login/device";

#[derive(Debug)]
struct Estado {
    roteiro: VecDeque<RespostaToken>,
    /// Tokens aceitos pelo `/user`, guardados como `Segredo` (o `Debug` não os mostra).
    contas: Vec<(Segredo, UsuarioGitHub)>,
    fora_do_ar: bool,
    falha_ao_pedir: Option<String>,
    pedidos_de_codigo: Vec<(String, Vec<String>)>,
    consultas: Vec<Instant>,
}

/// GitHub falso, em memória.
#[derive(Debug)]
pub struct GitHubFalso {
    estado: Mutex<Estado>,
    /// `expires_in` do código.
    pub expira_em: Duration,
    /// `interval` do código.
    pub intervalo: Duration,
}

impl Default for GitHubFalso {
    fn default() -> Self {
        Self::novo()
    }
}

impl GitHubFalso {
    /// Falso sem roteiro: código de 15 min, consultas a cada 5 s.
    pub fn novo() -> Self {
        Self {
            estado: Mutex::new(Estado {
                roteiro: VecDeque::new(),
                contas: Vec::new(),
                fora_do_ar: false,
                falha_ao_pedir: None,
                pedidos_de_codigo: Vec::new(),
                consultas: Vec::new(),
            }),
            expira_em: Duration::from_secs(900),
            intervalo: Duration::from_secs(5),
        }
    }

    fn estado(&self) -> std::sync::MutexGuard<'_, Estado> {
        self.estado.lock().expect("GitHub falso")
    }

    /// Acrescenta respostas ao roteiro das consultas do token.
    pub fn roteiro(&self, respostas: impl IntoIterator<Item = RespostaToken>) {
        self.estado().roteiro.extend(respostas);
    }

    /// Roteiro em que o Humano autoriza na 1ª consulta e o token é de `login`.
    pub fn autorizar(&self, token: &Segredo, login: &str, escopos: &[&str]) {
        self.conta(token, login, escopos);
        self.roteiro([RespostaToken::Concedido(token.clone())]);
    }

    /// Faz o `/user` aceitar `token` como sendo de `login`, com `escopos`.
    pub fn conta(&self, token: &Segredo, login: &str, escopos: &[&str]) {
        let usuario = UsuarioGitHub {
            login: login.to_string(),
            escopos: escopos.iter().map(|s| s.to_string()).collect(),
        };
        let mut estado = self.estado();
        estado.contas.retain(|(t, _)| t != token);
        estado.contas.push((token.clone(), usuario));
    }

    /// O `/user` passa a recusar `token` (401).
    pub fn revogar(&self, token: &Segredo) {
        self.estado().contas.retain(|(t, _)| t != token);
    }

    /// Simula a rede fora do ar (`true`) ou de volta (`false`).
    pub fn fora_do_ar(&self, sim: bool) {
        self.estado().fora_do_ar = sim;
    }

    /// Faz o próximo `device/code` falhar com o `error` dado (ex.: `device_flow_disabled`).
    pub fn falhar_ao_pedir(&self, codigo: &str) {
        self.estado().falha_ao_pedir = Some(codigo.to_string());
    }

    /// Quantas vezes o device flow foi iniciado, com o `client_id` e os escopos.
    pub fn pedidos_de_codigo(&self) -> Vec<(String, Vec<String>)> {
        self.estado().pedidos_de_codigo.clone()
    }

    /// Instantes (relógio do tokio) de cada consulta do token.
    pub fn consultas(&self) -> Vec<Instant> {
        self.estado().consultas.clone()
    }

    fn rede(&self, segredos: &[&Segredo]) -> Result<(), ErroHost> {
        if self.estado().fora_do_ar {
            // A mensagem ecoa os segredos de propósito: a máscara precisa tirá-los.
            let eco: Vec<_> = segredos.iter().map(|s| s.expor()).collect();
            return Err(ErroHost::Rede(TextoMascarado::com_segredos(
                &format!("dns error: failed to lookup address information ({eco:?})"),
                segredos,
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl GitHost for GitHubFalso {
    async fn pedir_codigo(
        &self,
        client_id: &str,
        escopos: &[String],
    ) -> Result<CodigoDispositivo, ErroHost> {
        self.rede(&[])?;
        let mut estado = self.estado();
        estado
            .pedidos_de_codigo
            .push((client_id.to_string(), escopos.to_vec()));
        if let Some(codigo) = estado.falha_ao_pedir.take() {
            return Err(ErroHost::Resposta(TextoMascarado::new(&format!(
                "`{codigo}`"
            ))));
        }
        let n = estado.pedidos_de_codigo.len();
        Ok(CodigoDispositivo {
            device_code: Segredo::new(format!("device-code-falso-{n}")),
            user_code: CODIGO_DO_USUARIO.to_string(),
            verification_uri: ENDERECO.to_string(),
            expira_em: self.expira_em,
            intervalo: self.intervalo,
        })
    }

    async fn trocar_codigo(
        &self,
        _client_id: &str,
        device_code: &Segredo,
    ) -> Result<RespostaToken, ErroHost> {
        self.rede(&[device_code])?;
        let mut estado = self.estado();
        estado.consultas.push(Instant::now());
        Ok(estado
            .roteiro
            .pop_front()
            .unwrap_or(RespostaToken::Pendente))
    }

    async fn usuario(&self, token: &Segredo) -> Result<UsuarioGitHub, ErroHost> {
        self.rede(&[token])?;
        self.estado()
            .contas
            .iter()
            .find(|(t, _)| t == token)
            .map(|(_, u)| u.clone())
            .ok_or(ErroHost::TokenRecusado)
    }
}
