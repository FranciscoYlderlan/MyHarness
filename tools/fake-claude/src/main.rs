//! Claude CLI falso para testes: nunca chama a rede nem a CLI real.
//!
//! Escreve no stdout, uma linha JSON por evento (estilo `--output-format stream-json`):
//! 1. `{"type":"system","subtype":"init",…}` com o modelo e o modo de permissão recebidos;
//! 2. `{"type":"fake_args","args":[…],"env":[…]}` com os argumentos recebidos, na ordem,
//!    e os nomes (nunca os valores) das variáveis de ambiente, em ordem alfabética;
//! 3. `{"type":"result","subtype":"success",…}` final. Sai com código 0.

use std::io::{self, Write};

use serde_json::{Value, json};

const SESSAO: &str = "fake-session";

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut env: Vec<String> = std::env::vars_os()
        .map(|(nome, _)| nome.to_string_lossy().into_owned())
        .collect();
    env.sort();

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
        json!({ "type": "fake_args", "args": args, "env": env }),
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
