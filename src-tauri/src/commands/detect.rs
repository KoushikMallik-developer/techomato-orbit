//! Finding database servers that are already running and adding them to Orbit as ordinary
//! services, so they can be started and stopped from here.

use crate::commands::ports::current_rows;
use crate::db::Db;
use crate::engine::detect::{self, Found};
use crate::engine::state::Status;
use crate::engine::supervisor::Supervisor;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::ServiceSpec;
use crate::repo::services as repo;
use crate::secrets::SecretStore;
use crate::windows::procs::{is_alive, ProcId};
use crate::windows::scm::{self, Svc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::State;

fn internal(e: impl ToString) -> AppError {
    AppError::new(ErrorKind::Internal, e.to_string())
}

fn refuse(kind: ErrorKind, msg: &str) -> AppError {
    AppError::new(kind, msg)
}

/// The Windows service whose process is `pid` or sits above it (a service host such as `pg_ctl`
/// counts). Shared hosts (`svchost.exe`) never count: a program that merely runs below one, such
/// as a scheduled task, is not that service.
fn host_service(sys: &System, by_pid: &HashMap<u32, Svc>, pid: u32) -> Option<Svc> {
    let mut cur = Pid::from_u32(pid);
    for _ in 0..6 {
        if let Some(s) = by_pid.get(&cur.as_u32()) {
            return Some(s.clone());
        }
        cur = sys.process(cur)?.parent()?;
    }
    None
}

fn scan(rows: &[crate::engine::ports::PortRow]) -> Vec<Found> {
    let Ok(mut sys) = System::new() else {
        return vec![];
    };
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let by_pid: HashMap<u32, Svc> = scm::list()
        .into_iter()
        .filter(|s| !scm::is_shared_host(s))
        .filter_map(|s| s.pid.map(|p| (p, s)))
        .collect();
    detect::found(rows, &|pid| host_service(&sys, &by_pid, pid))
}

/// The service for `new`: the stopped one Orbit already has for the same program and port, or a
/// new one. `true` when it was created.
fn ensure(
    db: &Db,
    secrets: &dyn SecretStore,
    sup: &Supervisor,
    new: &ServiceSpec,
) -> AppResult<(ServiceSpec, bool)> {
    let same = repo::list(db)?.into_iter().find(|s| {
        s.executable.eq_ignore_ascii_case(&new.executable)
                && s.port == new.port
                // A Windows service is matched to its card by the exact command, so a card with
                // other arguments is a different service.
                && s.args == new.args
    });
    match same {
        // The states a service can be started from: it is not running, so the program is not it.
        Some(s)
            if matches!(
                sup.runtime(&s.id).status,
                Status::Stopped | Status::Failed | Status::Unknown
            ) =>
        {
            Ok((s, false))
        }
        Some(_) => Err(refuse(
            ErrorKind::Conflict,
            "Orbit already has a service for that program and it is busy.",
        )),
        None => Ok((repo::create(db, secrets, new)?, true)),
    }
}

/// A program a Windows service runs: Orbit starts and stops it through Windows.
fn take_over_service(
    db: &Db,
    secrets: &dyn SecretStore,
    sup: &Supervisor,
    f: &Found,
    svc: &Svc,
) -> AppResult<bool> {
    let (exe, args) = scm::split_command(&svc.command);
    let (spec, created) = ensure(
        db,
        secrets,
        sup,
        &detect::spec_for(f, Path::new(&exe), args, None),
    )?;
    sup.set_external(&spec.id, svc.pid);
    Ok(created)
}

/// A program somebody started by hand: Orbit watches it and can stop it, and starts it again with
/// the same command afterwards.
fn take_over_process(
    db: &Db,
    secrets: &dyn SecretStore,
    sup: &Supervisor,
    f: &Found,
) -> AppResult<bool> {
    let gone = || refuse(ErrorKind::Conflict, "That program has just stopped.");
    let id = ProcId {
        pid: f.pid,
        created_at_ms: f.created_at_ms.ok_or_else(|| {
            refuse(
                ErrorKind::Os,
                "Another account runs that program, so Orbit cannot manage it.",
            )
        })?,
    };
    let pid = Pid::from_u32(f.pid);
    let mut sys = System::new().map_err(internal)?;
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::everything(),
    );
    let p = sys
        .process(pid)
        .filter(|_| is_alive(&id))
        .ok_or_else(gone)?;
    let (Some(exe), false) = (p.exe(), p.cmd().is_empty()) else {
        return Err(refuse(
            ErrorKind::Os,
            "Orbit cannot read how that program was started, so it cannot start it again.",
        ));
    };
    let args = p
        .cmd()
        .iter()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let (spec, created) = ensure(db, secrets, sup, &detect::spec_for(f, exe, args, p.cwd()))?;
    sup.adopt(&spec.id, id);
    Ok(created)
}

fn take_over(db: &Db, secrets: &dyn SecretStore, sup: &Supervisor, f: &Found) -> AppResult<bool> {
    match &f.service {
        Some(svc) => take_over_service(db, secrets, sup, f, svc),
        None => take_over_process(db, secrets, sup, f),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    /// Services Orbit created just now.
    pub added: u32,
    /// Running programs Orbit could not take over (another account runs them, or Orbit cannot read
    /// how they were started).
    pub skipped: Vec<Found>,
}

/// Adds every database server running on this PC that Orbit does not manage as a service, and
/// reports the ones it could not.
#[tauri::command]
pub async fn service_detect(
    db: State<'_, Arc<Db>>,
    secrets: State<'_, Arc<dyn SecretStore>>,
    sup: State<'_, Supervisor>,
) -> AppResult<Detected> {
    let rows = current_rows(&db, &sup).await?;
    let (db, secrets, sup) = (
        db.inner().clone(),
        secrets.inner().clone(),
        sup.inner().clone(),
    );
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Detected {
            added: 0,
            skipped: vec![],
        };
        for f in scan(&rows) {
            match take_over(&db, secrets.as_ref(), &sup, &f) {
                Ok(created) => out.added += u32::from(created),
                Err(_) => out.skipped.push(f),
            }
        }
        out
    })
    .await
    .map_err(internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::engine::ports::annotate;
    use crate::secrets::InMemorySecretStore;
    use crate::windows::net::list_endpoints;
    use crate::windows::procs::process_tree;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn supervisor(db: &Arc<Db>) -> Supervisor {
        Supervisor::new(
            db.clone(),
            Arc::new(InMemorySecretStore::default()),
            std::env::temp_dir(),
            std::path::PathBuf::new(),
            Arc::new(|_| {}),
        )
    }

    fn rows(db: &Db, sup: &Supervisor) -> Vec<crate::engine::ports::PortRow> {
        let tree = process_tree();
        annotate(
            list_endpoints().unwrap(),
            &sup.all_runtime().unwrap(),
            &repo::list(db).unwrap(),
            &|p| tree.get(&p).cloned().unwrap_or_default(),
        )
    }

    fn found(port: u16, service: Option<Svc>) -> Found {
        Found {
            engine: "redis",
            name: "Redis",
            port,
            pid: 1,
            created_at_ms: None,
            service,
        }
    }

    #[test]
    fn a_program_that_runs_as_a_windows_service_becomes_a_service_once() {
        let db = Arc::new(open_in_memory().unwrap());
        let secrets = InMemorySecretStore::default();
        let sup = supervisor(&db);
        let svc = Svc {
            name: "Redis".into(),
            command: r#""C:\Redis\redis-server.exe" --service-run "C:\Redis\r.conf""#.into(),
            pid: Some(4321),
        };
        let f = found(6379, Some(svc));
        assert!(take_over(&db, &secrets, &sup, &f).unwrap());
        let all = repo::list(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].executable, r"C:\Redis\redis-server.exe");
        assert_eq!(all[0].args, ["--service-run", r"C:\Redis\r.conf"]);
        assert_eq!(sup.runtime(&all[0].id).status, Status::Running);
        assert_eq!(sup.runtime(&all[0].id).pid, Some(4321));

        // The service stops: the card follows. It starts again and is found again: no second card.
        sup.set_external(&all[0].id, None);
        assert!(!take_over(&db, &secrets, &sup, &f).unwrap());
        assert_eq!(repo::list(&db).unwrap().len(), 1);
        assert_eq!(sup.runtime(&all[0].id).status, Status::Running);
    }

    #[test]
    fn a_card_with_other_arguments_is_not_mistaken_for_the_windows_service() {
        let db = Arc::new(open_in_memory().unwrap());
        let secrets = InMemorySecretStore::default();
        let sup = supervisor(&db);
        let mut mine = detect::spec_for(
            &found(6379, None),
            Path::new(r"C:\Redis\redis-server.exe"),
            vec!["--port".into()],
            None,
        );
        mine.name = "My Redis".into();
        repo::create(&db, &secrets, &mine).unwrap();
        let svc = Svc {
            name: "Redis".into(),
            command: r#""C:\Redis\redis-server.exe" --service-run "C:\Redis\r.conf""#.into(),
            pid: Some(7),
        };
        assert!(take_over(&db, &secrets, &sup, &found(6379, Some(svc))).unwrap());
        assert_eq!(repo::list(&db).unwrap().len(), 2);
    }

    #[test]
    fn a_busy_service_for_the_same_program_is_not_taken_over_twice() {
        let db = Arc::new(open_in_memory().unwrap());
        let secrets = InMemorySecretStore::default();
        let sup = supervisor(&db);
        let svc = Svc {
            name: "Redis".into(),
            command: r"C:\Redis\redis-server.exe".into(),
            pid: Some(9),
        };
        let f = found(6379, Some(svc));
        take_over(&db, &secrets, &sup, &f).unwrap();
        // Still running from Orbit's point of view: the same program cannot be added again.
        let e = take_over(&db, &secrets, &sup, &f).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Conflict);
        assert_eq!(repo::list(&db).unwrap().len(), 1);
    }

    /// Starts a real Redis by hand on another port, finds it, takes it over, then stops it from
    /// Orbit. Needs the Redis that Orbit's "Add database" downloads; ignored because it uses the
    /// real machine.
    #[test]
    #[ignore = "starts a real Redis from Orbit's engine folder"]
    fn a_redis_started_by_hand_is_found_taken_over_and_stopped() {
        let root = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
            .join("Techomato Orbit");
        let exe = crate::engines::install::server_path(
            &root,
            crate::engines::catalog::find("redis").unwrap(),
        );
        if !exe.is_file() {
            eprintln!("skipped: Redis is not downloaded on this machine");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let mut by_hand = Command::new(&exe)
            .args(["--port", "6391", "--bind", "127.0.0.1", "--dir"])
            .arg(dir.path())
            .stdout(Stdio::null())
            .spawn()
            .expect("redis-server.exe from the Orbit engine folder");
        std::thread::sleep(Duration::from_millis(1500));

        let db = Arc::new(open_in_memory().unwrap());
        let secrets = InMemorySecretStore::default();
        let sup = supervisor(&db);
        let found = scan(&rows(&db, &sup))
            .into_iter()
            .find(|f| f.pid == by_hand.id())
            .expect("the Redis is found");
        assert_eq!((found.engine, found.port), ("redis", 6391));
        assert!(found.service.is_none());

        assert!(take_over(&db, &secrets, &sup, &found).unwrap());
        let spec = repo::list(&db).unwrap().remove(0);
        assert_eq!(spec.port, Some(6391));
        assert!(spec.args.contains(&"6391".to_string()), "{:?}", spec.args);
        assert_eq!(sup.runtime(&spec.id).status, Status::Running);
        // It now belongs to a service, so it is no longer offered.
        assert!(scan(&rows(&db, &sup)).iter().all(|f| f.pid != by_hand.id()));

        let t = Instant::now();
        sup.stop(&spec.id, false).unwrap();
        eprintln!("stopped in {:?}", t.elapsed());
        assert_eq!(sup.runtime(&spec.id).status, Status::Stopped);
        assert!(by_hand.try_wait().unwrap().is_some() || by_hand.wait().is_ok());
        assert!(std::net::TcpStream::connect(("127.0.0.1", 6391)).is_err());
    }
}
