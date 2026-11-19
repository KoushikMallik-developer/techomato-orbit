//! Projects and service dependencies.

use crate::db::Db;
use crate::engine::deps::{self, Cycle};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::project::{Dep, Edge, OnFailure, Project};
use crate::repo::services;
use crate::secrets::SecretStore;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn not_found(what: &str, id: &str) -> AppError {
    AppError::new(ErrorKind::NotFound, format!("No {what} with id {id}."))
}

fn row_to_project(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        icon: r.get(3)?,
        root_dir: r.get(4)?,
        archived: r.get::<_, i64>(5)? != 0,
    })
}

const COLS: &str = "id, name, description, icon, root_dir, archived";

pub fn create(db: &Db, p: &Project) -> AppResult<Project> {
    p.validate()?;
    let id = uuid::Uuid::new_v4().to_string();
    db.0.lock().unwrap().execute(
        "INSERT INTO projects (id, name, description, icon, root_dir, archived, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            p.name,
            p.description,
            p.icon,
            p.root_dir,
            p.archived as i64,
            now_ms()
        ],
    )?;
    get(db, &id)
}

pub fn update(db: &Db, p: &Project) -> AppResult<Project> {
    p.validate()?;
    let changed = db.0.lock().unwrap().execute(
        "UPDATE projects SET name=?2, description=?3, icon=?4, root_dir=?5, archived=?6 WHERE id=?1",
        params![p.id, p.name, p.description, p.icon, p.root_dir, p.archived as i64],
    )?;
    if changed == 0 {
        return Err(not_found("project", &p.id));
    }
    get(db, &p.id)
}

pub fn get(db: &Db, id: &str) -> AppResult<Project> {
    db.0.lock()
        .unwrap()
        .query_row(
            &format!("SELECT {COLS} FROM projects WHERE id = ?1"),
            [id],
            row_to_project,
        )
        .optional()?
        .ok_or_else(|| not_found("project", id))
}

/// Active projects first, then archived; each group by name.
pub fn list(db: &Db) -> AppResult<Vec<Project>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM projects ORDER BY archived, name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], row_to_project)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn archive(db: &Db, id: &str, archived: bool) -> AppResult<()> {
    let changed = db.0.lock().unwrap().execute(
        "UPDATE projects SET archived = ?2 WHERE id = ?1",
        params![id, archived as i64],
    )?;
    if changed == 0 {
        return Err(not_found("project", id));
    }
    Ok(())
}

/// Removes the project only; its services stay and simply lose their project.
pub fn delete(db: &Db, id: &str) -> AppResult<()> {
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE services SET project_id = NULL WHERE project_id = ?1",
        [id],
    )?;
    let removed = tx.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    if removed == 0 {
        return Err(not_found("project", id));
    }
    tx.commit()?;
    Ok(())
}

/// Copies the project, its services (with secrets) and the edges between them.
/// Edges to services outside the project are kept pointing at the originals.
pub fn duplicate(db: &Db, secrets: &dyn SecretStore, id: &str) -> AppResult<Project> {
    let original = get(db, id)?;
    let copy = create(
        db,
        &Project {
            id: String::new(),
            name: format!("{} (copy)", original.name),
            archived: false,
            ..original.clone()
        },
    )?;
    let mut created: Vec<String> = Vec::new();
    let result = (|| -> AppResult<()> {
        let mut map: HashMap<String, String> = HashMap::new();
        for spec in services::list(db)?
            .into_iter()
            .filter(|s| s.project_id.as_deref() == Some(id))
        {
            let old_id = spec.id.clone();
            let mut new_spec = spec;
            new_spec.id = String::new();
            new_spec.project_id = Some(copy.id.clone());
            for v in new_spec.env.iter_mut().filter(|v| v.is_secret) {
                v.value = secrets.get(&old_id, &v.key)?;
            }
            let made = services::create(db, secrets, &new_spec)?;
            created.push(made.id.clone());
            map.insert(old_id, made.id);
        }
        for (old, new) in &map {
            let deps: Vec<Dep> = dependencies(db, old)?
                .into_iter()
                .map(|d| Dep {
                    depends_on_id: map
                        .get(&d.depends_on_id)
                        .cloned()
                        .unwrap_or(d.depends_on_id),
                    on_failure: d.on_failure,
                })
                .collect();
            if !deps.is_empty() {
                set_dependencies(db, new, deps)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        for sid in &created {
            let _ = services::delete(db, secrets, sid);
        }
        let _ = delete(db, &copy.id);
        return Err(e);
    }
    Ok(copy)
}

fn on_failure_to_str(o: OnFailure) -> &'static str {
    match o {
        OnFailure::Abort => "abort",
        OnFailure::Continue => "continue",
    }
}

fn on_failure_from_str(s: &str) -> OnFailure {
    if s == "continue" {
        OnFailure::Continue
    } else {
        OnFailure::Abort
    }
}

fn edges_with(conn: &Connection) -> AppResult<Vec<Edge>> {
    let mut stmt = conn
        .prepare("SELECT service_id, depends_on_id, on_failure FROM service_deps ORDER BY rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok(Edge {
            service_id: r.get(0)?,
            depends_on_id: r.get(1)?,
            on_failure: on_failure_from_str(&r.get::<_, String>(2)?),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Replaces the dependencies of `service_id`. Rejected with a `Validation` error naming the
/// cycle if they would create one; the previous edges are then left as they were.
pub fn set_dependencies(db: &Db, service_id: &str, deps: Vec<Dep>) -> AppResult<()> {
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction()?;
    let names: HashMap<String, String> = {
        let mut stmt = tx.prepare("SELECT id, name FROM services")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    if !names.contains_key(service_id) {
        return Err(not_found("service", service_id));
    }
    let mut seen = HashSet::new();
    let deps: Vec<Dep> = deps
        .into_iter()
        .filter(|d| seen.insert(d.depends_on_id.clone()))
        .collect();
    if let Some(missing) = deps.iter().find(|d| !names.contains_key(&d.depends_on_id)) {
        return Err(AppError::new(
            ErrorKind::Validation,
            format!(
                "A dependency refers to a service that does not exist ({}).",
                missing.depends_on_id
            ),
        ));
    }
    // Check the graph as it would be after the change.
    let mut graph: HashMap<String, Vec<String>> =
        names.keys().map(|k| (k.clone(), vec![])).collect();
    for e in edges_with(&tx)?
        .into_iter()
        .filter(|e| e.service_id != service_id)
    {
        graph.entry(e.service_id).or_default().push(e.depends_on_id);
    }
    graph.insert(
        service_id.to_string(),
        deps.iter().map(|d| d.depends_on_id.clone()).collect(),
    );
    let mut nodes: Vec<(String, Vec<String>)> = graph.into_iter().collect();
    nodes.sort();
    if let Err(Cycle(path)) = deps::order(&nodes) {
        let named: Vec<&str> = path
            .iter()
            .map(|id| names.get(id).map_or(id.as_str(), String::as_str))
            .collect();
        return Err(AppError::new(
            ErrorKind::Validation,
            format!(
                "Those dependencies form a loop: {}.",
                named.join(" \u{2192} ")
            ),
        ));
    }
    tx.execute(
        "DELETE FROM service_deps WHERE service_id = ?1",
        [service_id],
    )?;
    for d in &deps {
        tx.execute(
            "INSERT INTO service_deps (service_id, depends_on_id, on_failure) VALUES (?1,?2,?3)",
            params![service_id, d.depends_on_id, on_failure_to_str(d.on_failure)],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn dependencies(db: &Db, service_id: &str) -> AppResult<Vec<Dep>> {
    Ok(all_edges(db)?
        .into_iter()
        .filter(|e| e.service_id == service_id)
        .map(|e| Dep {
            depends_on_id: e.depends_on_id,
            on_failure: e.on_failure,
        })
        .collect())
}

pub fn all_edges(db: &Db) -> AppResult<Vec<Edge>> {
    edges_with(&db.0.lock().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::error::ErrorKind;
    use crate::models::project::OnFailure;
    use crate::models::service::fixtures::sample;
    use crate::secrets::InMemorySecretStore;

    fn project(name: &str) -> Project {
        Project {
            id: String::new(),
            name: name.into(),
            description: "d".into(),
            icon: "🍅".into(),
            root_dir: "C:/work".into(),
            archived: false,
        }
    }
    fn svc(db: &Db, s: &InMemorySecretStore, name: &str, project: Option<&str>) -> String {
        let mut spec = sample();
        spec.name = name.into();
        spec.port = None;
        spec.project_id = project.map(Into::into);
        services::create(db, s, &spec).unwrap().id
    }
    fn dep(on: &str) -> Dep {
        Dep {
            depends_on_id: on.into(),
            on_failure: OnFailure::Abort,
        }
    }

    #[test]
    fn a_project_round_trips_and_validates() {
        let db = open_in_memory().unwrap();
        let p = create(&db, &project("Shop")).unwrap();
        assert!(!p.id.is_empty());
        assert_eq!(get(&db, &p.id).unwrap(), p);
        let mut edited = p.clone();
        edited.name = "Shop 2".into();
        update(&db, &edited).unwrap();
        assert_eq!(get(&db, &p.id).unwrap().name, "Shop 2");
        assert_eq!(
            create(&db, &project("  ")).unwrap_err().kind,
            ErrorKind::Validation
        );
        assert_eq!(get(&db, "nope").unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn archived_projects_sort_after_active_ones() {
        let db = open_in_memory().unwrap();
        let a = create(&db, &project("Alpha")).unwrap();
        create(&db, &project("beta")).unwrap();
        archive(&db, &a.id, true).unwrap();
        let names: Vec<_> = list(&db).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["beta", "Alpha"]);
        archive(&db, &a.id, false).unwrap();
        assert!(!get(&db, &a.id).unwrap().archived);
    }

    #[test]
    fn deleting_a_project_keeps_its_services() {
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let p = create(&db, &project("Shop")).unwrap();
        let id = svc(&db, &s, "api", Some(&p.id));
        delete(&db, &p.id).unwrap();
        assert_eq!(services::get(&db, &id).unwrap().project_id, None);
        assert_eq!(delete(&db, &p.id).unwrap_err().kind, ErrorKind::NotFound);
    }

    #[test]
    fn dependencies_are_replaced_as_a_set() {
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let (a, b, c) = (
            svc(&db, &s, "a", None),
            svc(&db, &s, "b", None),
            svc(&db, &s, "c", None),
        );
        set_dependencies(&db, &c, vec![dep(&a), dep(&b)]).unwrap();
        assert_eq!(dependencies(&db, &c).unwrap().len(), 2);
        set_dependencies(&db, &c, vec![dep(&b)]).unwrap();
        assert_eq!(dependencies(&db, &c).unwrap(), vec![dep(&b)]);
        assert_eq!(all_edges(&db).unwrap().len(), 1);
    }

    #[test]
    fn cycles_are_rejected_naming_the_services_and_leave_old_edges_intact() {
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let (a, b, c) = (
            svc(&db, &s, "alpha", None),
            svc(&db, &s, "bravo", None),
            svc(&db, &s, "charlie", None),
        );
        // alpha <- bravo <- charlie
        set_dependencies(&db, &b, vec![dep(&a)]).unwrap();
        set_dependencies(&db, &c, vec![dep(&b)]).unwrap();
        let before = all_edges(&db).unwrap();

        let self_dep = set_dependencies(&db, &a, vec![dep(&a)]).unwrap_err();
        assert_eq!(self_dep.kind, ErrorKind::Validation);
        let two = set_dependencies(&db, &a, vec![dep(&b)]).unwrap_err();
        assert_eq!(two.kind, ErrorKind::Validation);
        assert!(
            two.message.contains("alpha") && two.message.contains("bravo"),
            "{}",
            two.message
        );
        let three = set_dependencies(&db, &a, vec![dep(&c)]).unwrap_err();
        for name in ["alpha", "bravo", "charlie"] {
            assert!(three.message.contains(name), "{}", three.message);
        }
        assert_eq!(all_edges(&db).unwrap(), before);
        assert!(dependencies(&db, &a).unwrap().is_empty());
    }

    #[test]
    fn a_dependency_on_a_missing_service_is_rejected() {
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let a = svc(&db, &s, "a", None);
        assert_eq!(
            set_dependencies(&db, &a, vec![dep("ghost")])
                .unwrap_err()
                .kind,
            ErrorKind::Validation
        );
    }

    #[test]
    fn deleting_a_service_removes_its_edges() {
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let (a, b) = (svc(&db, &s, "a", None), svc(&db, &s, "b", None));
        set_dependencies(&db, &b, vec![dep(&a)]).unwrap();
        services::delete(&db, &s, &a).unwrap();
        assert!(all_edges(&db).unwrap().is_empty());
    }

    #[test]
    fn duplicating_copies_services_secrets_and_inner_edges_with_new_ids() {
        use crate::models::service::EnvVar;
        use crate::secrets::SecretStore;
        let db = open_in_memory().unwrap();
        let s = InMemorySecretStore::default();
        let p = create(&db, &project("Shop")).unwrap();
        let outside = svc(&db, &s, "shared-db", None);
        let mut spec = sample();
        spec.name = "api".into();
        spec.port = None;
        spec.project_id = Some(p.id.clone());
        spec.env = vec![EnvVar {
            key: "TOKEN".into(),
            value: Some("hunter2".into()),
            is_secret: true,
        }];
        let api = services::create(&db, &s, &spec).unwrap().id;
        let web = svc(&db, &s, "web", Some(&p.id));
        set_dependencies(&db, &web, vec![dep(&api), dep(&outside)]).unwrap();

        let copy = duplicate(&db, &s, &p.id).unwrap();
        assert_ne!(copy.id, p.id);
        assert!(copy.name.contains("Shop") && copy.name.contains("copy"));

        let all = services::list(&db).unwrap();
        let mine: Vec<_> = all
            .iter()
            .filter(|x| x.project_id.as_deref() == Some(&copy.id))
            .collect();
        assert_eq!(mine.len(), 2);
        let new_api = mine.iter().find(|x| x.name == "api").unwrap();
        let new_web = mine.iter().find(|x| x.name == "web").unwrap();
        assert_ne!(new_api.id, api);
        assert_eq!(
            s.get(&new_api.id, "TOKEN").unwrap().as_deref(),
            Some("hunter2")
        );

        let deps_of_new_web = dependencies(&db, &new_web.id).unwrap();
        let targets: Vec<_> = deps_of_new_web
            .iter()
            .map(|d| d.depends_on_id.as_str())
            .collect();
        assert!(
            targets.contains(&new_api.id.as_str()),
            "inner edge points at the copy"
        );
        assert!(
            targets.contains(&outside.as_str()),
            "outside edge keeps the original"
        );
        // The original is untouched.
        assert_eq!(dependencies(&db, &web).unwrap().len(), 2);
        assert_eq!(
            services::get(&db, &api).unwrap().project_id.as_deref(),
            Some(p.id.as_str())
        );
    }
}
