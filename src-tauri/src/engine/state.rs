//! The service lifecycle as a pure transition table.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Unresponsive,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start,
    /// The process stayed alive (and passed its health check, if it has one).
    Ready,
    /// The process ended. `clean` means exit code 0.
    Exited {
        clean: bool,
    },
    Stop,
    Unhealthy,
    Healthy,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: Status,
    pub event: Event,
}

pub fn next(current: Status, event: Event) -> Result<Status, IllegalTransition> {
    use Event::*;
    use Status::*;
    let to = match (current, event) {
        (Stopped | Failed | Unknown, Start) => Starting,
        (Starting, Ready) => Running,
        (Starting | Running | Unresponsive, Exited { clean: true }) => Stopped,
        (Starting | Running | Unresponsive, Exited { clean: false }) => Failed,
        (Stopping, Exited { .. }) => Stopped,
        (Starting | Running | Unresponsive, Stop) => Stopping,
        (Running, Unhealthy) => Unresponsive,
        (Unresponsive, Healthy) => Running,
        (from, event) => return Err(IllegalTransition { from, event }),
    };
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::Event::*;
    use super::Status::*;
    use super::*;

    #[test]
    fn legal_transitions() {
        assert_eq!(next(Stopped, Start), Ok(Starting));
        assert_eq!(next(Failed, Start), Ok(Starting));
        assert_eq!(next(Unknown, Start), Ok(Starting));
        assert_eq!(next(Starting, Ready), Ok(Running));
        assert_eq!(next(Starting, Exited { clean: false }), Ok(Failed));
        assert_eq!(next(Running, Exited { clean: true }), Ok(Stopped));
        assert_eq!(next(Running, Exited { clean: false }), Ok(Failed));
        assert_eq!(next(Unresponsive, Exited { clean: false }), Ok(Failed));
        assert_eq!(next(Running, Stop), Ok(Stopping));
        assert_eq!(next(Starting, Stop), Ok(Stopping));
        assert_eq!(next(Unresponsive, Stop), Ok(Stopping));
        assert_eq!(next(Stopping, Exited { clean: false }), Ok(Stopped));
        assert_eq!(next(Running, Unhealthy), Ok(Unresponsive));
        assert_eq!(next(Unresponsive, Healthy), Ok(Running));
    }

    #[test]
    fn start_is_refused_while_starting_running_stopping_or_unresponsive() {
        for s in [Starting, Running, Stopping, Unresponsive] {
            assert_eq!(
                next(s, Start),
                Err(IllegalTransition {
                    from: s,
                    event: Start
                })
            );
        }
    }

    #[test]
    fn stop_is_refused_when_nothing_is_running() {
        for s in [Stopped, Failed, Unknown, Stopping] {
            assert!(next(s, Stop).is_err(), "{s:?}");
        }
    }

    #[test]
    fn an_exit_is_meaningless_for_a_service_that_is_not_running() {
        assert!(next(Stopped, Exited { clean: true }).is_err());
        assert!(next(Failed, Exited { clean: false }).is_err());
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(serde_json::to_value(Unresponsive).unwrap(), "unresponsive");
    }
}
