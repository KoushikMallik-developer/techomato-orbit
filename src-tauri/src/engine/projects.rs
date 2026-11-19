//! Starting and stopping a whole project in dependency order.

use crate::db::Db;
use crate::engine::deps::{self, Cycle};
use crate::engine::state::Status;
use crate::engine::supervisor::Supervisor;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::project::{Edge, OnFailure};
use crate::models::service::ServiceSpec;
use crate::repo::{projects, services};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Outcome {
    Started,
    AlreadyRunning,
    Stopped,
    NotRunning,
    Failed { message: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReport {
    pub service_id: String,
    pub name: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRunReport {
    pub project_id: String,
    /// In the order they were handled.
    pub services: Vec<ServiceReport>,
    /// Things that went ahead despite a problem (a dependency set to "continue" that failed).
    pub warnings: Vec<String>,
}

/// The `project://progress` event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub project_id: String,
    pub service_id: String,
    /// `starting`, `ready`, `failed`, `skipped`, `alreadyRunning`, `stopping`, `stopped`.
    pub step: String,
}

pub type Emit<'a> = &'a (dyn Fn(&Progress) + Sync);

struct Plan {
    /// Dependencies first.
    levels: Vec<Vec<String>>,
    specs: HashMap<String, ServiceSpec>,
    edges: Vec<Edge>,
}

/// Builds the ordered plan for `ids`. With `with_deps`, everything they depend on (in this
/// project or another) is pulled in; otherwise dependencies outside `ids` are ignored.
fn plan(db: &Db, ids: &[String], with_deps: bool) -> AppResult<Plan> {
    let all: HashMap<String, ServiceSpec> = services::list(db)?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    let edges = projects::all_edges(db)?;
    let mut wanted: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = ids.to_vec();
    while let Some(id) = queue.pop() {
        if !all.contains_key(&id) || !seen.insert(id.clone()) {
            continue;
        }
        wanted.push(id.clone());
        if with_deps {
            queue.extend(
                edges
                    .iter()
                    .filter(|e| e.service_id == id)
                    .map(|e| e.depends_on_id.clone()),
            );
        }
    }
    let nodes: Vec<(String, Vec<String>)> = {
        let mut v: Vec<_> = wanted
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    edges
                        .iter()
                        .filter(|e| &e.service_id == id)
                        .map(|e| e.depends_on_id.clone())
                        .collect(),
                )
            })
            .collect();
        // Stable, readable order: by name.
        v.sort_by_key(|(id, _)| all[id].name.to_lowercase());
        v
    };
    let levels = deps::order(&nodes).map_err(|Cycle(path)| {
        let named: Vec<&str> = path
            .iter()
            .map(|id| all.get(id).map_or(id.as_str(), |s| s.name.as_str()))
            .collect();
        AppError::new(
            ErrorKind::Validation,
            format!(
                "These services depend on each other in a loop: {}.",
                named.join(" \u{2192} ")
            ),
        )
    })?;
    Ok(Plan {
        levels,
        specs: all,
        edges,
    })
}

fn project_service_ids(db: &Db, project_id: &str) -> AppResult<Vec<String>> {
    projects::get(db, project_id)?;
    Ok(services::list(db)?
        .into_iter()
        .filter(|s| s.project_id.as_deref() == Some(project_id))
        .map(|s| s.id)
        .collect())
}

fn say(emit: Emit, project_id: &str, service_id: &str, step: &str) {
    emit(&Progress {
        project_id: project_id.into(),
        service_id: service_id.into(),
        step: step.into(),
    });
}

/// Waits for `Running` (which already includes a passing health check when there is one).
fn wait_ready(sup: &Supervisor, spec: &ServiceSpec) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(u64::from(spec.startup_timeout_secs) + 2);
    loop {
        let rt = sup.runtime(&spec.id);
        match rt.status {
            Status::Running => return Ok(()),
            Status::Starting => {}
            Status::Failed | Status::Stopped | Status::Unknown | Status::Stopping => {
                return Err(rt.last_error.unwrap_or_else(|| match rt.exit_code {
                    Some(code) => format!("It exited with code {code} before it was ready."),
                    None => "It stopped before it was ready.".into(),
                }));
            }
            Status::Unresponsive => return Err("Its health check is failing.".into()),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "It did not become ready within {} seconds.",
                spec.startup_timeout_secs
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn start_one(sup: &Supervisor, spec: &ServiceSpec) -> Outcome {
    match sup.runtime(&spec.id).status {
        Status::Running | Status::Unresponsive => return Outcome::AlreadyRunning,
        Status::Starting => {
            return match wait_ready(sup, spec) {
                Ok(()) => Outcome::AlreadyRunning,
                Err(message) => Outcome::Failed { message },
            }
        }
        _ => {}
    }
    if let Err(e) = sup.start(&spec.id) {
        return Outcome::Failed { message: e.message };
    }
    match wait_ready(sup, spec) {
        Ok(()) => Outcome::Started,
        Err(message) => Outcome::Failed { message },
    }
}

/// Starts the project's services (or `selection` plus everything they depend on) level by level.
/// Services in one level start together; each counts as ready when it is `Running` within its
/// startup timeout. A dependency that does not come up either skips its dependants (`Abort`) or
/// lets them start with a warning (`Continue`).
pub fn start_project(
    sup: &Supervisor,
    db: &Db,
    project_id: &str,
    selection: Option<Vec<String>>,
    emit: Emit,
) -> AppResult<ProjectRunReport> {
    let ids = match selection {
        Some(s) => s,
        None => project_service_ids(db, project_id)?,
    };
    let plan = plan(db, &ids, true)?;
    let mut outcomes: HashMap<String, Outcome> = HashMap::new();
    let mut report = Vec::new();
    let mut warnings = Vec::new();
    let name = |id: &str| plan.specs[id].name.clone();

    for level in &plan.levels {
        let mut go: Vec<&String> = Vec::new();
        for id in level {
            let mut skip: Option<String> = None;
            for e in plan.edges.iter().filter(|e| &e.service_id == id) {
                let Some(dep) = outcomes.get(&e.depends_on_id) else {
                    continue;
                };
                let problem = match dep {
                    Outcome::Failed { .. } => Some("did not start"),
                    Outcome::Skipped { .. } => Some("was skipped"),
                    _ => None,
                };
                if let Some(problem) = problem {
                    let dep_name = name(&e.depends_on_id);
                    match e.on_failure {
                        OnFailure::Abort => {
                            skip.get_or_insert(format!("{dep_name} {problem}."));
                        }
                        OnFailure::Continue => warnings.push(format!(
                            "{} was started even though {dep_name} {problem}.",
                            name(id)
                        )),
                    }
                }
            }
            match skip {
                Some(reason) => {
                    say(emit, project_id, id, "skipped");
                    outcomes.insert(id.clone(), Outcome::Skipped { reason });
                }
                None => go.push(id),
            }
        }
        let results: Vec<(String, Outcome)> = std::thread::scope(|scope| {
            let handles: Vec<_> = go
                .iter()
                .map(|id| {
                    let spec = &plan.specs[*id];
                    scope.spawn(move || {
                        say(emit, project_id, &spec.id, "starting");
                        let out = start_one(sup, spec);
                        let step = match out {
                            Outcome::Started => "ready",
                            Outcome::AlreadyRunning => "alreadyRunning",
                            _ => "failed",
                        };
                        say(emit, project_id, &spec.id, step);
                        (spec.id.clone(), out)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("start thread"))
                .collect()
        });
        outcomes.extend(results);
        for id in level {
            report.push(ServiceReport {
                service_id: id.clone(),
                name: name(id),
                outcome: outcomes[id].clone(),
            });
        }
    }
    Ok(ProjectRunReport {
        project_id: project_id.into(),
        services: report,
        warnings,
    })
}

/// Stops the project's running services, dependants first. From quit, services marked
/// keep-running-on-quit are left alone.
pub fn stop_project(
    sup: &Supervisor,
    db: &Db,
    project_id: &str,
    from_quit: bool,
    emit: Emit,
) -> AppResult<ProjectRunReport> {
    let ids = project_service_ids(db, project_id)?;
    let plan = plan(db, &ids, false)?;
    let mut report = Vec::new();
    for level in deps::stop_order(&plan.levels) {
        let mut stopping: Vec<&ServiceSpec> = Vec::new();
        for id in &level {
            let spec = &plan.specs[id];
            let running = matches!(
                sup.runtime(id).status,
                Status::Running | Status::Starting | Status::Unresponsive | Status::Stopping
            );
            let outcome = if !running {
                Some(Outcome::NotRunning)
            } else if from_quit && spec.keep_running_on_quit {
                Some(Outcome::Skipped {
                    reason: "It is set to keep running when Orbit quits.".into(),
                })
            } else {
                None
            };
            match outcome {
                Some(outcome) => report.push(ServiceReport {
                    service_id: id.clone(),
                    name: spec.name.clone(),
                    outcome,
                }),
                None => stopping.push(spec),
            }
        }
        let results: Vec<ServiceReport> = std::thread::scope(|scope| {
            let handles: Vec<_> = stopping
                .iter()
                .map(|spec| {
                    scope.spawn(move || {
                        say(emit, project_id, &spec.id, "stopping");
                        let outcome = match sup.stop(&spec.id, false) {
                            Ok(()) => Outcome::Stopped,
                            Err(e) => Outcome::Failed { message: e.message },
                        };
                        say(
                            emit,
                            project_id,
                            &spec.id,
                            if outcome == Outcome::Stopped {
                                "stopped"
                            } else {
                                "failed"
                            },
                        );
                        ServiceReport {
                            service_id: spec.id.clone(),
                            name: spec.name.clone(),
                            outcome,
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("stop thread"))
                .collect()
        });
        report.extend(results);
    }
    Ok(ProjectRunReport {
        project_id: project_id.into(),
        services: report,
        warnings: vec![],
    })
}

pub fn restart_project(
    sup: &Supervisor,
    db: &Db,
    project_id: &str,
    emit: Emit,
) -> AppResult<ProjectRunReport> {
    stop_project(sup, db, project_id, false, emit)?;
    start_project(sup, db, project_id, None, emit)
}
