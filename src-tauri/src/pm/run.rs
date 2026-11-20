//! Runs a program with an argument array, a timeout, and every output line passed to a callback
//! as it arrives. No shell is involved, so text stays literal.

use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// An executable plus leading arguments (for example `powershell -File script.ps1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launcher {
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    Spawn(String),
    TimedOut,
}

/// Splits a byte stream into lines on LF and CR, so a multi-byte character cut between two
/// reads is decoded only once its line is complete. Blank lines and bare spinner frames are dropped.
#[derive(Default)]
pub struct LineSplitter(Vec<u8>);

fn is_noise(line: &str) -> bool {
    line.trim().chars().all(|c| "-\\|/".contains(c))
}

impl LineSplitter {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\n' || b == b'\r' {
                self.flush_into(&mut out);
            } else {
                self.0.push(b);
            }
        }
        out
    }

    pub fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.flush_into(&mut out);
        out
    }

    fn flush_into(&mut self, out: &mut Vec<String>) {
        if self.0.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(&self.0).into_owned();
        self.0.clear();
        if !is_noise(&line) {
            out.push(line);
        }
    }
}

fn pump(mut r: impl Read + Send + 'static, is_err: bool, tx: mpsc::Sender<(bool, Vec<u8>)>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = r.read(&mut buf) {
            if n == 0 || tx.send((is_err, buf[..n].to_vec())).is_err() {
                break;
            }
        }
    });
}

/// Runs `launcher` with `args`; `on_line` gets each output line while the process is running.
/// The process is killed if it outlives `timeout`.
pub fn run(
    launcher: &Launcher,
    args: &[String],
    timeout: Duration,
    mut on_line: impl FnMut(&str),
) -> Result<Output, RunError> {
    let mut child = Command::new(&launcher.program)
        .args(&launcher.prefix)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| RunError::Spawn(e.to_string()))?;
    let (tx, rx) = mpsc::channel();
    pump(child.stdout.take().unwrap(), false, tx.clone());
    pump(child.stderr.take().unwrap(), true, tx);
    let deadline = Instant::now() + timeout;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut so, mut se) = (LineSplitter::default(), LineSplitter::default());
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((is_err, bytes)) => {
                let (all, split) = if is_err {
                    (&mut err, &mut se)
                } else {
                    (&mut out, &mut so)
                };
                all.extend_from_slice(&bytes);
                split.push(&bytes).iter().for_each(|l| on_line(l));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
        }
    }
    for l in so.finish().iter().chain(se.finish().iter()) {
        on_line(l);
    }
    let status = child.wait().map_err(|e| RunError::Spawn(e.to_string()))?;
    Ok(Output {
        code: status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sysroot_exe(rel: &str) -> PathBuf {
        PathBuf::from(std::env::var("SystemRoot").unwrap()).join(rel)
    }

    fn cmd() -> Launcher {
        Launcher {
            program: sysroot_exe(r"System32\cmd.exe"),
            prefix: vec!["/C".into()],
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_multibyte_character_split_across_reads_is_decoded_whole() {
        let bytes = "caf\u{e9} \u{1f600}ok".as_bytes();
        for cut in 1..bytes.len() {
            let mut sp = LineSplitter::default();
            let mut lines = sp.push(&bytes[..cut]);
            lines.extend(sp.push(&bytes[cut..]));
            lines.extend(sp.finish());
            assert_eq!(lines, ["caf\u{e9} \u{1f600}ok"], "cut at {cut}");
        }
    }

    #[test]
    fn lines_split_on_crlf_and_progress_redraws_and_spinner_frames_are_dropped() {
        let mut sp = LineSplitter::default();
        let mut l = sp.push(b"  -\r  \\\r  |\r\nInstalling\r\n\r\nDone");
        l.extend(sp.finish());
        assert_eq!(l, ["Installing", "Done"]);
    }

    #[test]
    fn output_is_captured_and_streamed_with_the_exit_code() {
        let mut seen = Vec::new();
        let o = run(
            &cmd(),
            &s(&["echo one& echo two& exit 3"]),
            Duration::from_secs(10),
            |l| seen.push(l.to_string()),
        )
        .unwrap();
        assert_eq!(o.code, 3);
        assert_eq!(seen, ["one", "two"]);
        assert!(o.stdout.contains("one"));
    }

    #[test]
    fn a_hung_process_is_killed_at_the_timeout() {
        let t = Instant::now();
        let r = run(
            &cmd(),
            &s(&["ping -n 20 127.0.0.1"]),
            Duration::from_millis(400),
            |_| {},
        );
        assert_eq!(r, Err(RunError::TimedOut));
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_missing_program_is_a_spawn_error_not_a_panic() {
        let l = Launcher {
            program: PathBuf::from(r"C:\definitely\not\here.exe"),
            prefix: vec![],
        };
        assert!(matches!(
            run(&l, &[], Duration::from_secs(1), |_| {}),
            Err(RunError::Spawn(_))
        ));
    }

    #[test]
    fn hostile_text_reaches_the_program_as_one_argument() {
        // A shell would run `echo pwned` if the text were joined into a command string.
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("args.ps1");
        std::fs::write(&script, "$args.Count; $args[0]").unwrap();
        let l = Launcher {
            program: sysroot_exe(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
            prefix: s(&[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.to_string_lossy(),
            ]),
        };
        let o = run(&l, &s(&["a & echo pwned"]), Duration::from_secs(20), |_| {}).unwrap();
        let lines: Vec<&str> = o.stdout.lines().collect();
        assert_eq!(lines, ["1", "a & echo pwned"]);
    }
}
