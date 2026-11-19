//! The elevated helper: a closed set of typed operations Orbit may ask a UAC-elevated copy of
//! itself to perform. There is no "run this command" operation, and the helper re-validates
//! everything it is told, because the request arrives on a command line.

use crate::windows::procs::{identify, ProcId};
use serde::{Deserialize, Serialize};

/// Largest accepted request, as JSON before encoding.
pub const MAX_REQUEST_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ElevatedOp {
    ServiceStart { name: String },
    ServiceStop { name: String },
    ServiceRestart { name: String },
    EndProcess { pid: u32, created_at_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpResult {
    pub ok: bool,
    pub message: String,
}

impl OpResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

const FORBIDDEN: &[char] = &['\\', '/', '*', '?', '"', '<', '>', '|'];

fn check_name(name: &str) -> Result<(), String> {
    let n = name.chars().count();
    if name.trim().is_empty() || n > 256 {
        return Err("A service name must be 1 to 256 characters.".into());
    }
    if name
        .chars()
        .any(|c| c.is_control() || FORBIDDEN.contains(&c))
    {
        return Err("That is not a valid service name.".into());
    }
    Ok(())
}

fn check_shape(op: &ElevatedOp) -> Result<(), String> {
    match op {
        ElevatedOp::ServiceStart { name }
        | ElevatedOp::ServiceStop { name }
        | ElevatedOp::ServiceRestart { name } => check_name(name),
        ElevatedOp::EndProcess { .. } => Ok(()),
    }
}

/// Encodes a request for the command line (URL-safe base64, no padding, so no quoting issues).
pub fn encode(op: &ElevatedOp) -> Result<String, String> {
    check_shape(op)?;
    let json = serde_json::to_vec(op).map_err(|e| e.to_string())?;
    if json.len() > MAX_REQUEST_BYTES {
        return Err("The request is too large.".into());
    }
    Ok(URL_SAFE_NO_PAD.encode(json))
}

/// Decodes and syntactically checks a request. Never trusts its input: size, base64, JSON shape,
/// unknown operations or fields, and the form of service names are all checked here.
pub fn decode(text: &str) -> Result<ElevatedOp, String> {
    if text.len() > MAX_REQUEST_BYTES * 4 / 3 + 4 {
        return Err("The request is too large.".into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(text.trim())
        .map_err(|_| "The request is not valid.".to_string())?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("The request is too large.".into());
    }
    let op: ElevatedOp =
        serde_json::from_slice(&bytes).map_err(|_| "The request is not understood.".to_string())?;
    check_shape(&op)?;
    Ok(op)
}

fn process_name(pid: u32) -> Option<String> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut sys = System::new().ok()?;
    let pid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid)
        .map(|p| p.name().to_string_lossy().into_owned())
}

/// Semantic checks, made in the elevated process just before acting: the service exists and is
/// not protected, or the process is still the one that was chosen.
pub fn validate(op: &ElevatedOp) -> Result<(), String> {
    match op {
        ElevatedOp::ServiceStart { name }
        | ElevatedOp::ServiceStop { name }
        | ElevatedOp::ServiceRestart { name } => {
            check_name(name)?;
            match crate::windows::scm::find(name).map_err(|e| e.message)? {
                None => Err(format!("There is no service named {name}.")),
                Some(s) if s.protected => Err(format!(
                    "{} is part of Windows and is not managed here.",
                    s.display_name
                )),
                Some(_) => Ok(()),
            }
        }
        ElevatedOp::EndProcess { pid, created_at_ms } => {
            let target = ProcId {
                pid: *pid,
                created_at_ms: *created_at_ms,
            };
            let name = process_name(*pid);
            if let Some(why) =
                crate::engine::ports::refusal(target, std::process::id(), name.as_deref())
            {
                return Err(why);
            }
            if identify(*pid) != Some(target) {
                return Err("That process has changed or already ended.".into());
            }
            Ok(())
        }
    }
}

/// Performs a validated operation.
pub fn execute(op: &ElevatedOp) -> OpResult {
    use crate::windows::scm;
    let done = |r: crate::error::AppResult<()>, ok: &str| match r {
        Ok(()) => OpResult::ok(ok),
        Err(e) => OpResult::refused(e.message),
    };
    match op {
        ElevatedOp::ServiceStart { name } => done(scm::start(name), "Service started."),
        ElevatedOp::ServiceStop { name } => done(scm::stop(name), "Service stopped."),
        ElevatedOp::ServiceRestart { name } => done(
            scm::stop(name).and_then(|()| scm::start(name)),
            "Service restarted.",
        ),
        ElevatedOp::EndProcess { pid, created_at_ms } => done(
            crate::engine::ports::end_process(
                ProcId {
                    pid: *pid,
                    created_at_ms: *created_at_ms,
                },
                true,
            ),
            "Process ended.",
        ),
    }
}

/// Body of `--elevated-op <request> --result <file>`: decode, validate, execute, report. Any
/// failure before execution is written as a refusal and nothing is done. Returns the exit code.
/// The result file is only written if the caller already created it.
pub fn run_helper(request: &str, result_path: &std::path::Path) -> i32 {
    let result = match decode(request).and_then(|op| validate(&op).map(|()| op)) {
        Ok(op) => execute(&op),
        Err(why) => OpResult::refused(why),
    };
    if result_path.exists() {
        let json = serde_json::to_vec(&result).unwrap_or_default();
        let _ = std::fs::write(result_path, json);
    }
    i32::from(!result.ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(op: &str, name: &str) -> String {
        use base64::Engine;
        let json = format!(
            r#"{{"op":"{op}","name":{}}}"#,
            serde_json::to_string(name).unwrap()
        );
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    }
    fn raw(json: &str) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    }

    #[test]
    fn a_request_round_trips_for_every_operation() {
        for op in [
            ElevatedOp::ServiceStart {
                name: "Redis".into(),
            },
            ElevatedOp::ServiceStop {
                name: "postgresql-x64-16".into(),
            },
            ElevatedOp::ServiceRestart {
                name: "MSSQL$SQLEXPRESS".into(),
            },
            ElevatedOp::EndProcess {
                pid: 42,
                created_at_ms: 1_700_000_000_000,
            },
        ] {
            assert_eq!(decode(&encode(&op).unwrap()).unwrap(), op);
        }
    }

    #[test]
    fn encoded_requests_are_safe_on_a_command_line() {
        let text = encode(&ElevatedOp::ServiceStart {
            name: "a b$c".into(),
        })
        .unwrap();
        assert!(
            text.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{text}"
        );
    }

    #[test]
    fn oversize_requests_are_rejected_before_decoding() {
        let big = svc("serviceStart", &"a".repeat(2000));
        assert!(decode(&big).unwrap_err().contains("too large"));
        assert!(decode(&"A".repeat(5000)).unwrap_err().contains("too large"));
        let long_name = ElevatedOp::ServiceStart {
            name: "a".repeat(300),
        };
        assert!(
            encode(&long_name).is_err(),
            "names over 256 chars are refused when encoding too"
        );
    }

    #[test]
    fn garbage_is_rejected_with_a_plain_message() {
        assert!(decode("").is_err());
        assert!(decode("%%%not base64%%%").is_err());
        assert!(decode(&raw("not json")).is_err());
        assert!(decode(&raw("[]")).is_err());
    }

    #[test]
    fn unknown_operations_and_fields_are_rejected() {
        assert!(decode(&raw(r#"{"op":"runCommand","name":"x"}"#)).is_err());
        assert!(decode(&raw(r#"{"op":"serviceStart"}"#)).is_err());
        assert!(decode(&raw(r#"{"op":"serviceStart","name":"x","extra":1}"#)).is_err());
        assert!(decode(&raw(
            r#"{"op":"endProcess","pid":1,"createdAtMs":2,"cmd":"calc"}"#
        ))
        .is_err());
        assert!(decode(&raw(r#"{"name":"x"}"#)).is_err());
    }

    #[test]
    fn bad_service_names_are_rejected() {
        for name in [
            "",
            "   ",
            r"..\x",
            "a/b",
            r"a\b",
            "a*",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "nul\0byte",
            "tab\tname",
            "new\nline",
        ] {
            assert!(
                decode(&svc("serviceStop", name)).is_err(),
                "{name:?} should be rejected"
            );
        }
        assert!(decode(&svc("serviceStop", &"a".repeat(257))).is_err());
        assert!(decode(&svc("serviceStop", &"a".repeat(256))).is_ok());
    }

    #[test]
    fn ordinary_service_names_are_accepted() {
        for name in [
            "Redis",
            "postgresql-x64-16",
            "MSSQL$SQLEXPRESS",
            "com.docker.service",
            "My Service 2",
        ] {
            assert!(decode(&svc("serviceStart", name)).is_ok(), "{name}");
        }
    }

    #[test]
    fn protected_services_are_refused_by_validate() {
        for name in ["EventLog", "RpcSs", "WinDefend"] {
            let err = validate(&ElevatedOp::ServiceStop { name: name.into() }).unwrap_err();
            assert!(err.contains("part of Windows"), "{name}: {err}");
        }
    }

    #[test]
    fn a_service_that_does_not_exist_is_refused_by_validate() {
        let err = validate(&ElevatedOp::ServiceStart {
            name: "no-such-service-xyz".into(),
        })
        .unwrap_err();
        assert!(err.contains("no service"), "{err}");
    }

    fn sleeper() -> std::process::Child {
        std::process::Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn a_process_that_changed_is_refused_by_validate() {
        let mut child = sleeper();
        let id = crate::windows::procs::identify_child(&child).unwrap();
        assert!(validate(&ElevatedOp::EndProcess {
            pid: id.pid,
            created_at_ms: id.created_at_ms
        })
        .is_ok());
        let stale = ElevatedOp::EndProcess {
            pid: id.pid,
            created_at_ms: id.created_at_ms + 60_000,
        };
        assert!(validate(&stale).unwrap_err().contains("changed"));
        let _ = child.kill();
        let _ = child.wait();
        let gone = ElevatedOp::EndProcess {
            pid: 4_000_000,
            created_at_ms: 1,
        };
        assert!(validate(&gone).is_err());
    }

    #[test]
    fn orbit_itself_is_refused_by_validate() {
        let me = identify(std::process::id()).unwrap();
        let op = ElevatedOp::EndProcess {
            pid: me.pid,
            created_at_ms: me.created_at_ms,
        };
        assert!(validate(&op).unwrap_err().contains("Orbit itself"));
    }

    #[test]
    fn execute_end_process_really_ends_a_spawned_process() {
        let mut child = sleeper();
        let id = crate::windows::procs::identify_child(&child).unwrap();
        let result = execute(&ElevatedOp::EndProcess {
            pid: id.pid,
            created_at_ms: id.created_at_ms,
        });
        assert!(result.ok, "{}", result.message);
        assert!(child.wait().is_ok());
        assert!(identify(id.pid).is_none());
    }

    #[test]
    #[ignore = "needs a UAC click; run by hand against a throwaway service"]
    fn manual_real_elevated_service_stop() {
        let name = std::env::var("ORBIT_TEST_SERVICE").expect("set ORBIT_TEST_SERVICE");
        let r = crate::windows::elevate::run_elevated(&ElevatedOp::ServiceStop { name }).unwrap();
        assert!(r.ok, "{}", r.message);
    }

    #[test]
    fn critical_windows_processes_are_refused_by_validate() {
        let op = ElevatedOp::EndProcess {
            pid: 4,
            created_at_ms: 0,
        };
        assert!(validate(&op).is_err());
    }

    #[test]
    fn the_helper_refuses_bad_requests_writes_why_and_acts_on_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("r.json");
        std::fs::write(&out, b"").unwrap();
        let code = run_helper(&raw(r#"{"op":"runCommand","name":"calc"}"#), &out);
        assert_eq!(code, 1);
        let result: OpResult = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert!(!result.ok && !result.message.is_empty());

        std::fs::write(&out, b"").unwrap();
        let code = run_helper(&svc("serviceStop", "EventLog"), &out);
        assert_eq!(code, 1);
        let result: OpResult = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert!(result.message.contains("part of Windows"));
    }

    #[test]
    fn the_helper_does_not_create_its_result_file() {
        // The caller creates it (so a privileged process never creates files at a chosen path).
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("missing.json");
        let code = run_helper(&raw("garbage"), &out);
        assert_eq!(code, 1);
        assert!(!out.exists());
    }
}
