use crate::db::Db;
use crate::engine::notify::{port_conflict, Notice};
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

/// Called by the Ports page when a watched port is taken by another program.
#[tauri::command]
pub async fn notify_port_conflict(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    port: u16,
    process: String,
    pid: u32,
) -> AppResult<()> {
    let prefs = settings::prefs(&db)?;
    if let Some(n) = port_conflict(port, &process, pid, &prefs.notify) {
        deliver(&app, &db, &n, None);
    }
    Ok(())
}

#[tauri::command]
pub async fn prefs_get(db: State<'_, Arc<Db>>) -> AppResult<Prefs> {
    settings::prefs(&db)
}

#[tauri::command]
pub async fn prefs_set(db: State<'_, Arc<Db>>, prefs: Prefs) -> AppResult<()> {
    settings::set_prefs(&db, &prefs)
}
