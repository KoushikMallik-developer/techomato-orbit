//! Checks run before a service is started, so a bad configuration produces a plain-language
//! message instead of a failed spawn.

use crate::models::service::ServiceSpec;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightIssue {
    MissingExecutable(String),
    InvalidWorkingDir(String),
    PortInUse(u16),
}

impl PreflightIssue {
    pub fn message(&self) -> String {
        match self {
            Self::MissingExecutable(e) => format!(
                "Could not find the executable \"{e}\". Check the path, or that it is installed and on PATH."
            ),
            Self::InvalidWorkingDir(d) => {
                format!("The working directory \"{d}\" does not exist or is not a folder.")
            }
            Self::PortInUse(p) => format!(
                "Port {p} is already in use by another process. Orbit will not stop it for you; free the port or change this service's port."
            ),
        }
    }
}

/// True if something is already listening on `port`.
pub fn port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_err()
}

fn resolve_executable(exe: &str) -> Option<PathBuf> {
    let has_dir = exe.contains(['/', '\\']) || Path::new(exe).is_absolute();
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .map(str::to_string)
        .collect();
    let candidates = |base: PathBuf| -> Option<PathBuf> {
        if base.is_file() {
            return Some(base);
        }
        exts.iter().find_map(|e| {
            let mut s = base.clone().into_os_string();
            s.push(e);
            let p = PathBuf::from(s);
            p.is_file().then_some(p)
        })
    };
    if has_dir {
        return candidates(PathBuf::from(exe));
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| candidates(dir.join(exe)))
}

pub fn check(spec: &ServiceSpec, port_busy: &dyn Fn(u16) -> bool) -> Vec<PreflightIssue> {
    let mut issues = Vec::new();
    if !spec.run_through_shell && resolve_executable(&spec.executable).is_none() {
        issues.push(PreflightIssue::MissingExecutable(spec.executable.clone()));
    }
    if !spec.working_dir.is_empty() && !Path::new(&spec.working_dir).is_dir() {
        issues.push(PreflightIssue::InvalidWorkingDir(spec.working_dir.clone()));
    }
    if let Some(port) = spec.port {
        if port_busy(port) {
            issues.push(PreflightIssue::PortInUse(port));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::service::fixtures::sample;

    fn issues(
        mutate: impl FnOnce(&mut crate::models::service::ServiceSpec),
        busy: &[u16],
    ) -> Vec<PreflightIssue> {
        let mut s = sample();
        s.executable = "cmd".into();
        s.working_dir = std::env::temp_dir().to_string_lossy().into_owned();
        s.port = None;
        mutate(&mut s);
        let busy = busy.to_vec();
        check(&s, &move |p| busy.contains(&p))
    }

    #[test]
    fn a_healthy_spec_has_no_issues() {
        assert!(issues(|_| {}, &[]).is_empty());
    }

    #[test]
    fn a_bare_executable_name_is_resolved_through_path() {
        assert!(issues(|s| s.executable = "cmd".into(), &[]).is_empty());
        assert!(issues(|s| s.executable = "cmd.exe".into(), &[]).is_empty());
    }

    #[test]
    fn a_missing_executable_is_reported() {
        let found = issues(
            |s| s.executable = "definitely-not-installed-xyz".into(),
            &[],
        );
        assert_eq!(
            found,
            [PreflightIssue::MissingExecutable(
                "definitely-not-installed-xyz".into()
            )]
        );
        assert!(found[0].message().contains("definitely-not-installed-xyz"));
    }

    #[test]
    fn an_absolute_path_to_nothing_is_reported() {
        let found = issues(|s| s.executable = "C:/no/such/dir/app.exe".into(), &[]);
        assert!(matches!(found[0], PreflightIssue::MissingExecutable(_)));
    }

    #[test]
    fn a_file_given_as_working_directory_is_reported() {
        let file = std::env::current_exe().unwrap();
        let found = issues(|s| s.working_dir = file.to_string_lossy().into_owned(), &[]);
        assert!(matches!(found[0], PreflightIssue::InvalidWorkingDir(_)));
    }

    #[test]
    fn a_missing_working_directory_is_reported() {
        let found = issues(|s| s.working_dir = "C:/no/such/dir".into(), &[]);
        assert!(matches!(found[0], PreflightIssue::InvalidWorkingDir(_)));
    }

    #[test]
    fn an_occupied_port_is_reported() {
        let found = issues(|s| s.port = Some(8123), &[8123]);
        assert_eq!(found, [PreflightIssue::PortInUse(8123)]);
        assert!(found[0].message().contains("8123"));
        assert!(issues(|s| s.port = Some(8124), &[8123]).is_empty());
    }

    #[test]
    fn the_real_port_probe_sees_a_listener() {
        let l = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(port_in_use(port));
        drop(l);
        assert!(!port_in_use(port));
    }

    #[test]
    fn shell_mode_skips_the_executable_check() {
        let found = issues(
            |s| {
                s.executable = "echo".into();
                s.run_through_shell = true;
            },
            &[],
        );
        assert!(found.is_empty());
    }
}
