//! Health probes (TCP, HTTP, command) and the 3-strikes rule. Synchronous: the supervisor is
//! thread-based, so a probe simply blocks its own watcher/health thread.

use crate::engine::state::Status;
use crate::models::service::HealthCheck;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::ops::RangeInclusive;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthResult {
    Healthy,
    Unhealthy(String),
}

pub const STRIKES: u32 = 3;

pub fn run_once(check: &HealthCheck) -> HealthResult {
    let outcome = match check {
        HealthCheck::Tcp {
            port, timeout_secs, ..
        } => connect("127.0.0.1", *port, secs(*timeout_secs)).map(|_| ()),
        HealthCheck::Http {
            url,
            min_status,
            max_status,
            timeout_secs,
            ..
        } => http_get(url, *min_status..=*max_status, secs(*timeout_secs)),
        HealthCheck::Command {
            exe,
            args,
            timeout_secs,
            ..
        } => command(exe, args, secs(*timeout_secs)),
    };
    match outcome {
        Ok(()) => HealthResult::Healthy,
        Err(m) => HealthResult::Unhealthy(m),
    }
}

/// Three failures in a row mark a running service unresponsive; one pass restores it.
/// Health never changes a state it does not own (starting, stopping, stopped, failed).
pub fn next_health_status(consecutive_failures: u32, current: Status) -> Status {
    match current {
        Status::Running if consecutive_failures >= STRIKES => Status::Unresponsive,
        Status::Unresponsive if consecutive_failures == 0 => Status::Running,
        other => other,
    }
}

fn secs(n: u32) -> Duration {
    Duration::from_secs(u64::from(n.max(1)))
}

fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Could not resolve {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("Could not resolve {host}."))?;
    TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("Could not connect to {host}:{port}: {e}"))
}

/// Minimal HTTP/1.1 GET over std TCP, localhost-style use, no new dependency.
/// shortcut: https:// is only checked as a TCP connect (no TLS stack), upgrade with a TLS crate.
fn http_get(url: &str, accepted: RangeInclusive<u16>, timeout: Duration) -> Result<(), String> {
    let (tls, rest) = match url.strip_prefix("https://") {
        Some(r) => (true, r),
        None => (
            false,
            url.strip_prefix("http://")
                .ok_or("Unsupported URL scheme.")?,
        ),
    };
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| "Invalid port in URL.")?),
        None => (authority, if tls { 443 } else { 80 }),
    };
    let mut stream = connect(host, port, timeout)?;
    if tls {
        return Ok(());
    }
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    write!(
        stream,
        "GET {path} HTTP/1.1
Host: {authority}
Connection: close
User-Agent: Techomato-Orbit

"
    )
    .map_err(|e| format!("Request failed: {e}"))?;
    let mut head = [0u8; 64];
    let mut got = 0;
    while got < 12 {
        match stream.read(&mut head[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => return Err(format!("No response within {}s: {e}", timeout.as_secs())),
        }
    }
    let status = std::str::from_utf8(&head[..got])
        .ok()
        .and_then(|h| h.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or("The response was not valid HTTP.")?;
    if accepted.contains(&status) {
        Ok(())
    } else {
        Err(format!(
            "Responded {status}, expected {}-{}.",
            accepted.start(),
            accepted.end()
        ))
    }
}

fn command(exe: &str, args: &[String], timeout: Duration) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("Could not run \"{exe}\": {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("Exited with code {:?}.", st.code())),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Did not finish within {}s.", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    fn tcp(port: u16) -> HealthCheck {
        HealthCheck::Tcp {
            port,
            interval_secs: 1,
            timeout_secs: 1,
        }
    }

    fn http(port: u16, min: u16, max: u16, timeout: u32) -> HealthCheck {
        HealthCheck::Http {
            url: format!("http://127.0.0.1:{port}/health"),
            min_status: min,
            max_status: max,
            interval_secs: 1,
            timeout_secs: timeout,
        }
    }

    /// Serves one canned response per connection; `delay` holds the reply back.
    fn server(status: u16, delay: Duration) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                std::thread::sleep(delay);
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        port
    }

    #[test]
    fn tcp_is_healthy_when_open_and_unhealthy_when_closed() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert_eq!(run_once(&tcp(port)), HealthResult::Healthy);
        drop(l);
        assert!(matches!(run_once(&tcp(port)), HealthResult::Unhealthy(_)));
    }

    #[test]
    fn http_is_healthy_on_200() {
        let p = server(200, Duration::ZERO);
        assert_eq!(run_once(&http(p, 200, 399, 2)), HealthResult::Healthy);
    }

    #[test]
    fn http_500_is_unhealthy_with_the_status_in_the_message() {
        let p = server(500, Duration::ZERO);
        match run_once(&http(p, 200, 399, 2)) {
            HealthResult::Unhealthy(m) => assert!(m.contains("500"), "{m}"),
            HealthResult::Healthy => panic!("500 must be unhealthy"),
        }
    }

    #[test]
    fn http_honours_the_accepted_range() {
        let p = server(404, Duration::ZERO);
        assert_eq!(run_once(&http(p, 200, 499, 2)), HealthResult::Healthy);
    }

    #[test]
    fn http_honours_the_timeout() {
        let p = server(200, Duration::from_secs(3));
        let t = std::time::Instant::now();
        assert!(matches!(
            run_once(&http(p, 200, 399, 1)),
            HealthResult::Unhealthy(_)
        ));
        assert!(t.elapsed() < Duration::from_millis(2500));
    }

    #[test]
    fn command_is_healthy_on_exit_0_and_unhealthy_on_exit_1_or_missing() {
        let cmd = |args: &[&str]| HealthCheck::Command {
            exe: "cmd".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            interval_secs: 1,
            timeout_secs: 5,
        };
        assert_eq!(run_once(&cmd(&["/c", "exit", "0"])), HealthResult::Healthy);
        assert!(matches!(
            run_once(&cmd(&["/c", "exit", "1"])),
            HealthResult::Unhealthy(_)
        ));
        let missing = HealthCheck::Command {
            exe: "definitely-not-installed-xyz".into(),
            args: vec![],
            interval_secs: 1,
            timeout_secs: 5,
        };
        assert!(matches!(run_once(&missing), HealthResult::Unhealthy(_)));
    }

    #[test]
    fn three_strikes_make_running_unresponsive_and_a_pass_restores_it() {
        use Status::*;
        assert_eq!(next_health_status(2, Running), Running);
        assert_eq!(next_health_status(3, Running), Unresponsive);
        assert_eq!(next_health_status(9, Unresponsive), Unresponsive);
        assert_eq!(next_health_status(0, Unresponsive), Running);
        // Health never overrides a lifecycle state it does not own.
        for s in [Starting, Stopping, Stopped, Failed] {
            assert_eq!(next_health_status(5, s), s);
            assert_eq!(next_health_status(0, s), s);
        }
    }
}
