//! Real databases: downloads each engine (checked against the pinned SHA-256), creates an
//! instance, starts it under the supervisor, connects, and stops it gracefully. Ignored by
//! default (it downloads hundreds of megabytes). Run one engine or several:
//!   ORBIT_ENGINES=redis,mariadb,postgres cargo test --test real_databases -- --ignored --nocapture

use orbit_lib::db::open_in_memory;
use orbit_lib::engine::state::Status;
use orbit_lib::engine::supervisor::{ServiceRuntime, Supervisor};
use orbit_lib::engines::{catalog, instance};
use orbit_lib::secrets::{InMemorySecretStore, SecretStore};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

fn wait(what: &str, limit: Duration, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < limit, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[ignore = "downloads databases"]
fn each_database_installs_starts_and_stops_gracefully() {
    let wanted = std::env::var("ORBIT_ENGINES").unwrap_or_else(|_| "redis".into());
    for id in wanted.split(',') {
        let e = catalog::find(id.trim()).expect("unknown engine");
        let root = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let db = Arc::new(open_in_memory().unwrap());
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::default());
        let t = Instant::now();
        let spec = instance::create(
            &db,
            secrets.as_ref(),
            root.path(),
            e,
            "test db",
            &|p, d, tot| {
                if d == 0 || d % (50 << 20) < (2 << 20) {
                    eprintln!("[{id}] {p} {d}/{tot}");
                }
            },
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_or_else(|x| panic!("{id}: {x:?}"));
        eprintln!("[{id}] created in {:?}, port {:?}", t.elapsed(), spec.port);
        let sup = Supervisor::new(
            db.clone(),
            secrets.clone(),
            logs.path().to_path_buf(),
            PathBuf::from(SVC),
            Arc::new(|_: &ServiceRuntime| {}),
        );
        sup.start(&spec.id).unwrap();
        let port = spec.port.unwrap();
        wait("the server to listen", Duration::from_secs(90), || {
            TcpStream::connect(("127.0.0.1", port)).is_ok()
        });
        wait("status Running", Duration::from_secs(30), || {
            sup.runtime(&spec.id).status == Status::Running
        });
        let t = Instant::now();
        sup.stop(&spec.id, false).unwrap();
        let took = t.elapsed();
        eprintln!("[{id}] stopped in {took:?}");
        assert_eq!(sup.runtime(&spec.id).status, Status::Stopped, "{id}");
        assert!(
            took < Duration::from_secs(20),
            "{id} ignored the graceful stop and had to be killed ({took:?})"
        );
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "{id} still listening"
        );
    }
}
