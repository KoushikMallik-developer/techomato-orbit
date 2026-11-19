//! Doctor commands: a scan that streams findings as events, and confirmed fixes.

use crate::commands::confirm_action;
use crate::doctor::{self, fixes, Finding, Fix};
use crate::error::{AppError, AppResult, ErrorKind};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, State};

/// One scan at a time.
#[derive(Default)]
pub struct DoctorBusy(AtomicBool);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScanDone {
    count: usize,
    millis: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    message: String,
    freed_bytes: u64,
}

fn internal(e: impl ToString) -> AppError {
    AppError::new(ErrorKind::Internal, e.to_string())
}

/// Emits `doctor://finding` for each finding as it is found, then `doctor://done`.
#[tauri::command]
pub async fn doctor_scan(app: AppHandle, busy: State<'_, DoctorBusy>) -> AppResult<()> {
    if busy.0.swap(true, Ordering::SeqCst) {
        return Err(AppError::new(
            ErrorKind::Conflict,
            "A scan is already running.",
        ));
    }
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&busy.0);
    let started = std::time::Instant::now();
    let count = std::sync::atomic::AtomicUsize::new(0);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        doctor::scan(&|f: Finding| {
            count.fetch_add(1, Ordering::Relaxed);
            let _ = handle.emit("doctor://finding", f);
        });
        let _ = handle.emit(
            "doctor://done",
            ScanDone {
                count: count.load(Ordering::Relaxed),
                millis: started.elapsed().as_millis() as u64,
            },
        );
    })
    .await
    .map_err(internal)
}

/// Asks first (natively, so the UI cannot skip it), then applies one fix.
#[tauri::command]
pub async fn doctor_fix(app: AppHandle, fix: Fix) -> AppResult<FixResult> {
    let (title, message) = fixes::describe(&fix);
    confirm_action(&app, &title, &message, "Fix")?;
    tauri::async_runtime::spawn_blocking(move || fixes::run(&fix))
        .await
        .map_err(internal)?
        .map(|o| FixResult {
            message: o.message,
            freed_bytes: o.freed_bytes,
        })
        .map_err(|m| AppError::new(ErrorKind::Os, m))
}
