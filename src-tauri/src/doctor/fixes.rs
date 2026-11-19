//! The closed set of fixes. Each is validated against the live machine before it runs, so a
//! stale or forged request (an unknown startup entry, a power plan that does not exist, a cache
//! whose browser is open) is refused instead of acted on.

use super::fsutil::clear_dir;
use super::parse::{is_guid, PowerScheme, StartupItem};
use super::probes::{self, system_tool, APPROVED_KEY};
use super::rules::cache_label;
use super::{CacheId, Fix};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub message: String,
    pub freed_bytes: u64,
}

/// What the confirm dialog says. Cache clearing is described as freeing space, never as a speed-up.
pub fn describe(fix: &Fix) -> (String, String) {
    match fix {
        Fix::ClearCache { id } => (
            format!("Clear {}", cache_label(*id)),
            match id {
                CacheId::Temp => "Delete temporary files that have not changed for a day? This frees disk space.".to_string(),
                CacheId::Chrome | CacheId::Edge => format!("Delete the {}? Close the browser first. This frees disk space; pages may load a little slower once while the cache refills.", cache_label(*id)),
                _ => format!("Delete the {}? It is rebuilt when needed. This frees disk space.", cache_label(*id)),
            },
        ),
        Fix::DisableStartup { name } => (
            "Disable startup program".to_string(),
            format!("Stop {name} from starting with Windows? You can turn it back on in Task Manager's Startup tab."),
        ),
        Fix::SetPowerPlan { .. } => (
            "Switch power plan".to_string(),
            "Switch to the Balanced power plan?".to_string(),
        ),
    }
}

/// Only programs listed in the user's own Run key that are currently enabled may be disabled.
pub fn validate_startup<'a>(
    name: &str,
    items: &'a [StartupItem],
) -> Result<&'a StartupItem, String> {
    items
        .iter()
        .find(|i| i.name == name && i.enabled)
        .ok_or_else(|| format!("'{name}' is not an enabled startup program any more."))
}

pub fn validate_power<'a>(
    guid: &str,
    schemes: &'a [PowerScheme],
) -> Result<&'a PowerScheme, String> {
    if !is_guid(guid) {
        return Err("That is not a power plan id.".into());
    }
    let guid = guid.to_lowercase();
    schemes
        .iter()
        .find(|s| s.guid == guid)
        .ok_or_else(|| "That power plan no longer exists.".to_string())
}

/// A cache folder must sit well below a drive root before anything is deleted in it.
pub fn is_safe_root(p: &Path) -> bool {
    p.is_absolute() && p.components().count() >= 5
}

fn browser_exe(id: CacheId) -> Option<&'static str> {
    match id {
        CacheId::Chrome => Some("chrome.exe"),
        CacheId::Edge => Some("msedge.exe"),
        _ => None,
    }
}

pub fn run(fix: &Fix) -> Result<Outcome, String> {
    match fix {
        Fix::ClearCache { id } => {
            if let Some(exe) = browser_exe(*id) {
                if probes::running_processes().contains(exe) {
                    return Err(format!(
                        "Close {} first, then try again.",
                        cache_label(*id).replace(" cache", "")
                    ));
                }
            }
            let path = probes::cache_path(*id).ok_or("Could not locate that folder.")?;
            if !is_safe_root(&path) {
                return Err(format!(
                    "Orbit will not clear {} because it is too close to the top of the drive.",
                    path.display()
                ));
            }
            if !path.is_dir() {
                return Err("That folder is not there.".into());
            }
            let age = (*id == CacheId::Temp).then(|| Duration::from_secs(24 * 3600));
            let freed = clear_dir(&path, age);
            let note = if freed.skipped > 0 {
                format!(" {} item(s) were in use and left alone.", freed.skipped)
            } else {
                String::new()
            };
            Ok(Outcome {
                message: format!("Freed {}.{note}", super::human_bytes(freed.bytes)),
                freed_bytes: freed.bytes,
            })
        }
        Fix::DisableStartup { name } => {
            let items = probes::read_startup_items();
            validate_startup(name, &items)?;
            // The same switch Task Manager uses: a disabled marker, reversible, nothing deleted.
            let out = system_tool(
                "reg.exe",
                &[
                    "add",
                    APPROVED_KEY,
                    "/v",
                    name,
                    "/t",
                    "REG_BINARY",
                    "/d",
                    "030000000000000000000000",
                    "/f",
                ],
            )
            .ok_or("Could not run reg.exe.")?;
            if out.code == 0 {
                Ok(Outcome {
                    message: format!("{name} will no longer start with Windows."),
                    freed_bytes: 0,
                })
            } else {
                Err("Windows refused the change.".into())
            }
        }
        Fix::SetPowerPlan { guid } => {
            let schemes = probes::read_power_schemes();
            let s = validate_power(guid, &schemes)?;
            let out = system_tool("powercfg.exe", &["/setactive", &s.guid])
                .ok_or("Could not run powercfg.")?;
            if out.code == 0 {
                Ok(Outcome {
                    message: format!("Power plan is now {}.", s.name),
                    freed_bytes: 0,
                })
            } else {
                Err("Windows refused the change.".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, enabled: bool) -> StartupItem {
        StartupItem {
            name: name.into(),
            command: String::new(),
            enabled,
        }
    }

    #[test]
    fn only_enabled_startup_entries_can_be_disabled() {
        let items = [item("A", true), item("B", false)];
        assert!(validate_startup("A", &items).is_ok());
        assert!(validate_startup("B", &items).is_err());
        assert!(validate_startup("C", &items).is_err());
        assert!(validate_startup("A\" /f & calc", &items).is_err());
    }

    #[test]
    fn a_power_plan_must_be_a_real_listed_guid() {
        let s = [PowerScheme {
            guid: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
            name: "Balanced".into(),
            active: false,
        }];
        assert!(validate_power("381B4222-F694-41F0-9685-FF5BB260DF2E", &s).is_ok());
        assert!(validate_power("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", &s).is_err());
        assert!(validate_power("381b4222-f694-41f0-9685-ff5bb260df2e & calc", &s).is_err());
        assert!(validate_power("", &s).is_err());
    }

    #[test]
    fn shallow_folders_are_never_cleared() {
        assert!(!is_safe_root(Path::new(r"C:\")));
        assert!(!is_safe_root(Path::new(r"C:\Users")));
        assert!(!is_safe_root(Path::new(r"C:\Users\U")));
        assert!(!is_safe_root(Path::new(r"relative\a\b\c\d")));
        assert!(is_safe_root(Path::new(
            r"C:\Users\U\AppData\Local\npm-cache"
        )));
    }

    #[test]
    fn confirm_text_never_promises_speed_for_a_cleanup() {
        for id in probes::ALL_CACHES {
            let (_, msg) = describe(&Fix::ClearCache { id });
            assert!(msg.contains("frees disk space"), "{msg}");
            assert!(
                !msg.to_lowercase().contains("faster") || msg.contains("slower"),
                "{msg}"
            );
        }
    }
}
