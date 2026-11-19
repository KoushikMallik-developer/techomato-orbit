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

fn process_info(pid: u32) -> (Option<String>, Option<std::path::PathBuf>) {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let Ok(mut sys) = System::new() else {
        return (None, None);
    };
    let pid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    match sys.process(pid) {
        Some(p) => (
            Some(p.name().to_string_lossy().into_owned()),
            p.exe().map(std::path::Path::to_path_buf),
        ),
        None => (None, None),
    }
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
            let (name, image) = process_info(*pid);
            if let Some(why) =
                crate::engine::ports::refusal(target, std::process::id(), name.as_deref())
            {
                return Err(why);
            }
            let hosts: Vec<u32> = crate::windows::scm::list()
                .map_err(|e| e.message)?
                .into_iter()
                .filter(|s| s.protected)
                .filter_map(|s| s.pid)
                .collect();
            let own = std::env::current_exe().unwrap_or_default();
            if let Some(why) = end_refusal(*pid, image.as_deref(), &own, &hosts) {
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

/// Validates the helper's whole command line (`<exe> --elevated-op <request> --result <file>`):
/// exact layout, and a result file that is `orbit-elevated-<uuid>.json` directly in the user's
/// temp folder. Anything else is refused, so the elevated copy can never be pointed at some other
/// file.
pub fn parse_helper_args(
    args: &[String],
    temp_dir: &std::path::Path,
) -> Result<(String, std::path::PathBuf), String> {
    let [_, flag, request, result_flag, path] = args else {
        return Err("The helper was started with the wrong arguments.".into());
    };
    if flag != "--elevated-op" || result_flag != "--result" {
        return Err("The helper was started with the wrong arguments.".into());
    }
    let path = std::path::PathBuf::from(path);
    let same_dir = path.parent().is_some_and(|p| {
        p.to_string_lossy().trim_end_matches('\\').to_lowercase()
            == temp_dir
                .to_string_lossy()
                .trim_end_matches('\\')
                .to_lowercase()
    });
    let name_ok = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("orbit-elevated-"))
        .and_then(|n| n.strip_suffix(".json"))
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok());
    if !(same_dir && name_ok) {
        return Err("The helper's result file is not an Orbit temp file.".into());
    }
    Ok((request.clone(), path))
}

/// Writes the result into a file the caller created: never creates it, never follows a link,
/// only touches a plain file.
fn write_result(path: &std::path::Path, result: &OpResult) {
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    // `symlink_metadata` does not follow links, so a link reports as neither file nor directory.
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
    else {
        return;
    };
    let _ = file.write_all(&serde_json::to_vec(result).unwrap_or_default());
}

/// Extra refusals for ending a process, applied inside the elevated helper: Orbit's own image
/// and any process hosting a protected Windows service.
pub fn end_refusal(
    pid: u32,
    image: Option<&std::path::Path>,
    own_image: &std::path::Path,
    protected_pids: &[u32],
) -> Option<String> {
    let lower = |p: &std::path::Path| p.to_string_lossy().to_lowercase();
    if image.is_some_and(|i| lower(i) == lower(own_image)) {
        return Some("That is Techomato Orbit itself.".into());
    }
    if protected_pids.contains(&pid) {
        return Some("That process runs part of Windows and cannot be ended.".into());
    }
    None
}

/// Body of `--elevated-op <request> --result <file>`: decode, validate, execute, report. Any
/// failure before execution is written as a refusal and nothing is done. Returns the exit code.
/// The result file is only written if the caller already created it.
pub fn run_helper(request: &str, result_path: &std::path::Path) -> i32 {
    let result = match decode(request).and_then(|op| validate(&op).map(|()| op)) {
        Ok(op) => execute(&op),
        Err(why) => OpResult::refused(why),
    };
    write_result(result_path, &result);
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

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    const UUID: &str = "0b0e6f0a-5a52-4f3a-9d1e-6a1c2b3d4e5f";

    #[test]
    fn helper_args_must_have_the_exact_layout_and_a_result_file_in_temp() {
        let tmp = std::path::Path::new(r"C:\Users\u\AppData\Local\Temp");
        let good = tmp.join(format!("orbit-elevated-{UUID}.json"));
        let g = good.to_str().unwrap();
        let ok = args(&["orbit.exe", "--elevated-op", "abc", "--result", g]);
        assert_eq!(
            parse_helper_args(&ok, tmp).unwrap(),
            ("abc".to_string(), good.clone())
        );
        let sub = format!(r"C:\Users\u\AppData\Local\Temp\sub\orbit-elevated-{UUID}.json");
        let dots = format!(r"C:\Users\u\AppData\Local\Temp\..\orbit-elevated-{UUID}.json");
        let bad_name = r"C:\Users\u\AppData\Local\Temp\orbit-elevated-notauuid.json";
        let hosts = r"C:\Windows\System32\drivers\etc\hosts";
        for bad in [
            args(&["orbit.exe", "--elevated-op", "abc", "--result", hosts]),
            args(&["orbit.exe", "--elevated-op", "abc", "--out", g]),
            args(&["orbit.exe", "--elevated-op", "abc"]),
            args(&["orbit.exe", "--elevated-op", "abc", "--result", g, "x"]),
            args(&["orbit.exe", "--elevated-op", "abc", "--result", &sub]),
            args(&["orbit.exe", "--elevated-op", "abc", "--result", bad_name]),
            args(&["orbit.exe", "--elevated-op", "abc", "--result", &dots]),
        ] {
            assert!(parse_helper_args(&bad, tmp).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_result_is_written_only_into_an_existing_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let ok = OpResult::ok("done");
        let file = dir.path().join("r.json");
        std::fs::write(&file, b"").unwrap();
        write_result(&file, &ok);
        assert!(std::fs::read_to_string(&file).unwrap().contains("done"));
        // A directory is not a plain file; nothing is written or created.
        write_result(dir.path(), &ok);
        let missing = dir.path().join("missing.json");
        write_result(&missing, &ok);
        assert!(!missing.exists());
    }

    #[test]
    fn a_result_is_not_written_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("precious.txt");
        std::fs::write(&target, b"keep").unwrap();
        let link = dir.path().join("link.json");
        if std::os::windows::fs::symlink_file(&target, &link).is_err() {
            eprintln!("skipped: creating symlinks needs Developer Mode");
            return;
        }
        write_result(&link, &OpResult::ok("x"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }

    #[test]
    fn ending_orbit_or_a_protected_services_host_is_refused() {
        use std::path::Path;
        let own = Path::new(r"C:\Orbit\orbit.exe");
        let image = Some(Path::new(r"c:\orbit\ORBIT.EXE"));
        assert!(end_refusal(10, image, own, &[]).unwrap().contains("Orbit"));
        let other = Some(Path::new(r"C:\tools\node.exe"));
        assert!(end_refusal(10, other, own, &[]).is_none());
        let svchost = Some(Path::new(r"C:\Windows\System32\svchost.exe"));
        assert!(end_refusal(20, svchost, own, &[5, 20])
            .unwrap()
            .contains("Windows"));
        assert!(end_refusal(20, svchost, own, &[5]).is_none());
        assert!(end_refusal(30, None, own, &[]).is_none());
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
