//! Lifecycle tests for the service supervisor, against the scripted `orbit-testsvc` binary.
//! Ignored by default (they spawn processes); CI runs them with `--ignored`.

use orbit_lib::db::{open_in_memory, Db};
use orbit_lib::engine::state::Status;
use orbit_lib::engine::supervisor::{ServiceRuntime, Supervisor};
use orbit_lib::error::ErrorKind;
use orbit_lib::models::service::*;
use orbit_lib::repo::services as repo;
use orbit_lib::secrets::{InMemorySecretStore, SecretStore};
use orbit_lib::windows::procs::{identify, is_alive, ProcId};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

struct Harness {
    sup: Supervisor,
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
    logs: tempfile::TempDir,
    events: Arc<Mutex<Vec<ServiceRuntime>>>,
}

fn harness() -> Harness {
    let db = Arc::new(open_in_memory().unwrap());
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::default());
    let logs = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let sup = Supervisor::new(
        db.clone(),
        secrets.clone(),
        logs.path().to_path_buf(),
        PathBuf::from(SVC),
        Arc::new(move |r: &ServiceRuntime| sink.lock().unwrap().push(r.clone())),
    );
    Harness {
        sup,
        db,
        secrets,
        logs,
        events,
    }
}

fn svc(h: &Harness, name: &str, args: &[&str], port: Option<u16>) -> ServiceSpec {
    let spec = ServiceSpec {
        id: String::new(),
        name: name.into(),
        description: String::new(),
        category: Category::Custom,
        project_id: None,
        working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        executable: SVC.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        run_through_shell: false,
        env: vec![],
        port,
        startup_timeout_secs: 10,
        shutdown_timeout_secs: 5,
        restart: RestartPolicy::Never,
        health: None,
        keep_running_on_quit: false,
        allow_multiple: false,
        enabled: true,
    };
    repo::create(&h.db, h.secrets.as_ref(), &spec).unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_ok(port: u16) -> bool {
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

fn wait_until(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn wait_status(h: &Harness, id: &str, want: Status, secs: u64) -> ServiceRuntime {
    wait_until(&format!("{want:?}"), Duration::from_secs(secs), || {
        h.sup.runtime(id).status == want
    });
    h.sup.runtime(id)
}

fn proc_id(r: &ServiceRuntime) -> ProcId {
    identify(r.pid.expect("a pid")).expect("process is alive")
}

#[test]
#[ignore]
fn start_runs_a_real_process_and_stop_ends_it() {
    let h = harness();
    let port = free_port();
    let s = svc(&h, "api", &["serve", &port.to_string()], Some(port));
    h.sup.start(&s.id).unwrap();
    let r = wait_status(&h, &s.id, Status::Running, 5);
    let id = proc_id(&r);
    assert!(is_alive(&id));
    assert_eq!(r.parent_pid, Some(std::process::id()));
    wait_until("http", Duration::from_secs(5), || http_ok(port));
    h.sup.stop(&s.id, false).unwrap();
    let r = h.sup.runtime(&s.id);
    assert_eq!(r.status, Status::Stopped);
    assert!(!is_alive(&id));
    assert!(r.exit_code.is_some());
}

#[test]
#[ignore]
fn an_unresponsive_service_is_force_stopped_after_the_shutdown_timeout() {
    let h = harness();
    let port = free_port();
    let mut spec = svc(
        &h,
        "stubborn",
        &["ignore-stop", &port.to_string()],
        Some(port),
    );
    spec.shutdown_timeout_secs = 2;
    repo::update(&h.db, h.secrets.as_ref(), &spec).unwrap();
    h.sup.start(&spec.id).unwrap();
    let id = proc_id(&wait_status(&h, &spec.id, Status::Running, 5));
    let t = Instant::now();
    h.sup.stop(&spec.id, false).unwrap();
    let took = t.elapsed();
    assert!(
        took >= Duration::from_secs(2),
        "stopped too early: {took:?}"
    );
    assert!(took < Duration::from_secs(8), "took {took:?}");
    assert!(!is_alive(&id));
    assert_eq!(h.sup.runtime(&spec.id).status, Status::Stopped);
}

#[test]
#[ignore]
fn restart_gives_a_new_pid_and_leaves_the_restart_count_alone() {
    let h = harness();
    let port = free_port();
    let s = svc(&h, "api", &["serve", &port.to_string()], Some(port));
    h.sup.start(&s.id).unwrap();
    let first = wait_status(&h, &s.id, Status::Running, 5).pid.unwrap();
    h.sup.restart(&s.id).unwrap();
    let second = wait_status(&h, &s.id, Status::Running, 8);
    assert_ne!(second.pid.unwrap(), first);
    assert_eq!(second.restart_count, 0);
    h.sup.stop(&s.id, true).unwrap();
}

#[test]
#[ignore]
fn two_services_on_different_ports_run_together() {
    let h = harness();
    let (p1, p2) = (free_port(), free_port());
    let a = svc(&h, "a", &["serve", &p1.to_string()], Some(p1));
    let b = svc(&h, "b", &["serve", &p2.to_string()], Some(p2));
    h.sup.start(&a.id).unwrap();
    h.sup.start(&b.id).unwrap();
    wait_status(&h, &a.id, Status::Running, 5);
    wait_status(&h, &b.id, Status::Running, 5);
    wait_until("both http", Duration::from_secs(5), || {
        http_ok(p1) && http_ok(p2)
    });
    h.sup.stop(&a.id, true).unwrap();
    h.sup.stop(&b.id, true).unwrap();
}

#[test]
#[ignore]
fn an_occupied_port_is_refused_before_anything_is_spawned() {
    let h = harness();
    let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let s = svc(&h, "api", &["serve", &port.to_string()], Some(port));
    let err = h.sup.start(&s.id).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Conflict);
    assert!(err.message.contains(&port.to_string()), "{}", err.message);
    let r = h.sup.runtime(&s.id);
    assert_eq!(r.status, Status::Stopped);
    assert_eq!(r.pid, None);
}

#[test]
#[ignore]
fn a_missing_executable_and_a_bad_working_directory_are_reported() {
    let h = harness();
    let mut s = svc(&h, "ghost", &[], None);
    s.executable = "definitely-not-installed-xyz".into();
    repo::update(&h.db, h.secrets.as_ref(), &s).unwrap();
    let err = h.sup.start(&s.id).unwrap_err();
    assert!(
        err.message.contains("definitely-not-installed-xyz"),
        "{}",
        err.message
    );
    assert_eq!(h.sup.runtime(&s.id).status, Status::Stopped);

    s.executable = SVC.into();
    s.working_dir = "C:/no/such/dir".into();
    repo::update(&h.db, h.secrets.as_ref(), &s).unwrap();
    let err = h.sup.start(&s.id).unwrap_err();
    assert!(err.message.contains("working directory"), "{}", err.message);
    assert_eq!(h.sup.runtime(&s.id).status, Status::Stopped);
}

#[test]
#[ignore]
fn a_process_that_exits_immediately_ends_failed_with_its_exit_code() {
    let h = harness();
    let s = svc(&h, "quick", &["exit", "3"], None);
    h.sup.start(&s.id).unwrap();
    let r = wait_status(&h, &s.id, Status::Failed, 2);
    assert_eq!(r.exit_code, Some(3));
    assert_eq!(r.pid, None);
}

#[test]
#[ignore]
fn twenty_rapid_starts_launch_exactly_one_process() {
    let h = harness();
    let port = free_port();
    let s = svc(&h, "api", &["serve", &port.to_string()], Some(port));
    let sup = h.sup.clone();
    let id = s.id.clone();
    let threads: Vec<_> = (0..20)
        .map(|_| {
            let (sup, id) = (sup.clone(), id.clone());
            std::thread::spawn(move || sup.start(&id))
        })
        .collect();
    let ok = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(Result::is_ok)
        .count();
    assert_eq!(ok, 1);
    wait_status(&h, &s.id, Status::Running, 5);
    h.sup.stop(&s.id, true).unwrap();
}

#[test]
#[ignore]
fn a_crash_is_restarted_by_policy_then_stays_failed() {
    let h = harness();
    let mut s = svc(&h, "flaky", &["crash"], None);
    s.restart = RestartPolicy::OnFailure {
        max_retries: 2,
        backoff_secs: 1,
    };
    repo::update(&h.db, h.secrets.as_ref(), &s).unwrap();
    h.sup.start(&s.id).unwrap();
    wait_until("two restarts", Duration::from_secs(15), || {
        let r = h.sup.runtime(&s.id);
        r.restart_count == 2 && r.status == Status::Failed
    });
    std::thread::sleep(Duration::from_millis(2500));
    let r = h.sup.runtime(&s.id);
    assert_eq!(r.restart_count, 2);
    assert_eq!(r.status, Status::Failed);
    assert_eq!(r.exit_code, Some(101));
}

#[test]
#[ignore]
fn a_stale_record_is_deleted_at_startup_and_the_other_process_is_untouched() {
    let h = harness();
    let s = svc(&h, "api", &["serve", "1"], None);
    let mut bystander = std::process::Command::new("cmd")
        .args(["/c", "ping", "-n", "20", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let real = identify(bystander.id()).unwrap();
    // Same PID, wrong creation time: exactly what a reused PID looks like.
    h.db.0
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO service_processes (service_id,pid,created_at_ms,started_at) VALUES (?1,?2,?3,0)",
            rusqlite::params![s.id, real.pid, 1i64],
        )
        .unwrap();
    h.sup.adopt_on_startup();
    let rows: i64 =
        h.db.0
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM service_processes", [], |r| r.get(0))
            .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(h.sup.runtime(&s.id).status, Status::Stopped);
    assert!(is_alive(&real), "the unrelated process must not be touched");
    let _ = bystander.kill();
    let _ = bystander.wait();
}

#[test]
#[ignore]
fn a_live_record_is_readopted_and_can_then_be_stopped() {
    let h = harness();
    let port = free_port();
    let s = svc(&h, "api", &["serve", &port.to_string()], Some(port));
    h.sup.start(&s.id).unwrap();
    let before = wait_status(&h, &s.id, Status::Running, 5).pid.unwrap();

    // A fresh supervisor over the same database, as after an Orbit restart.
    let sup2 = Supervisor::new(
        h.db.clone(),
        h.secrets.clone(),
        h.logs.path().to_path_buf(),
        PathBuf::from(SVC),
        Arc::new(|_| {}),
    );
    sup2.adopt_on_startup();
    let r = sup2.runtime(&s.id);
    assert_eq!(r.status, Status::Running);
    assert_eq!(r.pid, Some(before));
    sup2.stop(&s.id, false).unwrap();
    wait_until("process gone", Duration::from_secs(5), || {
        identify(before).is_none()
    });
    assert_eq!(sup2.runtime(&s.id).status, Status::Stopped);
}

#[test]
#[ignore]
fn quitting_stops_services_unless_they_are_marked_keep_running() {
    let h = harness();
    let (p1, p2) = (free_port(), free_port());
    let stop_me = svc(&h, "stop-me", &["serve", &p1.to_string()], Some(p1));
    let mut keep = svc(&h, "keep-me", &["serve", &p2.to_string()], Some(p2));
    keep.keep_running_on_quit = true;
    repo::update(&h.db, h.secrets.as_ref(), &keep).unwrap();
    h.sup.start(&stop_me.id).unwrap();
    h.sup.start(&keep.id).unwrap();
    let a = proc_id(&wait_status(&h, &stop_me.id, Status::Running, 5));
    let b = proc_id(&wait_status(&h, &keep.id, Status::Running, 5));
    h.sup.stop_all_on_quit();
    assert!(!is_alive(&a), "the normal service must be stopped");
    assert!(is_alive(&b), "the keep-running service must survive");
    // Clean up the survivor ourselves.
    let sup2 = Supervisor::new(
        h.db.clone(),
        h.secrets.clone(),
        h.logs.path().to_path_buf(),
        PathBuf::from(SVC),
        Arc::new(|_| {}),
    );
    sup2.adopt_on_startup();
    sup2.stop(&keep.id, true).unwrap();
    wait_until("survivor gone", Duration::from_secs(5), || !is_alive(&b));
}

#[test]
#[ignore]
fn status_events_are_emitted_in_order() {
    let h = harness();
    let s = svc(&h, "quick", &["exit", "0"], None);
    h.sup.start(&s.id).unwrap();
    wait_status(&h, &s.id, Status::Stopped, 3);
    let seen: Vec<Status> = h
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.id == s.id)
        .map(|r| r.status)
        .collect();
    assert_eq!(seen.first(), Some(&Status::Starting));
    assert_eq!(seen.last(), Some(&Status::Stopped));
}
