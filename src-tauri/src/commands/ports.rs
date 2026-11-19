use crate::db::Db;
use crate::engine::ports::{annotate, PortRow};
use crate::engine::preflight::port_in_use;
use crate::engine::supervisor::Supervisor;
use crate::error::AppResult;
use crate::repo::services as repo;
use crate::windows::{net::list_endpoints, procs::children_of};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ports_list(
    db: State<'_, Arc<Db>>,
    sup: State<'_, Supervisor>,
) -> AppResult<Vec<PortRow>> {
    let endpoints = list_endpoints()?;
    let specs = repo::list(&db)?;
    let runtimes = sup.all_runtime()?;
    Ok(annotate(endpoints, &runtimes, &specs, &children_of))
}

/// True if something is listening on `port` (used by the "is it free?" box and the watch list).
#[tauri::command]
pub async fn port_check(port: u16) -> AppResult<bool> {
    Ok(port_in_use(port))
}
