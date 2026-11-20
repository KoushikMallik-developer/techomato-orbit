//! Creating a database instance: install the engine if needed, prepare an empty data folder, pick
//! a free port and save a service that runs it.

use super::catalog::{self, Engine};
use super::{install, pick_port, slug};
use crate::db::Db;
use crate::engine::preflight::port_in_use;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::{HealthCheck, RestartPolicy, ServiceSpec};
use crate::repo::services as repo;
use crate::secrets::SecretStore;
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn io(m: impl ToString) -> AppError {
    AppError::new(ErrorKind::Io, m.to_string())
}

/// `emit(phase, done, total)` reports progress: phase is download, unpack or init.
pub fn create(
    db: &Db,
    secrets: &dyn SecretStore,
    root: &Path,
    e: &'static Engine,
    name: &str,
    emit: &dyn Fn(&str, u64, u64),
    cancel: &AtomicBool,
) -> AppResult<ServiceSpec> {
    let server = install::ensure(root, e, emit, cancel).map_err(io)?;
    let taken: HashSet<u16> = repo::list(db)?.iter().filter_map(|s| s.port).collect();
    let port = pick_port(e.default_port, &|p| taken.contains(&p) || port_in_use(p))
        .ok_or_else(|| io("No free port was found near the usual one."))?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = root
        .join("instances")
        .join(format!("{}-{}", slug(name), &id[..8]));
    let data = dir.join("data");
    std::fs::create_dir_all(&dir).map_err(io)?;
    let cleanup = |dir: &Path| {
        let _ = std::fs::remove_dir_all(dir);
    };
    if let Err(m) = prepare(root, e, &dir, &data, emit) {
        cleanup(&dir);
        return Err(io(m));
    }
    let spec = ServiceSpec {
        id: String::new(),
        name: name.to_string(),
        description: format!("{} {} instance", e.name, e.version),
        category: e.category,
        project_id: None,
        working_dir: dir.to_string_lossy().into_owned(),
        executable: server.to_string_lossy().into_owned(),
        args: catalog::run_args(e, &data, port),
        run_through_shell: false,
        env: vec![],
        port: Some(port),
        startup_timeout_secs: 60,
        shutdown_timeout_secs: 30,
        restart: RestartPolicy::Never,
        health: Some(HealthCheck::Tcp {
            port,
            interval_secs: 10,
            timeout_secs: 3,
        }),
        keep_running_on_quit: false,
        enabled: true,
    };
    repo::create(db, secrets, &spec).inspect_err(|_| cleanup(&dir))
}

/// The instance folder a service owns, if its working directory is exactly `<root>/instances/<x>`.
/// Anything else (another place, a `..` trick, a nested folder) is not Orbit's to delete.
pub fn instance_dir(root: &Path, working_dir: &str) -> Option<std::path::PathBuf> {
    let wd = Path::new(working_dir);
    let ok = !wd.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) && wd
        .parent()
        .is_some_and(|p| plain(p) == plain(&root.join("instances")));
    ok.then(|| wd.to_path_buf())
}

fn plain(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Like `instance_dir`, but `None` while another service (a duplicated project's copy) runs from
/// the same folder: deleting one of them must not take the other's data.
pub fn owned_dir(
    root: &Path,
    spec: &ServiceSpec,
    all: &[ServiceSpec],
) -> Option<std::path::PathBuf> {
    let shared = all.iter().any(|s| {
        s.id != spec.id && plain(Path::new(&s.working_dir)) == plain(Path::new(&spec.working_dir))
    });
    instance_dir(root, &spec.working_dir).filter(|_| !shared)
}

/// Deletes the instance folder a service owns. A folder that is already gone is fine, and a
/// working directory that is not an owned instance folder is never touched.
pub fn remove_owned(root: &Path, working_dir: &str) -> std::io::Result<()> {
    let Some(dir) = instance_dir(root, working_dir) else {
        return Ok(());
    };
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Runs the engine's init program on a new data folder, or just makes the folder.
fn prepare(
    root: &Path,
    e: &Engine,
    dir: &Path,
    data: &Path,
    emit: &dyn Fn(&str, u64, u64),
) -> Result<(), String> {
    let Some(init) = e.init else {
        return std::fs::create_dir_all(data).map_err(|x| x.to_string());
    };
    emit("init", 0, 0);
    let log = dir.join("init.log");
    let out = std::fs::File::create(&log).map_err(|x| x.to_string())?;
    let err = out.try_clone().map_err(|x| x.to_string())?;
    let mut child = Command::new(install::engine_dir(root, e).join(init))
        .args(catalog::init_args(e, data))
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|x| format!("Could not run the setup program: {x}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|x| x.to_string())? {
            Some(st) if st.success() => return Ok(()),
            Some(_) => {
                let text = std::fs::read_to_string(&log).unwrap_or_default();
                let last = text
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("");
                return Err(format!("Setting up the database failed: {last}"));
            }
            None if started.elapsed() > Duration::from_secs(180) => {
                let _ = child.kill();
                return Err("Setting up the database took too long and was stopped.".into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_direct_child_of_the_instances_folder_is_owned() {
        let root = Path::new(r"C:\Users\u\AppData\Local\Techomato Orbit");
        let own = r"C:\Users\u\AppData\Local\Techomato Orbit\instances\db-1a2b3c4d";
        assert!(instance_dir(root, own).is_some());
        assert!(instance_dir(root, &own.to_uppercase()).is_some());
        for bad in [
            r"C:\Users\u\Documents",
            r"C:\Users\u\AppData\Local\Techomato Orbit\instances",
            r"C:\Users\u\AppData\Local\Techomato Orbit\instances\a\b",
            r"C:\Users\u\AppData\Local\Techomato Orbit\instances\..\..\..\Documents",
            r"C:\Users\u\AppData\Local\Techomato Orbit\engines\redis",
            "",
        ] {
            assert!(instance_dir(root, bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_folder_another_service_also_uses_is_not_owned() {
        use crate::models::service::fixtures::sample;
        let root = Path::new(r"C:\Orbit");
        let dir = r"C:\Orbit\instances\db-1a2b3c4d";
        let mut original = sample();
        original.id = "a".into();
        original.working_dir = dir.into();
        let mut copy = original.clone();
        copy.id = "b".into();
        copy.working_dir = dir.to_lowercase();

        assert!(owned_dir(root, &original, std::slice::from_ref(&original)).is_some());
        let both = [original.clone(), copy.clone()];
        assert!(owned_dir(root, &original, &both).is_none());
        assert!(owned_dir(root, &copy, &both).is_none());
    }

    #[test]
    fn removing_deletes_an_owned_folder_and_leaves_anything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let own = root.join("instances").join("db-1a2b3c4d");
        std::fs::create_dir_all(own.join("data")).unwrap();
        let other = root.join("documents");
        std::fs::create_dir_all(&other).unwrap();

        remove_owned(root, other.to_str().unwrap()).unwrap();
        assert!(other.exists());
        remove_owned(root, own.to_str().unwrap()).unwrap();
        assert!(!own.exists());
        // Already gone is not an error.
        remove_owned(root, own.to_str().unwrap()).unwrap();
    }
}
