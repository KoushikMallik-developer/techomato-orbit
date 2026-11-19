//! Owns every running service: start, graceful-then-forced stop, restart policy, stale-record
//! recovery, and keep-running-on-quit. One watcher thread per running process.

use crate::db::Db;
use crate::engine::health::{next_health_status, run_once, HealthResult};
use crate::engine::logs::{pump, LogSink, Stream};
use crate::engine::preflight::{self, PreflightIssue};
use crate::engine::state::{next, Event, Status};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::{HealthCheck, RestartPolicy, ServiceSpec};
use crate::repo::{processes, services as repo};
use crate::secrets::SecretStore;
use crate::windows::console::send_graceful_with;
use crate::windows::job::{spawn, terminate_tree, Job, ManagedProcess, SpawnSpec};
use crate::windows::procs::{is_alive, ProcId};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What the UI shows for a service right now. Pushed as the `service://status` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRuntime {
    pub id: String,
    pub status: Status,
    pub pid: Option<u32>,
    pub parent_pid: Option<u32>,
    pub started_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub restart_count: u32,
    pub last_error: Option<String>,
}

impl ServiceRuntime {
    fn idle(id: &str) -> Self {
        Self {
            id: id.to_string(),
            status: Status::Stopped,
            pid: None,
            parent_pid: None,
            started_at_ms: None,
            exit_code: None,
            restart_count: 0,
            last_error: None,
        }
    }
}

type Emit = Arc<dyn Fn(&ServiceRuntime) + Send + Sync>;

struct ProcHandle {
    id: ProcId,
    /// `None` for a process re-adopted after an Orbit restart (the job is gone).
    job: Option<Arc<Job>>,
}

struct SlotState {
    rt: ServiceRuntime,
    proc: Option<ProcHandle>,
    stop_requested: bool,
}

struct Slot {
    state: Mutex<SlotState>,
    gone: Condvar,
}

struct Inner {
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
    logs_dir: PathBuf,
    helper: PathBuf,
    emit: Emit,
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    sinks: Mutex<HashMap<String, Arc<LogSink>>>,
}

#[derive(Clone)]
pub struct Supervisor(Arc<Inner>);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn conflict(msg: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Conflict, msg)
}

impl Supervisor {
    pub fn new(
        db: Arc<Db>,
        secrets: Arc<dyn SecretStore>,
        logs_dir: PathBuf,
        stop_helper: PathBuf,
        emit: Emit,
    ) -> Self {
        Self(Arc::new(Inner {
            db,
            secrets,
            logs_dir,
            helper: stop_helper,
            emit,
            slots: Mutex::default(),
            sinks: Mutex::default(),
        }))
    }

    // ---- queries -------------------------------------------------------------------------

    pub fn runtime(&self, id: &str) -> ServiceRuntime {
        self.0.slot(id).state.lock().unwrap().rt.clone()
    }

    /// Runtime for every configured service (idle ones included).
    pub fn all_runtime(&self) -> AppResult<Vec<ServiceRuntime>> {
        Ok(repo::list(&self.0.db)?
            .iter()
            .map(|s| self.runtime(&s.id))
            .collect())
    }

    pub fn logs(&self, id: &str) -> Arc<LogSink> {
        self.0.sink(id)
    }

    // ---- lifecycle -----------------------------------------------------------------------

    pub fn start(&self, id: &str) -> AppResult<()> {
        self.0.start_inner(id, true)
    }

    pub fn restart(&self, id: &str) -> AppResult<()> {
        let status = self.runtime(id).status;
        if matches!(
            status,
            Status::Running | Status::Starting | Status::Unresponsive
        ) {
            self.stop(id, false)?;
        }
        self.0.start_inner(id, false)
    }

    /// Graceful stop, then forced after the service's shutdown timeout. `force` skips the
    /// graceful step. Blocks until the process is gone.
    pub fn stop(&self, id: &str, force: bool) -> AppResult<()> {
        let inner = &self.0;
        let slot = inner.slot(id);
        let timeout = repo::get(&inner.db, id)
            .map(|s| Duration::from_secs(u64::from(s.shutdown_timeout_secs)))
            .unwrap_or(Duration::from_secs(10));

        let (target, rt) = {
            let mut st = slot.state.lock().unwrap();
            if st.rt.status == Status::Stopping && force {
                (st.proc.as_ref().map(|p| (p.id, p.job.clone())), None)
            } else {
                let to = next(st.rt.status, Event::Stop)
                    .map_err(|_| conflict("This service is not running."))?;
                st.rt.status = to;
                st.stop_requested = true;
                (
                    st.proc.as_ref().map(|p| (p.id, p.job.clone())),
                    Some(st.rt.clone()),
                )
            }
        };
        if let Some(rt) = rt {
            (inner.emit)(&rt);
        }
        let Some((pid_id, job)) = target else {
            return Ok(());
        };

        if !force {
            // A failed signal just means we fall through to the forced stop after the timeout.
            let _ = send_graceful_with(&inner.helper, pid_id.pid);
            if inner.wait_gone(&slot, timeout) {
                return Ok(());
            }
        }
        inner.terminate(&pid_id, job.as_deref());
        if inner.wait_gone(&slot, Duration::from_secs(10)) {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorKind::Os,
                "The service did not stop even after being terminated.",
            ))
        }
    }

    /// On Orbit quitting: stop every service except those marked keep-running.
    pub fn stop_all_on_quit(&self) {
        let ids: Vec<String> = self
            .0
            .slots
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.state.lock().unwrap().proc.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        let threads: Vec<_> = ids
            .into_iter()
            .filter(|id| {
                repo::get(&self.0.db, id)
                    .map(|s| !s.keep_running_on_quit)
                    .unwrap_or(true)
            })
            .map(|id| {
                let me = self.clone();
                std::thread::spawn(move || {
                    let _ = me.stop(&id, false);
                })
            })
            .collect();
        for t in threads {
            let _ = t.join();
        }
    }

    /// Re-adopts processes recorded by a previous Orbit run that are still the same process;
    /// deletes records whose process is gone or whose PID was reused. Never signals anything.
    pub fn adopt_on_startup(&self) {
        let inner = &self.0;
        let Ok(records) = processes::list(&inner.db) else {
            return;
        };
        for rec in records {
            if !is_alive(&rec.id) {
                let _ = processes::clear(&inner.db, &rec.service_id);
                processes::note(
                    &inner.db,
                    Some(&rec.service_id),
                    "stale",
                    "Removed a stale process record (the process is gone or its PID was reused).",
                );
                continue;
            }
            let slot = inner.slot(&rec.service_id);
            let rt = {
                let mut st = slot.state.lock().unwrap();
                st.proc = Some(ProcHandle {
                    id: rec.id,
                    job: None,
                });
                st.rt.status = Status::Running;
                st.rt.pid = Some(rec.id.pid);
                st.rt.parent_pid = None;
                st.rt.started_at_ms = Some(rec.id.created_at_ms);
                st.rt.clone()
            };
            (inner.emit)(&rt);
            processes::note(
                &inner.db,
                Some(&rec.service_id),
                "adopted",
                "Re-attached to a service that kept running while Orbit was closed.",
            );
            let inner = self.0.clone();
            std::thread::spawn(move || inner.monitor_adopted(&rec.service_id, rec.id));
        }
    }
}

impl Inner {
    fn slot(&self, id: &str) -> Arc<Slot> {
        self.slots
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_insert_with(|| {
                Arc::new(Slot {
                    state: Mutex::new(SlotState {
                        rt: ServiceRuntime::idle(id),
                        proc: None,
                        stop_requested: false,
                    }),
                    gone: Condvar::new(),
                })
            })
            .clone()
    }

    fn sink(&self, id: &str) -> Arc<LogSink> {
        self.sinks
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_insert_with(|| {
                Arc::new(LogSink::new(
                    self.logs_dir.clone(),
                    id,
                    5 * 1024 * 1024,
                    5,
                    2000,
                ))
            })
            .clone()
    }

    /// True once the slot has no live process, within `timeout`.
    fn wait_gone(&self, slot: &Slot, timeout: Duration) -> bool {
        let guard = slot.state.lock().unwrap();
        let (guard, _) = slot
            .gone
            .wait_timeout_while(guard, timeout, |s| s.proc.is_some())
            .unwrap();
        guard.proc.is_none()
    }

    fn terminate(&self, id: &ProcId, job: Option<&Job>) {
        match job {
            Some(job) => {
                let _ = terminate_tree(job);
            }
            // Re-adopted: no job handle, so kill the tree by PID (identity re-checked first).
            None if is_alive(id) => {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &id.pid.to_string(), "/T", "/F"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            None => {}
        }
    }

    fn note(&self, id: &str, kind: &str, msg: &str) {
        processes::note(&self.db, Some(id), kind, msg);
    }

    fn start_inner(self: &Arc<Self>, id: &str, reset_count: bool) -> AppResult<()> {
        let spec = repo::get(&self.db, id)?;
        if !spec.enabled {
            return Err(AppError::new(
                ErrorKind::Validation,
                "This service is disabled. Enable it before starting.",
            ));
        }
        let slot = self.slot(id);

        // Claim the service. Doing this under the slot lock, before anything slow, is what makes
        // simultaneous start requests launch exactly one process.
        let claimed = {
            let mut st = slot.state.lock().unwrap();
            let to = next(st.rt.status, Event::Start).map_err(|_| {
                conflict(format!("\"{}\" is already running or starting.", spec.name))
            })?;
            st.rt.status = to;
            st.stop_requested = false;
            st.rt.exit_code = None;
            st.rt.last_error = None;
            if reset_count {
                st.rt.restart_count = 0;
            }
            st.rt.clone()
        };
        (self.emit)(&claimed);

        let issues = preflight::check(&spec, &preflight::port_in_use);
        if !issues.is_empty() {
            let message = issues
                .iter()
                .map(PreflightIssue::message)
                .collect::<Vec<_>>()
                .join(" ");
            let kind = if issues
                .iter()
                .any(|i| matches!(i, PreflightIssue::PortInUse(_)))
            {
                ErrorKind::Conflict
            } else {
                ErrorKind::Validation
            };
            self.settle(&slot, Status::Stopped, None, Some(message.clone()));
            return Err(AppError::new(kind, message));
        }

        let (env, secret_values) = self.build_env(&spec);
        let sink = self.sink(id);
        sink.set_secrets(secret_values);

        let cwd = if spec.working_dir.is_empty() {
            std::env::current_dir().unwrap_or_default()
        } else {
            PathBuf::from(&spec.working_dir)
        };
        let launched = spawn(&SpawnSpec {
            exe: PathBuf::from(&spec.executable),
            args: spec.args.clone(),
            cwd,
            env,
            through_shell: spec.run_through_shell,
        });
        let ManagedProcess {
            id: proc_id,
            job,
            child,
            stdout,
            stderr,
        } = match launched {
            Ok(p) => p,
            Err(e) => {
                self.settle(&slot, Status::Failed, None, Some(e.message.clone()));
                self.note(id, "failed", &e.message);
                return Err(e);
            }
        };

        let started_at = now_ms();
        let _ = processes::save(&self.db, id, &proc_id, started_at as i64);
        self.note(id, "started", &format!("Started (pid {}).", proc_id.pid));
        let rt = {
            let mut st = slot.state.lock().unwrap();
            st.proc = Some(ProcHandle {
                id: proc_id,
                job: Some(Arc::new(job)),
            });
            st.rt.pid = Some(proc_id.pid);
            st.rt.parent_pid = Some(std::process::id());
            st.rt.started_at_ms = Some(started_at);
            st.rt.clone()
        };
        (self.emit)(&rt);

        let me = self.clone();
        let sid = id.to_string();
        std::thread::Builder::new()
            .name(format!("watch-{id}"))
            .spawn(move || me.watch(&sid, spec, child, stdout, stderr, sink))
            .map_err(|e| {
                AppError::new(ErrorKind::Os, format!("Could not watch the service: {e}"))
            })?;
        Ok(())
    }

    fn build_env(&self, spec: &ServiceSpec) -> (Vec<(String, String)>, Vec<String>) {
        let mut env = Vec::new();
        let mut secrets = Vec::new();
        for v in &spec.env {
            if v.is_secret {
                if let Ok(Some(value)) = self.secrets.get(&spec.id, &v.key) {
                    secrets.push(value.clone());
                    env.push((v.key.clone(), value));
                }
            } else if let Some(value) = &v.value {
                env.push((v.key.clone(), value.clone()));
            }
        }
        (env, secrets)
    }

    /// Put a service that never got going into `status` and publish it.
    fn settle(&self, slot: &Slot, status: Status, exit: Option<i32>, error: Option<String>) {
        let rt = {
            let mut st = slot.state.lock().unwrap();
            st.proc = None;
            st.rt.status = status;
            st.rt.pid = None;
            st.rt.parent_pid = None;
            st.rt.exit_code = exit;
            st.rt.last_error = error;
            st.rt.clone()
        };
        slot.gone.notify_all();
        (self.emit)(&rt);
    }

    fn watch(
        self: Arc<Self>,
        id: &str,
        spec: ServiceSpec,
        mut child: Child,
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
        sink: Arc<LogSink>,
    ) {
        let slot = self.slot(id);
        let mut pumps = Vec::new();
        if let Some(r) = stdout {
            let sink = sink.clone();
            pumps.push(std::thread::spawn(move || pump(r, Stream::Stdout, &sink)));
        }
        if let Some(r) = stderr {
            let sink = sink.clone();
            pumps.push(std::thread::spawn(move || pump(r, Stream::Stderr, &sink)));
        }

        let grace = Duration::from_millis(500)
            .min(Duration::from_secs(u64::from(spec.startup_timeout_secs)));
        let started = Instant::now();
        // With a health check, readiness comes from the probe thread, not from surviving a grace.
        let mut ready = spec.health.is_some();
        let done = Arc::new(AtomicBool::new(false));
        if let Some(check) = spec.health.clone() {
            let me = self.clone();
            let id = id.to_string();
            let done = done.clone();
            let startup = Duration::from_secs(u64::from(spec.startup_timeout_secs));
            std::thread::spawn(move || me.health_loop(&id, &check, startup, &done));
        }
        let exit = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {}
                Err(_) => break None,
            }
            if !ready && started.elapsed() >= grace {
                ready = true;
                let rt = {
                    let mut st = slot.state.lock().unwrap();
                    next(st.rt.status, Event::Ready).ok().map(|to| {
                        st.rt.status = to;
                        st.rt.clone()
                    })
                };
                if let Some(rt) = rt {
                    (self.emit)(&rt);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        };

        done.store(true, Ordering::SeqCst);

        // Give the pipes a moment to drain, but never wait on a grandchild that still holds them.
        let drain = Instant::now();
        while drain.elapsed() < Duration::from_millis(500) && pumps.iter().any(|p| !p.is_finished())
        {
            std::thread::sleep(Duration::from_millis(25));
        }
        sink.flush();

        let clean = exit == Some(0);
        let (rt, stop_requested) = {
            let mut st = slot.state.lock().unwrap();
            let stop_requested = st.stop_requested;
            st.proc = None;
            st.rt.pid = None;
            st.rt.parent_pid = None;
            st.rt.exit_code = exit;
            st.rt.status = next(st.rt.status, Event::Exited { clean }).unwrap_or(if clean {
                Status::Stopped
            } else {
                Status::Failed
            });
            if st.rt.status == Status::Failed && st.rt.last_error.is_none() {
                st.rt.last_error = Some(match exit {
                    Some(c) => format!("The process exited with code {c}."),
                    None => "The process ended unexpectedly.".to_string(),
                });
            }
            (st.rt.clone(), stop_requested)
        };
        slot.gone.notify_all();
        let _ = processes::clear(&self.db, id);
        self.note(
            id,
            if rt.status == Status::Failed {
                "crashed"
            } else {
                "stopped"
            },
            &format!("Exited with code {exit:?}."),
        );
        (self.emit)(&rt);

        if !stop_requested {
            self.maybe_restart(id, &spec.restart, clean);
        }
    }

    /// Probes until the service is ready (or the startup timeout ends it), then applies the
    /// 3-strikes rule every interval until the process is gone (`done`).
    fn health_loop(&self, id: &str, check: &HealthCheck, startup: Duration, done: &AtomicBool) {
        let slot = self.slot(id);
        let interval = Duration::from_secs(u64::from(match check {
            HealthCheck::Tcp { interval_secs, .. }
            | HealthCheck::Http { interval_secs, .. }
            | HealthCheck::Command { interval_secs, .. } => (*interval_secs).max(1),
        }));
        let sleep = |d: Duration| {
            let t = Instant::now();
            while t.elapsed() < d && !done.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        let publish = |change: &dyn Fn(&mut SlotState) -> bool| {
            let rt = {
                let mut st = slot.state.lock().unwrap();
                change(&mut st).then(|| st.rt.clone())
            };
            if let Some(rt) = rt {
                (self.emit)(&rt);
            }
        };

        let began = Instant::now();
        let mut failures = 0u32;
        let mut ready = false;
        while !done.load(Ordering::SeqCst) {
            let healthy = run_once(check) == HealthResult::Healthy;
            if done.load(Ordering::SeqCst) {
                return;
            }
            if !ready {
                if healthy {
                    ready = true;
                    publish(&|st| {
                        next(st.rt.status, Event::Ready)
                            .map(|to| st.rt.status = to)
                            .is_ok()
                    });
                } else if began.elapsed() >= startup {
                    let msg = format!(
                        "The health check did not pass within {}s, so the service was stopped.",
                        startup.as_secs()
                    );
                    let target = {
                        let mut st = slot.state.lock().unwrap();
                        st.rt.last_error = Some(msg.clone());
                        st.proc.as_ref().map(|p| (p.id, p.job.clone()))
                    };
                    self.note(id, "failed", &msg);
                    if let Some((pid, job)) = target {
                        self.terminate(&pid, job.as_deref());
                    }
                    return;
                }
                sleep(Duration::from_secs(1).min(interval));
                continue;
            }
            failures = if healthy { 0 } else { failures + 1 };
            publish(&|st| {
                let to = next_health_status(failures, st.rt.status);
                let changed = to != st.rt.status;
                st.rt.status = to;
                changed
            });
            sleep(interval);
        }
    }

    fn maybe_restart(self: &Arc<Self>, id: &str, policy: &RestartPolicy, clean: bool) {
        let (max, backoff) = match *policy {
            RestartPolicy::Never => return,
            RestartPolicy::OnFailure {
                max_retries,
                backoff_secs,
            } if !clean => (max_retries, backoff_secs),
            RestartPolicy::OnFailure { .. } => return,
            RestartPolicy::Always {
                max_retries,
                backoff_secs,
            } => (max_retries, backoff_secs),
        };
        let slot = self.slot(id);
        if slot.state.lock().unwrap().rt.restart_count >= max {
            return;
        }
        std::thread::sleep(Duration::from_secs(u64::from(backoff)));
        let rt = {
            let mut st = slot.state.lock().unwrap();
            // The user may have started or stopped it during the backoff.
            if st.stop_requested || !matches!(st.rt.status, Status::Failed | Status::Stopped) {
                return;
            }
            st.rt.restart_count += 1;
            st.rt.clone()
        };
        (self.emit)(&rt);
        self.note(
            id,
            "restart",
            &format!("Automatic restart {}.", rt.restart_count),
        );
        let _ = self.start_inner(id, false);
    }

    fn monitor_adopted(self: Arc<Self>, id: &str, proc_id: ProcId) {
        let slot = self.slot(id);
        while is_alive(&proc_id) {
            std::thread::sleep(Duration::from_millis(500));
        }
        let rt = {
            let mut st = slot.state.lock().unwrap();
            st.proc = None;
            st.rt.pid = None;
            st.rt.status = Status::Stopped; // exit code is unknowable for an adopted process
            st.rt.clone()
        };
        slot.gone.notify_all();
        let _ = processes::clear(&self.db, id);
        self.note(id, "stopped", "The re-attached service ended.");
        (self.emit)(&rt);
    }
}
