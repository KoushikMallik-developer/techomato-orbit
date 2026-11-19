//! Process identity. A PID alone is not an identity (Windows reuses them), so a process is
//! `pid + creation time`.

use sysinfo::{ProcessesToUpdate, System};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcId {
    pub pid: u32,
    /// Creation time, milliseconds since the Unix epoch.
    pub created_at_ms: u64,
}

const STILL_ACTIVE: u32 = 259;
/// 100 ns intervals between 1601-01-01 and 1970-01-01.
const FILETIME_UNIX_OFFSET: u64 = 116_444_736_000_000_000;

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcess and is closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Identity of a *running* process, or `None` if it does not exist, has exited, or is not
/// readable by us.
pub fn identify(pid: u32) -> Option<ProcId> {
    // SAFETY: plain Win32 calls with valid out-pointers; the handle is closed by `Handle`.
    unsafe {
        let handle = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut code = 0u32;
        GetExitCodeProcess(handle.0, &mut code).ok()?;
        if code != STILL_ACTIVE {
            return None;
        }
        Some(ProcId {
            pid,
            created_at_ms: creation_ms(handle.0)?,
        })
    }
}

/// # Safety
/// `handle` must be a valid process handle with query rights.
unsafe fn creation_ms(handle: HANDLE) -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user).ok()?;
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    Some(ticks.saturating_sub(FILETIME_UNIX_OFFSET) / 10_000)
}

/// Identity of a child we spawned, read through its own handle. Unlike `identify`, this still
/// works if the child has already exited, so an instantly-exiting service still gets an identity.
pub fn identify_child(child: &std::process::Child) -> Option<ProcId> {
    use std::os::windows::io::AsRawHandle;
    // SAFETY: the Child owns a valid process handle for as long as it is borrowed here.
    let created_at_ms = unsafe { creation_ms(HANDLE(child.as_raw_handle())) }?;
    Some(ProcId {
        pid: child.id(),
        created_at_ms,
    })
}

/// True only if a process with this PID is running *and* was created at the recorded time.
pub fn is_alive(id: &ProcId) -> bool {
    identify(id.pid) == Some(*id)
}

/// Every process's direct children, from one snapshot (callers that look up many parents should
/// take this once instead of calling `children_of` per parent).
pub fn process_tree() -> std::collections::HashMap<u32, Vec<u32>> {
    let mut tree: std::collections::HashMap<u32, Vec<u32>> = Default::default();
    let Ok(mut sys) = System::new() else {
        return tree;
    };
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for p in sys.processes().values() {
        if let Some(parent) = p.parent() {
            tree.entry(parent.as_u32())
                .or_default()
                .push(p.pid().as_u32());
        }
    }
    tree
}

/// Direct children of `pid` (not the whole tree; Job Objects cover the tree).
pub fn children_of(pid: u32) -> Vec<u32> {
    process_tree().remove(&pid).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    #[test]
    fn the_current_process_is_identified_and_alive() {
        let id = identify(std::process::id()).expect("own process is visible");
        assert_eq!(id.pid, std::process::id());
        assert!(is_alive(&id));
    }

    #[test]
    fn a_reused_pid_with_a_different_creation_time_is_not_alive() {
        let me = identify(std::process::id()).unwrap();
        let impostor = ProcId {
            pid: me.pid,
            created_at_ms: 1,
        };
        assert!(!is_alive(&impostor));
    }

    #[test]
    fn a_reaped_child_is_not_alive() {
        let mut child = Command::new("cmd")
            .args(["/c", "exit", "0"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let id = identify(child.id());
        child.wait().unwrap();
        if let Some(id) = id {
            assert!(!is_alive(&id));
        }
    }

    #[test]
    fn a_child_that_already_exited_can_still_be_identified_from_its_handle() {
        let mut child = Command::new("cmd")
            .args(["/c", "exit", "0"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        child.wait().unwrap();
        let id = identify_child(&child).expect("handle outlives the process");
        assert_eq!(id.pid, child.id());
        assert!(id.created_at_ms > 0);
        assert!(!is_alive(&id));
    }

    #[test]
    fn a_nonexistent_pid_is_not_identified() {
        assert!(identify(0xFFFF_FFF0).is_none());
    }

    #[test]
    fn children_of_lists_a_spawned_grandchild() {
        let mut parent = Command::new("cmd")
            .args(["/c", "ping", "-n", "6", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut found = false;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if !children_of(parent.id()).is_empty() {
                found = true;
                break;
            }
        }
        let _ = parent.kill();
        let _ = parent.wait();
        assert!(found, "expected cmd to have a ping child");
    }
}
