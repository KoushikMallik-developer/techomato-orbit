//! Tauri command handlers, one file per domain.

pub mod logs;
pub mod notifications;
pub mod ports;
pub mod projects;
pub mod services;
pub mod winservices;

use crate::error::{AppError, AppResult, ErrorKind};
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Native confirm shown from Rust, so the frontend cannot skip it. Declining returns a
/// `Cancelled` error, which the UI does not toast.
/// Must be called from an async command (a sync one runs on the main thread and would deadlock).
pub fn confirm_action(
    app: &AppHandle,
    title: &str,
    message: &str,
    ok_label: &str,
) -> AppResult<()> {
    let accepted = app
        .dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            ok_label.into(),
            "Cancel".into(),
        ))
        .blocking_show();
    if accepted {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::Cancelled, "Cancelled."))
    }
}

/// Native file or folder picker; `None` if the user cancelled.
#[tauri::command]
pub async fn pick_path(app: AppHandle, folder: bool) -> AppResult<Option<String>> {
    let dialog = app.dialog().file();
    let picked = if folder {
        dialog.blocking_pick_folder()
    } else {
        dialog.blocking_pick_file()
    };
    picked
        .map(|p| {
            p.into_path()
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))
        })
        .transpose()
}
