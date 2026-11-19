//! Only one Orbit may run per Windows session: a second one would supervise the same services.
//! Held as a named mutex for the life of the process.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
};

/// Name used by the real app. `Local\` scopes it to the user's session.
pub const APP_MUTEX: &str = "Local\\TechomatoOrbit.SingleInstance";

pub struct Guard(HANDLE);

// SAFETY: a mutex handle is a kernel object reference, usable from any thread.
unsafe impl Send for Guard {}
unsafe impl Sync for Guard {}

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: closed exactly once; closing releases the name for the next instance.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `Some(guard)` if this is the first holder of `name`; `None` if another process (or guard)
/// already has it.
pub fn acquire(name: &str) -> Option<Guard> {
    let name = wide(name);
    // SAFETY: `name` is a valid NUL-terminated wide string for the duration of the call.
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }.ok()?;
    // SAFETY: read immediately after the call that sets it, on the same thread.
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        // SAFETY: we own this duplicate handle.
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    Some(Guard(handle))
}

/// Brings the already-running instance's window forward (it may be hidden in the tray).
pub fn focus_existing() {
    let title = wide("Techomato Orbit");
    // SAFETY: plain Win32 calls on a window handle we just looked up.
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_of_the_same_name_is_refused_until_the_first_lets_go() {
        let name = format!("Local\\TechomatoOrbit.Test.{}", std::process::id());
        let first = acquire(&name).expect("the first holder gets it");
        assert!(acquire(&name).is_none(), "the second must be refused");
        assert!(
            acquire(&format!("{name}.other")).is_some(),
            "other names are independent"
        );
        drop(first);
        assert!(acquire(&name).is_some(), "released on drop");
    }
}
