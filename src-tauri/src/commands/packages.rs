//! Package manager commands (winget, Scoop, Chocolatey). Reads return parsed rows; changes stream
//! their output as `pm://output` events and return when the manager exits.

use crate::commands::confirm_action;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::pm::{self, run, Manager, Op, Package};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

const READ_TIMEOUT: Duration = Duration::from_secs(90);
const CHANGE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Managers with a change in flight: two installs through one manager would fight over its lock.
#[derive(Default)]
pub struct PmBusy(Mutex<HashSet<Manager>>);

struct BusyGuard<'a>(&'a PmBusy, Manager);

impl<'a> BusyGuard<'a> {
    fn take(busy: &'a PmBusy, m: Manager) -> AppResult<Self> {
        if busy.0.lock().unwrap().insert(m) {
            Ok(Self(busy, m))
        } else {
            Err(AppError::new(
                ErrorKind::Conflict,
                "Another change is already running for this package manager.",
            ))
        }
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0 .0.lock().unwrap().remove(&self.1);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerInfo {
    manager: Manager,
    available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmResult {
    ok: bool,
    code: i32,
    message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PmLine<'a> {
    op_id: &'a str,
    manager: Manager,
    line: &'a str,
}

fn internal(e: impl ToString) -> AppError {
    AppError::new(ErrorKind::Internal, e.to_string())
}

fn name(m: Manager) -> &'static str {
    match m {
        Manager::Winget => "winget",
        Manager::Scoop => "Scoop",
        Manager::Choco => "Chocolatey",
    }
}

fn launcher(m: Manager) -> AppResult<run::Launcher> {
    run::locate(m).ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            format!("{} is not installed on this PC.", name(m)),
        )
    })
}

fn run_error(m: Manager, e: run::RunError, timeout: Duration) -> AppError {
    match e {
        run::RunError::TimedOut => AppError::new(
            ErrorKind::Os,
            format!(
                "{} did not finish within {} minutes and was stopped.",
                name(m),
                (timeout.as_secs() / 60).max(1)
            ),
        ),
        run::RunError::Spawn(d) => {
            AppError::new(ErrorKind::Os, format!("Could not start {}.", name(m))).with_detail(d)
        }
    }
}

async fn read(m: Manager, op: Op) -> AppResult<Vec<Package>> {
    let args = pm::args::build(m, &op).map_err(|e| AppError::new(ErrorKind::Validation, e))?;
    let l = launcher(m)?;
    tauri::async_runtime::spawn_blocking(move || {
        let out =
            run::run(&l, &args, READ_TIMEOUT, |_| {}).map_err(|e| run_error(m, e, READ_TIMEOUT))?;
        pm::interpret(m, &op, &out).map_err(|e| AppError::new(ErrorKind::Os, e))
    })
    .await
    .map_err(internal)?
}

#[tauri::command]
pub async fn pm_managers() -> Vec<ManagerInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        [Manager::Winget, Manager::Choco, Manager::Scoop]
            .into_iter()
            .map(|manager| ManagerInfo {
                manager,
                available: run::locate(manager).is_some(),
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn pm_installed(manager: Manager) -> AppResult<Vec<Package>> {
    read(manager, Op::Installed).await
}

#[tauri::command]
pub async fn pm_outdated(manager: Manager) -> AppResult<Vec<Package>> {
    read(manager, Op::Outdated).await
}

#[tauri::command]
pub async fn pm_search(manager: Manager, query: String) -> AppResult<Vec<Package>> {
    read(manager, Op::Search(query)).await
}

fn valid_op_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Hint for the common Chocolatey failure: it needs an elevated process and Orbit is not one.
fn failure_message(out: &run::Output) -> String {
    let all = format!("{}\n{}", out.stdout, out.stderr).to_lowercase();
    if all.contains("administrator")
        || all.contains("access to the path")
        || all.contains("access is denied")
    {
        return "This needs administrator rights. Start Orbit as administrator and try again."
            .to_string();
    }
    pm::last_line(&format!("{}\n{}", out.stdout, out.stderr))
        .unwrap_or_else(|| format!("Exited with code {}.", out.code))
}

async fn change(
    app: AppHandle,
    busy: &PmBusy,
    manager: Manager,
    op: Op,
    op_id: String,
) -> AppResult<PmResult> {
    if !valid_op_id(&op_id) {
        return Err(AppError::new(
            ErrorKind::Validation,
            "Invalid operation id.",
        ));
    }
    let args =
        pm::args::build(manager, &op).map_err(|e| AppError::new(ErrorKind::Validation, e))?;
    let l = launcher(manager)?;
    let _guard = BusyGuard::take(busy, manager)?;
    tauri::async_runtime::spawn_blocking(move || {
        let out = run::run(&l, &args, CHANGE_TIMEOUT, |line| {
            let _ = app.emit(
                "pm://output",
                PmLine {
                    op_id: &op_id,
                    manager,
                    line,
                },
            );
        })
        .map_err(|e| run_error(manager, e, CHANGE_TIMEOUT))?;
        let ok = out.code == 0;
        Ok(PmResult {
            ok,
            code: out.code,
            message: if ok {
                String::new()
            } else {
                failure_message(&out)
            },
        })
    })
    .await
    .map_err(internal)?
}

#[tauri::command]
pub async fn pm_install(
    app: AppHandle,
    busy: State<'_, PmBusy>,
    manager: Manager,
    id: String,
    op_id: String,
) -> AppResult<PmResult> {
    change(app, &busy, manager, Op::Install(id), op_id).await
}

#[tauri::command]
pub async fn pm_upgrade(
    app: AppHandle,
    busy: State<'_, PmBusy>,
    manager: Manager,
    id: String,
    op_id: String,
) -> AppResult<PmResult> {
    change(app, &busy, manager, Op::Upgrade(id), op_id).await
}

#[tauri::command]
pub async fn pm_upgrade_all(
    app: AppHandle,
    busy: State<'_, PmBusy>,
    manager: Manager,
    op_id: String,
) -> AppResult<PmResult> {
    confirm_action(
        &app,
        "Upgrade all packages",
        &format!("Upgrade every outdated package through {}?", name(manager)),
        "Upgrade all",
    )?;
    change(app, &busy, manager, Op::UpgradeAll, op_id).await
}

#[tauri::command]
pub async fn pm_uninstall(
    app: AppHandle,
    busy: State<'_, PmBusy>,
    manager: Manager,
    id: String,
    op_id: String,
) -> AppResult<PmResult> {
    // Validate before asking, so the dialog never shows text that will be refused.
    crate::pm::args::check_id(&id).map_err(|e| AppError::new(ErrorKind::Validation, e))?;
    confirm_action(
        &app,
        "Uninstall package",
        &format!("Uninstall {id} with {}?", name(manager)),
        "Uninstall",
    )?;
    change(app, &busy, manager, Op::Uninstall(id), op_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_change_on_the_same_manager_is_refused_until_the_first_ends() {
        let busy = PmBusy::default();
        let first = BusyGuard::take(&busy, Manager::Winget).unwrap();
        assert!(BusyGuard::take(&busy, Manager::Winget).is_err());
        assert!(BusyGuard::take(&busy, Manager::Choco).is_ok());
        drop(first);
        assert!(BusyGuard::take(&busy, Manager::Winget).is_ok());
    }

    #[test]
    fn operation_ids_are_short_and_plain() {
        assert!(valid_op_id("a1-B2"));
        assert!(!valid_op_id(""));
        assert!(!valid_op_id("a b"));
        assert!(!valid_op_id(&"a".repeat(65)));
    }

    #[test]
    fn an_access_denied_failure_says_to_run_as_administrator() {
        let o = run::Output {
            code: 1,
            stdout: "Chocolatey detected you are not running from an elevated command shell".into(),
            stderr: "Access to the path is denied".into(),
        };
        assert!(failure_message(&o).contains("administrator"));
        let plain = run::Output {
            code: 1,
            stdout: "x\r\nPackage not found\r\n".into(),
            stderr: String::new(),
        };
        assert_eq!(failure_message(&plain), "Package not found");
    }

    // Real managers, read-only. Run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs winget"]
    fn real_winget_lists_installed_packages() {
        let l = run::locate(Manager::Winget).unwrap();
        let args = pm::args::build(Manager::Winget, &Op::Installed).unwrap();
        let out = run::run(&l, &args, READ_TIMEOUT, |_| {}).unwrap();
        let rows = pm::interpret(Manager::Winget, &Op::Installed, &out).unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|p| !p.id.is_empty()));
    }

    #[test]
    #[ignore = "needs Chocolatey"]
    fn real_choco_lists_installed_packages() {
        let l = run::locate(Manager::Choco).unwrap();
        let args = pm::args::build(Manager::Choco, &Op::Installed).unwrap();
        let out = run::run(&l, &args, READ_TIMEOUT, |_| {}).unwrap();
        assert!(!pm::interpret(Manager::Choco, &Op::Installed, &out)
            .unwrap()
            .is_empty());
    }
}
