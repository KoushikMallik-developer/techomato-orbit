//! Doctor: read-only probes that report on the health of this PC, each finding tagged honestly
//! (speed, storage or info). Probes only read; they never start a service or daemon to look at it.
//! Fixes run only when the user presses a button and confirms, and are a closed set.

pub mod fixes;
pub mod fsutil;
pub mod parse;
pub mod probes;
pub mod rules;

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// What a finding is about. Disk cleanup is `Storage`, never `Speed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Speed,
    Storage,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ok,
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheId {
    Npm,
    Nuget,
    Pip,
    Temp,
    Chrome,
    Edge,
}

/// The closed set of fixes. The UI sends one back exactly as a finding offered it, and it is
/// checked against the live machine again before anything happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Fix {
    ClearCache { id: CacheId },
    DisableStartup { name: String },
    SetPowerPlan { guid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable id, so a rescan replaces the same row.
    pub id: String,
    /// disk, cache, startup, power, memory, system or tools.
    pub category: &'static str,
    pub title: String,
    pub detail: String,
    pub tag: Tag,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

impl Finding {
    pub fn new(
        id: impl Into<String>,
        category: &'static str,
        tag: Tag,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            title: title.into(),
            detail: detail.into(),
            tag,
            severity,
            fix: None,
        }
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// `1.5 GB`, `320 MB`.
pub fn human_bytes(b: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.0} MB", f / MB)
    } else {
        format!("{} KB", b / 1024)
    }
}

/// Longest the scan waits for the slowest probe.
const PROBE_LIMIT: Duration = Duration::from_secs(20);

/// Runs every probe (in parallel) and hands each finding to `emit` as soon as it exists.
pub fn scan(emit: &dyn Fn(Finding)) {
    scan_with(probes::ALL, PROBE_LIMIT, emit);
}

/// Each probe has its own thread. One that is stuck (an offline network drive, say) is given up
/// on after `limit`, so the scan still finishes and can be run again.
fn scan_with(list: &[fn(probes::Emit)], limit: Duration, emit: &dyn Fn(Finding)) {
    use std::sync::mpsc::{channel, RecvTimeoutError};
    enum Msg {
        Found(Finding),
        Done,
    }
    let (tx, rx) = channel();
    for &probe in list {
        let tx = tx.clone();
        std::thread::spawn(move || {
            probe(&|f| {
                let _ = tx.send(Msg::Found(f));
            });
            let _ = tx.send(Msg::Done);
        });
    }
    drop(tx);
    let deadline = Instant::now() + limit;
    let mut pending = list.len();
    while pending > 0 {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Msg::Found(f)) => emit(f),
            Ok(Msg::Done) => pending -= 1,
            Err(RecvTimeoutError::Timeout) => {
                emit(Finding::new(
                    "scan-timeout",
                    "system",
                    Tag::Info,
                    Severity::Info,
                    "Some checks did not finish",
                    "A check took too long, often an offline network drive. Scan again later.",
                ));
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_bytes(1536 * 1024 * 1024), "1.5 GB");
        assert_eq!(human_bytes(320 * 1024 * 1024), "320 MB");
        assert_eq!(human_bytes(5 * 1024), "5 KB");
    }

    #[test]
    fn a_fix_crosses_ipc_as_a_tagged_object() {
        let f: Fix = serde_json::from_str(r#"{"kind":"clearCache","id":"npm"}"#).unwrap();
        assert_eq!(f, Fix::ClearCache { id: CacheId::Npm });
        assert!(serde_json::from_str::<Fix>(r#"{"kind":"clearCache","id":"windows"}"#).is_err());
        assert!(serde_json::from_str::<Fix>(r#"{"kind":"runCommand","cmd":"x"}"#).is_err());
    }

    fn quick(emit: probes::Emit) {
        emit(Finding::new(
            "quick",
            "disk",
            Tag::Info,
            Severity::Ok,
            "t",
            "d",
        ));
    }
    fn stuck(_: probes::Emit) {
        std::thread::sleep(Duration::from_secs(5));
    }

    #[test]
    fn a_stuck_probe_is_given_up_on_and_the_rest_still_report() {
        let seen = std::sync::Mutex::new(Vec::new());
        let t = Instant::now();
        scan_with(&[quick, stuck], Duration::from_millis(300), &|f| {
            seen.lock().unwrap().push(f.id)
        });
        assert!(t.elapsed() < Duration::from_secs(3));
        assert_eq!(*seen.lock().unwrap(), ["quick", "scan-timeout"]);
    }

    #[test]
    fn severity_orders_from_ok_to_critical() {
        assert!(Severity::Critical > Severity::Warn && Severity::Warn > Severity::Info);
        assert!(Severity::Info > Severity::Ok);
    }
}
