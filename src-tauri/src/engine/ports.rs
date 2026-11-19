//! Ports page logic: matching listening endpoints to Orbit's services.

use crate::engine::supervisor::ServiceRuntime;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::ServiceSpec;
use crate::windows::net::Endpoint;
use crate::windows::procs::{is_alive, ProcId};
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::time::{Duration, Instant};

/// An endpoint plus which Orbit service (if any) it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRow {
    #[serde(flatten)]
    pub endpoint: Endpoint,
    pub service_id: Option<String>,
    pub service_name: Option<String>,
    /// A different program holds the port a stopped/failed service is configured to use.
    pub conflicts_with: Option<String>,
}

/// Matches by owner PID (or its children, which covers `cmd /c node` shells), then by configured
/// port. A configured-port match on a running service means the service owns it; on any other
/// status the listener is a conflict.
pub fn annotate(
    endpoints: Vec<Endpoint>,
    runtimes: &[ServiceRuntime],
    specs: &[ServiceSpec],
    children: &dyn Fn(u32) -> Vec<u32>,
) -> Vec<PortRow> {
    // pid -> owning service id, covering the service process and two levels of descendants.
    let mut owner: std::collections::HashMap<u32, &str> = std::collections::HashMap::new();
    for r in runtimes {
        let Some(root) = r.pid else { continue };
        owner.insert(root, &r.id);
        for c in children(root) {
            owner.insert(c, &r.id);
            for g in children(c) {
                owner.insert(g, &r.id);
            }
        }
    }
    let name_of = |id: &str| specs.iter().find(|s| s.id == id).map(|s| s.name.clone());
    endpoints
        .into_iter()
        .map(|endpoint| {
            let service_id = owner.get(&endpoint.pid).map(|s| s.to_string());
            let conflicts_with = if service_id.is_some() {
                None
            } else {
                specs
                    .iter()
                    .find(|s| s.port == Some(endpoint.port) && s.enabled)
                    .map(|s| s.name.clone())
            };
            PortRow {
                service_name: service_id.as_deref().and_then(name_of),
                service_id,
                conflicts_with,
                endpoint,
            }
        })
        .collect()
}

const CRITICAL: [&str; 6] = [
    "system",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
];

/// Why Orbit will not end this process, in plain words; `None` if it may be offered.
pub fn refusal(target: ProcId, own_pid: u32, name: Option<&str>) -> Option<String> {
    if target.pid == own_pid {
        return Some("That is Techomato Orbit itself.".into());
    }
    if target.pid == 0 || target.pid == 4 {
        return Some("That is part of Windows and cannot be ended.".into());
    }
    let lower = name.map(str::to_ascii_lowercase);
    if lower.as_deref().is_some_and(|n| CRITICAL.contains(&n)) {
        return Some(format!(
            "{} is a core Windows process; ending it would crash or log you out.",
            name.unwrap_or_default()
        ));
    }
    None
}

/// `taskkill /PID` (tree + force when `force`); true if Windows accepted the request.
fn taskkill(pid: u32, force: bool) -> bool {
    let mut args = vec!["/PID".to_string(), pid.to_string()];
    if force {
        args.extend(["/T".into(), "/F".into()]);
    }
    std::process::Command::new("taskkill")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn changed() -> AppError {
    AppError::new(
        ErrorKind::Conflict,
        "That process has changed or already ended.",
    )
}

/// Asks `target` to close (up to 5 s), then kills its tree.
/// The identity (PID + creation time) is re-checked right before each action so a reused PID
/// is never touched.
pub fn end_process(target: ProcId, force: bool) -> AppResult<()> {
    if !is_alive(&target) {
        return Err(changed());
    }
    if !force && taskkill(target.pid, false) {
        // Windows delivered a close request (a GUI app); give it time to wrap up.
        // Console signals are deliberately not used here: a process we did not start may share
        // a terminal with other programs, and a Ctrl+Break would hit them too.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if !is_alive(&target) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if !is_alive(&target) {
        return Ok(());
    }
    let killed = taskkill(target.pid, true);
    let deadline = Instant::now() + Duration::from_secs(3);
    while is_alive(&target) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if is_alive(&target) {
        return Err(AppError::new(
            ErrorKind::Os,
            if killed {
                "The process is still running.".to_string()
            } else {
                "Windows refused to end that process (it may need administrator rights)."
                    .to_string()
            },
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::state::Status;
    use crate::windows::net::Proto;

    fn ep(port: u16, pid: u32) -> Endpoint {
        Endpoint {
            proto: Proto::Tcp,
            local_addr: "127.0.0.1".into(),
            port,
            state: Some("LISTEN".into()),
            pid,
            created_at_ms: Some(1),
            process_name: Some("x.exe".into()),
            path: None,
        }
    }
    fn rt(id: &str, status: Status, pid: Option<u32>) -> ServiceRuntime {
        ServiceRuntime {
            id: id.into(),
            status,
            pid,
            parent_pid: None,
            started_at_ms: None,
            exit_code: None,
            restart_count: 0,
            last_error: None,
        }
    }
    fn spec(id: &str, name: &str, port: Option<u16>) -> ServiceSpec {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "category": "custom", "projectId": null,
            "workingDir": "C:\\", "executable": "x", "args": [], "runThroughShell": false,
            "env": [], "port": port, "startupTimeoutSecs": 5, "shutdownTimeoutSecs": 5,
            "restart": {"kind": "never"}, "health": null, "keepRunningOnQuit": false,
            "allowMultiple": false, "enabled": true
        }))
        .unwrap()
    }
    const NONE: &dyn Fn(u32) -> Vec<u32> = &|_| vec![];

    #[test]
    fn an_endpoint_owned_by_a_service_pid_is_attributed_to_it() {
        let rows = annotate(
            vec![ep(3000, 10)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", None)],
            NONE,
        );
        assert_eq!(rows[0].service_id.as_deref(), Some("a"));
        assert_eq!(rows[0].service_name.as_deref(), Some("API"));
        assert_eq!(rows[0].conflicts_with, None);
    }

    #[test]
    fn an_endpoint_owned_by_a_child_of_a_service_is_attributed_to_it() {
        let kids = |p: u32| if p == 10 { vec![11] } else { vec![] };
        let rows = annotate(
            vec![ep(3000, 11)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", None)],
            &kids,
        );
        assert_eq!(rows[0].service_id.as_deref(), Some("a"));
    }

    #[test]
    fn a_foreign_listener_on_a_stopped_services_port_is_a_conflict() {
        let rows = annotate(
            vec![ep(8080, 99)],
            &[rt("a", Status::Stopped, None)],
            &[spec("a", "API", Some(8080))],
            NONE,
        );
        assert_eq!(rows[0].service_id, None);
        assert_eq!(rows[0].conflicts_with.as_deref(), Some("API"));
    }

    #[test]
    fn an_unrelated_endpoint_has_no_annotations() {
        let rows = annotate(
            vec![ep(5000, 99)],
            &[rt("a", Status::Running, Some(10))],
            &[spec("a", "API", Some(8080))],
            NONE,
        );
        assert_eq!(rows[0].service_id, None);
        assert_eq!(rows[0].conflicts_with, None);
    }

    fn id(pid: u32) -> ProcId {
        ProcId {
            pid,
            created_at_ms: 1,
        }
    }

    #[test]
    fn orbit_itself_the_idle_process_and_system_are_refused() {
        assert!(refusal(id(500), 500, Some("techomato-orbit.exe")).is_some());
        assert!(refusal(id(0), 500, None).is_some());
        assert!(refusal(id(4), 500, Some("System")).is_some());
    }

    #[test]
    fn critical_windows_processes_are_refused_by_name_ignoring_case() {
        for n in [
            "lsass.exe",
            "LSASS.EXE",
            "csrss.exe",
            "wininit.exe",
            "services.exe",
        ] {
            assert!(refusal(id(900), 500, Some(n)).is_some(), "{n}");
        }
    }

    #[test]
    fn an_ordinary_process_and_svchost_are_not_refused() {
        assert_eq!(refusal(id(900), 500, Some("node.exe")), None);
        assert_eq!(refusal(id(900), 500, Some("svchost.exe")), None);
        assert_eq!(refusal(id(900), 500, None), None);
    }
}
