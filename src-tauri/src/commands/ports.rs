use crate::db::Db;
use crate::engine::ports::{annotate, PortRow};
use crate::engine::preflight::port_in_use;
use crate::engine::supervisor::Supervisor;
use crate::error::AppResult;
use crate::repo::services as repo;
use crate::windows::{net::list_endpoints, procs::process_tree};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ports_list(
    db: State<'_, Arc<Db>>,
    sup: State<'_, Supervisor>,
) -> AppResult<Vec<PortRow>> {
    let specs = repo::list(&db)?;
    let runtimes = sup.all_runtime()?;
    // One process snapshot per refresh, off the async threads.
    tauri::async_runtime::spawn_blocking(move || {
        let endpoints = list_endpoints()?;
        let tree = process_tree();
        Ok(annotate(endpoints, &runtimes, &specs, &|p| {
            tree.get(&p).cloned().unwrap_or_default()
        }))
    })
    .await
    .map_err(|e| crate::error::AppError::new(crate::error::ErrorKind::Internal, e.to_string()))?
}

/// True if something is listening on `port` (used by the "is it free?" box and the watch list).
#[tauri::command]
pub async fn port_check(port: u16) -> AppResult<bool> {
    Ok(port_in_use(port))
}

/// Ends the process that owns a port, after a native confirm naming it. Refusals never reach the
/// confirm.
#[tauri::command]
pub async fn port_end_process(
    app: tauri::AppHandle,
    pid: u32,
    created_at_ms: u64,
    force: bool,
) -> AppResult<()> {
    use crate::engine::ports::{end_process, refusal};
    use crate::windows::procs::ProcId;
    let target = ProcId { pid, created_at_ms };
    // Name and port come from a fresh read, never from the caller.
    let row = list_endpoints()?
        .into_iter()
        .find(|e| e.pid == pid && e.created_at_ms == Some(created_at_ms));
    let name = row.as_ref().and_then(|r| r.process_name.clone());
    if let Some(reason) = refusal(target, std::process::id(), name.as_deref()) {
        return Err(crate::error::AppError::new(
            crate::error::ErrorKind::Validation,
            reason,
        ));
    }
    let what = match &row {
        Some(r) => format!(
            "{} (PID {pid}), which is using port {}",
            name.as_deref().unwrap_or("This process"),
            r.port
        ),
        None => format!("The process with PID {pid}"),
    };
    crate::commands::confirm_action(
        &app,
        "End process",
        &format!("End {what}? Unsaved work in it may be lost."),
        "End process",
    )?;
    tauri::async_runtime::spawn_blocking(move || end_process(target, force))
        .await
        .map_err(|e| crate::error::AppError::new(crate::error::ErrorKind::Os, e.to_string()))?
}
