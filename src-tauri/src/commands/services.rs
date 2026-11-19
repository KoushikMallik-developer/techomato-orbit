use crate::commands::confirm_action;
use crate::db::Db;
use crate::engine::supervisor::{ServiceRuntime, Supervisor};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::ServiceSpec;
use crate::repo::services as repo;
use crate::secrets::SecretStore;
use std::sync::Arc;
use tauri::{AppHandle, State};

type Secrets<'a> = State<'a, Arc<dyn SecretStore>>;

#[tauri::command]
pub async fn service_list(db: State<'_, Arc<Db>>) -> AppResult<Vec<ServiceSpec>> {
    repo::list(&db)
}

#[tauri::command]
pub async fn service_get(db: State<'_, Arc<Db>>, id: String) -> AppResult<ServiceSpec> {
    repo::get(&db, &id)
}

#[tauri::command]
pub async fn service_create(
    db: State<'_, Arc<Db>>,
    secrets: Secrets<'_>,
    spec: ServiceSpec,
) -> AppResult<ServiceSpec> {
    repo::create(&db, secrets.inner().as_ref(), &spec)
}

#[tauri::command]
pub async fn service_update(
    db: State<'_, Arc<Db>>,
    secrets: Secrets<'_>,
    spec: ServiceSpec,
) -> AppResult<ServiceSpec> {
    repo::update(&db, secrets.inner().as_ref(), &spec)
}

/// Only a service with no live process can be deleted (a Failed one has none).
pub fn can_delete(status: crate::engine::state::Status) -> bool {
    use crate::engine::state::Status::*;
    matches!(status, Stopped | Failed | Unknown)
}

#[tauri::command]
pub async fn service_delete(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    secrets: Secrets<'_>,
    sup: State<'_, Supervisor>,
    id: String,
) -> AppResult<()> {
    let spec = repo::get(&db, &id)?;
    if !can_delete(sup.runtime(&id).status) {
        return Err(AppError::new(
            ErrorKind::Conflict,
            "Stop this service before deleting it.",
        ));
    }
    confirm_action(
        &app,
        "Delete service",
        &format!(
            "Delete \"{}\"? Its configuration and saved secrets are removed. Files on disk are not touched.",
            spec.name
        ),
        "Delete",
    )?;
    repo::delete(&db, secrets.inner().as_ref(), &id)
}

/// Lifecycle calls block (stop waits for the process), so they run off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?
}

#[tauri::command]
pub async fn service_start(sup: State<'_, Supervisor>, id: String) -> AppResult<()> {
    let sup = sup.inner().clone();
    blocking(move || sup.start(&id)).await
}

#[tauri::command]
pub async fn service_stop(sup: State<'_, Supervisor>, id: String) -> AppResult<()> {
    let sup = sup.inner().clone();
    blocking(move || sup.stop(&id, false)).await
}

#[tauri::command]
pub async fn service_force_stop(
    app: AppHandle,
    sup: State<'_, Supervisor>,
    db: State<'_, Arc<Db>>,
    id: String,
) -> AppResult<()> {
    let name = repo::get(&db, &id)?.name;
    confirm_action(
        &app,
        "Force stop",
        &format!("Force stop \"{name}\"? It is ended immediately and may lose unsaved work."),
        "Force stop",
    )?;
    let sup = sup.inner().clone();
    blocking(move || sup.stop(&id, true)).await
}

#[tauri::command]
pub async fn service_restart(sup: State<'_, Supervisor>, id: String) -> AppResult<()> {
    let sup = sup.inner().clone();
    blocking(move || sup.restart(&id)).await
}

#[tauri::command]
pub async fn service_runtime_list(sup: State<'_, Supervisor>) -> AppResult<Vec<ServiceRuntime>> {
    sup.all_runtime()
}

/// Whether a secret env value is stored, so the UI can show "set" without ever receiving it.
#[tauri::command]
pub async fn service_secret_is_set(
    secrets: Secrets<'_>,
    id: String,
    key: String,
) -> AppResult<bool> {
    Ok(secrets.get(&id, &key)?.is_some())
}

#[cfg(test)]
mod tests {
    use super::can_delete;
    use crate::engine::state::Status::*;

    #[test]
    fn only_a_service_with_no_live_process_can_be_deleted() {
        for s in [Stopped, Failed, Unknown] {
            assert!(can_delete(s), "{s:?}");
        }
        for s in [Starting, Running, Stopping, Unresponsive] {
            assert!(!can_delete(s), "{s:?}");
        }
    }
}
