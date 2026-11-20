//! Real-process tests for dependency-ordered project start and stop. Run with `--ignored`.

use orbit_lib::db::{open_in_memory, Db};
use orbit_lib::engine::projects::{
    restart_project, start_project, stop_project, Outcome, Progress,
};
use orbit_lib::engine::state::Status;
use orbit_lib::engine::supervisor::{ServiceRuntime, Supervisor};
use orbit_lib::models::project::{Dep, OnFailure, Project};
use orbit_lib::models::service::*;
use orbit_lib::repo::{projects, services};
use orbit_lib::secrets::{InMemorySecretStore, SecretStore};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

struct H {
    sup: Supervisor,
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
    events: Arc<Mutex<Vec<ServiceRuntime>>>,
    project: String,
    _logs: tempfile::TempDir,
}

fn harness() -> H {
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
    let project = projects::create(
        &db,
        &Project {
            id: String::new(),
            name: "P".into(),
            description: String::new(),
            icon: String::new(),
            root_dir: String::new(),
            archived: false,
        },
    )
    .unwrap()
    .id;
    H {
        sup,
        db,
        secrets,
        events,
        project,
        _logs: logs,
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A service in the project: `serve` on its own port unless `args` says otherwise.
fn svc(h: &H, name: &str, args: Option<&[&str]>, keep_running: bool) -> String {
    let port = free_port();
    let args: Vec<String> = match args {
        Some(a) => a.iter().map(|s| s.to_string()).collect(),
        None => vec!["serve".into(), port.to_string()],
    };
    let spec = ServiceSpec {
        id: String::new(),
        name: name.into(),
        description: String::new(),
        category: Category::Custom,
        project_id: Some(h.project.clone()),
        working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        executable: SVC.into(),
        args,
        run_through_shell: false,
        env: vec![],
        port: None,
        startup_timeout_secs: 10,
        shutdown_timeout_secs: 5,
        restart: RestartPolicy::Never,
        health: None,
        keep_running_on_quit: keep_running,
        enabled: true,
    };
    services::create(&h.db, h.secrets.as_ref(), &spec)
        .unwrap()
        .id
}

fn needs(h: &H, service: &str, on: &str, on_failure: OnFailure) {
    let mut deps = projects::dependencies(&h.db, service).unwrap();
    deps.push(Dep {
        depends_on_id: on.into(),
        on_failure,
    });
    projects::set_dependencies(&h.db, service, deps).unwrap();
}

fn no_progress() -> impl Fn(&Progress) + Sync {
    |_| {}
}

/// Index of the first recorded event for `id` with `status`.
fn first(h: &H, id: &str, status: Status) -> Option<usize> {
    h.events
        .lock()
        .unwrap()
        .iter()
        .position(|e| e.id == id && e.status == status)
}

fn outcome(report: &orbit_lib::engine::projects::ProjectRunReport, id: &str) -> Outcome {
    report
        .services
        .iter()
        .find(|s| s.service_id == id)
        .unwrap_or_else(|| panic!("{id} is not in the report"))
        .outcome
        .clone()
}

fn stop_all(h: &H) {
    for s in services::list(&h.db).unwrap() {
        let _ = h.sup.stop(&s.id, true);
    }
}

#[test]
#[ignore]
fn a_chain_starts_dependencies_first_and_stops_dependants_first() {
    let h = harness();
    let (a, b, c) = (
        svc(&h, "a", None, false),
        svc(&h, "b", None, false),
        svc(&h, "c", None, false),
    );
    needs(&h, &b, &a, OnFailure::Abort);
    needs(&h, &c, &b, OnFailure::Abort);

    let report = start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    for id in [&a, &b, &c] {
        assert_eq!(outcome(&report, id), Outcome::Started);
    }
    // Each starts only after the one it needs is Running.
    assert!(first(&h, &a, Status::Running).unwrap() < first(&h, &b, Status::Starting).unwrap());
    assert!(first(&h, &b, Status::Running).unwrap() < first(&h, &c, Status::Starting).unwrap());

    let stopped = stop_project(&h.sup, &h.db, &h.project, false, &no_progress()).unwrap();
    for id in [&a, &b, &c] {
        assert_eq!(outcome(&stopped, id), Outcome::Stopped);
    }
    assert!(first(&h, &c, Status::Stopped).unwrap() < first(&h, &b, Status::Stopping).unwrap());
    assert!(first(&h, &b, Status::Stopped).unwrap() < first(&h, &a, Status::Stopping).unwrap());
}

#[test]
#[ignore]
fn a_failed_dependency_with_abort_skips_its_dependants_and_says_why() {
    let h = harness();
    let bad = svc(&h, "database", Some(&["exit", "1"]), false);
    let dependant = svc(&h, "api", None, false);
    let later = svc(&h, "web", None, false);
    needs(&h, &dependant, &bad, OnFailure::Abort);
    needs(&h, &later, &dependant, OnFailure::Abort);

    let report = start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    assert!(matches!(outcome(&report, &bad), Outcome::Failed { .. }));
    let Outcome::Skipped { reason } = outcome(&report, &dependant) else {
        panic!("api should be skipped");
    };
    assert!(reason.contains("database"), "{reason}");
    assert!(
        matches!(outcome(&report, &later), Outcome::Skipped { .. }),
        "skips spread"
    );
    assert!(
        first(&h, &dependant, Status::Starting).is_none(),
        "never started"
    );
    stop_all(&h);
}

#[test]
#[ignore]
fn a_failed_dependency_with_continue_still_starts_dependants_with_a_warning() {
    let h = harness();
    let bad = svc(&h, "cache", Some(&["exit", "1"]), false);
    let dependant = svc(&h, "api", None, false);
    needs(&h, &dependant, &bad, OnFailure::Continue);

    let report = start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    assert!(matches!(outcome(&report, &bad), Outcome::Failed { .. }));
    assert_eq!(outcome(&report, &dependant), Outcome::Started);
    assert!(
        report.warnings.iter().any(|w| w.contains("cache")),
        "{:?}",
        report.warnings
    );
    stop_all(&h);
}

#[test]
#[ignore]
fn an_already_running_dependency_is_reused_not_restarted() {
    let h = harness();
    let (a, b) = (svc(&h, "a", None, false), svc(&h, "b", None, false));
    needs(&h, &b, &a, OnFailure::Abort);
    h.sup.start(&a).unwrap();
    while h.sup.runtime(&a).status != Status::Running {
        std::thread::sleep(Duration::from_millis(50));
    }
    let pid = h.sup.runtime(&a).pid;

    let report = start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    assert_eq!(outcome(&report, &a), Outcome::AlreadyRunning);
    assert_eq!(outcome(&report, &b), Outcome::Started);
    assert_eq!(h.sup.runtime(&a).pid, pid, "same process");
    stop_all(&h);
}

#[test]
#[ignore]
fn the_two_middle_services_of_a_diamond_start_together() {
    let h = harness();
    let (a, b, c, d) = (
        svc(&h, "a", None, false),
        svc(&h, "b", None, false),
        svc(&h, "c", None, false),
        svc(&h, "d", None, false),
    );
    needs(&h, &b, &a, OnFailure::Abort);
    needs(&h, &c, &a, OnFailure::Abort);
    needs(&h, &d, &b, OnFailure::Abort);
    needs(&h, &d, &c, OnFailure::Abort);

    let report = start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    assert_eq!(outcome(&report, &d), Outcome::Started);
    let both_starting = first(&h, &b, Status::Starting)
        .unwrap()
        .max(first(&h, &c, Status::Starting).unwrap());
    let either_running = first(&h, &b, Status::Running)
        .unwrap()
        .min(first(&h, &c, Status::Running).unwrap());
    assert!(
        both_starting < either_running,
        "b and c were started one after the other"
    );
    assert!(first(&h, &b, Status::Running).unwrap() < first(&h, &d, Status::Starting).unwrap());
    assert!(first(&h, &c, Status::Running).unwrap() < first(&h, &d, Status::Starting).unwrap());
    stop_all(&h);
}

#[test]
#[ignore]
fn a_selection_starts_those_services_plus_what_they_need_and_nothing_else() {
    let h = harness();
    let (a, b, c, other) = (
        svc(&h, "a", None, false),
        svc(&h, "b", None, false),
        svc(&h, "c", None, false),
        svc(&h, "other", None, false),
    );
    needs(&h, &b, &a, OnFailure::Abort);
    needs(&h, &c, &b, OnFailure::Abort);

    let report = start_project(
        &h.sup,
        &h.db,
        &h.project,
        Some(vec![c.clone()]),
        &no_progress(),
    )
    .unwrap();
    for id in [&a, &b, &c] {
        assert_eq!(outcome(&report, id), Outcome::Started);
    }
    assert!(report.services.iter().all(|s| s.service_id != other));
    assert_eq!(h.sup.runtime(&other).status, Status::Stopped);
    stop_all(&h);
}

#[test]
#[ignore]
fn stopping_from_quit_leaves_keep_running_services_alone() {
    let h = harness();
    let (keep, normal) = (
        svc(&h, "keeper", None, true),
        svc(&h, "normal", None, false),
    );
    start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();

    let report = stop_project(&h.sup, &h.db, &h.project, true, &no_progress()).unwrap();
    assert_eq!(outcome(&report, &normal), Outcome::Stopped);
    assert!(matches!(outcome(&report, &keep), Outcome::Skipped { .. }));
    assert_eq!(h.sup.runtime(&keep).status, Status::Running);
    stop_all(&h);
}

#[test]
#[ignore]
fn restart_replaces_the_processes_and_progress_is_reported() {
    let h = harness();
    let (a, b) = (svc(&h, "a", None, false), svc(&h, "b", None, false));
    needs(&h, &b, &a, OnFailure::Abort);
    start_project(&h.sup, &h.db, &h.project, None, &no_progress()).unwrap();
    let old = (h.sup.runtime(&a).pid, h.sup.runtime(&b).pid);

    let steps = Mutex::new(Vec::new());
    let emit = |p: &Progress| {
        steps
            .lock()
            .unwrap()
            .push((p.service_id.clone(), p.step.clone()))
    };
    restart_project(&h.sup, &h.db, &h.project, &emit).unwrap();
    assert_ne!(h.sup.runtime(&a).pid, old.0);
    assert_ne!(h.sup.runtime(&b).pid, old.1);
    assert_eq!(h.sup.runtime(&b).status, Status::Running);
    let steps = steps.into_inner().unwrap();
    assert!(steps.contains(&(a.clone(), "ready".into())), "{steps:?}");
    assert!(steps.contains(&(b.clone(), "stopped".into())), "{steps:?}");
    stop_all(&h);
}
