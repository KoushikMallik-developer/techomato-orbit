//! Turns service state changes into user-facing notices (pure; the caller stores and shows them).

use crate::engine::state::Status;
use crate::engine::supervisor::ServiceRuntime;
use crate::models::prefs::NotifyPrefs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Started,
    Stopped,
    Crashed,
    StartFailed,
    HealthFailed,
    AutoRestart,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Started => "started",
            Kind::Stopped => "stopped",
            Kind::Crashed => "crashed",
            Kind::StartFailed => "startFailed",
            Kind::HealthFailed => "healthFailed",
            Kind::AutoRestart => "autoRestart",
        }
    }

    pub fn enabled(self, p: &NotifyPrefs) -> bool {
        match self {
            Kind::Started => p.started,
            Kind::Stopped => p.stopped,
            Kind::Crashed => p.crashed,
            Kind::StartFailed => p.start_failed,
            Kind::HealthFailed => p.health_failed,
            Kind::AutoRestart => p.auto_restart,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: Kind,
    pub title: String,
    pub body: String,
}

/// The notice (if any) for the change from `prev` to `next`, honouring the user's switches.
pub fn notice_for(
    prev: Option<&ServiceRuntime>,
    next: &ServiceRuntime,
    name: &str,
    prefs: &NotifyPrefs,
) -> Option<Notice> {
    let prev_status = prev.map(|p| p.status);
    let prev_restarts = prev.map_or(0, |p| p.restart_count);
    let notice = |kind, title: String, body: String| Notice { kind, title, body };
    let found = if next.restart_count > prev_restarts {
        Some(notice(
            Kind::AutoRestart,
            format!("{name} is restarting"),
            format!("Automatic restart {}.", next.restart_count),
        ))
    } else if prev_status == Some(next.status) {
        None
    } else {
        match (prev_status, next.status) {
            (Some(Status::Starting), Status::Running) => Some(notice(
                Kind::Started,
                format!("{name} started"),
                String::new(),
            )),
            (Some(Status::Stopping), Status::Stopped) => Some(notice(
                Kind::Stopped,
                format!("{name} stopped"),
                String::new(),
            )),
            // The process never ran: a refused start (preflight settles to Stopped) or a failed
            // spawn (Failed with an error but no exit code).
            (Some(Status::Starting) | None, Status::Stopped | Status::Failed)
                if next.last_error.is_some() && next.exit_code.is_none() =>
            {
                Some(notice(
                    Kind::StartFailed,
                    format!("{name} could not start"),
                    next.last_error.clone().unwrap_or_default(),
                ))
            }
            (Some(Status::Running | Status::Starting | Status::Unresponsive), Status::Failed) => {
                Some(notice(
                    Kind::Crashed,
                    format!("{name} crashed"),
                    match next.exit_code {
                        Some(code) => format!("It exited with code {code}."),
                        None => "It ended unexpectedly.".into(),
                    },
                ))
            }
            (_, Status::Failed) if next.last_error.is_some() => Some(notice(
                Kind::StartFailed,
                format!("{name} could not start"),
                next.last_error.clone().unwrap_or_default(),
            )),
            (Some(Status::Running), Status::Unresponsive) => Some(notice(
                Kind::HealthFailed,
                format!("{name} stopped responding"),
                "Its health check is failing.".into(),
            )),
            _ => None,
        }
    };
    found.filter(|n| n.kind.enabled(prefs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(status: Status) -> ServiceRuntime {
        ServiceRuntime {
            id: "s".into(),
            status,
            pid: None,
            parent_pid: None,
            started_at_ms: None,
            exit_code: None,
            restart_count: 0,
            last_error: None,
        }
    }
    fn all_on() -> NotifyPrefs {
        NotifyPrefs {
            started: true,
            stopped: true,
            crashed: true,
            start_failed: true,
            health_failed: true,
            auto_restart: true,
        }
    }
    fn kind(prev: Option<&ServiceRuntime>, next: &ServiceRuntime) -> Option<Kind> {
        notice_for(prev, next, "API", &all_on()).map(|n| n.kind)
    }

    #[test]
    fn running_after_starting_is_started() {
        let n = notice_for(
            Some(&rt(Status::Starting)),
            &rt(Status::Running),
            "API",
            &all_on(),
        )
        .unwrap();
        assert_eq!(n.kind, Kind::Started);
        assert!(n.title.contains("API"));
    }

    #[test]
    fn stopped_after_stopping_is_stopped() {
        assert_eq!(
            kind(Some(&rt(Status::Stopping)), &rt(Status::Stopped)),
            Some(Kind::Stopped)
        );
    }

    #[test]
    fn failed_after_running_is_a_crash_and_the_body_carries_the_exit_code() {
        let mut next = rt(Status::Failed);
        next.exit_code = Some(3);
        let n = notice_for(Some(&rt(Status::Running)), &next, "API", &all_on()).unwrap();
        assert_eq!(n.kind, Kind::Crashed);
        assert!(n.body.contains('3'), "{}", n.body);
        let from_starting = notice_for(Some(&rt(Status::Starting)), &next, "API", &all_on());
        assert_eq!(from_starting.unwrap().kind, Kind::Crashed);
    }

    #[test]
    fn failed_with_an_error_and_no_run_is_a_start_failure() {
        let mut next = rt(Status::Failed);
        next.last_error = Some("Port 80 is already in use.".into());
        let n = notice_for(Some(&rt(Status::Stopped)), &next, "API", &all_on()).unwrap();
        assert_eq!(n.kind, Kind::StartFailed);
        assert!(n.body.contains("Port 80"));
        assert_eq!(kind(None, &next), Some(Kind::StartFailed));
    }

    #[test]
    fn unresponsive_is_a_health_failure() {
        assert_eq!(
            kind(Some(&rt(Status::Running)), &rt(Status::Unresponsive)),
            Some(Kind::HealthFailed)
        );
    }

    #[test]
    fn a_restart_count_increase_is_an_automatic_restart() {
        let mut next = rt(Status::Starting);
        next.restart_count = 2;
        let mut prev = rt(Status::Failed);
        prev.restart_count = 1;
        assert_eq!(kind(Some(&prev), &next), Some(Kind::AutoRestart));
    }

    #[test]
    fn a_manual_stop_never_reads_as_a_crash() {
        // Stop moves Running -> Stopping -> Stopped, even when the process exits non-zero.
        let mut stopped = rt(Status::Stopped);
        stopped.exit_code = Some(1);
        assert_ne!(
            kind(Some(&rt(Status::Stopping)), &stopped),
            Some(Kind::Crashed)
        );
        assert_eq!(
            kind(Some(&rt(Status::Running)), &rt(Status::Stopping)),
            None
        );
    }

    #[test]
    fn a_repeated_identical_state_is_silent() {
        assert_eq!(kind(Some(&rt(Status::Running)), &rt(Status::Running)), None);
        let mut a = rt(Status::Failed);
        a.last_error = Some("x".into());
        assert_eq!(kind(Some(&a), &a), None);
    }

    #[test]
    fn switched_off_kinds_produce_nothing() {
        let off = NotifyPrefs {
            crashed: false,
            started: false,
            ..all_on()
        };
        assert!(notice_for(Some(&rt(Status::Running)), &rt(Status::Failed), "API", &off).is_none());
        assert!(notice_for(
            Some(&rt(Status::Starting)),
            &rt(Status::Running),
            "API",
            &off
        )
        .is_none());
        assert!(notice_for(
            Some(&rt(Status::Running)),
            &rt(Status::Unresponsive),
            "API",
            &off
        )
        .is_some());
    }

    #[test]
    fn a_refused_start_is_a_start_failure_not_a_stop() {
        // Preflight problems (missing executable, port in use) settle Starting -> Stopped + error.
        let mut next = rt(Status::Stopped);
        next.last_error = Some("Could not find the executable.".into());
        let n = notice_for(Some(&rt(Status::Starting)), &next, "API", &all_on()).unwrap();
        assert_eq!(n.kind, Kind::StartFailed);
        assert!(n.body.contains("executable"));
    }

    #[test]
    fn a_spawn_failure_with_no_exit_code_is_a_start_failure_not_a_crash() {
        let mut next = rt(Status::Failed);
        next.last_error = Some("Windows refused to start it.".into());
        let n = notice_for(Some(&rt(Status::Starting)), &next, "API", &all_on()).unwrap();
        assert_eq!(n.kind, Kind::StartFailed);
    }
}
