//! MyHarness desktop shell: Tauri commands and infrastructure adapters.

pub mod commands;
pub mod infra;

use std::sync::Arc;

use tauri::Manager;
use tauri_specta::Event;

/// Starts the desktop app: opens the SQLite database (running migrations), registers the
/// typed IPC contract (`commands`) and shows the main window.
pub fn run() {
    let ipc = commands::construtor();
    tauri::Builder::default()
        .invoke_handler(ipc.invoke_handler())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let db = tauri::async_runtime::block_on(infra::db::Database::open(&data_dir))?;
            app.manage(db);
            // Core: the three integrations are real (UC-01); the rest is still provisional.
            let mascara = Arc::new(commands::integracoes::MascaraNucleo::default());
            let registro: Arc<dyn commands::integracoes::RegistroNucleo> =
                Arc::new(commands::integracoes::RegistroNoTerminal);
            let janela = app.handle().clone();
            let integracoes = commands::integracoes::ServicoIntegracoes::new(
                commands::integracoes::integracoes_do_ambiente(),
                mascara,
                registro.clone(),
            )
            .com_aviso(Arc::new(move |estado| {
                let evento = commands::eventos::IntegracaoMudou {
                    integracao: estado.clone().into(),
                };
                // Window not ready yet: the screen reads the state with `listar_integracoes`.
                let _ = evento.emit(&janela);
            }));
            let nucleo: commands::comandos::NucleoApp = Arc::new(
                commands::nucleo::NucleoMyHarness::new(integracoes, registro),
            );
            app.manage(nucleo);
            ipc.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MyHarness");
}
