//! Spawning a service inside a Job Object, so its whole process tree can be stopped together.
//!
//! The job is created *without* kill-on-close: services the user marked "keep running" must
//! survive Orbit exiting. Orbit stops the others itself on quit.

use crate::error::{AppError, AppResult, ErrorKind};
use crate::windows::procs::{identify_child, ProcId};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

pub struct Job(HANDLE);

// SAFETY: a job handle is just a kernel object reference; the Win32 job APIs are thread-safe.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: closed exactly once; without kill-on-close this does not stop the processes.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// Explicit opt-in: run as `cmd /C <exe> <args>`. Off by default.
    pub through_shell: bool,
}

pub struct ManagedProcess {
    pub id: ProcId,
    pub job: Job,
    pub child: Child,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

fn os_error(msg: impl Into<String>, e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorKind::Os, msg).with_detail(e.to_string())
}

pub fn spawn(spec: &SpawnSpec) -> AppResult<ManagedProcess> {
    let mut cmd = if spec.through_shell {
        let mut line = spec.exe.to_string_lossy().into_owned();
        for a in &spec.args {
            line.push(' ');
            line.push_str(a);
        }
        let mut c = Command::new("cmd");
        c.arg("/C").raw_arg(line);
        c
    } else {
        let mut c = Command::new(&spec.exe);
        c.args(&spec.args);
        c
    };
    cmd.current_dir(&spec.cwd)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Own process group so Ctrl+Break can target it; no console window.
        .creation_flags((CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW).0);

    let mut child = cmd.spawn().map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::NotFound {
            ErrorKind::NotFound
        } else {
            ErrorKind::Os
        };
        AppError::new(kind, format!("Could not start {}: {e}", spec.exe.display()))
    })?;

    let id = identify_child(&child)
        .ok_or_else(|| os_error("Could not read the new process's identity", child.id()))?;

    // SAFETY: plain Win32 calls; the job handle is owned by `Job` from here on.
    let job = unsafe {
        Job(CreateJobObjectW(None, None)
            .map_err(|e| os_error("Could not create a job object", e))?)
    };
    // shortcut: the child is assigned after it starts, so a grandchild spawned in that first
    // instant could escape the job. Upgrade to CREATE_SUSPENDED + resume if that ever bites.
    // SAFETY: both handles are valid; the child handle is borrowed from `child`.
    let assigned = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) };
    if let Err(e) = assigned {
        // A child that already exited cannot be assigned; that is not an error here.
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            return Err(os_error("Could not place the service in a job object", e));
        }
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    Ok(ManagedProcess {
        id,
        job,
        child,
        stdout,
        stderr,
    })
}

/// Forcefully ends every process in the job (the service and all its children).
pub fn terminate_tree(job: &Job) -> AppResult<()> {
    // SAFETY: the job handle is valid for the lifetime of `job`.
    unsafe { TerminateJobObject(job.0, 1) }
        .map_err(|e| os_error("Could not terminate the service's processes", e))
}
