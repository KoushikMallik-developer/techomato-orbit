//! Plan 2 end to end through the Rust API (no UI): a project with a dependency chain started in
//! order, its ports mapped back to services, one port holder ended, the project stopped in reverse
//! order, then the data directory reopened with everything intact. Run with `--ignored`.

use orbit_lib::db;
use orbit_lib::engine::ports::annotate;
use orbit_lib::engine::projects::{start_project, stop_project, Outcome, Progress};
use orbit_lib::engine::supervisor::{ServiceRuntime, Supervisor};
use orbit_lib::models::project::{Dep, OnFailure, Project};
use orbit_lib::models::service::*;
use orbit_lib::repo::{projects, services, settings};
use orbit_lib::secrets::{InMemorySecretStore, SecretStore};
use orbit_lib::windows::net::list_endpoints;
use orbit_lib::windows::procs::children_of;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn spec(name: &str, port: u16, project: &str) -> ServiceSpec {
    ServiceSpec {
        id: String::new(),
        name: name.into(),
        description: String::new(),
        category: Category::Custom,
        project_id: Some(project.into()),
        working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        executable: SVC.into(),
        args: vec!["serve".into(), port.to_string()],
        run_through_shell: false,
        env: vec![],
        port: Some(port),
        startup_timeout_secs: 10,
        shutdown_timeout_secs: 5,
        restart: RestartPolicy::Never,
        health: None,
        keep_running_on_quit: false,
        enabled: true,
    }
}

fn supervisor(dir: &std::path::Path, db: Arc<db::Db>) -> Supervisor {
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::default());
    Supervisor::new(
        db,
        secrets,
        dir.join("logs"),
        PathBuf::from(SVC),
        Arc::new(|_: &ServiceRuntime| {}),
    )
}

#[test]
#[ignore = "starts real processes and opens real ports"]
fn plan_two_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let database = Arc::new(db::open(&dir.path().join("orbit.db")).unwrap());
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::default());
    let sup = supervisor(dir.path(), database.clone());

    // db <- api <- web, all in one project.
    let project = projects::create(
        &database,
        &Project {
            id: String::new(),
            name: "Shop".into(),
            description: String::new(),
            icon: String::new(),
            root_dir: String::new(),
            archived: false,
        },
    )
    .unwrap();
    let ports = [free_port(), free_port(), free_port()];
    let mk = |name: &str, port: u16| {
        services::create(&database, secrets.as_ref(), &spec(name, port, &project.id))
            .unwrap()
            .id
    };
    let (db_id, api_id, web_id) = (mk("db", ports[0]), mk("api", ports[1]), mk("web", ports[2]));
    let needs = |service: &str, on: &str| {
        projects::set_dependencies(
            &database,
            service,
            vec![Dep {
                depends_on_id: on.into(),
                on_failure: OnFailure::Abort,
            }],
        )
        .unwrap()
    };
    needs(&api_id, &db_id);
    needs(&web_id, &api_id);
    settings::set(&database, "acceptance.marker", "kept").unwrap();

    // Start: dependency order.
    let steps: Mutex<Vec<(String, String)>> = Mutex::default();
    let record = |p: &Progress| {
        steps
            .lock()
            .unwrap()
            .push((p.service_id.clone(), p.step.clone()));
    };
    let report = start_project(&sup, &database, &project.id, None, &record).unwrap();
    assert!(
        report
            .services
            .iter()
            .all(|s| matches!(s.outcome, Outcome::Started)),
        "{report:?}"
    );
    let order = |step: &str| -> Vec<String> {
        steps
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s == step)
            .map(|(id, _)| id.clone())
            .collect()
    };
    assert_eq!(
        order("starting"),
        [db_id.clone(), api_id.clone(), web_id.clone()]
    );

    // Ports: every listener maps back to its service, and the service to its project.
    let specs = services::list(&database).unwrap();
    let rows = annotate(
        list_endpoints().unwrap(),
        &sup.all_runtime().unwrap(),
        &specs,
        &children_of,
    );
    for (id, port) in [&db_id, &api_id, &web_id].into_iter().zip(ports) {
        let row = rows
            .iter()
            .find(|r| r.endpoint.port == port && r.service_id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("port {port} is not mapped to its service"));
        let owner = specs.iter().find(|s| &s.id == id).unwrap();
        assert_eq!(owner.project_id.as_deref(), Some(project.id.as_str()));
        assert!(row.conflicts_with.is_none());
    }

    // Stop: reverse order of the services that were still up.
    steps.lock().unwrap().clear();
    stop_project(&sup, &database, &project.id, false, &record).unwrap();
    let stopped = order("stopping");
    let pos = |id: &String| stopped.iter().position(|s| s == id);
    assert!(
        pos(&api_id) < pos(&db_id),
        "api stops before db: {stopped:?}"
    );
    for id in [&db_id, &api_id, &web_id] {
        assert!(sup.runtime(id).pid.is_none(), "{id} still has a pid");
    }

    // Reopen: projects, edges, services and settings survived.
    drop(sup);
    drop(database);
    let again = db::open(&dir.path().join("orbit.db")).unwrap();
    assert_eq!(projects::list(&again).unwrap().len(), 1);
    assert_eq!(projects::all_edges(&again).unwrap().len(), 2);
    assert_eq!(services::list(&again).unwrap().len(), 3);
    assert_eq!(
        settings::get(&again, "acceptance.marker")
            .unwrap()
            .as_deref(),
        Some("kept")
    );
}
