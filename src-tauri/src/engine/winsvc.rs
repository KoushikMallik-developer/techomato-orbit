//! Keeps services that Windows runs (a database installed as a Windows service) in step with what
//! Windows says, so the card shows Running or Stopped even when someone used services.msc.

use crate::db::Db;
use crate::engine::supervisor::Supervisor;
use crate::repo::services as repo;
use crate::windows::scm;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Which Windows service runs each service's command, looked up once per command.
type Names = HashMap<String, ((String, Vec<String>), Option<String>)>;

fn tick(db: &Db, sup: &Supervisor, names: &mut Names) {
    let Ok(specs) = repo::list(db) else {
        return;
    };
    names.retain(|id, _| specs.iter().any(|s| &s.id == id));
    let mut listed = None;
    for s in &specs {
        let key = (s.executable.clone(), s.args.clone());
        if names.get(&s.id).map(|(k, _)| k) != Some(&key) {
            let all = listed.get_or_insert_with(scm::list);
            let name = all
                .iter()
                .find(|v| scm::runs(v, &s.executable, &s.args))
                .map(|v| v.name.clone());
            names.insert(s.id.clone(), (key, name));
        }
        if let Some((_, Some(name))) = names.get(&s.id) {
            sup.set_external(&s.id, scm::running_pid(name));
        }
    }
}

pub fn spawn_poller(db: Arc<Db>, sup: Supervisor) {
    std::thread::spawn(move || {
        let mut names = Names::new();
        loop {
            tick(&db, &sup, &mut names);
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::engine::detect::{spec_for, Found};
    use crate::engine::state::Status;
    use crate::secrets::InMemorySecretStore;
    use std::path::Path;

    fn card(db: &Db, exe: &str, args: Vec<String>) -> String {
        let f = Found {
            engine: "redis",
            name: "Redis",
            port: 1,
            pid: 1,
            created_at_ms: None,
            service: None,
        };
        let spec = spec_for(&f, Path::new(exe), args, None);
        repo::create(db, &InMemorySecretStore::default(), &spec)
            .unwrap()
            .id
    }

    #[test]
    fn a_card_follows_the_windows_service_that_runs_its_command() {
        let db = Arc::new(open_in_memory().unwrap());
        let sup = Supervisor::new(
            db.clone(),
            Arc::new(InMemorySecretStore::default()),
            std::path::PathBuf::new(),
            std::path::PathBuf::new(),
            Arc::new(|_| {}),
        );
        // Any running service with a process of its own; its state can be read without any rights.
        let Some(own) = scm::list().into_iter().find(|s| {
            s.pid.is_some() && !scm::is_shared_host(s) && scm::running_pid(&s.name).is_some()
        }) else {
            eprintln!("skipped: no service with a process of its own is running here");
            return;
        };
        let (exe, args) = scm::split_command(&own.command);
        let mine = card(&db, &exe, args);
        let other = card(&db, r"C:\nothing\x.exe", vec![]);

        let mut names = Names::new();
        tick(&db, &sup, &mut names);
        std::thread::sleep(Duration::from_millis(100));
        let stable_pid = scm::running_pid(&own.name);
        if stable_pid.is_none() {
            eprintln!("skipped: selected Windows service stopped while the test was running");
            return;
        }
        assert_eq!(sup.runtime(&mine).status, Status::Running);
        assert_eq!(sup.runtime(&mine).pid, stable_pid);
        assert_eq!(sup.runtime(&other).status, Status::Stopped);
        // Both lookups are remembered, so the next tick lists nothing.
        assert_eq!(names.len(), 2);
    }
}
