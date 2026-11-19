//! The probes. Each one only reads: registry queries, folder sizes, memory and disk counters,
//! and the process list. None of them starts a service, a VM or a daemon to look at it.

use super::fsutil::{dir_size, Size};
use super::parse::{self, parse_power_list, parse_reg_query, startup_items};
use super::rules;
use super::{CacheId, Finding};
use crate::pm::run::{self, Launcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub type Emit<'a> = &'a (dyn Fn(Finding) + Sync);

pub const ALL: &[fn(Emit)] = &[disk, memory, startup, power, reboot, caches, tools];

pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const APPROVED_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

const SIZE_BUDGET: Duration = Duration::from_secs(5);

fn env_path(var: &str) -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os(var)?);
    p.is_absolute().then_some(p)
}

/// The folder a cache lives in, built only from the two roots Windows gives us.
pub fn cache_path_in(id: CacheId, local: &Path, temp: &Path) -> PathBuf {
    match id {
        CacheId::Npm => local.join("npm-cache"),
        CacheId::Nuget => local.join(r"NuGet\v3-cache"),
        CacheId::Pip => local.join(r"pip\Cache"),
        CacheId::Temp => temp.to_path_buf(),
        CacheId::Chrome => local.join(r"Google\Chrome\User Data\Default\Cache"),
        CacheId::Edge => local.join(r"Microsoft\Edge\User Data\Default\Cache"),
    }
}

pub fn cache_path(id: CacheId) -> Option<PathBuf> {
    Some(cache_path_in(
        id,
        &env_path("LOCALAPPDATA")?,
        &env_path("TEMP")?,
    ))
}

pub const ALL_CACHES: [CacheId; 6] = [
    CacheId::Npm,
    CacheId::Nuget,
    CacheId::Pip,
    CacheId::Temp,
    CacheId::Chrome,
    CacheId::Edge,
];

/// A Windows tool from System32, run with an argument array and a short timeout.
pub fn system_tool(exe: &str, args: &[&str]) -> Option<run::Output> {
    let root = env_path("SystemRoot")?;
    let l = Launcher {
        program: root.join("System32").join(exe),
        prefix: vec![],
    };
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    run::run(&l, &args, Duration::from_secs(10), |_| {}).ok()
}

pub fn reg_query(key: &str) -> Vec<parse::RegValue> {
    system_tool("reg.exe", &["query", key])
        .map(|o| parse_reg_query(&o.stdout))
        .unwrap_or_default()
}

pub fn read_startup_items() -> Vec<parse::StartupItem> {
    startup_items(&reg_query(RUN_KEY), &reg_query(APPROVED_KEY))
}

pub fn read_power_schemes() -> Vec<parse::PowerScheme> {
    system_tool("powercfg.exe", &["/list"])
        .map(|o| parse_power_list(&o.stdout))
        .unwrap_or_default()
}

/// Lower-cased names of every running process.
pub fn running_processes() -> HashSet<String> {
    use sysinfo::{ProcessesToUpdate, System};
    let Ok(mut sys) = System::new() else {
        return HashSet::new();
    };
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_lowercase())
        .collect()
}

fn disk(emit: Emit) {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
        return;
    };
    for d in &disks {
        if d.is_removable() || d.total_space() == 0 {
            continue;
        }
        let mount = d
            .mount_point()
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_string();
        emit(rules::disk_finding(
            &mount,
            d.total_space(),
            d.available_space(),
        ));
    }
}

fn memory(emit: Emit) {
    use sysinfo::System;
    let Ok(mut sys) = System::new() else { return };
    sys.refresh_memory();
    emit(rules::memory_finding(
        sys.total_memory(),
        sys.available_memory(),
    ));
}

fn startup(emit: Emit) {
    for f in rules::startup_findings(&read_startup_items()) {
        emit(f);
    }
}

fn power(emit: Emit) {
    if let Some(f) = rules::power_finding(&read_power_schemes()) {
        emit(f);
    }
}

fn reboot(emit: Emit) {
    let keys = [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
    ];
    let pending = keys
        .iter()
        .any(|k| system_tool("reg.exe", &["query", k]).is_some_and(|o| o.code == 0));
    emit(rules::reboot_finding(pending));
}

fn caches(emit: Emit) {
    std::thread::scope(|s| {
        for id in ALL_CACHES {
            s.spawn(move || {
                let Some(path) = cache_path(id).filter(|p| p.is_dir()) else {
                    return;
                };
                emit(rules::cache_finding(id, dir_size(&path, SIZE_BUDGET)));
            });
        }
    });
}

fn file_len(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

fn tools(emit: Emit) {
    let running = running_processes();
    let any = |names: &[&str]| names.iter().any(|n| running.contains(*n));

    // WSL: distributions are registered under Lxss; the VM shows up as vmmem.
    let lxss = system_tool(
        "reg.exe",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss",
        ],
    )
    .map(|o| {
        o.stdout
            .lines()
            .filter(|l| l.starts_with("HKEY_") && l.contains(r"\Lxss\{"))
            .count()
    })
    .unwrap_or(0);
    let vm = any(&["vmmem", "vmmemwsl"]);
    if lxss > 0 || vm {
        emit(rules::wsl_finding(lxss, vm));
    }

    let local = env_path("LOCALAPPDATA");
    let docker_disk = local.as_ref().and_then(|l| {
        file_len(&l.join(r"Docker\wsl\disk\docker_data.vhdx"))
            .or_else(|| file_len(&l.join(r"Docker\wsl\data\ext4.vhdx")))
    });
    let docker_up = any(&["docker desktop.exe", "com.docker.backend.exe"]);
    if docker_up || docker_disk.is_some() {
        emit(rules::docker_finding(docker_up, docker_disk));
    }

    let models = env_path("USERPROFILE")
        .map(|u| u.join(r".ollama\models"))
        .filter(|p| p.is_dir())
        .map(|p| dir_size(&p, SIZE_BUDGET));
    let ollama_up = any(&["ollama.exe", "ollama app.exe"]);
    if ollama_up || models.is_some() {
        emit(rules::ollama_finding(
            ollama_up,
            models.map(|Size { bytes, .. }| bytes),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn cache_paths_are_built_from_the_two_roots_only() {
        let local = Path::new(r"C:\Users\U\AppData\Local");
        let temp = Path::new(r"C:\Users\U\AppData\Local\Temp");
        for id in ALL_CACHES {
            let p = cache_path_in(id, local, temp);
            assert!(p.starts_with(local), "{id:?} -> {p:?}");
            assert!(p.components().count() >= 5, "{p:?}");
        }
        assert_eq!(cache_path_in(CacheId::Temp, local, temp), temp);
    }

    /// Runs every probe on this machine: nothing may panic and every finding needs a title.
    /// Ignored by default because it reads the real registry, disks and process list.
    #[test]
    #[ignore = "reads this machine"]
    fn a_real_scan_produces_well_formed_findings() {
        let found = Mutex::new(Vec::new());
        super::super::scan(&|f| found.lock().unwrap().push(f));
        let found = found.into_inner().unwrap();
        assert!(found.iter().any(|f| f.id == "memory"), "{found:#?}");
        assert!(found.iter().any(|f| f.category == "disk"));
        assert!(found
            .iter()
            .all(|f| !f.title.is_empty() && !f.id.is_empty()));
        let ids: HashSet<_> = found.iter().map(|f| f.id.clone()).collect();
        assert_eq!(ids.len(), found.len(), "finding ids must be unique");
    }
}
