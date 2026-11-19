//! Tauri command handlers, one file per domain.

pub mod services;

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
