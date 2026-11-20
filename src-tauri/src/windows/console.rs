//! Graceful stop of a console child from a GUI process.
//!
//! Orbit has no console of its own, so it cannot call `GenerateConsoleCtrlEvent` directly.
//! Instead it launches a short-lived helper (a second mode of the Orbit binary:
//! `--stop-helper <pid>`) that attaches to the child's console, ignores the signal itself, and
//! sends Ctrl+Break to the child's process group. GUI children (no console) are asked to close
//! their top-level windows instead.

use crate::error::{AppError, AppResult, ErrorKind};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler, CTRL_BREAK_EVENT,
};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
};

/// Asks the process to stop, using `helper` (the Orbit binary; tests use the test service, which
/// also implements `--stop-helper`).
pub fn send_graceful_with(helper: &Path, pid: u32) -> AppResult<()> {
    let status = Command::new(helper)
        .args(["--stop-helper", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW.0)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorKind::Os,
            "Could not deliver the stop signal to the service.",
        )
        .with_detail(format!("helper exit code {:?}", status.code())))
    }
}

unsafe extern "system" fn swallow(_ctrl: u32) -> BOOL {
    BOOL(1)
}

struct CloseRequest {
    pid: u32,
    posted: bool,
}

unsafe extern "system" fn close_if_ours(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let req = &mut *(lparam.0 as *mut CloseRequest);
    let mut owner = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut owner));
    if owner == req.pid && PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() {
        req.posted = true;
    }
    BOOL(1)
}

/// Body of `--stop-helper <pid>`. Returns the process exit code: 0 delivered, 1 failed to send,
/// 2 nothing to signal (no console and no windows).
pub fn stop_helper(pid: u32) -> i32 {
    // SAFETY: Win32 console and window calls with valid arguments; this process exists only to
    // make them and then exits.
    unsafe {
        let _ = FreeConsole();
        if AttachConsole(pid).is_ok() {
            // Never let the signal we are about to send stop us.
            let _ = SetConsoleCtrlHandler(None, true);
            let _ = SetConsoleCtrlHandler(Some(swallow), true);
            let sent = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid).is_ok();
            std::thread::sleep(Duration::from_millis(150));
            let _ = FreeConsole();
            return if sent { 0 } else { 1 };
        }
        let mut req = CloseRequest { pid, posted: false };
        let _ = EnumWindows(Some(close_if_ours), LPARAM(&mut req as *mut _ as isize));
        if req.posted {
            0
        } else {
            2
        }
    }
}
