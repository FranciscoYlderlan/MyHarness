//! Roda o `fake-claude` com a chamada montada pelo núcleo e confere o que ele devolve.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use my_harness_core::application::chamada_claude::{ChamadaClaude, montar_chamada};
use my_harness_core::domain::execucao::EspecificacaoExecucao;
use serde_json::Value;

fn especificacao() -> EspecificacaoExecucao {
    let tmp = std::env::temp_dir();
    EspecificacaoExecucao {
        papel: "implementador".into(),
        prompt_do_papel: "Você é o implementador; entregue uma tarefa só.".into(),
        prompt_da_tarefa: "-p começa com hífen e tem \"aspas\", $HOME e espaços".into(),
        modelo: "sonnet".into(),
        ferramentas_permitidas: vec!["Read".into(), "Bash(cargo test:*)".into()],
        ferramentas_proibidas: vec!["WebFetch".into()],
        pastas_extras: vec![tmp.join("pasta com espaço")],
        portao: tmp.join("harness-gate"),
        mcp_config: Some(tmp.join("mcp.json")),
    }
}

/// Roda o binário fake como o `AgentRunner` fará: ambiente herdado limpo, só o montado.
fn rodar(chamada: &ChamadaClaude) -> Vec<Value> {
    let saida = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
        .args(chamada.argumentos())
        .env_clear()
        .envs(chamada.ambiente().iter().cloned())
        .output()
        .expect("fake-claude abre");
    assert!(saida.status.success(), "{saida:?}");
    String::from_utf8(saida.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("cada linha é um JSON"))
        .collect()
}

#[test]
fn fake_devolve_os_argumentos_montados_em_stream_json() {
    let mut pai: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    pai.push(("GITHUB_TOKEN".into(), "ghp_naoDeveChegar".into()));
    pai.push(("GH_TOKEN".into(), "gho_naoDeveChegar".into()));
    pai.push((
        "MH_TESTE_REPO_URL".into(),
        "https://x-access-token:ghs_abcdefabcdef@github.com/o/r".into(),
    ));
    pai.push((
        "MH_TESTE_GIT_CONFIG_VALUE".into(),
        "Authorization: bearer ghp_abc".into(),
    ));
    pai.push(("MEU_TESTE_FICA".into(), "1".into()));
    // O PATH do pai, com uma pasta que traz token do GitHub no meio.
    let pastas_do_pai: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    let com_token = std::env::temp_dir()
        .join("ghp_naoDeveChegarPath")
        .join("bin");
    let mut path_do_pai = pastas_do_pai.clone();
    path_do_pai.insert(path_do_pai.len().min(1), com_token);
    pai.retain(|(n, _)| !n.eq_ignore_ascii_case("PATH"));
    pai.push(("PATH".into(), std::env::join_paths(&path_do_pai).unwrap()));
    let chamada = montar_chamada(&especificacao(), pai).unwrap();

    let eventos = rodar(&chamada);
    assert_eq!(eventos.len(), 3, "{eventos:?}");

    assert_eq!(eventos[0]["type"], "system");
    assert_eq!(eventos[0]["subtype"], "init");
    assert_eq!(eventos[0]["model"], "sonnet");
    assert_eq!(eventos[0]["permissionMode"], "dontAsk");

    let recebidos: Vec<&str> = eventos[1]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(recebidos, chamada.argumentos());
    // Cada argumento chegou inteiro, inclusive o vazio e o prompt com espaços e aspas.
    let n = recebidos.len();
    assert_eq!(recebidos[n - 2], "--");
    assert_eq!(
        recebidos[n - 1],
        "-p começa com hífen e tem \"aspas\", $HOME e espaços"
    );
    let fonte = recebidos
        .iter()
        .position(|a| *a == "--setting-sources")
        .unwrap();
    assert_eq!(recebidos[fonte + 1], "");
    let pasta =
        PathBuf::from(recebidos[recebidos.iter().position(|a| *a == "--add-dir").unwrap() + 1]);
    assert_eq!(pasta, std::env::temp_dir().join("pasta com espaço"));
    let mcp = recebidos.iter().position(|a| *a == "--mcp-config").unwrap();
    assert_eq!(
        PathBuf::from(recebidos[mcp + 1]),
        std::env::temp_dir().join("mcp.json")
    );
    assert_eq!(
        recebidos
            .iter()
            .filter(|a| **a == "--strict-mcp-config")
            .count(),
        1
    );

    let env: Vec<&str> = eventos[1]["env"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert!(env.contains(&"MEU_TESTE_FICA"), "{env:?}");
    assert!(
        !env.iter().any(|n| {
            [
                "GITHUB_TOKEN",
                "GH_TOKEN",
                "MH_TESTE_REPO_URL",
                "MH_TESTE_GIT_CONFIG_VALUE",
            ]
            .iter()
            .any(|t| n.eq_ignore_ascii_case(t))
        }),
        "{env:?}"
    );

    // O PATH chega ao processo sem a pasta com token, com as outras na mesma ordem.
    assert!(env.contains(&"PATH"), "{env:?}");
    let path_recebido: Vec<PathBuf> = eventos[1]["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| PathBuf::from(p.as_str().unwrap()))
        .collect();
    assert_eq!(path_recebido, pastas_do_pai);

    assert_eq!(eventos[2]["type"], "result");
    assert_eq!(eventos[2]["subtype"], "success");
    assert_eq!(eventos[2]["is_error"], false);
}

#[test]
fn sem_mcp_a_chamada_ainda_leva_strict_mcp_config() {
    let mut e = especificacao();
    e.mcp_config = None;
    let chamada = montar_chamada(&e, std::env::vars_os()).unwrap();
    let eventos = rodar(&chamada);
    let recebidos: Vec<&str> = eventos[1]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(recebidos, chamada.argumentos());
    assert!(recebidos.contains(&"--strict-mcp-config"), "{recebidos:?}");
    assert!(!recebidos.contains(&"--mcp-config"), "{recebidos:?}");
}
