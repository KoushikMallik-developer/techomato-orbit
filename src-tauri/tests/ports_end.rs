//! Real-process tests for ending a process by port owner. Run with `--ignored`.

use orbit_lib::engine::ports::end_process;
use orbit_lib::windows::procs::{identify, is_alive, ProcId};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

/// Starts the test service and waits until it prints that it is listening.
fn start(mode: &str, port: u16) -> (Child, ProcId, Box<dyn std::any::Any>) {
    let mut child = Command::new(SVC)
        .args([mode, &port.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    // The reader must outlive the test, or the service dies writing to a closed pipe.
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("listening"), "got {line:?}");
    let id = identify(child.id()).unwrap();
    (child, id, Box::new(reader))
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn wait_until(mut cond: impl FnMut() -> bool, secs: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[ignore]
fn ending_a_listener_stops_it_and_frees_its_port() {
    let port = free_port();
    let (mut child, id, _keep) = start("serve", port);
    end_process(id, false).unwrap();
    assert!(wait_until(|| port_free(port), 8));
    let _ = child.wait();
    assert!(!is_alive(&id));
}

#[test]
#[ignore]
fn a_reused_pid_with_the_wrong_creation_time_is_refused_and_the_live_process_survives() {
    let port = free_port();
    let (mut child, id, _keep) = start("serve", port);
    let stale = ProcId {
        pid: id.pid,
        created_at_ms: id.created_at_ms + 60_000,
    };
    let err = end_process(stale, true).unwrap_err();
    assert!(
        err.message.contains("changed or already ended"),
        "{}",
        err.message
    );
    assert!(is_alive(&id));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
#[ignore]
fn a_console_process_that_ignores_close_requests_is_ended_by_force() {
    let port = free_port();
    let (mut child, id, _keep) = start("ignore-stop", port);
    end_process(id, false).unwrap();
    assert!(wait_until(|| port_free(port), 8));
    let _ = child.wait();
    assert!(!is_alive(&id));
}
