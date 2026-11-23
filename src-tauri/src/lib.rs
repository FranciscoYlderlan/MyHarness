//! MyHarness desktop shell: Tauri commands and infrastructure adapters.

pub mod commands;
pub mod infra;

use std::sync::Arc;

use tauri::Manager;

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
            // Provisional core (no database, no business rules) until the use cases land.
            let nucleo: commands::comandos::NucleoApp =
                Arc::new(commands::nucleo::NucleoProvisorio);
            app.manage(nucleo);
            ipc.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running MyHarness");
}
