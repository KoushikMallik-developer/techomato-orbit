pub mod commands;
pub mod db;
pub mod engine;
pub mod error;
pub mod models;
pub mod repo;
pub mod secrets;
pub mod single_instance;
pub mod tray;
pub mod windows;

use secrets::SecretStore;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// `%APPDATA%\Techomato Orbit`
fn data_dir(app: &tauri::App) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().data_dir()?.join("Techomato Orbit"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A second Orbit would supervise the same services; bring the first one forward instead.
    let Some(_single) = single_instance::acquire(single_instance::APP_MUTEX) else {
        single_instance::focus_existing();
        return;
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; Quit in the tray menu really exits.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let dir = data_dir(app)?;
            match db::open(&dir.join("orbit.db")) {
                Ok(database) => {
                    let database = Arc::new(database);
                    let secrets: Arc<dyn SecretStore> =
                        Arc::new(windows::creds::CredentialManagerStore);
                    let handle = app.handle().clone();
                    let sup = engine::supervisor::Supervisor::new(
                        database.clone(),
                        secrets.clone(),
                        dir.join("logs"),
                        std::env::current_exe()?,
                        Arc::new(move |rt| {
                            let _ = handle.emit("service://status", rt);
                        }),
                    );
                    let log_handle = app.handle().clone();
                    sup.spawn_log_emitter(move |batch| {
                        let _ = log_handle.emit("log://line", batch);
                    });
                    sup.adopt_on_startup();
                    app.manage(database);
                    app.manage(secrets);
                    app.manage(sup);
                    tray::setup(app)?;
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
            commands::services::service_start,
            commands::services::service_stop,
            commands::services::service_force_stop,
            commands::services::service_restart,
            commands::services::service_runtime_list,
            commands::services::service_secret_is_set,
            commands::logs::log_recent,
            commands::logs::log_history,
            commands::logs::log_clear,
            commands::logs::log_export,
            commands::ports::ports_list,
            commands::ports::port_check,
            commands::pick_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Techomato Orbit");
}
