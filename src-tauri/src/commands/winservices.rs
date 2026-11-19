use crate::db::Db;
use crate::error::AppResult;
use crate::repo::settings;
use crate::windows::scm::{self, WinService, DEV_PATTERNS};
use std::sync::Arc;
use tauri::State;

pub const PINS_KEY: &str = "winservices.pins";

/// Services the user pinned as "mine", stored as a JSON array of service names.
fn pins(db: &crate::db::Db) -> Vec<String> {
    settings::get(db, PINS_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// `all = false` returns only development-related services (patterns plus pins).
#[tauri::command]
pub async fn winservice_list(db: State<'_, Arc<Db>>, all: bool) -> AppResult<Vec<WinService>> {
    let pins = pins(&db);
    let mut list = tauri::async_runtime::spawn_blocking(scm::list)
        .await
        .map_err(|e| {
            crate::error::AppError::new(crate::error::ErrorKind::Internal, e.to_string())
        })??;
    for s in &mut list {
        s.developer = scm::is_developer(&s.name, &s.display_name, DEV_PATTERNS, &pins);
    }
    if !all {
        list.retain(|s| s.developer);
    }
    Ok(list)
}

#[tauri::command]
pub async fn winservice_pins(db: State<'_, Arc<Db>>) -> AppResult<Vec<String>> {
    Ok(pins(&db))
}

/// Replaces the pinned names.
#[tauri::command]
pub async fn winservice_set_pins(db: State<'_, Arc<Db>>, names: Vec<String>) -> AppResult<()> {
    let json = serde_json::to_string(&names).expect("names serialize");
    settings::set(&db, PINS_KEY, &json)
}
