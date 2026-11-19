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
        .plugin(tauri_plugin_notification::init())
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
                    let notify_db = database.clone();
                    // Last runtime seen per service, so each change can be turned into a notice.
                    let last: std::sync::Mutex<
                        std::collections::HashMap<String, engine::supervisor::ServiceRuntime>,
                    > = Default::default();
                    let sup = engine::supervisor::Supervisor::new(
                        database.clone(),
                        secrets.clone(),
                        dir.join("logs"),
                        std::env::current_exe()?,
                        Arc::new(move |rt| {
                            let _ = handle.emit("service://status", rt);
                            let prev = last.lock().unwrap().insert(rt.id.clone(), rt.clone());
                            if prev.as_ref() == Some(rt) {
                                return;
                            }
                            let (Ok(prefs), Ok(spec)) = (
                                repo::settings::prefs(&notify_db),
                                repo::services::get(&notify_db, &rt.id),
                            ) else {
                                return;
                            };
                            if let Some(n) = engine::notify::notice_for(
                                prev.as_ref(),
                                rt,
                                &spec.name,
                                &prefs.notify,
                            ) {
                                commands::notifications::deliver(
                                    &handle,
                                    &notify_db,
                                    &n,
                                    Some(&rt.id),
                                );
                            }
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
            commands::ports::port_end_process,
            commands::notifications::notification_list,
            commands::notifications::notification_clear,
            commands::notifications::notify_port_conflict,
            commands::notifications::prefs_get,
            commands::notifications::prefs_set,
            commands::projects::project_list,
            commands::projects::project_create,
            commands::projects::project_update,
            commands::projects::project_archive,
            commands::projects::project_delete,
            commands::projects::project_duplicate,
            commands::projects::dependency_edges,
            commands::projects::dependency_set,
            commands::projects::project_start,
            commands::projects::project_stop,
            commands::projects::project_restart,
            commands::pick_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Techomato Orbit");
}
