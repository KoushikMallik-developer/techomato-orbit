//! Open an address in the user's default browser.

use crate::error::{AppError, AppResult, ErrorKind};
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

/// Only ever called with a fixed address from this program, never with user input.
pub fn open_url(url: &str) -> AppResult<()> {
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: 1, // SW_SHOWNORMAL
        ..Default::default()
    };
    // SAFETY: `info` and the wide strings it points at outlive the call.
    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|e| AppError::new(ErrorKind::Os, format!("Could not open the browser: {e}")))
}
