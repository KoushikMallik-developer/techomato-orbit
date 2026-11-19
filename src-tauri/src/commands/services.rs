use crate::commands::confirm_action;
use crate::db::Db;
use crate::error::AppResult;
use crate::models::service::ServiceSpec;
use crate::repo::services as repo;
use crate::secrets::SecretStore;
use std::sync::Arc;
use tauri::{AppHandle, State};

type Secrets<'a> = State<'a, Arc<dyn SecretStore>>;

#[tauri::command]
pub async fn service_list(db: State<'_, Db>) -> AppResult<Vec<ServiceSpec>> {
    repo::list(&db)
}

#[tauri::command]
pub async fn service_get(db: State<'_, Db>, id: String) -> AppResult<ServiceSpec> {
    repo::get(&db, &id)
}

#[tauri::command]
pub async fn service_create(
    db: State<'_, Db>,
    secrets: Secrets<'_>,
    spec: ServiceSpec,
) -> AppResult<ServiceSpec> {
    repo::create(&db, secrets.inner().as_ref(), &spec)
}

#[tauri::command]
pub async fn service_update(
    db: State<'_, Db>,
    secrets: Secrets<'_>,
    spec: ServiceSpec,
) -> AppResult<ServiceSpec> {
    repo::update(&db, secrets.inner().as_ref(), &spec)
}

#[tauri::command]
pub async fn service_delete(
    app: AppHandle,
    db: State<'_, Db>,
    secrets: Secrets<'_>,
    id: String,
) -> AppResult<()> {
    let spec = repo::get(&db, &id)?;
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
