//! Claude CLI falso para testes: nunca chama a rede nem a CLI real.
//!
//! Sem cenário, escreve no stdout, uma linha JSON por evento (estilo
//! `--output-format stream-json`):
//! 1. `{"type":"system","subtype":"init",…}` com o modelo e o modo de permissão recebidos;
//! 2. `{"type":"fake_args","args":[…],"env":[…]}` com os argumentos recebidos, na ordem,
//!    os nomes (nunca os valores) das variáveis de ambiente, em ordem alfabética, e em
//!    `path` as pastas do `PATH` recebido, na ordem (o único valor de ambiente que sai,
//!    para conferir o filtro da camada 4; `null` sem `PATH`);
//! 3. `{"type":"result","subtype":"success",…}` final. Sai com código 0.
//!
//! Com cenário (arquivo `<executável>.cenario.json` ao lado do binário, que o teste copia
//! para uma pasta temporária), responde também a `--version`, `auth status` e
//! `auth login` como o teste mandar (ver [`Cenario`]); o login bem-sucedido cria o
//! arquivo `<executável>.logado`, que o `auth status` gerado passa a ler.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

const SESSAO: &str = "fake-session";

/// Resposta configurada para um subcomando.
#[derive(Default)]
struct Resposta {
    stdout: String,
    stderr: String,
    codigo: i32,
    dormir_ms: u64,
}

impl Resposta {
    fn de(valor: Option<&Value>) -> Option<Self> {
        let valor = valor?;
        let texto = |chave: &str| {
            valor
                .get(chave)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(Self {
            stdout: texto("stdout"),
            stderr: texto("stderr"),
            codigo: valor.get("codigo").and_then(Value::as_i64).unwrap_or(0) as i32,
            dormir_ms: valor.get("dormir_ms").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    /// Escreve as saídas, linha a linha, e sai com o código configurado.
    fn responder(self) -> ! {
        let mut out = io::stdout().lock();
        let mut err = io::stderr().lock();
        let _ = out.write_all(self.stdout.as_bytes());
        let _ = out.flush();
        let _ = err.write_all(self.stderr.as_bytes());
        let _ = err.flush();
        if self.dormir_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.dormir_ms));
        }
        std::process::exit(self.codigo)
    }
}

/// Cenário lido de `<executável>.cenario.json`. Campos (todos opcionais):
/// - `versao`: resposta a `--version` (padrão: `2.1.0 (Claude Code)`, código 0);
/// - `status`: resposta a `auth status`; sem ela, gera o JSON da CLI real
///   (`loggedIn` e, logado, `email`), com código 0 logado e 1 sem login;
/// - `logado`: começa logado (padrão `false`); `email`: conta mostrada quando logado;
/// - `login`: resposta a `auth login` (padrão: uma linha com um link, código 0) e
///   `loga` (padrão `true`): se o login com código 0 deixa a CLI logada.
struct Cenario {
    json: Value,
    marcador: PathBuf,
}

impl Cenario {
    fn carregar() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let mut arquivo = exe.clone().into_os_string();
        arquivo.push(".cenario.json");
        let texto = std::fs::read_to_string(PathBuf::from(arquivo)).ok()?;
        let mut marcador = exe.into_os_string();
        marcador.push(".logado");
        Some(Self {
            json: serde_json::from_str(&texto).expect("cenário em JSON"),
            marcador: PathBuf::from(marcador),
        })
    }

    fn logado(&self) -> bool {
        self.marcador.exists() || self.json.get("logado").and_then(Value::as_bool) == Some(true)
    }

    fn versao(&self) -> ! {
        Resposta::de(self.json.get("versao"))
            .unwrap_or_else(|| Resposta {
                stdout: "2.1.0 (Claude Code)\n".into(),
                ..Resposta::default()
            })
            .responder()
    }

    fn status(&self) -> ! {
        if let Some(resposta) = Resposta::de(self.json.get("status")) {
            resposta.responder()
        }
        let logado = self.logado();
        let mut corpo =
            json!({ "loggedIn": logado, "authMethod": if logado { "claude.ai" } else { "none" } });
        if logado {
            corpo["email"] = self.json.get("email").cloned().unwrap_or(Value::Null);
        }
        Resposta {
            stdout: format!("{}\n", serde_json::to_string_pretty(&corpo).unwrap()),
            codigo: if logado { 0 } else { 1 },
            ..Resposta::default()
        }
        .responder()
    }

    fn login(&self) -> ! {
        let configurado = self.json.get("login");
        let resposta = Resposta::de(configurado).unwrap_or_else(|| Resposta {
            stdout: "Opening browser to sign in…\nhttps://claude.ai/oauth/authorize?fake=1\n"
                .into(),
            ..Resposta::default()
        });
        let loga = configurado
            .and_then(|c| c.get("loga"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if resposta.codigo == 0 && loga {
            std::fs::write(&self.marcador, b"").expect("grava o marcador de login");
        }
        resposta.responder()
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    if let Some(cenario) = Cenario::carregar() {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match args.as_slice() {
            ["--version", ..] => cenario.versao(),
            ["auth", "status", ..] => cenario.status(),
            ["auth", "login", ..] => cenario.login(),
            _ => {}
        }
    }

    let mut env: Vec<String> = std::env::vars_os()
        .map(|(nome, _)| nome.to_string_lossy().into_owned())
        .collect();
    env.sort();
    let path: Value = std::env::var_os("PATH").map_or(Value::Null, |valor| {
        std::env::split_paths(&valor)
            .map(|p| Value::String(p.to_string_lossy().into_owned()))
            .collect()
    });

    let depois = |opcao: &str| -> Value {
        args.iter()
            .position(|a| a == opcao)
            .and_then(|i| args.get(i + 1))
            .map_or(Value::Null, |v| Value::String(v.clone()))
    };

    let eventos = [
        json!({
            "type": "system",
            "subtype": "init",
            "session_id": SESSAO,
            "model": depois("--model"),
            "permissionMode": depois("--permission-mode"),
        }),
        json!({ "type": "fake_args", "args": args, "env": env, "path": path }),
        json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "num_turns": 0,
            "result": "fake-claude ok",
            "session_id": SESSAO,
        }),
    ];

    let mut saida = io::stdout().lock();
    for evento in eventos {
        writeln!(saida, "{evento}")?;
    }
    saida.flush()
}
