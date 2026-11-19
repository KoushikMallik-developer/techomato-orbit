//! Pure rules: measurements in, findings out. Thresholds live here and nowhere else, and every
//! finding carries an honest tag (clearing disk space is `Storage`, never `Speed`).

use super::fsutil::Size;
use super::parse::{PowerScheme, StartupItem};
use super::{human_bytes, CacheId, Finding, Fix, Severity, Tag};

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

pub fn cache_label(id: CacheId) -> &'static str {
    match id {
        CacheId::Npm => "npm cache",
        CacheId::Nuget => "NuGet cache",
        CacheId::Pip => "pip cache",
        CacheId::Temp => "Temporary files",
        CacheId::Chrome => "Chrome cache",
        CacheId::Edge => "Edge cache",
    }
}

pub fn cache_key(id: CacheId) -> &'static str {
    match id {
        CacheId::Npm => "npm",
        CacheId::Nuget => "nuget",
        CacheId::Pip => "pip",
        CacheId::Temp => "temp",
        CacheId::Chrome => "chrome",
        CacheId::Edge => "edge",
    }
}

pub fn disk_finding(mount: &str, total: u64, free: u64) -> Finding {
    let pct = if total == 0 {
        100.0
    } else {
        free as f64 * 100.0 / total as f64
    };
    let severity = if total == 0 {
        Severity::Ok
    } else if pct < 5.0 || free < 5 * GB {
        Severity::Critical
    } else if pct < 10.0 || free < 15 * GB {
        Severity::Warn
    } else {
        Severity::Ok
    };
    Finding::new(
        format!("disk-{mount}"),
        "disk",
        Tag::Storage,
        severity,
        format!("Drive {mount}"),
        format!(
            "{} free of {} ({:.0}% free)",
            human_bytes(free),
            human_bytes(total),
            pct
        ),
    )
}

pub fn memory_finding(total: u64, available: u64) -> Finding {
    let pct = if total == 0 {
        100.0
    } else {
        available as f64 * 100.0 / total as f64
    };
    let severity = if pct < 8.0 {
        Severity::Critical
    } else if pct < 15.0 {
        Severity::Warn
    } else {
        Severity::Ok
    };
    Finding::new(
        "memory",
        "memory",
        Tag::Speed,
        severity,
        "Memory",
        format!(
            "{} available of {} ({:.0}% free). Low memory makes everything slower.",
            human_bytes(available),
            human_bytes(total),
            pct
        ),
    )
}

/// Edge writes one auto-launch entry per profile; they are reported as one row.
const EDGE_PREFIX: &str = "MicrosoftEdgeAutoLaunch_";

/// One summary, one finding per enabled startup item, and one row for all Edge auto-launch entries.
pub fn startup_findings(items: &[StartupItem]) -> Vec<Finding> {
    let (edge, other): (Vec<&StartupItem>, Vec<&StartupItem>) = items
        .iter()
        .filter(|i| i.enabled)
        .partition(|i| i.name.starts_with(EDGE_PREFIX));
    let entries = other.len() + usize::from(!edge.is_empty());
    let severity = if entries >= 10 {
        Severity::Warn
    } else {
        Severity::Ok
    };
    let mut out = vec![Finding::new(
        "startup-summary",
        "startup",
        Tag::Speed,
        severity,
        "Startup entries",
        format!(
            "Startup entries in your own list: {entries}. Programs started for all users are not counted. Each one adds to sign-in time."
        ),
    )];
    for i in other {
        out.push(
            Finding::new(
                format!("startup-{}", i.name),
                "startup",
                Tag::Speed,
                Severity::Info,
                format!("Starts with Windows: {}", i.name),
                i.command.clone(),
            )
            .with_fix(Fix::DisableStartup {
                name: i.name.clone(),
            }),
        );
    }
    if !edge.is_empty() {
        out.push(Finding::new(
            "startup-edge",
            "startup",
            Tag::Speed,
            Severity::Info,
            format!("Microsoft Edge: {} auto-launch entries", edge.len()),
            "Edge adds one entry per profile. Turn them off in Task Manager's Startup tab if you do not want Edge to start at sign-in.",
        ));
    }
    out
}

pub fn power_finding(schemes: &[PowerScheme]) -> Option<Finding> {
    let active = schemes.iter().find(|s| s.active)?;
    let name = active.name.to_lowercase();
    let balanced = schemes
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("balanced"));
    let (severity, detail) = if name.contains("saver") {
        (
            Severity::Warn,
            "Power saver slows the processor down to save energy.".to_string(),
        )
    } else if name.contains("high") || name.contains("ultimate") {
        (
            Severity::Info,
            "This plan favours speed and uses more power than Balanced.".to_string(),
        )
    } else {
        (
            Severity::Ok,
            "Windows balances speed and power.".to_string(),
        )
    };
    let f = Finding::new(
        "power",
        "power",
        Tag::Speed,
        severity,
        format!("Power plan: {}", active.name),
        detail,
    );
    Some(match balanced {
        Some(b) if severity != Severity::Ok && !b.active => f.with_fix(Fix::SetPowerPlan {
            guid: b.guid.clone(),
        }),
        _ => f,
    })
}

pub fn reboot_finding(pending: bool) -> Finding {
    Finding::new(
        "reboot",
        "system",
        Tag::Info,
        if pending {
            Severity::Warn
        } else {
            Severity::Ok
        },
        if pending {
            "A restart is pending"
        } else {
            "No restart pending"
        },
        if pending {
            "Windows is waiting for a restart to finish an update or install."
        } else {
            "Nothing is waiting for a restart."
        },
    )
}

pub fn cache_finding(id: CacheId, size: Size) -> Finding {
    let severity = if size.bytes >= GB {
        Severity::Warn
    } else if size.bytes >= 100 * MB {
        Severity::Info
    } else {
        Severity::Ok
    };
    let at_least = if size.complete { "" } else { "at least " };
    let f = Finding::new(
        format!("cache-{}", cache_key(id)),
        "cache",
        Tag::Storage,
        severity,
        cache_label(id),
        format!(
            "Uses {at_least}{}. Clearing it frees disk space; it does not make the PC faster.",
            human_bytes(size.bytes)
        ),
    );
    if size.bytes > 0 {
        f.with_fix(Fix::ClearCache { id })
    } else {
        f
    }
}

pub fn wsl_finding(distros: usize, vm_running: bool) -> Finding {
    Finding::new(
        "wsl",
        "tools",
        Tag::Info,
        Severity::Info,
        "WSL",
        match (distros, vm_running) {
            (0, _) => "No Linux distributions installed.".to_string(),
            (n, true) => format!("{n} distribution(s) installed; the WSL virtual machine is running and holds memory."),
            (n, false) => format!("{n} distribution(s) installed; the WSL virtual machine is stopped."),
        },
    )
}

pub fn docker_finding(running: bool, disk_bytes: Option<u64>) -> Finding {
    let disk = disk_bytes
        .map(|b| format!(" Its disk image is {}.", human_bytes(b)))
        .unwrap_or_default();
    Finding::new(
        "docker",
        "tools",
        Tag::Info,
        Severity::Info,
        "Docker Desktop",
        format!(
            "{}{disk}",
            if running { "Running." } else { "Not running." }
        ),
    )
}

pub fn ollama_finding(running: bool, models_bytes: Option<u64>) -> Finding {
    let models = models_bytes
        .map(|b| format!(" Downloaded models use {}.", human_bytes(b)))
        .unwrap_or_default();
    Finding::new(
        "ollama",
        "tools",
        Tag::Info,
        Severity::Info,
        "Ollama",
        format!(
            "{}{models}",
            if running { "Running." } else { "Not running." }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, enabled: bool) -> StartupItem {
        StartupItem {
            name: name.into(),
            command: format!("C:\\{name}.exe"),
            enabled,
        }
    }

    fn scheme(name: &str, active: bool) -> PowerScheme {
        PowerScheme {
            guid: format!("381b4222-f694-41f0-9685-ff5bb260df2{}", name.len() % 10),
            name: name.into(),
            active,
        }
    }

    #[test]
    fn disk_thresholds() {
        assert_eq!(
            disk_finding("C:", 500 * GB, 300 * GB).severity,
            Severity::Ok
        );
        assert_eq!(
            disk_finding("C:", 500 * GB, 40 * GB).severity,
            Severity::Warn
        );
        assert_eq!(
            disk_finding("C:", 500 * GB, 4 * GB).severity,
            Severity::Critical
        );
        assert_eq!(
            disk_finding("C:", 1000 * GB, 40 * GB).severity,
            Severity::Critical
        ); // under 5%
        assert_eq!(disk_finding("C:", 0, 0).severity, Severity::Ok);
        assert_eq!(disk_finding("C:", GB, GB).tag, Tag::Storage);
    }

    #[test]
    fn memory_is_a_speed_finding_with_thresholds() {
        let ok = memory_finding(16 * GB, 8 * GB);
        assert_eq!((ok.severity, ok.tag), (Severity::Ok, Tag::Speed));
        assert_eq!(memory_finding(16 * GB, 2 * GB).severity, Severity::Warn);
        assert_eq!(memory_finding(16 * GB, GB).severity, Severity::Critical);
    }

    #[test]
    fn only_enabled_startup_items_get_a_fix_and_many_of_them_warn() {
        let f = startup_findings(&[item("A", true), item("B", false)]);
        assert_eq!(f.len(), 2); // summary + A
        assert!(f[0].fix.is_none());
        assert_eq!(f[1].fix, Some(Fix::DisableStartup { name: "A".into() }));
        assert_eq!(f[0].severity, Severity::Ok);
        let many: Vec<_> = (0..10).map(|i| item(&format!("P{i}"), true)).collect();
        assert_eq!(startup_findings(&many)[0].severity, Severity::Warn);
    }

    #[test]
    fn edge_auto_launch_entries_are_one_row_and_count_once() {
        let mut items: Vec<_> = (0..30)
            .map(|i| item(&format!("MicrosoftEdgeAutoLaunch_{i:032X}"), true))
            .collect();
        items.push(item("Docker", true));
        let f = startup_findings(&items);
        assert_eq!(f.len(), 3, "summary + Docker + one Edge row");
        assert!(f[0].detail.contains("list: 2."), "{}", f[0].detail);
        assert_eq!(f[0].severity, Severity::Ok);
        let edge = f.iter().find(|x| x.id == "startup-edge").unwrap();
        assert!(edge.title.contains("30") && edge.fix.is_none());
    }

    #[test]
    fn power_saver_offers_balanced_and_balanced_offers_nothing() {
        let saver =
            power_finding(&[scheme("Balanced", false), scheme("Power saver", true)]).unwrap();
        assert_eq!(saver.severity, Severity::Warn);
        assert!(matches!(saver.fix, Some(Fix::SetPowerPlan { .. })));
        let balanced =
            power_finding(&[scheme("Balanced", true), scheme("Power saver", false)]).unwrap();
        assert_eq!((balanced.severity, balanced.fix), (Severity::Ok, None));
        assert!(power_finding(&[scheme("Balanced", false)]).is_none());
        // No Balanced plan to go back to: warn, but offer no fix.
        let none = power_finding(&[scheme("Power saver", true)]).unwrap();
        assert_eq!(none.fix, None);
    }

    #[test]
    fn cache_findings_are_storage_and_never_claim_speed() {
        let big = cache_finding(
            CacheId::Npm,
            Size {
                bytes: 2 * GB,
                complete: true,
            },
        );
        assert_eq!((big.tag, big.severity), (Tag::Storage, Severity::Warn));
        assert!(big.detail.contains("not make the PC faster"));
        assert_eq!(big.fix, Some(Fix::ClearCache { id: CacheId::Npm }));
        let empty = cache_finding(
            CacheId::Pip,
            Size {
                bytes: 0,
                complete: true,
            },
        );
        assert_eq!((empty.severity, empty.fix), (Severity::Ok, None));
        let partial = cache_finding(
            CacheId::Temp,
            Size {
                bytes: 5 * GB,
                complete: false,
            },
        );
        assert!(partial.detail.contains("at least 5.0 GB"));
    }

    #[test]
    fn nothing_here_tags_a_cleanup_as_speed() {
        for id in [
            CacheId::Npm,
            CacheId::Nuget,
            CacheId::Pip,
            CacheId::Temp,
            CacheId::Chrome,
            CacheId::Edge,
        ] {
            assert_eq!(
                cache_finding(
                    id,
                    Size {
                        bytes: GB,
                        complete: true
                    }
                )
                .tag,
                Tag::Storage
            );
        }
        assert_eq!(disk_finding("D:", GB, 0).tag, Tag::Storage);
    }

    #[test]
    fn tool_findings_are_info() {
        assert_eq!(wsl_finding(2, true).tag, Tag::Info);
        assert!(docker_finding(true, Some(3 * GB)).detail.contains("3.0 GB"));
        assert!(ollama_finding(false, None)
            .detail
            .starts_with("Not running"));
        assert_eq!(reboot_finding(true).severity, Severity::Warn);
        assert_eq!(reboot_finding(false).severity, Severity::Ok);
    }
}
