//! Database servers that are already running on this PC but were not started by Orbit, found from
//! the listening ports. Only the programs in `PROGRAMS` are ever offered, so adopting one can
//! never be used to take over an arbitrary process.

use crate::engine::ports::PortRow;
use crate::engines::catalog;
use crate::models::service::{HealthCheck, RestartPolicy, ServiceSpec};
use crate::windows::net::Proto;
use crate::windows::scm::Svc;
use serde::Serialize;
use std::path::Path;

/// (program file name in lower case, catalog engine id, name shown). `mysqld.exe` may be MySQL or
/// MariaDB, hence the label.
const PROGRAMS: &[(&str, &str, &str)] = &[
    ("redis-server.exe", "redis", "Redis"),
    ("postgres.exe", "postgres", "PostgreSQL"),
    ("mysqld.exe", "mariadb", "MySQL / MariaDB"),
    ("mariadbd.exe", "mariadb", "MariaDB"),
    ("mongod.exe", "mongo", "MongoDB"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// Catalog engine id.
    pub engine: &'static str,
    pub name: &'static str,
    pub port: u16,
    pub pid: u32,
    /// `None` when Orbit may not inspect the process (another account runs it): it is listed but
    /// cannot be taken over.
    pub created_at_ms: Option<u64>,
    /// The Windows service that runs it, when it is one.
    #[serde(skip)]
    pub service: Option<Svc>,
}

/// One entry per running program that listens on TCP and does not belong to an Orbit service.
/// With several ports, the engine's usual one wins, else the lowest.
pub fn found(rows: &[PortRow], service_of: &dyn Fn(u32) -> Option<Svc>) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for r in rows {
        let e = &r.endpoint;
        if r.service_id.is_some() || e.proto != Proto::Tcp || e.state.as_deref() != Some("LISTEN") {
            continue;
        }
        let Some(file) = e.process_name.as_deref().map(str::to_lowercase) else {
            continue;
        };
        let Some(&(_, engine, name)) = PROGRAMS.iter().find(|(f, _, _)| *f == file) else {
            continue;
        };
        let usual = catalog::find(engine).map(|c| c.default_port);
        match out.iter_mut().find(|f| f.pid == e.pid) {
            Some(f) => {
                if Some(e.port) == usual || (Some(f.port) != usual && e.port < f.port) {
                    f.port = e.port;
                }
            }
            None => out.push(Found {
                engine,
                name,
                port: e.port,
                pid: e.pid,
                created_at_ms: e.created_at_ms,
                service: service_of(e.pid),
            }),
        }
    }
    out
}

/// The service that runs `f` the way it was started. Nothing about the environment it was started
/// with is known, so none is kept. Quitting Orbit leaves it running, as it was before Orbit.
pub fn spec_for(f: &Found, exe: &Path, args: Vec<String>, cwd: Option<&Path>) -> ServiceSpec {
    let working_dir = cwd.or_else(|| exe.parent()).unwrap_or(exe);
    ServiceSpec {
        id: String::new(),
        name: format!("{} (port {})", f.name, f.port),
        description: "Found already running; Orbit took it over.".into(),
        category: catalog::find(f.engine)
            .map_or(crate::models::service::Category::Custom, |c| c.category),
        project_id: None,
        working_dir: working_dir.to_string_lossy().into_owned(),
        executable: exe.to_string_lossy().into_owned(),
        args,
        run_through_shell: false,
        env: vec![],
        port: Some(f.port),
        startup_timeout_secs: 60,
        // The gentle stop signal rarely reaches a program Orbit did not start, so do not wait long.
        shutdown_timeout_secs: 3,
        restart: RestartPolicy::Never,
        health: Some(HealthCheck::Tcp {
            port: f.port,
            interval_secs: 10,
            timeout_secs: 3,
        }),
        keep_running_on_quit: true,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::net::Endpoint;

    fn row(name: &str, port: u16, pid: u32, owner: Option<&str>) -> PortRow {
        PortRow {
            endpoint: Endpoint {
                proto: Proto::Tcp,
                local_addr: "127.0.0.1".into(),
                port,
                state: Some("LISTEN".into()),
                pid,
                created_at_ms: Some(1000 + u64::from(pid)),
                process_name: Some(name.into()),
                path: None,
            },
            service_id: owner.map(Into::into),
            service_name: None,
            conflicts_with: None,
        }
    }
    const NOT_A_SERVICE: &dyn Fn(u32) -> Option<Svc> = &|_| None;
    fn a_service() -> Option<Svc> {
        Some(Svc {
            name: "Redis".into(),
            command: r#""C:\Redis\redis-server.exe" --service-run "C:\Redis\r.conf""#.into(),
            pid: Some(1),
        })
    }

    #[test]
    fn every_program_maps_to_a_catalog_engine() {
        for (_, engine, _) in PROGRAMS {
            assert!(catalog::find(engine).is_some(), "{engine}");
        }
    }

    #[test]
    fn a_redis_on_another_port_is_found_and_others_are_ignored() {
        let rows = [
            row("redis-server.exe", 6380, 10, None),
            row("Redis-Server.EXE", 6380, 10, None), // same program, IPv6 row
            row("redis-server.exe", 6379, 11, Some("orbit-svc")), // Orbit's own
            row("chrome.exe", 9222, 12, None),
        ];
        let got = found(&rows, NOT_A_SERVICE);
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].engine, got[0].port, got[0].pid),
            ("redis", 6380, 10)
        );
        assert_eq!(got[0].created_at_ms, Some(1010));
        assert!(got[0].service.is_none());
    }

    #[test]
    fn udp_is_ignored_and_a_process_orbit_may_not_inspect_is_still_listed() {
        let mut udp = row("mongod.exe", 27017, 20, None);
        udp.endpoint.proto = Proto::Udp;
        let mut locked = row("redis-server.exe", 6379, 21, None);
        locked.endpoint.created_at_ms = None;
        let got = found(&[udp, locked], &|_| a_service());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].created_at_ms, None);
        assert!(got[0].service.is_some());
    }

    #[test]
    fn the_usual_port_wins_and_a_windows_service_is_flagged() {
        let rows = [
            row("postgres.exe", 5433, 30, None),
            row("postgres.exe", 5432, 30, None),
            row("postgres.exe", 5440, 30, None),
        ];
        let got = found(&rows, &|pid| (pid == 30).then(a_service).flatten());
        assert_eq!((got.len(), got[0].port), (1, 5432));
        assert_eq!(got[0].service.as_ref().unwrap().name, "Redis");
    }

    #[test]
    fn the_service_it_creates_runs_the_same_command_and_survives_quit() {
        let f = found(&[row("redis-server.exe", 6380, 10, None)], NOT_A_SERVICE).remove(0);
        let exe = Path::new(r"C:\Redis\redis-server.exe");
        let spec = spec_for(&f, exe, vec!["--port".into(), "6380".into()], None);
        assert_eq!(spec.name, "Redis (port 6380)");
        assert_eq!(spec.executable, r"C:\Redis\redis-server.exe");
        assert_eq!(spec.working_dir, r"C:\Redis");
        assert_eq!(spec.args, ["--port", "6380"]);
        assert_eq!(spec.port, Some(6380));
        assert!(spec.keep_running_on_quit);
        assert!(spec.validate().is_ok());
    }
}
