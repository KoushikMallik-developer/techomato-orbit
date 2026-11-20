//! Windows services (Service Control Manager), only as far as a database that runs as one needs:
//! find the service that runs a command, read its state, and start or stop it. Reading needs no
//! rights; starting and stopping asks Windows for administrator rights (one UAC prompt each).

use crate::error::{AppError, AppResult, ErrorKind};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, ERROR_MORE_DATA, WAIT_OBJECT_0};
use windows::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
    QueryServiceStatusEx, ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW,
    SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_ENUMERATE_SERVICE, SC_STATUS_PROCESS_INFO,
    SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_STATE_ALL, SERVICE_STATUS_PROCESS,
    SERVICE_WIN32,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

const SERVICE_RUNNING: u32 = 4;
/// `sc.exe` exit codes that already mean "the service is in the state you asked for".
const ALREADY_RUNNING: u32 = 1056;
const NOT_STARTED: u32 = 1062;

/// One service as the manager lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Svc {
    pub name: String,
    /// The command Windows runs, as configured.
    pub command: String,
    /// The process hosting it while it runs.
    pub pid: Option<u32>,
}

struct Handle(SC_HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenSCManagerW/OpenServiceW and is closed once.
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn config_command(scm: &Handle, name: &str) -> Option<String> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and outlives the call; the buffer is sized from the length
    // Windows reports, in u64 units so the struct is aligned.
    unsafe {
        let svc = Handle(OpenServiceW(scm.0, PCWSTR(name.as_ptr()), SERVICE_QUERY_CONFIG).ok()?);
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(svc.0, None, 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let cfg = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        QueryServiceConfigW(svc.0, Some(cfg), (buf.len() * 8) as u32, &mut needed).ok()?;
        (*cfg).lpBinaryPathName.to_string().ok()
    }
}

/// Every service that runs in a process of its own or a shared one, running or not.
pub fn list() -> Vec<Svc> {
    // SAFETY: standard two-call enumeration; the buffer is u64-aligned and sized from the reply,
    // and every name pointer read from it stays valid while `buf` lives.
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)
        else {
            return vec![];
        };
        let scm = Handle(scm);
        let (mut needed, mut count, mut resume) = (0u32, 0u32, 0u32);
        let mut buf: Vec<u64> = vec![];
        let mut out = vec![];
        loop {
            let slice = (!buf.is_empty()).then(|| {
                std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), buf.len() * 8)
            });
            let r = EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                slice,
                &mut needed,
                &mut count,
                Some(&mut resume),
                PCWSTR::null(),
            );
            let more = match &r {
                Ok(()) => false,
                Err(e) if e.code() == ERROR_MORE_DATA.to_hresult() => true,
                Err(_) => break,
            };
            let items = buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
            for i in 0..count as usize {
                let item = &*items.add(i);
                let Ok(name) = item.lpServiceName.to_string() else {
                    continue;
                };
                let pid = item.ServiceStatusProcess.dwProcessId;
                if let Some(command) = config_command(&scm, &name) {
                    out.push(Svc {
                        name,
                        command,
                        pid: (pid != 0).then_some(pid),
                    });
                }
            }
            if !more {
                break;
            }
            if count == 0 {
                // Too small for even one entry: grow, and make sure we always do.
                let want = (needed as usize).max(buf.len() * 16);
                buf = vec![0u64; want.div_ceil(8)];
            }
        }
        out
    }
}

/// The name of the service that runs exactly this program and arguments, if one does.
pub fn find(exe: &str, args: &[String]) -> Option<String> {
    list()
        .into_iter()
        .find(|s| runs(s, exe, args))
        .map(|s| s.name)
}

/// The hosting process id while the service runs; `None` when it is not running (or unreadable).
pub fn running_pid(name: &str) -> Option<u32> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and outlives the call; the out buffer is a plain struct.
    unsafe {
        let scm = Handle(
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE).ok()?,
        );
        let svc = Handle(OpenServiceW(scm.0, PCWSTR(name.as_ptr()), SERVICE_QUERY_STATUS).ok()?);
        let mut st = SERVICE_STATUS_PROCESS::default();
        let mut needed = 0u32;
        let bytes = std::slice::from_raw_parts_mut(
            (&mut st as *mut SERVICE_STATUS_PROCESS).cast::<u8>(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
        );
        QueryServiceStatusEx(svc.0, SC_STATUS_PROCESS_INFO, Some(bytes), &mut needed).ok()?;
        (st.dwCurrentState.0 == SERVICE_RUNNING && st.dwProcessId != 0).then_some(st.dwProcessId)
    }
}

/// Splits a service's command into program and arguments, the way Windows reads it: double
/// quotes group, spaces separate.
pub fn split_command(command: &str) -> (String, Vec<String>) {
    let mut parts: Vec<String> = vec![];
    let (mut cur, mut quoted, mut any) = (String::new(), false, false);
    for c in command.trim().chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    parts.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        parts.push(cur);
    }
    let mut it = parts.into_iter();
    (it.next().unwrap_or_default(), it.collect())
}

/// True when `svc` runs exactly this program with exactly these arguments. Exact, so a service of
/// your own that merely uses the same program is never mistaken for the Windows service. A shared
/// host never matches: its command is the same for many services, so it names none of them.
pub fn runs(svc: &Svc, exe: &str, args: &[String]) -> bool {
    let (prog, a) = split_command(&svc.command);
    prog.eq_ignore_ascii_case(exe) && a == args && !is_shared_host(svc)
}

/// A host shared by many services (`svchost.exe`). A program that merely runs below one, such as
/// a scheduled task, is not any one of those services.
pub fn is_shared_host(svc: &Svc) -> bool {
    split_command(&svc.command)
        .0
        .to_ascii_lowercase()
        .ends_with("svchost.exe")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Start,
    Stop,
}

/// Starts or stops the service through `sc.exe`, run with administrator rights. The caller passes
/// a name that came from `list`; anything else is refused. Cancelling the prompt is
/// `ErrorKind::Cancelled`. Returns once `sc.exe` has accepted the request; the caller watches
/// `running_pid` for the result.
pub fn control(name: &str, what: Control) -> AppResult<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-$".contains(c))
    {
        return Err(AppError::new(
            ErrorKind::Validation,
            "Orbit will not control a Windows service with that name.",
        ));
    }
    // Asked from Windows, not read from an environment variable: this program runs elevated.
    let mut dir = [0u16; 260];
    // SAFETY: the buffer is valid for the length passed.
    let len = unsafe { GetSystemDirectoryW(Some(&mut dir)) } as usize;
    if len == 0 || len >= dir.len() {
        return Err(AppError::new(
            ErrorKind::Os,
            "Could not find the Windows system folder.",
        ));
    }
    let sc = format!(r"{}\sc.exe", String::from_utf16_lossy(&dir[..len]));
    let verb = wide("runas");
    let file = wide(&sc);
    let params = wide(&format!(
        "{} {name}",
        if what == Control::Start {
            "start"
        } else {
            "stop"
        }
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
    // SAFETY: `info` and the wide strings it points at outlive the call; the process handle is
    // waited on and closed exactly once.
    let code = unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
                AppError::new(
                    ErrorKind::Cancelled,
                    "The administrator prompt was cancelled.",
                )
            } else {
                AppError::new(
                    ErrorKind::Os,
                    format!(
                        "Could not ask Windows for administrator rights: {}",
                        e.message()
                    ),
                )
            });
        }
        let process = info.hProcess;
        let waited = WaitForSingleObject(process, INFINITE);
        let mut code = 0u32;
        let got = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        if waited != WAIT_OBJECT_0 || got.is_err() {
            return Err(AppError::new(ErrorKind::Os, "Waiting for sc.exe failed."));
        }
        code
    };
    match (code, what) {
        (0, _) | (ALREADY_RUNNING, Control::Start) | (NOT_STARTED, Control::Stop) => Ok(()),
        _ => Err(AppError::new(
            ErrorKind::Os,
            format!("Windows refused to change the service (sc.exe exit code {code})."),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(command: &str) -> Svc {
        Svc {
            name: "Redis".into(),
            command: command.into(),
            pid: None,
        }
    }

    #[test]
    fn a_service_command_splits_the_way_windows_reads_it() {
        let (exe, args) = split_command(
            r#""C:\Program Files\Redis\redis-server.exe" --service-run "C:\Program Files\Redis\redis.conf" --x"#,
        );
        assert_eq!(exe, r"C:\Program Files\Redis\redis-server.exe");
        assert_eq!(
            args,
            [
                r"--service-run",
                r"C:\Program Files\Redis\redis.conf",
                "--x"
            ]
        );
        assert_eq!(split_command("  "), (String::new(), vec![]));
        assert_eq!(split_command(r#"a "" b"#).1, ["", "b"]);
    }

    #[test]
    fn only_the_exact_command_counts_as_the_service() {
        let host = svc(r"C:\Windows\system32\svchost.exe -k netsvcs -p");
        let (exe, args) = split_command(&host.command);
        assert!(!runs(&host, &exe, &args));
        let s = svc(r#""C:\R\redis-server.exe" --service-run "C:\R\r.conf""#);
        let args = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(runs(
            &s,
            r"c:\r\REDIS-server.exe",
            &args(&["--service-run", r"C:\R\r.conf"])
        ));
        assert!(!runs(
            &s,
            r"C:\R\redis-server.exe",
            &args(&["--port", "6390"])
        ));
        assert!(!runs(
            &s,
            r"C:\R\other.exe",
            &args(&["--service-run", r"C:\R\r.conf"])
        ));
    }

    #[test]
    fn a_shared_service_host_is_recognised() {
        assert!(is_shared_host(&svc(
            r"C:\Windows\system32\svchost.exe -k netsvcs -p"
        )));
        assert!(is_shared_host(&svc(
            r#""C:\Windows\System32\SvcHost.exe" -k x"#
        )));
        assert!(!is_shared_host(&svc(
            r#""C:\R\redis-server.exe" --service-run"#
        )));
    }

    #[test]
    fn a_name_that_is_not_a_plain_service_name_is_refused_before_anything_runs() {
        for bad in ["", "a b", "x & calc", "a\"b", r"..\evil", "a/b"] {
            let e = control(bad, Control::Stop).unwrap_err();
            assert_eq!(e.kind, ErrorKind::Validation, "{bad}");
        }
    }

    #[test]
    fn this_pc_lists_services_and_reads_a_state() {
        let all = list();
        assert!(all.len() > 20, "{} services", all.len());
        // RpcSs always runs; its host process id must be readable without any rights.
        assert!(all.iter().any(|s| s.name == "RpcSs" && s.pid.is_some()));
        assert!(running_pid("RpcSs").is_some());
        assert!(running_pid("no-such-service-xyz").is_none());
    }
}
