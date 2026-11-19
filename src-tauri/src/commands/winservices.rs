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

/// Confirms (naming the UAC prompt that follows), then runs one elevated operation on a Windows
/// service. Protected and unknown services are refused before anything is shown.
async fn control(
    app: tauri::AppHandle,
    name: String,
    verb: &'static str,
    make: fn(String) -> crate::elevated::ElevatedOp,
) -> AppResult<()> {
    use crate::error::{AppError, ErrorKind};
    let lookup = name.clone();
    let found = tauri::async_runtime::spawn_blocking(move || scm::find(&lookup))
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))??;
    let svc = found.ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            format!("There is no service named {name}."),
        )
    })?;
    if svc.protected {
        return Err(AppError::new(
            ErrorKind::Validation,
            format!(
                "{} is part of Windows and is not managed here.",
                svc.display_name
            ),
        ));
    }
    crate::commands::confirm_action(
        &app,
        &format!("{verb} Windows service"),
        &format!(
            "{verb} {}? Windows will ask for administrator permission next.",
            svc.display_name
        ),
        verb,
    )?;
    let op = make(svc.name);
    let result =
        tauri::async_runtime::spawn_blocking(move || crate::windows::elevate::run_elevated(&op))
            .await
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))??;
    if result.ok {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::Os, result.message))
    }
}

#[tauri::command]
pub async fn winservice_start(app: tauri::AppHandle, name: String) -> AppResult<()> {
    control(app, name, "Start", |name| {
        crate::elevated::ElevatedOp::ServiceStart { name }
    })
    .await
}

#[tauri::command]
pub async fn winservice_stop(app: tauri::AppHandle, name: String) -> AppResult<()> {
    control(app, name, "Stop", |name| {
        crate::elevated::ElevatedOp::ServiceStop { name }
    })
    .await
}

#[tauri::command]
pub async fn winservice_restart(app: tauri::AppHandle, name: String) -> AppResult<()> {
    control(app, name, "Restart", |name| {
        crate::elevated::ElevatedOp::ServiceRestart { name }
    })
    .await
}
