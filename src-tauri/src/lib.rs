pub mod commands;
pub mod db;
pub mod error;
pub mod models;
pub mod repo;
pub mod secrets;
pub mod windows;

use secrets::{InMemorySecretStore, SecretStore};
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// `%APPDATA%\Techomato Orbit`
fn data_dir(app: &tauri::App) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().data_dir()?.join("Techomato Orbit"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = data_dir(app)?;
            match db::open(&dir.join("orbit.db")) {
                Ok(database) => {
                    app.manage(database);
                    // Replaced by the Windows Credential Manager store in the secrets task.
                    app.manage::<Arc<dyn SecretStore>>(Arc::new(InMemorySecretStore::default()));
                    Ok(())
                }
                Err(e) => {
                    app.dialog()
                        .message(e.message.clone())
                        .title("Techomato Orbit cannot start")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    Err(Box::new(e))
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::services::service_list,
            commands::services::service_get,
            commands::services::service_create,
            commands::services::service_update,
            commands::services::service_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Techomato Orbit");
}
