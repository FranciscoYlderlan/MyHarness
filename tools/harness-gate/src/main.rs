//! `harness-gate`: gancho `PreToolUse` de todo agente (camada 3 da chamada ao `claude -p`).
//!
//! A CLI roda o portão antes de cada ferramenta e passa a tentativa no stdin, em JSON
//! (`tool_name`, `tool_input`, `cwd`). O portão aplica a política do papel
//! ([`my_harness_core::domain::politica`]):
//! - permitida: sai com 0 e nada no stdout (as outras camadas de permissão seguem valendo);
//! - negada: sai com 2 e o motivo no stderr (a CLI bloqueia a ferramenta e mostra o motivo
//!   ao agente). O motivo começa pelo ERR do contrato quando há um (ex.: `ERR-14`).
//!
//! **Nega por padrão:** sem a variável [`VARIAVEL`], com JSON inválido, com papel
//! desconhecido, com stdin ilegível ou com pânico, a saída é 2. Nunca sai com outro código
//! (a CLI trata código diferente de 0 e 2 como erro que não bloqueia).
//!
//! A política vem do núcleo, na variável de ambiente [`VARIAVEL`] do processo do agente
//! (o gancho herda o ambiente dele):
//!
//! ```json
//! {"papel": "implementador",
//!  "cofres": [{"nome": "meu-cofre", "caminho": "C:\\cofres\\meu-cofre", "memoria": true}],
//!  "pastas_de_escrita": ["C:\\wt\\RF-01"],
//!  "leitura_direta_autorizada": false}
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use my_harness_core::domain::politica::{
    Cofre, Contexto, Decisao, Motivo, Papel, Tentativa, decidir,
};
use serde_json::Value;

/// Variável de ambiente com a política da execução, em JSON.
const VARIAVEL: &str = "MYHARNESS_POLITICA";
/// Código de saída que a CLI entende como "bloquear a ferramenta".
const BLOQUEAR: u8 = 2;
/// Maior entrada aceita no stdin (uma tentativa com arquivo grande no `Write`).
const ENTRADA_MAXIMA: u64 = 16 * 1024 * 1024;

fn main() -> ExitCode {
    // Pânico sairia com 101, que a CLI não trata como bloqueio: sai com 2.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("harness-gate: erro interno: tudo negado");
        std::process::exit(i32::from(BLOQUEAR));
    }));

    let (papel, contexto) = match std::env::var(VARIAVEL) {
        Ok(texto) => ler_politica(&texto).unwrap_or((None, Contexto::default())),
        Err(_) => (None, Contexto::default()),
    };

    let mut entrada = String::new();
    let lida = std::io::stdin()
        .take(ENTRADA_MAXIMA)
        .read_to_string(&mut entrada);
    let tentativa: Option<Value> = lida.ok().and_then(|_| serde_json::from_str(&entrada).ok());
    let Some(tentativa) = tentativa else {
        return negar(papel, "?", Motivo::EntradaInvalida);
    };
    let Some(ferramenta) = tentativa.get("tool_name").and_then(Value::as_str) else {
        return negar(papel, "?", Motivo::EntradaInvalida);
    };
    let vazio = Value::Object(Default::default());
    let t = Tentativa {
        ferramenta,
        entrada: tentativa.get("tool_input").unwrap_or(&vazio),
        pasta_atual: tentativa.get("cwd").and_then(Value::as_str).map(Path::new),
    };

    match decidir(papel, &contexto, &t) {
        Decisao::Permitir => ExitCode::SUCCESS,
        Decisao::Negar(motivo) => negar(papel, ferramenta, motivo),
    }
}

fn negar(papel: Option<Papel>, ferramenta: &str, motivo: Motivo) -> ExitCode {
    let papel = papel.map_or("sem papel", Papel::nome);
    eprintln!("harness-gate: {papel}: {ferramenta}: {motivo}");
    ExitCode::from(BLOQUEAR)
}

/// Lê a política. Qualquer campo inválido invalida tudo (`None`): o portão nega tudo.
fn ler_politica(texto: &str) -> Option<(Option<Papel>, Contexto)> {
    let v: Value = serde_json::from_str(texto).ok()?;
    let papel = Papel::do_nome(v.get("papel")?.as_str()?)?;
    let caminhos = |chave: &str| -> Option<Vec<PathBuf>> {
        match v.get(chave) {
            None => Some(Vec::new()),
            Some(lista) => lista
                .as_array()?
                .iter()
                .map(|c| {
                    c.as_str()
                        .filter(|c| !c.trim().is_empty())
                        .map(PathBuf::from)
                })
                .collect(),
        }
    };
    let cofres = match v.get("cofres") {
        None => Vec::new(),
        Some(lista) => lista
            .as_array()?
            .iter()
            .map(|c| {
                let nome = c.get("nome")?.as_str()?.to_owned();
                let caminho = c.get("caminho")?.as_str()?;
                if nome.trim().is_empty() || caminho.trim().is_empty() {
                    return None;
                }
                Some(Cofre {
                    nome,
                    caminho: PathBuf::from(caminho),
                    memoria: c.get("memoria").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect::<Option<Vec<_>>>()?,
    };
    let leitura_direta_autorizada = match v.get("leitura_direta_autorizada") {
        None => false,
        Some(b) => b.as_bool()?,
    };
    Some((
        Some(papel),
        Contexto {
            cofres,
            pastas_de_escrita: caminhos("pastas_de_escrita")?,
            leitura_direta_autorizada,
        },
    ))
}
