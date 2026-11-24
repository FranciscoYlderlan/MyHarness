//! Contrato IPC entre a tela (React) e o núcleo (Rust): comandos, eventos e os tipos
//! que cruzam a ponte, exportados para TypeScript em `src/bindings/` pelo `tauri-specta`.
//!
//! - [`dto`]: tipos derivados dos ENT do contrato funcional.
//! - [`erro`]: erros e avisos com o código do catálogo `05-erros`.
//! - [`eventos`]: eventos globais e o canal por execução.
//! - [`integracoes`]: o núcleo das integrações (UC-01): máscara, registro, RN-08.
//! - [`nucleo`]: a interface que os comandos chamam e o corpo dela.
//! - [`comandos`]: os comandos do Tauri.
//!
//! Gerar os bindings de novo: `pnpm bindings`. O teste `bindings_estao_em_dia` falha
//! se o arquivo commitado divergir do que o Rust gera.

pub mod comandos;
pub mod dto;
pub mod erro;
pub mod eventos;
pub mod integracoes;
pub mod nucleo;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use specta_typescript::Typescript;
use tauri_specta::{Builder, ErrorHandlingMode, collect_events};

/// Caminho dos bindings gerados, a partir da raiz do repositório.
pub const CAMINHO_BINDINGS: &str = "src/bindings/index.ts";

/// Cabeçalho do arquivo gerado.
const CABECALHO: &str = "// Gerado pelo tauri-specta a partir de src-tauri/src/commands/. Nunca editar à mão:\n// rode `pnpm bindings` para gerar de novo.\n/* eslint-disable */\n";

/// O construtor do `tauri-specta` com todos os comandos e eventos do contrato.
pub fn construtor() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(comandos::todos())
        .events(collect_events![
            eventos::IntegracaoMudou,
            eventos::ConsumoAtualizado,
            eventos::ChatMudou,
            eventos::ExecucaoMudou,
            eventos::LogRegistrado,
            eventos::LeituraRegistrada,
            eventos::PendenciasMudaram,
        ])
        .typ::<eventos::EventoExecucao>()
        .typ::<erro::ErroIpc>()
        // Relatório (ENT-09) e Critério (ENT-24): no contrato, ainda sem comando nem evento.
        .typ::<dto::Relatorio>()
        .disable_serde_phases()
        .error_handling(ErrorHandlingMode::Result)
        .mocks()
}

/// Falha ao gerar os bindings.
#[derive(Debug, thiserror::Error)]
pub enum ErroBindings {
    #[error("tauri-specta não exportou os bindings: {0}")]
    Exportar(#[from] specta_typescript::Error),
    #[error("não deu para ler ou gravar os bindings: {0}")]
    Arquivo(#[from] std::io::Error),
    #[error("o Prettier do projeto não formatou os bindings ({0}); rode `pnpm install` na raiz")]
    Prettier(String),
}

/// Gera os bindings TypeScript em `destino`, já formatados pelo Prettier do projeto
/// (`node_modules/prettier`, com a configuração da raiz), para passarem no
/// `prettier --check` sem exceção no `.prettierignore`.
///
/// `raiz` é a raiz do repositório: dela vêm o Prettier e a configuração, como se o
/// arquivo estivesse em [`CAMINHO_BINDINGS`], qualquer que seja o `destino`.
pub fn exportar_bindings(raiz: &Path, destino: &Path) -> Result<(), ErroBindings> {
    if let Some(pasta) = destino.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    construtor().export(Typescript::default().header(CABECALHO), destino)?;
    let cru = std::fs::read_to_string(destino)?;
    let formatado = formatar_com_prettier(raiz, &cru)?;
    std::fs::write(destino, formatado)?;
    Ok(())
}

fn formatar_com_prettier(raiz: &Path, codigo: &str) -> Result<String, ErroBindings> {
    let prettier = raiz.join("node_modules/prettier/bin/prettier.cjs");
    if !prettier.is_file() {
        return Err(ErroBindings::Prettier(format!(
            "{} não existe",
            prettier.display()
        )));
    }
    let mut processo = Command::new("node")
        .arg(&prettier)
        .arg("--stdin-filepath")
        .arg(raiz.join(CAMINHO_BINDINGS))
        .current_dir(raiz)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ErroBindings::Prettier(format!("não abriu o node: {e}")))?;
    processo
        .stdin
        .take()
        .expect("stdin com pipe")
        .write_all(codigo.as_bytes())?;
    let saida = processo.wait_with_output()?;
    if !saida.status.success() {
        return Err(ErroBindings::Prettier(
            String::from_utf8_lossy(&saida.stderr).into_owned(),
        ));
    }
    String::from_utf8(saida.stdout).map_err(|e| ErroBindings::Prettier(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn raiz() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri fica na raiz do repositório")
            .to_path_buf()
    }

    fn sem_cr(t: &str) -> String {
        t.replace("\r\n", "\n")
    }

    /// `pnpm bindings` roda este teste para gravar `src/bindings/index.ts`.
    #[test]
    #[ignore = "gera os bindings; rode com `pnpm bindings`"]
    fn gerar_bindings() {
        let destino = raiz().join(CAMINHO_BINDINGS);
        exportar_bindings(&raiz(), &destino).expect("exportar os bindings");
        println!("bindings gravados em {}", destino.display());
    }

    #[test]
    fn bindings_estao_em_dia() {
        let temporario = tempfile::tempdir().unwrap();
        let gerado = temporario.path().join("index.ts");
        exportar_bindings(&raiz(), &gerado).expect("exportar os bindings");
        let gerado = sem_cr(&std::fs::read_to_string(&gerado).unwrap());

        let commitado = raiz().join(CAMINHO_BINDINGS);
        let commitado = std::fs::read_to_string(&commitado).unwrap_or_default();
        assert!(
            sem_cr(&commitado) == gerado,
            "{CAMINHO_BINDINGS} diverge do que o Rust gera: rode `pnpm bindings` e commite o \
             resultado"
        );
    }

    #[test]
    fn todo_comando_esta_nos_bindings() {
        let temporario = tempfile::tempdir().unwrap();
        let gerado = temporario.path().join("index.ts");
        exportar_bindings(&raiz(), &gerado).unwrap();
        let ts = std::fs::read_to_string(&gerado).unwrap();
        for nome in comandos::NOMES {
            let camel = camel_case(nome);
            assert!(
                ts.contains(&format!("\"{nome}\"")) && ts.contains(&format!("  {camel}: (")),
                "comando {nome} ({camel}) fora dos bindings"
            );
        }
        for evento in [
            "integracaoMudou",
            "consumoAtualizado",
            "chatMudou",
            "execucaoMudou",
            "logRegistrado",
            "leituraRegistrada",
            "pendenciasMudaram",
        ] {
            assert!(ts.contains(evento), "evento {evento} fora dos bindings");
        }
        // Nenhuma data sai como número nem com deslocamento: o tipo é texto.
        assert!(ts.contains("export type DataHoraUtc = string"));
    }

    fn camel_case(snake: &str) -> String {
        let mut saida = String::new();
        let mut maiuscula = false;
        for c in snake.chars() {
            if c == '_' {
                maiuscula = true;
            } else if maiuscula {
                saida.extend(c.to_uppercase());
                maiuscula = false;
            } else {
                saida.push(c);
            }
        }
        saida
    }
}
