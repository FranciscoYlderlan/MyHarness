//! MyHarness desktop shell: Tauri commands and infrastructure adapters.

pub mod commands;
pub mod infra;

use tauri::Manager;

/// Starts the desktop app: opens the SQLite database (running migrations) and shows the main window.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db = tauri::async_runtime::block_on(infra::db::Database::open(&data_dir))?;
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running MyHarness");
}
