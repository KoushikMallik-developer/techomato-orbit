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
    current_rows(&db, &sup).await
}

/// Every listening endpoint, marked with the Orbit service that owns it (if any).
pub async fn current_rows(db: &Db, sup: &Supervisor) -> AppResult<Vec<PortRow>> {
    let specs = repo::list(db)?;
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
