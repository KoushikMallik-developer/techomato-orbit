use crate::commands::confirm_action;
use crate::db::Db;
use crate::engine::logs::{LogLine, Stream};
use crate::engine::supervisor::Supervisor;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::repo::services as repo;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn log_recent(
    sup: State<'_, Supervisor>,
    db: State<'_, Arc<Db>>,
    service_id: String,
    n: usize,
) -> AppResult<Vec<LogLine>> {
    // Only real services: the id becomes a file name under the logs folder.
    repo::get(&db, &service_id)?;
    Ok(sup.logs(&service_id).recent(n.min(5000)))
}

#[tauri::command]
pub async fn log_history(
    sup: State<'_, Supervisor>,
    db: State<'_, Arc<Db>>,
    service_id: String,
    offset: usize,
    limit: usize,
    filter: Option<String>,
) -> AppResult<Vec<LogLine>> {
    repo::get(&db, &service_id)?;
    let sink = sup.logs(&service_id);
    tauri::async_runtime::spawn_blocking(move || {
        sink.read_history(offset, limit.min(5000), filter.as_deref())
    })
    .await
    .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))
}

#[tauri::command]
pub async fn log_clear(
    app: AppHandle,
    sup: State<'_, Supervisor>,
    db: State<'_, Arc<Db>>,
    service_id: String,
) -> AppResult<()> {
    let name = repo::get(&db, &service_id)?.name;
    confirm_action(
        &app,
        "Clear logs",
        &format!("Delete all saved logs for \"{name}\"? This cannot be undone."),
        "Clear",
    )?;
    sup.logs(&service_id).clear();
    Ok(())
}

/// Asks where to save with the native dialog, then writes every retained line.
/// Returns false if the user cancelled the dialog.
#[tauri::command]
pub async fn log_export(
    app: AppHandle,
    sup: State<'_, Supervisor>,
    db: State<'_, Arc<Db>>,
    service_id: String,
) -> AppResult<bool> {
    let name = repo::get(&db, &service_id)?.name;
    let picked = app
        .dialog()
        .file()
        .set_file_name(format!("{name}.log"))
        .blocking_save_file();
    let Some(path) = picked else {
        return Ok(false);
    };
    let path = path
        .into_path()
        .map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))?;
    let sink = sup.logs(&service_id);
    tauri::async_runtime::spawn_blocking(move || {
        let text: String = sink
            .read_history(0, usize::MAX, None)
            .iter()
            .map(|l| {
                let tag = if l.stream == Stream::Stderr {
                    "ERR"
                } else {
                    "OUT"
                };
                format!("{} {tag} {}\n", format_ts(l.ts_ms), l.text)
            })
            .collect();
        std::fs::write(path, text).map_err(AppError::from)
    })
    .await
    .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))??;
    Ok(true)
}

/// `YYYY-MM-DD HH:MM:SS.mmm` in UTC (civil-from-days, no date crate needed).
fn format_ts(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let rem = ms % 86_400_000;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::format_ts;

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(format_ts(0), "1970-01-01 00:00:00.000");
        assert_eq!(format_ts(1_709_210_096_789), "2024-02-29 12:34:56.789");
    }
}
