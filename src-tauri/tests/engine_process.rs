//! Real-process tests for the Job Object / graceful-stop / log-capture layer.
//! Ignored by default (they spawn processes); CI runs them with `--ignored`.

use orbit_lib::engine::logs::{pump, LogSink, Stream};
use orbit_lib::windows::console::send_graceful_with;
use orbit_lib::windows::job::{spawn, terminate_tree, ManagedProcess, SpawnSpec};
use orbit_lib::windows::procs::{identify, is_alive};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SVC: &str = env!("CARGO_BIN_EXE_orbit-testsvc");

fn spec(exe: impl Into<PathBuf>, args: &[&str]) -> SpawnSpec {
    SpawnSpec {
        exe: exe.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: std::env::temp_dir(),
        env: vec![],
        through_shell: false,
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_ok(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(300),
    ) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = s.write_all(b"GET / HTTP/1.0\r\n\r\n");
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.contains("200")
}

fn wait_until(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn kill(p: &ManagedProcess) {
    let _ = terminate_tree(&p.job);
}

#[test]
#[ignore]
fn a_service_starts_and_stops_gracefully_with_exit_code_zero() {
    let port = free_port();
    let mut p = spawn(&spec(SVC, &["serve", &port.to_string()])).unwrap();
    wait_until("http", Duration::from_secs(5), || http_ok(port));
    send_graceful_with(std::path::Path::new(SVC), p.id.pid).unwrap();
    wait_until("exit", Duration::from_secs(3), || {
        p.child.try_wait().unwrap().is_some()
    });
    assert_eq!(p.child.wait().unwrap().code(), Some(0));
    assert!(!is_alive(&p.id));
}

#[test]
#[ignore]
fn a_process_that_ignores_stop_survives_graceful_and_dies_on_terminate() {
    let port = free_port();
    let mut p = spawn(&spec(SVC, &["ignore-stop", &port.to_string()])).unwrap();
    wait_until("http", Duration::from_secs(5), || http_ok(port));
    send_graceful_with(std::path::Path::new(SVC), p.id.pid).unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    assert!(is_alive(&p.id), "ignore-stop should have survived");
    terminate_tree(&p.job).unwrap();
    wait_until("death", Duration::from_secs(3), || !is_alive(&p.id));
    let _ = p.child.wait();
}

#[test]
#[ignore]
fn terminating_the_job_kills_the_child_as_well_as_the_parent() {
    let port = free_port();
    let mut p = spawn(&spec(SVC, &["spawn-child", &port.to_string()])).unwrap();
    let mut out = BufReader::new(p.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let child_pid: u32 = line.trim().strip_prefix("CHILD ").unwrap().parse().unwrap();
    wait_until("child http", Duration::from_secs(5), || http_ok(port));
    let child_id = identify(child_pid).expect("child is running");
    terminate_tree(&p.job).unwrap();
    wait_until("parent death", Duration::from_secs(3), || !is_alive(&p.id));
    wait_until("child death", Duration::from_secs(3), || {
        !is_alive(&child_id)
    });
    let _ = p.child.wait();
}

#[test]
#[ignore]
fn an_output_flood_never_grows_memory_without_bound() {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let me = Pid::from_u32(std::process::id());
    fn rss(sys: &mut System, me: Pid) -> u64 {
        sys.refresh_processes(ProcessesToUpdate::Some(&[me]), true);
        sys.process(me).map(|p| p.memory()).unwrap_or(0)
    }
    let mut sys = System::new().unwrap();
    let baseline = rss(&mut sys, me);
    let peak = Arc::new(std::sync::atomic::AtomicU64::new(baseline));
    let done = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (peak, done) = (peak.clone(), done.clone());
        std::thread::spawn(move || {
            let mut sys = System::new().unwrap();
            while !done.load(Ordering::Relaxed) {
                peak.fetch_max(rss(&mut sys, me), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };

    let dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(LogSink::new(
        dir.path().to_path_buf(),
        "spam",
        4 << 20,
        3,
        1000,
    ));
    let mut p = spawn(&spec(SVC, &["spam", "300"])).unwrap();
    let (out, err) = (p.stdout.take().unwrap(), p.stderr.take().unwrap());
    let (s1, s2) = (sink.clone(), sink.clone());
    let t1 = std::thread::spawn(move || pump(out, Stream::Stdout, &s1));
    let t2 = std::thread::spawn(move || pump(err, Stream::Stderr, &s2));
    t1.join().unwrap();
    t2.join().unwrap();
    let _ = p.child.wait();
    done.store(true, Ordering::Relaxed);
    sampler.join().unwrap();

    let growth = peak.load(Ordering::Relaxed).saturating_sub(baseline);
    assert!(growth < 50 * 1024 * 1024, "memory grew by {growth} bytes");
    assert!(sink.recent(10_000).len() <= 1000);
}

#[test]
#[ignore]
fn paths_with_spaces_and_non_ascii_characters_start_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("orbit test ü").join("bin dir");
    std::fs::create_dir_all(&odd).unwrap();
    let exe = odd.join("orbit-testsvc.exe");
    std::fs::copy(SVC, &exe).unwrap();
    let port = free_port();
    let mut s = spec(&exe, &["serve", &port.to_string()]);
    s.cwd = odd.clone();
    let p = spawn(&s).unwrap();
    wait_until("http", Duration::from_secs(5), || http_ok(port));
    kill(&p);
}

#[test]
#[ignore]
fn environment_variables_reach_the_child() {
    let mut s = spec(SVC, &["env", "ORBIT_TEST_VALUE"]);
    s.env = vec![("ORBIT_TEST_VALUE".into(), "héllo wörld".into())];
    let mut p = spawn(&s).unwrap();
    let mut text = String::new();
    p.stdout.take().unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text.trim(), "héllo wörld");
    let _ = p.child.wait();
}

#[test]
#[ignore]
fn run_through_shell_is_explicit_and_works() {
    let mut s = spec("echo", &["hi there"]);
    s.through_shell = true;
    let mut p = spawn(&s).unwrap();
    let mut text = String::new();
    p.stdout.take().unwrap().read_to_string(&mut text).unwrap();
    assert!(text.contains("hi there"), "{text:?}");
    let _ = p.child.wait();
}
