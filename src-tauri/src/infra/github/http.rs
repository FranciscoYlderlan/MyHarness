//! [`GitHost`] real, por HTTPS: `github.com/login/device/code`,
//! `github.com/login/oauth/access_token` e `api.github.com/user`.
//!
//! O token vai só no cabeçalho `Authorization` de `/user`; todo texto de erro passa pela
//! máscara com o token e o `device_code` da chamada (RN-04).

use std::time::Duration;

use async_trait::async_trait;
use my_harness_core::domain::segredo::{Segredo, TextoMascarado};
use serde::Deserialize;

use super::host::{CodigoDispositivo, ErroHost, GitHost, RespostaToken, UsuarioGitHub};

/// Endereço do login do GitHub.
pub const BASE_LOGIN: &str = "https://github.com";
/// Endereço da API do GitHub.
pub const BASE_API: &str = "https://api.github.com";
/// `grant_type` do device flow.
const GRANT_DEVICE_CODE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Cliente HTTPS do GitHub.
#[derive(Debug, Clone)]
pub struct GitHubHttp {
    cliente: reqwest::Client,
    base_login: String,
    base_api: String,
}

impl GitHubHttp {
    /// Cliente para `github.com` e `api.github.com`, com o proxy do sistema.
    pub fn novo() -> Result<Self, ErroHost> {
        Self::montar(BASE_LOGIN, BASE_API, true)
    }

    /// Cliente para outros endereços (ex.: servidor falso local nos testes), sem proxy.
    pub fn com_enderecos(base_login: &str, base_api: &str) -> Result<Self, ErroHost> {
        Self::montar(base_login, base_api, false)
    }

    fn montar(base_login: &str, base_api: &str, com_proxy: bool) -> Result<Self, ErroHost> {
        // O rustls sem provedor embutido precisa de um padrão; já instalado não é erro.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut construtor = reqwest::Client::builder()
            .user_agent(concat!("MyHarness/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20));
        if !com_proxy {
            construtor = construtor.no_proxy();
        }
        let cliente = construtor
            .build()
            .map_err(|e| ErroHost::Rede(TextoMascarado::new(&e.to_string())))?;
        Ok(Self {
            cliente,
            base_login: base_login.trim_end_matches('/').to_string(),
            base_api: base_api.trim_end_matches('/').to_string(),
        })
    }
}

/// Erro do `reqwest` como [`ErroHost::Rede`], mascarando `segredos`.
fn rede(erro: reqwest::Error, segredos: &[&Segredo]) -> ErroHost {
    ErroHost::Rede(TextoMascarado::com_segredos(
        &erro.without_url().to_string(),
        segredos,
    ))
}

fn resposta(motivo: &str, segredos: &[&Segredo]) -> ErroHost {
    ErroHost::Resposta(TextoMascarado::com_segredos(motivo, segredos))
}

/// Corpo JSON do login do GitHub: o pedido deu certo ou traz `error`.
#[derive(Deserialize)]
struct CorpoLogin {
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
    // device/code
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_in: Option<u64>,
    // access_token
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct CorpoUsuario {
    login: String,
}

/// Lê o corpo JSON de `/login/...`; o GitHub manda o `error` com status 200 ou 4xx.
async fn ler_login(r: reqwest::Response, segredos: &[&Segredo]) -> Result<CorpoLogin, ErroHost> {
    let status = r.status();
    let texto = r.text().await.map_err(|e| rede(e, segredos))?;
    match serde_json::from_str::<CorpoLogin>(&texto) {
        Ok(corpo) if status.is_success() || corpo.error.is_some() => Ok(corpo),
        _ => Err(resposta(
            &format!("HTTP {}: {}", status.as_u16(), resumo(&texto)),
            segredos,
        )),
    }
}

/// Até 200 caracteres do corpo, para o motivo do erro.
fn resumo(texto: &str) -> String {
    texto.trim().chars().take(200).collect()
}

fn erro_de_protocolo(codigo: String, descricao: Option<String>) -> RespostaToken {
    RespostaToken::Erro {
        codigo,
        descricao: descricao.map(|d| TextoMascarado::new(&d)),
    }
}

#[async_trait]
impl GitHost for GitHubHttp {
    async fn pedir_codigo(
        &self,
        client_id: &str,
        escopos: &[String],
    ) -> Result<CodigoDispositivo, ErroHost> {
        let escopo = escopos.join(" ");
        let r = self
            .cliente
            .post(format!("{}/login/device/code", self.base_login))
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[("client_id", client_id), ("scope", escopo.as_str())])
            .send()
            .await
            .map_err(|e| rede(e, &[]))?;
        let corpo = ler_login(r, &[]).await?;
        if let Some(codigo) = corpo.error {
            let descricao = corpo.error_description.unwrap_or_default();
            return Err(resposta(&format!("`{codigo}`: {descricao}"), &[]));
        }
        let (Some(device_code), Some(user_code), Some(verification_uri), Some(expires_in)) = (
            corpo.device_code,
            corpo.user_code,
            corpo.verification_uri,
            corpo.expires_in,
        ) else {
            return Err(resposta("device/code sem os campos do device flow", &[]));
        };
        Ok(CodigoDispositivo {
            device_code: Segredo::new(device_code),
            user_code,
            verification_uri,
            expira_em: Duration::from_secs(expires_in),
            intervalo: Duration::from_secs(corpo.interval.unwrap_or(5)),
        })
    }

    async fn trocar_codigo(
        &self,
        client_id: &str,
        device_code: &Segredo,
    ) -> Result<RespostaToken, ErroHost> {
        let r = self
            .cliente
            .post(format!("{}/login/oauth/access_token", self.base_login))
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code.expor()),
                ("grant_type", GRANT_DEVICE_CODE),
            ])
            .send()
            .await
            .map_err(|e| rede(e, &[device_code]))?;
        let corpo = ler_login(r, &[device_code]).await?;
        if let Some(token) = corpo.access_token.filter(|t| !t.is_empty()) {
            return Ok(RespostaToken::Concedido(Segredo::new(token)));
        }
        Ok(match corpo.error.as_deref() {
            Some("authorization_pending") => RespostaToken::Pendente,
            Some("slow_down") => RespostaToken::Devagar {
                intervalo: corpo.interval.map(Duration::from_secs),
            },
            Some("expired_token") => RespostaToken::Expirado,
            Some("access_denied") => RespostaToken::Recusado,
            Some(outro) => erro_de_protocolo(outro.to_string(), corpo.error_description),
            None => return Err(resposta("access_token sem token nem `error`", &[])),
        })
    }

    async fn usuario(&self, token: &Segredo) -> Result<UsuarioGitHub, ErroHost> {
        let segredos = [token];
        let r = self
            .cliente
            .get(format!("{}/user", self.base_api))
            .bearer_auth(token.expor())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| rede(e, &segredos))?;
        let status = r.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ErroHost::TokenRecusado);
        }
        let escopos = r
            .headers()
            .get("x-oauth-scopes")
            .and_then(|v| v.to_str().ok())
            .map(ler_escopos)
            .unwrap_or_default();
        let texto = r.text().await.map_err(|e| rede(e, &segredos))?;
        if !status.is_success() {
            return Err(resposta(
                &format!("HTTP {} em /user: {}", status.as_u16(), resumo(&texto)),
                &segredos,
            ));
        }
        let corpo: CorpoUsuario = serde_json::from_str(&texto)
            .map_err(|_| resposta(&format!("/user ilegível: {}", resumo(&texto)), &segredos))?;
        Ok(UsuarioGitHub {
            login: corpo.login,
            escopos,
        })
    }
}

/// `X-OAuth-Scopes: repo, read:org` → `["repo", "read:org"]`.
pub fn ler_escopos(cabecalho: &str) -> Vec<String> {
    cabecalho
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    //! Contra um servidor HTTP local que imita o GitHub (nunca o GitHub real).

    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const TOKEN: &str = "gho_T0k3nD0GitHubFalsoParaOTeste1234";

    /// Pedido recebido pelo servidor falso.
    #[derive(Debug, Clone)]
    struct Pedido {
        linha: String,
        cabecalhos: String,
        corpo: String,
    }

    /// Resposta roteirizada: status, cabeçalhos extras e corpo.
    type Roteiro = (u16, Vec<(&'static str, &'static str)>, String);

    /// Servidor que responde, em ordem, as respostas dadas.
    async fn servidor(respostas: Vec<Roteiro>) -> (String, Arc<Mutex<Vec<Pedido>>>) {
        let ouvinte = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", ouvinte.local_addr().unwrap());
        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let gravados = pedidos.clone();
        tokio::spawn(async move {
            for (status, cabecalhos, corpo) in respostas {
                let (mut conexao, _) = ouvinte.accept().await.unwrap();
                let mut lido = Vec::new();
                let mut buf = [0u8; 4096];
                let pedido = loop {
                    let n = conexao.read(&mut buf).await.unwrap();
                    lido.extend_from_slice(&buf[..n]);
                    let texto = String::from_utf8_lossy(&lido).to_string();
                    if let Some(fim) = texto.find("\r\n\r\n") {
                        let cab = texto[..fim].to_string();
                        let tamanho = cab
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if lido.len() >= fim + 4 + tamanho || n == 0 {
                            let (linha, resto) = cab.split_once("\r\n").unwrap_or((&cab, ""));
                            break Pedido {
                                linha: linha.to_string(),
                                cabecalhos: resto.to_ascii_lowercase(),
                                corpo: texto[fim + 4..].to_string(),
                            };
                        }
                    }
                };
                gravados.lock().unwrap().push(pedido);
                let mut saida = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
                    corpo.len()
                );
                for (n, v) in cabecalhos {
                    saida.push_str(&format!("{n}: {v}\r\n"));
                }
                saida.push_str("\r\n");
                saida.push_str(&corpo);
                conexao.write_all(saida.as_bytes()).await.unwrap();
                conexao.shutdown().await.ok();
            }
        });
        (base, pedidos)
    }

    fn cliente(base: &str) -> GitHubHttp {
        GitHubHttp::com_enderecos(base, base).unwrap()
    }

    #[tokio::test]
    async fn pede_o_codigo_com_client_id_e_escopos() {
        let (base, pedidos) = servidor(vec![(
            200,
            vec![],
            r#"{"device_code":"dc-123","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#.into(),
        )])
        .await;

        let codigo = cliente(&base)
            .pedir_codigo("Ov23li904l2z5LWigHyr", &["repo".into(), "read:org".into()])
            .await
            .unwrap();

        assert_eq!(codigo.user_code, "WDJB-MJHT");
        assert_eq!(codigo.verification_uri, "https://github.com/login/device");
        assert_eq!(codigo.device_code.expor(), "dc-123");
        assert_eq!(codigo.expira_em, Duration::from_secs(900));
        assert_eq!(codigo.intervalo, Duration::from_secs(5));
        let p = pedidos.lock().unwrap()[0].clone();
        assert_eq!(p.linha, "POST /login/device/code HTTP/1.1");
        assert!(p.cabecalhos.contains("accept: application/json"), "{p:?}");
        assert_eq!(
            p.corpo,
            "client_id=Ov23li904l2z5LWigHyr&scope=repo+read%3Aorg"
        );
    }

    #[tokio::test]
    async fn le_cada_resposta_do_access_token() {
        let corpos = [
            r#"{"error":"authorization_pending","error_description":"pending"}"#,
            r#"{"error":"slow_down","interval":10}"#,
            r#"{"error":"expired_token"}"#,
            r#"{"error":"access_denied"}"#,
            r#"{"error":"device_flow_disabled","error_description":"Device Flow must be enabled"}"#,
        ];
        let mut respostas: Vec<_> = corpos
            .iter()
            .map(|c| (200, vec![], c.to_string()))
            .collect();
        respostas.push((
            200,
            vec![],
            format!(r#"{{"access_token":"{TOKEN}","token_type":"bearer","scope":"repo"}}"#),
        ));
        let (base, pedidos) = servidor(respostas).await;
        let gh = cliente(&base);
        let dc = Segredo::new("dc-123");

        let mut lidas = Vec::new();
        for _ in 0..6 {
            lidas.push(gh.trocar_codigo("cid", &dc).await.unwrap());
        }

        assert_eq!(lidas[0], RespostaToken::Pendente);
        assert_eq!(
            lidas[1],
            RespostaToken::Devagar {
                intervalo: Some(Duration::from_secs(10))
            }
        );
        assert_eq!(lidas[2], RespostaToken::Expirado);
        assert_eq!(lidas[3], RespostaToken::Recusado);
        assert!(
            matches!(&lidas[4], RespostaToken::Erro { codigo, .. } if codigo == "device_flow_disabled")
        );
        assert_eq!(lidas[5], RespostaToken::Concedido(Segredo::new(TOKEN)));
        assert!(!format!("{:?}", lidas[5]).contains(TOKEN));
        let p = pedidos.lock().unwrap()[0].clone();
        assert_eq!(p.linha, "POST /login/oauth/access_token HTTP/1.1");
        assert_eq!(
            p.corpo,
            "client_id=cid&device_code=dc-123&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        );
    }

    #[tokio::test]
    async fn usuario_com_conta_e_escopos_do_cabecalho() {
        let (base, pedidos) = servidor(vec![(
            200,
            vec![("x-oauth-scopes", "repo, read:org")],
            r#"{"login":"octocat","id":1}"#.into(),
        )])
        .await;

        let u = cliente(&base).usuario(&Segredo::new(TOKEN)).await.unwrap();

        assert_eq!(u.login, "octocat");
        assert_eq!(u.escopos, ["repo", "read:org"]);
        let p = pedidos.lock().unwrap()[0].clone();
        assert_eq!(p.linha, "GET /user HTTP/1.1");
        assert!(
            p.cabecalhos.contains(&format!(
                "authorization: bearer {}",
                TOKEN.to_ascii_lowercase()
            )),
            "{p:?}"
        );
    }

    #[tokio::test]
    async fn usuario_401_e_token_recusado_e_500_sai_mascarado() {
        let (base, _) = servidor(vec![
            (401, vec![], r#"{"message":"Bad credentials"}"#.into()),
            (500, vec![], format!(r#"{{"message":"eco {TOKEN}"}}"#)),
        ])
        .await;
        let gh = cliente(&base);
        let token = Segredo::new(TOKEN);

        assert_eq!(gh.usuario(&token).await, Err(ErroHost::TokenRecusado));
        let erro = gh.usuario(&token).await.unwrap_err();
        let saida = format!("{erro} {erro:?}");
        assert!(matches!(erro, ErroHost::Resposta(_)), "{saida}");
        assert!(!saida.contains("T0k3n"), "{saida}");
        assert!(saida.contains("HTTP 500"), "{saida}");
    }

    #[tokio::test]
    async fn rede_fora_do_ar_e_erro_de_rede_sem_o_token() {
        // Porta que acabou de ser liberada: conexão recusada.
        let ouvinte = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", ouvinte.local_addr().unwrap());
        drop(ouvinte);

        let erro = cliente(&base)
            .usuario(&Segredo::new(TOKEN))
            .await
            .unwrap_err();

        let saida = format!("{erro} {erro:?}");
        assert!(matches!(erro, ErroHost::Rede(_)), "{saida}");
        assert!(!saida.contains("T0k3n"), "{saida}");
    }

    #[test]
    fn escopos_do_cabecalho() {
        assert_eq!(ler_escopos(""), Vec::<String>::new());
        assert_eq!(ler_escopos("repo"), ["repo"]);
        assert_eq!(ler_escopos(" repo ,  workflow,"), ["repo", "workflow"]);
    }
}
