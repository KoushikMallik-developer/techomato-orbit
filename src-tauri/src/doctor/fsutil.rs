//! Folder sizing and clearing that never follows links out of the folder it was given.

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const REPARSE_POINT: u32 = 0x400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub bytes: u64,
    /// False when the time budget ran out first, so `bytes` is a lower bound.
    pub complete: bool,
}

fn is_link(m: &fs::Metadata) -> bool {
    m.file_attributes() & REPARSE_POINT != 0
}

/// Total size of the files under `root`, stopping after `budget`. Links and junctions are not
/// followed.
pub fn dir_size(root: &Path, budget: Duration) -> Size {
    let deadline = Instant::now() + budget;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            if Instant::now() > deadline {
                return Size {
                    bytes,
                    complete: false,
                };
            }
            let Ok(m) = entry.path().symlink_metadata() else {
                continue;
            };
            if is_link(&m) {
                continue;
            }
            if m.is_dir() {
                stack.push(entry.path());
            } else {
                bytes += m.len();
            }
        }
    }
    Size {
        bytes,
        complete: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Freed {
    pub bytes: u64,
    pub files: u64,
    /// Entries left alone because they were in use or not allowed.
    pub skipped: u64,
}

/// Deletes what is inside `dir` (never `dir` itself), file by file so `freed` is exact. With
/// `older_than`, only files not modified for that long are removed. Folders go once empty. Links and junctions are skipped.
pub fn clear_dir(dir: &Path, older_than: Option<Duration>) -> Freed {
    let cutoff = older_than.and_then(|d| SystemTime::now().checked_sub(d));
    let mut freed = Freed::default();
    clear_into(dir, cutoff, &mut freed);
    freed
}

fn clear_into(dir: &Path, cutoff: Option<SystemTime>, freed: &mut Freed) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    for path in entries {
        let Ok(m) = path.symlink_metadata() else {
            continue;
        };
        if is_link(&m) {
            freed.skipped += 1;
            continue;
        }
        if m.is_dir() {
            clear_into(&path, cutoff, freed);
            let _ = fs::remove_dir(&path); // only succeeds when empty
        } else {
            let old_enough = cutoff.is_none_or(|c| m.modified().is_ok_and(|t| t < c));
            if !old_enough {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                freed.bytes += m.len();
                freed.files += 1;
            } else {
                freed.skipped += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn file(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        File::create(p).unwrap().write_all(&vec![7u8; n]).unwrap();
    }

    #[test]
    fn sizes_add_up_across_folders() {
        let d = tempfile::tempdir().unwrap();
        file(&d.path().join("a.bin"), 100);
        file(&d.path().join("sub/b.bin"), 50);
        let s = dir_size(d.path(), Duration::from_secs(5));
        assert_eq!(
            s,
            Size {
                bytes: 150,
                complete: true
            }
        );
        assert_eq!(
            dir_size(&d.path().join("missing"), Duration::from_secs(1)).bytes,
            0
        );
    }

    #[test]
    fn an_exhausted_budget_reports_a_lower_bound() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..50 {
            file(&d.path().join(format!("f{i}")), 10);
        }
        let s = dir_size(d.path(), Duration::ZERO);
        assert!(!s.complete);
    }

    #[test]
    fn clearing_empties_the_folder_but_keeps_the_folder() {
        let d = tempfile::tempdir().unwrap();
        file(&d.path().join("a.bin"), 100);
        file(&d.path().join("sub/deep/b.bin"), 50);
        let f = clear_dir(d.path(), None);
        assert_eq!(f.bytes, 150);
        assert!(d.path().exists());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn age_limited_clearing_keeps_new_files() {
        let d = tempfile::tempdir().unwrap();
        file(&d.path().join("new.bin"), 10);
        file(&d.path().join("sub/new2.bin"), 10);
        let f = clear_dir(d.path(), Some(Duration::from_secs(24 * 3600)));
        assert_eq!(f.files, 0);
        assert!(d.path().join("new.bin").exists() && d.path().join("sub/new2.bin").exists());
        // With a zero age limit everything counts as old.
        let g = clear_dir(d.path(), Some(Duration::ZERO));
        assert_eq!(g.files, 2);
    }

    #[test]
    fn a_junction_is_skipped_and_its_target_survives() {
        let d = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        file(&target.path().join("keep.bin"), 10);
        let link = d.path().join("link");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(target.path())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(
            ok,
            "mklink /J failed, so this test cannot check junction handling"
        );
        let f = clear_dir(d.path(), None);
        assert_eq!(f.skipped, 1);
        assert!(target.path().join("keep.bin").exists());
        assert_eq!(dir_size(d.path(), Duration::from_secs(5)).bytes, 0);
    }
}
