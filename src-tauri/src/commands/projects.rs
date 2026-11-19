use crate::db::Db;
use crate::engine::projects::{self, Progress, ProjectRunReport};
use crate::engine::supervisor::Supervisor;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::project::{Dep, Edge, Project};
use crate::repo::projects as repo;
use crate::secrets::SecretStore;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type Secrets<'a> = State<'a, Arc<dyn SecretStore>>;

#[tauri::command]
pub async fn project_list(db: State<'_, Arc<Db>>) -> AppResult<Vec<Project>> {
    repo::list(&db)
}

#[tauri::command]
pub async fn project_create(db: State<'_, Arc<Db>>, project: Project) -> AppResult<Project> {
    repo::create(&db, &project)
}

#[tauri::command]
pub async fn project_update(db: State<'_, Arc<Db>>, project: Project) -> AppResult<Project> {
    repo::update(&db, &project)
}

#[tauri::command]
pub async fn project_archive(db: State<'_, Arc<Db>>, id: String, archived: bool) -> AppResult<()> {
    repo::archive(&db, &id, archived)
}

/// Removes the project only; its services stay (without a project).
#[tauri::command]
pub async fn project_delete(app: AppHandle, db: State<'_, Arc<Db>>, id: String) -> AppResult<()> {
    let name = repo::get(&db, &id)?.name;
    crate::commands::confirm_action(
        &app,
        "Delete project",
        &format!("Delete the project \"{name}\"? Its services are kept, just no longer grouped."),
        "Delete project",
    )?;
    repo::delete(&db, &id)
}

#[tauri::command]
pub async fn project_duplicate(
    db: State<'_, Arc<Db>>,
    secrets: Secrets<'_>,
    id: String,
) -> AppResult<Project> {
    repo::duplicate(&db, secrets.inner().as_ref(), &id)
}

#[tauri::command]
pub async fn dependency_edges(db: State<'_, Arc<Db>>) -> AppResult<Vec<Edge>> {
    repo::all_edges(&db)
}

#[tauri::command]
pub async fn dependency_set(
    db: State<'_, Arc<Db>>,
    service_id: String,
    deps: Vec<Dep>,
) -> AppResult<()> {
    repo::set_dependencies(&db, &service_id, deps)
}

/// Runs a blocking orchestration on a worker thread, emitting `project://progress` as it goes.
async fn run<F>(app: AppHandle, f: F) -> AppResult<ProjectRunReport>
where
    F: FnOnce(&(dyn Fn(&Progress) + Sync)) -> AppResult<ProjectRunReport> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        f(&|p: &Progress| {
            let _ = app.emit("project://progress", p);
        })
    })
    .await
    .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?
}

#[tauri::command]
pub async fn project_start(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    sup: State<'_, Supervisor>,
    id: String,
    selection: Option<Vec<String>>,
) -> AppResult<ProjectRunReport> {
    let (db, sup) = (db.inner().clone(), sup.inner().clone());
    run(app, move |emit| {
        projects::start_project(&sup, &db, &id, selection, emit)
    })
    .await
}

#[tauri::command]
pub async fn project_stop(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    sup: State<'_, Supervisor>,
    id: String,
) -> AppResult<ProjectRunReport> {
    let (db, sup) = (db.inner().clone(), sup.inner().clone());
    run(app, move |emit| {
        projects::stop_project(&sup, &db, &id, false, emit)
    })
    .await
}

#[tauri::command]
pub async fn project_restart(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    sup: State<'_, Supervisor>,
    id: String,
) -> AppResult<ProjectRunReport> {
    let (db, sup) = (db.inner().clone(), sup.inner().clone());
    run(app, move |emit| {
        projects::restart_project(&sup, &db, &id, emit)
    })
    .await
}
