//! One-click databases: list the engines, and create an instance (installing the engine first).
//! An instance is an ordinary service, so everything else in Orbit works on it unchanged.

use crate::db::Db;
use crate::engines::{catalog, install, instance};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::ServiceSpec;
use crate::secrets::SecretStore;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

/// Where engines and instances live: local (not roaming) app data, because they are large.
pub struct DataRoot(pub PathBuf);

/// One install or create per engine at a time. `cancel` is shared by all of them (the screen
/// sets one up at a time); it is cleared when a new one starts.
#[derive(Default)]
pub struct EngineBusy {
    ids: Mutex<HashSet<&'static str>>,
    cancel: Arc<AtomicBool>,
}

struct BusyGuard<'a>(&'a EngineBusy, &'static str);

impl<'a> BusyGuard<'a> {
    fn take(busy: &'a EngineBusy, id: &'static str) -> AppResult<Self> {
        if busy.ids.lock().unwrap().insert(id) {
            Ok(Self(busy, id))
        } else {
            Err(AppError::new(
                ErrorKind::Conflict,
                "This database is already being set up. Wait for it to finish.",
            ))
        }
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.ids.lock().unwrap().remove(self.1);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    bytes: u64,
    default_port: u16,
    installed: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    engine: &'a str,
    /// download, unpack or init
    phase: &'a str,
    done: u64,
    total: u64,
}

#[tauri::command]
pub async fn db_engines(root: State<'_, DataRoot>) -> AppResult<Vec<EngineInfo>> {
    Ok(catalog::ENGINES
        .iter()
        .map(|e| EngineInfo {
            id: e.id,
            name: e.name,
            version: e.version,
            bytes: e.bytes,
            default_port: e.default_port,
            installed: install::is_installed(&root.0, e),
        })
        .collect())
}

fn internal(e: impl ToString) -> AppError {
    AppError::new(ErrorKind::Internal, e.to_string())
}

/// Creates a database instance: installs the engine if needed, prepares an empty data folder,
/// picks a free port, and saves a service that runs it. Does not start it.
#[tauri::command]
pub async fn db_create(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    secrets: State<'_, Arc<dyn SecretStore>>,
    root: State<'_, DataRoot>,
    busy: State<'_, EngineBusy>,
    engine: String,
    name: String,
) -> AppResult<ServiceSpec> {
    let e = catalog::find(&engine)
        .ok_or_else(|| AppError::new(ErrorKind::Validation, "That database is not available."))?;
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::new(
            ErrorKind::Validation,
            "A name must be 1 to 60 characters.",
        ));
    }
    let _guard = BusyGuard::take(&busy, e.id)?;
    let cancel = busy.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let (db, secrets, root) = (db.inner().clone(), secrets.inner().clone(), root.0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let emit = |phase: &str, done: u64, total: u64| {
            let _ = app.emit(
                "engine://progress",
                Progress {
                    engine: e.id,
                    phase,
                    done,
                    total,
                },
            );
        };
        instance::create(&db, secrets.as_ref(), &root, e, &name, &emit, &cancel).map_err(|err| {
            // Not the flag: a late Cancel click leaves it set while a real failure happens.
            if err.message == install::CANCELLED {
                AppError::new(ErrorKind::Cancelled, "Cancelled.")
            } else {
                err
            }
        })
    })
    .await
    .map_err(internal)?
}

/// Stops a download in progress. Unpacking and setup, which take seconds, finish on their own.
#[tauri::command]
pub async fn db_cancel(busy: State<'_, EngineBusy>) -> AppResult<()> {
    busy.cancel.store(true, Ordering::Relaxed);
    Ok(())
}
