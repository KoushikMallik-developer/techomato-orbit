//! Launches a UAC-elevated copy of this executable to perform one typed operation.

use crate::elevated::{encode, ElevatedOp, OpResult};
use crate::error::{AppError, AppResult, ErrorKind};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

const MAX_RESULT_BYTES: u64 = 4096;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// One UAC prompt, then the operation runs in the elevated copy. Cancelling the prompt returns
/// `ErrorKind::Cancelled`. The result file is created here (random name, our own temp dir), so
/// the elevated process never creates files at a path of its choosing.
pub fn run_elevated(op: &ElevatedOp) -> AppResult<OpResult> {
    let request = encode(op).map_err(|m| AppError::new(ErrorKind::Validation, m))?;
    let exe = std::env::current_exe().map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))?;
    let result_path =
        std::env::temp_dir().join(format!("orbit-elevated-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&result_path, b"").map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))?;
    let outcome = launch_and_wait(&exe, &request, &result_path);
    let read = std::fs::File::open(&result_path).and_then(|f| {
        use std::io::Read;
        let mut buf = Vec::new();
        f.take(MAX_RESULT_BYTES).read_to_end(&mut buf).map(|_| buf)
    });
    let _ = std::fs::remove_file(&result_path);
    outcome?;
    let bytes = read.map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|_| {
        AppError::new(
            ErrorKind::Os,
            "The elevated helper did not report a result.",
        )
    })
}

fn launch_and_wait(
    exe: &std::path::Path,
    request: &str,
    result: &std::path::Path,
) -> AppResult<()> {
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&format!(
        "--elevated-op {request} --result \"{}\"",
        result.display()
    ));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: 0, // SW_HIDE
        ..Default::default()
    };
    // SAFETY: `info` and the wide strings it points at outlive the call; the returned process
    // handle is waited on and closed exactly once.
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
                AppError::new(
                    ErrorKind::Cancelled,
                    "The administrator prompt was cancelled.",
                )
            } else {
                AppError::new(
                    ErrorKind::Os,
                    format!("Could not start the helper: {}", e.message()),
                )
            });
        }
        let process = info.hProcess;
        let waited = WaitForSingleObject(process, INFINITE);
        let _ = CloseHandle(process);
        if waited != WAIT_OBJECT_0 {
            return Err(AppError::new(
                ErrorKind::Os,
                "Waiting for the helper failed.",
            ));
        }
    }
    Ok(())
}
