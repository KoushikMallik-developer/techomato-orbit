//! Windows services (Service Control Manager): read-only listing and the "protected" rule.
//! Nothing here opens a service with more than `SERVICE_QUERY_CONFIG`.

use crate::error::{AppError, AppResult, ErrorKind};
use serde::Serialize;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_MORE_DATA;
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
    QueryServiceConfig2W, QueryServiceConfigW, QueryServiceStatus, StartServiceW,
    ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW, SC_ENUM_PROCESS_INFO, SC_HANDLE,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONTROL_STOP,
    SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STATE_ALL, SERVICE_STATUS,
    SERVICE_STOP, SERVICE_WIN32,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartType {
    Auto,
    AutoDelayed,
    Manual,
    Disabled,
    Boot,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WinService {
    pub name: String,
    pub display_name: String,
    pub state: ServiceState,
    pub start_type: StartType,
    pub service_type: String,
    pub binary_path: Option<String>,
    pub pid: Option<u32>,
    /// Part of Windows itself: Orbit offers no way to change these.
    pub protected: bool,
    /// Looks like development tooling (databases, Docker, web servers...) or is pinned.
    pub developer: bool,
}

/// Names that are always protected, whatever their image path.
pub const CORE_SERVICES: &[&str] = &[
    "RpcSs",
    "DcomLaunch",
    "LSM",
    "SamSs",
    "EventLog",
    "Winmgmt",
    "BFE",
    "mpssvc",
    "CryptSvc",
    "Dhcp",
    "Dnscache",
    "PlugPlay",
    "Power",
    "ProfSvc",
    "SENS",
    "Schedule",
    "TrustedInstaller",
    "WinDefend",
];

/// Development-tool service names. A trailing `*` matches any suffix; everything else is exact.
pub const DEV_PATTERNS: &[&str] = &[
    "postgres*",
    "mysql*",
    "mariadb*",
    "mongo*",
    "redis*",
    "memurai*",
    "mssql*",
    "sqlserver*",
    "docker*",
    "com.docker*",
    "wsl*",
    "vmcompute",
    "hns",
    "rabbitmq*",
    "elastic*",
    "kafka*",
    "nginx*",
    "apache*",
    "httpd*",
    "iis*",
    "w3svc",
    "was",
    "mosquitto*",
    "neo4j*",
    "couchdb*",
    "minio*",
    "consul*",
    "vault*",
];

/// The executable part of an image path: the quoted part, or everything up to the first
/// `.exe`/`.sys`/`.dll` (service paths often have unquoted spaces), else the first word.
fn image_of(path: &str) -> String {
    let p = path.trim();
    if let Some(rest) = p.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest).to_string();
    }
    let lower = p.to_ascii_lowercase();
    let end = [".exe", ".sys", ".dll"]
        .iter()
        .filter_map(|ext| lower.find(ext).map(|i| i + ext.len()))
        .min();
    match end {
        Some(end) => p[..end].to_string(),
        None => p.split_whitespace().next().unwrap_or("").to_string(),
    }
}

/// Lower-case, backslash-separated, with `\??\`, `%SystemRoot%` and `\SystemRoot` expanded.
fn normalise(image: &str, system_root: &str) -> String {
    let root = system_root
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    let mut s = image.replace('/', "\\").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("\\??\\") {
        s = rest.to_string();
    }
    for token in ["%systemroot%", "%windir%", "\\systemroot"] {
        if let Some(rest) = s.strip_prefix(token) {
            s = format!("{root}{rest}");
            break;
        }
    }
    s
}

/// `service_type` is the raw `dwServiceType`; `system_root` is `%SystemRoot%` (`C:\Windows`).
pub fn is_protected(
    service_type: u32,
    binary_path: Option<&str>,
    name: &str,
    system_root: &str,
) -> bool {
    const DRIVER_MASK: u32 = 0x1 | 0x2 | 0x4 | 0x8;
    if service_type & DRIVER_MASK != 0 {
        return true;
    }
    if CORE_SERVICES.iter().any(|c| c.eq_ignore_ascii_case(name)) {
        return true;
    }
    let Some(path) = binary_path else {
        return false;
    };
    let image = normalise(&image_of(path), system_root);
    let root = system_root
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    image.starts_with(&format!("{root}\\")) || image.starts_with("system32\\")
}

fn pattern_matches(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.to_lowercase(), text.to_lowercase());
    match pattern.strip_suffix('*') {
        Some(prefix) => text.starts_with(prefix),
        None => text == pattern,
    }
}

pub fn is_developer(name: &str, display: &str, patterns: &[&str], pins: &[String]) -> bool {
    pins.iter().any(|p| p.eq_ignore_ascii_case(name))
        || patterns
            .iter()
            .any(|p| pattern_matches(p, name) || pattern_matches(p, display))
}

fn state_from(raw: u32) -> ServiceState {
    match raw {
        2 => ServiceState::StartPending,
        3 => ServiceState::StopPending,
        4 => ServiceState::Running,
        5 => ServiceState::ContinuePending,
        6 => ServiceState::PausePending,
        7 => ServiceState::Paused,
        _ => ServiceState::Stopped,
    }
}

fn type_label(raw: u32) -> String {
    let base = if raw & 0x1 != 0 {
        "Kernel driver"
    } else if raw & 0x2 != 0 {
        "File system driver"
    } else if raw & 0x20 != 0 {
        "Shared process"
    } else if raw & 0x10 != 0 {
        "Own process"
    } else {
        "Other"
    };
    if raw & 0x100 != 0 {
        format!("{base} (interactive)")
    } else {
        base.to_string()
    }
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

fn os_error(what: &str, e: windows::core::Error) -> AppError {
    AppError::new(ErrorKind::Os, format!("Could not {what}: {}", e.message()))
}

/// Start type, image path and delayed-start flag, from a handle opened for `SERVICE_QUERY_CONFIG`
/// only. `None` when Windows will not let us read this service.
fn read_config(scm: &Handle, name: &str) -> Option<(u32, Option<String>, bool)> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; every buffer is sized from the
    // length Windows reports, and u64 storage keeps the structs aligned.
    unsafe {
        let svc = Handle(OpenServiceW(scm.0, PCWSTR(wide.as_ptr()), SERVICE_QUERY_CONFIG).ok()?);
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(svc.0, None, 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let cfg = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        QueryServiceConfigW(svc.0, Some(cfg), (buf.len() * 8) as u32, &mut needed).ok()?;
        let start = (*cfg).dwStartType.0;
        let path = (*cfg)
            .lpBinaryPathName
            .to_string()
            .ok()
            .filter(|p| !p.is_empty());
        let mut delayed = [0u8; 8];
        let mut n = 0u32;
        let is_delayed = QueryServiceConfig2W(
            svc.0,
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            Some(&mut delayed),
            &mut n,
        )
        .is_ok()
            && u32::from_ne_bytes([delayed[0], delayed[1], delayed[2], delayed[3]]) != 0;
        Some((start, path, is_delayed))
    }
}

pub fn list() -> AppResult<Vec<WinService>> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    // SAFETY: standard Enum* paging loop; `buf` is u64-aligned and sized from `needed`, and the
    // entry strings point into it, so they are copied out before the buffer is reused.
    unsafe {
        let scm = Handle(
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)
                .map_err(|e| os_error("open the service manager", e))?,
        );
        let mut out = Vec::new();
        let mut resume = 0u32;
        let mut buf: Vec<u64> = vec![0; 1 << 15];
        loop {
            let mut needed = 0u32;
            let mut returned = 0u32;
            let bytes =
                std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), buf.len() * 8);
            let result = EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(bytes),
                &mut needed,
                &mut returned,
                Some(&mut resume),
                PCWSTR::null(),
            );
            let more = match &result {
                Ok(()) => false,
                Err(e) if e.code() == ERROR_MORE_DATA.to_hresult() => true,
                Err(e) => return Err(os_error("list Windows services", e.clone())),
            };
            let entries = buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
            for i in 0..returned as usize {
                let e = &*entries.add(i);
                let name = e.lpServiceName.to_string().unwrap_or_default();
                let display_name = e.lpDisplayName.to_string().unwrap_or_default();
                let st = &e.ServiceStatusProcess;
                let config = read_config(&scm, &name);
                let (start_raw, binary_path, delayed) = match config {
                    Some((s, p, d)) => (Some(s), p, d),
                    None => (None, None, false),
                };
                let start_type = match start_raw {
                    Some(0) => StartType::Boot,
                    Some(1) => StartType::System,
                    Some(2) if delayed => StartType::AutoDelayed,
                    Some(2) => StartType::Auto,
                    Some(4) => StartType::Disabled,
                    _ => StartType::Manual,
                };
                out.push(WinService {
                    protected: is_protected(
                        st.dwServiceType.0,
                        binary_path.as_deref(),
                        &name,
                        &system_root,
                    ),
                    developer: is_developer(&name, &display_name, DEV_PATTERNS, &[]),
                    state: state_from(st.dwCurrentState.0),
                    start_type,
                    service_type: type_label(st.dwServiceType.0),
                    pid: (st.dwProcessId != 0).then_some(st.dwProcessId),
                    binary_path,
                    name,
                    display_name,
                });
            }
            if !more {
                break;
            }
        }
        out.sort_by_key(|s| s.display_name.to_lowercase());
        Ok(out)
    }
}

/// The service named `name` (case-insensitive), if it exists.
pub fn find(name: &str) -> AppResult<Option<WinService>> {
    Ok(list()?
        .into_iter()
        .find(|s| s.name.eq_ignore_ascii_case(name)))
}

fn open_service(name: &str, access: u32) -> AppResult<(Handle, Handle)> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; handles are closed by `Handle`.
    unsafe {
        let scm = Handle(
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), 0x0001)
                .map_err(|e| os_error("open the service manager", e))?,
        );
        let svc = Handle(
            OpenServiceW(scm.0, PCWSTR(wide.as_ptr()), access)
                .map_err(|e| os_error(&format!("open service {name}"), e))?,
        );
        Ok((scm, svc))
    }
}

fn current_state(svc: &Handle) -> AppResult<u32> {
    let mut st = SERVICE_STATUS::default();
    // SAFETY: `st` is a valid out-pointer for the call.
    unsafe { QueryServiceStatus(svc.0, &mut st) }
        .map_err(|e| os_error("read service status", e))?;
    Ok(st.dwCurrentState.0)
}

fn wait_for(svc: &Handle, target: u32, what: &str) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while current_state(svc)? != target {
        if Instant::now() > deadline {
            return Err(AppError::new(
                ErrorKind::Os,
                format!("The service did not {what} within 30 seconds."),
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

/// Starts a service and waits until it is running. Opens it with `SERVICE_START` only.
pub fn start(name: &str) -> AppResult<()> {
    let (_scm, svc) = open_service(name, SERVICE_START | SERVICE_QUERY_STATUS)?;
    if current_state(&svc)? == 4 {
        return Ok(());
    }
    // SAFETY: valid service handle; no arguments are passed.
    unsafe { StartServiceW(svc.0, None) }.map_err(|e| os_error("start the service", e))?;
    wait_for(&svc, 4, "start")
}

/// Asks a service to stop and waits until it has. Opens it with `SERVICE_STOP` only.
pub fn stop(name: &str) -> AppResult<()> {
    let (_scm, svc) = open_service(name, SERVICE_STOP | SERVICE_QUERY_STATUS)?;
    if current_state(&svc)? == 1 {
        return Ok(());
    }
    let mut st = SERVICE_STATUS::default();
    // SAFETY: valid service handle and out-pointer.
    unsafe { ControlService(svc.0, SERVICE_CONTROL_STOP, &mut st) }
        .map_err(|e| os_error("stop the service", e))?;
    wait_for(&svc, 1, "stop")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r"C:\Windows";
    const OWN: u32 = 0x10;
    const SHARE: u32 = 0x20;
    const KERNEL_DRIVER: u32 = 0x1;
    const FS_DRIVER: u32 = 0x2;

    fn protected(t: u32, path: &str, name: &str) -> bool {
        is_protected(t, Some(path), name, ROOT)
    }

    #[test]
    fn drivers_are_protected_whatever_their_path() {
        assert!(protected(KERNEL_DRIVER, r"C:\Tools\thing.sys", "thing"));
        assert!(protected(FS_DRIVER, r"C:\Tools\fs.sys", "fs"));
    }

    #[test]
    fn images_under_the_windows_directory_are_protected() {
        assert!(protected(
            SHARE,
            r"C:\Windows\System32\svchost.exe -k netsvcs -p",
            "Spooler"
        ));
        assert!(protected(
            OWN,
            r"C:\Windows\System32\spoolsv.exe",
            "Spooler"
        ));
        assert!(protected(
            OWN,
            r#""C:\Windows\System32\svchost.exe" -k LocalService"#,
            "X"
        ));
        assert!(protected(OWN, r"c:\WINDOWS\system32\svchost.exe", "X"));
        assert!(protected(OWN, "C:/Windows/System32/svchost.exe", "X"));
        assert!(protected(
            OWN,
            r"%SystemRoot%\system32\svchost.exe -k x",
            "X"
        ));
        assert!(protected(OWN, r"\SystemRoot\System32\drivers\x.sys", "X"));
        assert!(protected(OWN, r"\??\C:\Windows\System32\x.exe", "X"));
    }

    #[test]
    fn a_path_with_spaces_is_judged_by_the_executable_not_its_arguments() {
        // Unquoted image path containing spaces: the .exe ends it.
        assert!(!protected(
            OWN,
            r"C:\Program Files\Some App\svc.exe --config C:\Windows\x.ini",
            "someapp"
        ));
        assert!(!protected(
            OWN,
            r#""C:\Program Files\App\a.exe" C:\Windows\arg"#,
            "someapp"
        ));
    }

    #[test]
    fn third_party_services_are_not_protected() {
        assert!(!protected(
            OWN,
            r"C:\Program Files\PostgreSQL\16\bin\postgres.exe",
            "postgresql-x64-16"
        ));
        assert!(!protected(
            OWN,
            r#""C:\Program Files\Docker\Docker\resources\com.docker.service""#,
            "com.docker.service"
        ));
        assert!(!is_protected(OWN, None, "mystery", ROOT));
    }

    #[test]
    fn a_folder_that_merely_starts_with_windows_is_not_the_windows_directory() {
        assert!(!protected(OWN, r"C:\Windows.old\x.exe", "old"));
        assert!(!protected(OWN, r"C:\WindowsApps\x.exe", "apps"));
    }

    #[test]
    fn core_names_are_protected_even_with_an_odd_path_and_ignoring_case() {
        for n in [
            "RpcSs",
            "EventLog",
            "winmgmt",
            "MPSSVC",
            "WinDefend",
            "TrustedInstaller",
        ] {
            assert!(protected(OWN, r"D:\elsewhere\x.exe", n), "{n}");
        }
        assert!(is_protected(OWN, None, "Dhcp", ROOT));
    }

    #[test]
    fn a_different_system_root_is_respected() {
        assert!(is_protected(
            OWN,
            Some(r"D:\WinNT\system32\x.exe"),
            "x",
            r"D:\WinNT"
        ));
        assert!(!is_protected(
            OWN,
            Some(r"C:\Windows\system32\x.exe"),
            "x",
            r"D:\WinNT"
        ));
    }

    fn dev(name: &str, display: &str, pins: &[&str]) -> bool {
        let pins: Vec<String> = pins.iter().map(|s| s.to_string()).collect();
        is_developer(name, display, DEV_PATTERNS, &pins)
    }

    #[test]
    fn developer_patterns_match_prefixes_and_exact_names_ignoring_case() {
        assert!(dev("postgresql-x64-16", "postgresql-x64-16", &[]));
        assert!(dev("MySQL80", "MySQL80", &[]));
        assert!(dev("com.docker.service", "Docker Desktop Service", &[]));
        assert!(dev("W3SVC", "World Wide Web Publishing Service", &[]));
        assert!(dev("hns", "Host Network Service", &[]));
        assert!(dev("MSSQL$SQLEXPRESS", "SQL Server (SQLEXPRESS)", &[]));
    }

    #[test]
    fn exact_patterns_do_not_match_longer_names() {
        assert!(!dev("wasabi", "Wasabi", &[]));
        assert!(!dev("hnsx", "Other", &[]));
        assert!(!dev("Spooler", "Print Spooler", &[]));
    }

    #[test]
    fn the_display_name_can_match_too() {
        assert!(dev("svc1234", "Redis Server", &[]));
    }

    #[test]
    fn pins_add_services_by_exact_name_ignoring_case() {
        assert!(!dev("MyApi", "My API", &[]));
        assert!(dev("MyApi", "My API", &["myapi"]));
        assert!(!dev("MyApiTwo", "x", &["myapi"]));
    }

    #[test]
    #[ignore]
    fn the_real_service_list_includes_the_event_log_as_protected_and_running() {
        let all = list().unwrap();
        assert!(all.len() > 20, "{} services", all.len());
        let log = all
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case("EventLog"))
            .expect("EventLog exists");
        assert!(log.protected);
        assert_eq!(log.state, ServiceState::Running);
        assert!(log.pid.is_some());
        assert!(log.binary_path.is_some());
        // Nothing is protected by accident: some services are not.
        assert!(all.iter().any(|s| !s.protected));
        assert!(all.iter().any(|s| s.start_type == StartType::AutoDelayed));
    }
}
