//! Test helper: a tiny, harmless process with scripted behaviours. Used by the engine's
//! real-process tests; never started by the app itself.
//!
//!   serve <port>        HTTP 200 on every request; exits 0 on Ctrl+Break / Ctrl+C
//!   ignore-stop <port>  same, but ignores every stop signal
//!   handle-stop         prints STOPPING on a stop signal, then exits 0
//!   exit <code>         exits immediately with <code>
//!   crash               panics (exit code 101)
//!   spawn-child <port>  starts `serve <port>` as a child, prints `CHILD <pid>`, then waits
//!   spam <mb>           writes <mb> MB of lines to stdout and one 4 MB line to stderr
//!   env <NAME>          prints the value of an environment variable
//!   --stop-helper <pid> the same stop helper the app uses (so tests can reuse it)

use std::io::{BufWriter, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};

static STOP: AtomicBool = AtomicBool::new(false);
static IGNORE: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_ctrl(ctrl: u32) -> BOOL {
    if IGNORE.load(Ordering::SeqCst) {
        return BOOL(1);
    }
    if ctrl == CTRL_C_EVENT || ctrl == CTRL_BREAK_EVENT || ctrl == CTRL_CLOSE_EVENT {
        println!("STOPPING");
        STOP.store(true, Ordering::SeqCst);
        return BOOL(1);
    }
    BOOL(0)
}

fn install_handler() {
    // SAFETY: registers a valid handler routine for this process.
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(on_ctrl), true);
    }
}

fn serve(port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    listener.set_nonblocking(true).unwrap();
    println!("listening on {port}");
    while !STOP.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut s, _)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
                let mut buf = [0u8; 512];
                let _ = s.read(&mut buf);
                let _ = s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match arg(0) {
        "--stop-helper" => std::process::exit(orbit_lib::windows::console::stop_helper(
            arg(1).parse().expect("pid"),
        )),
        "serve" => {
            install_handler();
            serve(arg(1).parse().expect("port"));
        }
        "ignore-stop" => {
            IGNORE.store(true, Ordering::SeqCst);
            install_handler();
            serve(arg(1).parse().expect("port"));
        }
        "handle-stop" => {
            install_handler();
            while !STOP.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        "exit" => std::process::exit(arg(1).parse().expect("code")),
        "crash" => panic!("orbit-testsvc crash"),
        "spawn-child" => {
            let me = std::env::current_exe().unwrap();
            // Deliberately never waited on: the test needs a live grandchild.
            #[allow(clippy::zombie_processes)]
            let child = Command::new(me).args(["serve", arg(1)]).spawn().unwrap();
            println!("CHILD {}", child.id());
            let _ = std::io::stdout().flush();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "spam" => {
            let mb: usize = arg(1).parse().expect("mb");
            let big = "y".repeat(4 * 1024 * 1024);
            let _ = writeln!(std::io::stderr(), "{big}");
            let mut out = BufWriter::new(std::io::stdout().lock());
            let line = "x".repeat(90);
            let mut written = 0usize;
            let mut i = 0u64;
            while written < mb * 1024 * 1024 {
                let n = writeln!(out, "line {i} {line}").map(|_| 100).unwrap_or(0);
                written += n.max(1);
                i += 1;
            }
            let _ = out.flush();
        }
        "env" => println!(
            "{}",
            std::env::var(arg(1)).unwrap_or_else(|_| "<unset>".into())
        ),
        other => {
            eprintln!("unknown mode {other:?}");
            std::process::exit(64);
        }
    }
}
