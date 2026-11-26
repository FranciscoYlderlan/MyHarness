//! HTTP/1.1 mínimo para o transporte HTTP do MCP: um `POST` por mensagem, sem TLS.
//!
//! O servidor MCP da Obsidian CLI roda na máquina do Humano (ou em `host.docker.internal`
//! no container), então só `http://` é aceito. A resposta pode vir com `Content-Length`,
//! em `chunked` ou até o fim da conexão, e o corpo pode ser JSON ou um fluxo SSE
//! (transporte Streamable HTTP do MCP): no SSE, a leitura para assim que chega o evento
//! que [`Parada`] reconhece, sem esperar o servidor fechar o fluxo.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::ErroMcp;

/// Endereço `http://host[:porta]/caminho` já separado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endereco {
    pub host: String,
    pub porta: u16,
    pub caminho: String,
}

impl Endereco {
    /// Lê `http://host[:porta][/caminho]`. Outro esquema (ex.: `https`) é recusado.
    pub fn ler(url: &str) -> Result<Self, ErroMcp> {
        let invalido = || ErroMcp::EnderecoInvalido(url.to_string());
        let resto = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("HTTP://"))
            .ok_or_else(invalido)?;
        let (autoridade, caminho) = match resto.find('/') {
            Some(i) => (&resto[..i], &resto[i..]),
            None => (resto, "/"),
        };
        let (host, porta) = match autoridade.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|_| invalido())?),
            None => (autoridade, 80),
        };
        if host.is_empty() {
            return Err(invalido());
        }
        Ok(Self {
            host: host.to_string(),
            porta,
            caminho: caminho.to_string(),
        })
    }
}

/// Resposta HTTP já com o corpo decodificado (sem `chunked`).
#[derive(Debug, Clone)]
pub(crate) struct RespostaHttp {
    pub status: u16,
    pub cabecalhos: Vec<(String, String)>,
    pub corpo: Vec<u8>,
}

impl RespostaHttp {
    /// Valor do cabeçalho, sem diferenciar maiúsculas no nome.
    pub fn cabecalho(&self, nome: &str) -> Option<&str> {
        self.cabecalhos
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nome))
            .map(|(_, v)| v.as_str())
    }

    /// `true` quando o corpo é um fluxo SSE.
    pub fn e_sse(&self) -> bool {
        self.cabecalho("content-type")
            .is_some_and(|c| c.to_ascii_lowercase().starts_with("text/event-stream"))
    }
}

/// Diz se o corpo SSE lido até agora já traz o que se espera (para parar de ler).
pub(crate) type Parada<'a> = &'a (dyn Fn(&[u8]) -> bool + Send + Sync);

/// Envia `POST` com `corpo` JSON e lê a resposta. Quem chama aplica o tempo limite.
pub(crate) async fn post(
    endereco: &Endereco,
    cabecalhos_extra: &[(&str, &str)],
    corpo: &[u8],
    parada_sse: Parada<'_>,
) -> Result<RespostaHttp, ErroMcp> {
    let mut conexao = TcpStream::connect((endereco.host.as_str(), endereco.porta))
        .await
        .map_err(|e| ErroMcp::Rede(e.to_string()))?;

    let mut pedido = format!(
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        endereco.caminho,
        endereco.host,
        endereco.porta,
        corpo.len()
    );
    for (nome, valor) in cabecalhos_extra {
        pedido.push_str(&format!("{nome}: {valor}\r\n"));
    }
    pedido.push_str("\r\n");
    conexao
        .write_all(pedido.as_bytes())
        .await
        .map_err(|e| ErroMcp::Rede(e.to_string()))?;
    conexao
        .write_all(corpo)
        .await
        .map_err(|e| ErroMcp::Rede(e.to_string()))?;

    let mut bruto = Vec::new();
    let mut pedaco = [0u8; 8192];
    loop {
        let lidos = conexao
            .read(&mut pedaco)
            .await
            .map_err(|e| ErroMcp::Rede(e.to_string()))?;
        if lidos > 0 {
            bruto.extend_from_slice(&pedaco[..lidos]);
        }
        let fim_da_conexao = lidos == 0;
        if let Some(resposta) = tentar_montar(&bruto, fim_da_conexao, parada_sse)? {
            return Ok(resposta);
        }
        if fim_da_conexao {
            return Err(ErroMcp::RespostaInvalida(
                "a conexão fechou antes da resposta completa".into(),
            ));
        }
    }
}

/// Monta a resposta se o que chegou já basta; `None` pede mais bytes.
fn tentar_montar(
    bruto: &[u8],
    fim_da_conexao: bool,
    parada_sse: Parada<'_>,
) -> Result<Option<RespostaHttp>, ErroMcp> {
    let Some(fim_cabecalho) = achar(bruto, b"\r\n\r\n") else {
        return Ok(None);
    };
    let cabeca = std::str::from_utf8(&bruto[..fim_cabecalho])
        .map_err(|_| ErroMcp::RespostaInvalida("cabeçalho HTTP fora de UTF-8".into()))?;
    let mut linhas = cabeca.split("\r\n");
    let status = linhas
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| ErroMcp::RespostaInvalida("linha de status HTTP ilegível".into()))?;
    let cabecalhos: Vec<(String, String)> = linhas
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let mut resposta = RespostaHttp {
        status,
        cabecalhos,
        corpo: Vec::new(),
    };
    let resto = &bruto[fim_cabecalho + 4..];

    let chunked = resposta
        .cabecalho("transfer-encoding")
        .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"));
    let tamanho = resposta
        .cabecalho("content-length")
        .and_then(|t| t.parse::<usize>().ok());

    let (corpo, completo) = if chunked {
        dechunk(resto)?
    } else if let Some(n) = tamanho {
        (resto[..resto.len().min(n)].to_vec(), resto.len() >= n)
    } else {
        (resto.to_vec(), fim_da_conexao)
    };

    let pronto = completo || fim_da_conexao || (resposta.e_sse() && parada_sse(&corpo));
    if !pronto {
        return Ok(None);
    }
    resposta.corpo = corpo;
    Ok(Some(resposta))
}

/// Decodifica os pedaços `chunked` completos; devolve o corpo e se o último (tamanho 0) chegou.
fn dechunk(mut dados: &[u8]) -> Result<(Vec<u8>, bool), ErroMcp> {
    let mut corpo = Vec::new();
    loop {
        let Some(fim_linha) = achar(dados, b"\r\n") else {
            return Ok((corpo, false));
        };
        let linha = std::str::from_utf8(&dados[..fim_linha])
            .map_err(|_| ErroMcp::RespostaInvalida("pedaço chunked ilegível".into()))?;
        let hex = linha.split(';').next().unwrap_or("").trim();
        let tamanho = usize::from_str_radix(hex, 16)
            .map_err(|_| ErroMcp::RespostaInvalida("tamanho de pedaço chunked ilegível".into()))?;
        if tamanho == 0 {
            return Ok((corpo, true));
        }
        let inicio = fim_linha + 2;
        if dados.len() < inicio + tamanho + 2 {
            return Ok((corpo, false));
        }
        corpo.extend_from_slice(&dados[inicio..inicio + tamanho]);
        dados = &dados[inicio + tamanho + 2..];
    }
}

fn achar(dados: &[u8], agulha: &[u8]) -> Option<usize> {
    dados.windows(agulha.len()).position(|j| j == agulha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_enderecos_http() {
        assert_eq!(
            Endereco::ler("http://host.docker.internal:27180/mcp").unwrap(),
            Endereco {
                host: "host.docker.internal".into(),
                porta: 27180,
                caminho: "/mcp".into()
            }
        );
        let e = Endereco::ler("http://127.0.0.1").unwrap();
        assert_eq!((e.porta, e.caminho.as_str()), (80, "/"));
        for ruim in ["https://x/mcp", "x:1/mcp", "http://:80/", "http://h:porta/"] {
            assert!(Endereco::ler(ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn dechunk_parcial_e_completo() {
        assert_eq!(dechunk(b"3\r\nabc\r\n").unwrap(), (b"abc".to_vec(), false));
        assert_eq!(
            dechunk(b"3\r\nabc\r\n2;x\r\nde\r\n0\r\n\r\n").unwrap(),
            (b"abcde".to_vec(), true)
        );
        assert_eq!(dechunk(b"5\r\nab").unwrap(), (Vec::new(), false));
        assert!(dechunk(b"zz\r\n").is_err());
    }

    #[test]
    fn monta_resposta_por_content_length() {
        let nunca: Parada<'_> = &|_| false;
        let bruto = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nab";
        assert!(tentar_montar(bruto, false, nunca).unwrap().is_none());
        let bruto = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nabcd";
        let r = tentar_montar(bruto, false, nunca).unwrap().unwrap();
        assert_eq!((r.status, r.corpo.as_slice()), (200, b"abcd".as_slice()));
    }
}
