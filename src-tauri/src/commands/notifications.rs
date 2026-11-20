use crate::db::Db;
use crate::engine::notify::Notice;
use crate::error::AppResult;
use crate::models::prefs::Prefs;
use crate::repo::notifications::{self as repo, StoredNotice};
use crate::repo::settings;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};
use tauri_plugin_notification::NotificationExt;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Records the notice in the history and shows it natively. Failing to show it (notifications
/// turned off in Windows) never fails the caller; the history entry is still there.
pub fn deliver(app: &AppHandle, db: &Db, notice: &Notice, service_id: Option<&str>) {
    let _ = repo::add(
        db,
        now_ms(),
        notice.kind.as_str(),
        &notice.title,
        &notice.body,
        service_id,
    );
    let _ = app
        .notification()
        .builder()
        .title(&notice.title)
        .body(&notice.body)
        .show();
}

#[tauri::command]
pub async fn notification_list(
    db: State<'_, Arc<Db>>,
    limit: Option<i64>,
) -> AppResult<Vec<StoredNotice>> {
    repo::list(&db, limit.unwrap_or(100).clamp(1, repo::KEEP))
}

#[tauri::command]
pub async fn notification_clear(db: State<'_, Arc<Db>>) -> AppResult<()> {
    repo::clear(&db)
}

#[tauri::command]
pub async fn prefs_get(db: State<'_, Arc<Db>>) -> AppResult<Prefs> {
    settings::prefs(&db)
}

#[tauri::command]
pub async fn prefs_set(db: State<'_, Arc<Db>>, prefs: Prefs) -> AppResult<()> {
    settings::set_prefs(&db, &prefs)
}

/// Whether Orbit starts with Windows (registry Run key, managed by the autostart plugin).
#[tauri::command]
pub async fn autostart_get(app: tauri::AppHandle) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| crate::error::AppError::new(crate::error::ErrorKind::Os, e.to_string()))
}

#[tauri::command]
pub async fn autostart_set(app: tauri::AppHandle, enabled: bool) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let launch = app.autolaunch();
    let result = if enabled {
        launch.enable()
    } else {
        launch.disable()
    };
    result.map_err(|e| crate::error::AppError::new(crate::error::ErrorKind::Os, e.to_string()))
}

/// Which global shortcuts are active, and why any are not.
#[tauri::command]
pub async fn shortcuts_status(
    state: State<'_, crate::shortcuts::Shortcuts>,
) -> AppResult<Vec<crate::shortcuts::ShortcutStatus>> {
    Ok(state.0.lock().unwrap().clone())
}

/// True when Windows started Orbit at sign-in; the window then stays in the tray.
#[tauri::command]
pub fn launched_in_background() -> bool {
    std::env::args().any(|a| a == "--background")
}
