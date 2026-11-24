use tauri_build::{Attributes, WindowsAttributes};

/// Manifesto do app no Windows (cópia do `windows-app-manifest.xml` padrão do `tauri-build`,
/// com a dependência do Common Controls v6).
const WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

fn main() {
    // O manifesto padrão do Tauri só entra no executável do app; os binários de teste ficam
    // sem ele e falham ao carregar no Windows (STATUS_ENTRYPOINT_NOT_FOUND, comctl32 v6).
    // Desligamos o manifesto do Tauri e embutimos o mesmo manifesto em todo alvo ligado.
    embed_windows_manifest();

    let attributes =
        Attributes::new().windows_attributes(WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("cargo:warning={error:#}");
        std::process::exit(1);
    }
}

fn embed_windows_manifest() {
    println!("cargo:rerun-if-changed={WINDOWS_MANIFEST_FILE}");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join(WINDOWS_MANIFEST_FILE);
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
