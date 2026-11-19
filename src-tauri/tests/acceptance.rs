//! Milestone 1, driven through the Rust API without the UI: create a service, run it, read its
//! logs, stop, restart, stop, then reopen the same data directory and find everything intact.
//! Uses the real Windows Credential Manager, so it is ignored by default.

use orbit_lib::db;
use orbit_lib::engine::state::Status;
use orbit_lib::engine::supervisor::{ServiceRuntime, Supervisor};
use orbit_lib::models::service::*;
use orbit_lib::repo::{processes, services as repo};
use orbit_lib::secrets::SecretStore;
use orbit_lib::windows::creds::CredentialManagerStore;
use orbit_lib::windows::procs::{identify, is_alive};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_200(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(300),
    ) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = s.write_all(b"GET / HTTP/1.0\r\n\r\n");
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.contains("200")
}

fn wait(what: &str, secs: u64, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn supervisor(dir: &std::path::Path, db: Arc<db::Db>, secrets: Arc<dyn SecretStore>) -> Supervisor {
    Supervisor::new(
        db,
        secrets,
        dir.join("logs"),
        PathBuf::from(SVC),
        Arc::new(|_: &ServiceRuntime| {}),
    )
}

#[test]
#[ignore]
fn create_run_stop_restart_reopen_and_everything_is_intact() {
    // A data dir with a space in its name, like %APPDATA%\Techomato Orbit.
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("Techomato Orbit");
    let workdir = dir.join("my project");
    std::fs::create_dir_all(&workdir).unwrap();
    let secrets: Arc<dyn SecretStore> = Arc::new(CredentialManagerStore);
    let port = free_port();

    let database = Arc::new(db::open(&dir.join("orbit.db")).unwrap());
    let created = repo::create(
        &database,
        secrets.as_ref(),
        &ServiceSpec {
            id: String::new(),
            name: "Acceptance API".into(),
            description: "milestone 1".into(),
            category: Category::Backend,
            project_id: None,
            working_dir: workdir.to_string_lossy().into_owned(),
            executable: SVC.into(),
            args: vec!["serve".into(), port.to_string()],
            run_through_shell: false,
            env: vec![
                EnvVar {
                    key: "MODE".into(),
                    value: Some("dev".into()),
                    is_secret: false,
                },
                EnvVar {
                    key: "API_TOKEN".into(),
                    value: Some("acceptance-secret".into()),
                    is_secret: true,
                },
            ],
            port: Some(port),
            startup_timeout_secs: 10,
            shutdown_timeout_secs: 5,
            restart: RestartPolicy::Never,
            health: None,
            keep_running_on_quit: false,
            allow_multiple: false,
            enabled: true,
        },
    )
    .unwrap();
    let id = created.id.clone();

    let sup = supervisor(&dir, database.clone(), secrets.clone());

    // Start: a real, alive process that answers HTTP, with logs.
    sup.start(&id).unwrap();
    wait("running", 8, || sup.runtime(&id).status == Status::Running);
    let first = sup.runtime(&id).pid.expect("a pid");
    let first_id = identify(first).expect("alive");
    assert!(is_alive(&first_id));
    wait("http 200", 5, || http_200(port));
    wait("a log line", 5, || {
        sup.logs(&id)
            .recent(10)
            .iter()
            .any(|l| l.text.contains("listening on"))
    });

    // Stop, restart (new process), stop.
    sup.stop(&id, false).unwrap();
    assert_eq!(sup.runtime(&id).status, Status::Stopped);
    assert!(!is_alive(&first_id));
    sup.start(&id).unwrap();
    wait("running again", 8, || {
        sup.runtime(&id).status == Status::Running
    });
    let second = sup.runtime(&id).pid.expect("a pid");
    assert_ne!(second, first, "a restart is a new process");
    let second_id = identify(second).expect("alive");
    sup.stop(&id, false).unwrap();
    assert!(!is_alive(&second_id));

    // "Quit and reopen": drop everything, open the same files again.
    drop(sup);
    drop(database);
    let reopened = Arc::new(db::open(&dir.join("orbit.db")).unwrap());
    let again = repo::get(&reopened, &id).unwrap();
    assert_eq!(again, created, "every field survives a restart");
    assert_eq!(again.env[1].value, None, "the secret is not in SQLite");
    assert_eq!(
        secrets.get(&id, "API_TOKEN").unwrap().as_deref(),
        Some("acceptance-secret"),
        "the secret is still set"
    );
    assert!(processes::list(&reopened).unwrap().is_empty());
    assert!(!is_alive(&first_id) && !is_alive(&second_id));

    // The reopened app sees the service as stopped and can run it again.
    let sup = supervisor(&dir, reopened.clone(), secrets.clone());
    sup.adopt_on_startup();
    assert_eq!(sup.runtime(&id).status, Status::Stopped);

    repo::delete(&reopened, secrets.as_ref(), &id).unwrap();
    assert_eq!(secrets.get(&id, "API_TOKEN").unwrap(), None);
}
