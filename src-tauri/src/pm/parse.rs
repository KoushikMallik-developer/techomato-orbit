//! Parsers for each manager's text output. They never panic on odd input: anything that is not a
//! recognisable row is skipped.

use super::{Manager, Package};

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Lines with CRLF removed and spinner redraws (text before the last `\r`) dropped.
fn clean_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|l| {
            let l = l.trim_end_matches('\r');
            l.rsplit('\r').next().unwrap_or("").to_string()
        })
        .collect()
}

fn is_rule(l: &str) -> bool {
    let t = l.trim();
    t.len() >= 3 && t.chars().all(|c| c == '-' || c == ' ') && t.contains('-')
}

fn cut(chars: &[char], from: usize, to: Option<usize>) -> String {
    let end = to.unwrap_or(chars.len()).min(chars.len());
    if from >= end {
        return String::new();
    }
    chars[from..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

/// winget prints a fixed-width table (`Name  Id  Version  [Available]  [Source|Match]`), sometimes
/// after spinner characters redrawn with carriage returns, and sometimes followed by summary
/// lines. Columns are cut at the header's character offsets.
pub fn parse_winget_table(text: &str) -> Vec<Package> {
    let lines = clean_lines(text);
    let Some(h) = lines.iter().enumerate().position(|(i, l)| {
        l.starts_with("Name")
            && l.contains("Id")
            && l.contains("Version")
            && lines.get(i + 1).is_some_and(|n| is_rule(n))
    }) else {
        return Vec::new();
    };
    let header: Vec<char> = lines[h].chars().collect();
    let find = |label: &str| -> Option<usize> {
        let l: Vec<char> = label.chars().collect();
        (0..header.len()).find(|&i| header[i..].starts_with(&l) && (i == 0 || header[i - 1] == ' '))
    };
    let (Some(id_at), Some(ver_at)) = (find("Id"), find("Version")) else {
        return Vec::new();
    };
    let avail_at = find("Available");
    let src_at = find("Source");
    // Match (search) and Source (list) are the trailing columns; Match is ignored.
    let match_at = find("Match");
    let mut starts = vec![id_at, ver_at];
    starts.extend(avail_at);
    starts.extend(src_at);
    starts.extend(match_at);
    starts.sort_unstable();
    let next = |at: usize| starts.iter().copied().find(|&s| s > at);

    let mut out = Vec::new();
    for line in &lines[h + 2..] {
        let c: Vec<char> = line.chars().collect();
        if c.len() <= id_at || starts.iter().any(|&s| s < c.len() && c[s - 1] != ' ') {
            continue;
        }
        let id = cut(&c, id_at, Some(ver_at));
        let name = cut(&c, 0, Some(id_at));
        if id.is_empty() || name.is_empty() {
            continue;
        }
        out.push(Package {
            id,
            name,
            version: cut(&c, ver_at, next(ver_at)),
            available: avail_at.and_then(|a| opt(&cut(&c, a, next(a)))),
            source: src_at.and_then(|a| opt(&cut(&c, a, next(a)))),
            manager: Manager::Winget,
        });
    }
    out
}

/// Chocolatey `--limit-output`: `name|version` (list, search) or `name|current|available|pinned`
/// (outdated).
pub fn parse_choco(text: &str, outdated: bool) -> Vec<Package> {
    text.lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.trim().split('|').map(str::trim).collect();
            if p.len() < 2 || p[0].is_empty() || p[1].is_empty() || p[0].contains(' ') {
                return None;
            }
            if outdated && (p.len() < 3 || p[2].is_empty()) {
                return None;
            }
            Some(Package {
                id: p[0].into(),
                name: p[0].into(),
                version: p[1].into(),
                available: outdated.then(|| p[2].to_string()),
                source: None,
                manager: Manager::Choco,
            })
        })
        .collect()
}

/// `scoop export` JSON: `{"apps":[{"Name","Version","Source",...}]}`.
pub fn parse_scoop_export(json: &str) -> Vec<Package> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let s = |a: &serde_json::Value, k: &str| a.get(k).and_then(|x| x.as_str()).map(str::to_string);
    v.get("apps")
        .and_then(|a| a.as_array())
        .map(|apps| {
            apps.iter()
                .filter_map(|a| {
                    let name = s(a, "Name").filter(|n| !n.is_empty())?;
                    Some(Package {
                        id: name.clone(),
                        name,
                        version: s(a, "Version").unwrap_or_default(),
                        available: None,
                        source: s(a, "Source").filter(|x| !x.is_empty()),
                        manager: Manager::Scoop,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Rows (whitespace-split) below a `Name ...` header and its dashes line.
fn scoop_rows(text: &str) -> Vec<Vec<String>> {
    let lines = clean_lines(text);
    let Some(h) = (0..lines.len()).find(|&i| {
        lines[i].trim_start().starts_with("Name") && lines.get(i + 1).is_some_and(|n| is_rule(n))
    }) else {
        return Vec::new();
    };
    lines[h + 2..]
        .iter()
        .map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|r| r.len() >= 2)
        .collect()
}

/// `scoop status` table: `Name  Installed Version  Latest Version  ...`.
pub fn parse_scoop_status(text: &str) -> Vec<Package> {
    scoop_rows(text)
        .into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| Package {
            id: r[0].clone(),
            name: r[0].clone(),
            version: r[1].clone(),
            available: Some(r[2].clone()),
            source: None,
            manager: Manager::Scoop,
        })
        .collect()
}

/// `scoop search` table: `Name  Version  Source  Binaries`.
pub fn parse_scoop_search(text: &str) -> Vec<Package> {
    scoop_rows(text)
        .into_iter()
        .map(|r| Package {
            id: r[0].clone(),
            name: r[0].clone(),
            version: r[1].clone(),
            available: None,
            source: r.get(2).cloned(),
            manager: Manager::Scoop,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINGET_LIST: &str = include_str!("../../tests/fixtures/pm/winget_list.txt");
    const WINGET_UPGRADE: &str = include_str!("../../tests/fixtures/pm/winget_upgrade.txt");
    const WINGET_SEARCH: &str = include_str!("../../tests/fixtures/pm/winget_search.txt");
    const CHOCO_LIST: &str = include_str!("../../tests/fixtures/pm/choco_list.txt");
    const CHOCO_OUTDATED: &str = include_str!("../../tests/fixtures/pm/choco_outdated.txt");
    const CHOCO_SEARCH: &str = include_str!("../../tests/fixtures/pm/choco_search.txt");

    #[test]
    fn winget_list_is_cut_at_the_header_columns() {
        let rows = parse_winget_table(WINGET_LIST);
        assert_eq!(rows.len(), 12, "{rows:#?}");
        let zip = rows.iter().find(|p| p.id == "7zip.7zip").unwrap();
        assert_eq!(zip.name, "7-Zip 22.01");
        assert_eq!(zip.version, "22.01");
        assert_eq!(zip.available.as_deref(), Some("26.06"));
        assert_eq!(zip.source.as_deref(), Some("winget"));
        assert_eq!(zip.manager, Manager::Winget);
        // A package with no source and no update.
        let viewer = rows.iter().find(|p| p.name == "3D Viewer").unwrap();
        assert!(viewer.id.starts_with("MSIX\\"));
        assert_eq!(viewer.available, None);
        assert_eq!(viewer.source, None);
        // An id that itself contains a space stays whole.
        let studio = rows.iter().find(|p| p.name == "Android Studio").unwrap();
        assert_eq!(studio.id, r"ARP\Machine\X64\Android Studio");
    }

    #[test]
    fn winget_upgrade_skips_the_summary_lines() {
        let rows = parse_winget_table(WINGET_UPGRADE);
        assert_eq!(rows.len(), 7, "{rows:#?}");
        assert!(rows.iter().all(|p| p.available.is_some()));
        assert!(rows.iter().all(|p| !p.id.contains("upgrades")));
        assert!(!rows.iter().any(|p| p.name.contains("package(s)")));
    }

    #[test]
    fn winget_search_has_a_match_column_instead_of_available() {
        let rows = parse_winget_table(WINGET_SEARCH);
        assert_eq!(rows.len(), 12, "{rows:#?}");
        let git = rows.iter().find(|p| p.id == "Git.Git").unwrap();
        assert_eq!(git.name, "Git");
        assert_eq!(git.available, None);
    }

    #[test]
    fn spinner_noise_and_crlf_before_the_table_are_ignored() {
        let noisy = format!("   -\r   \\\r   |\r   /\r\n{WINGET_UPGRADE}");
        assert_eq!(parse_winget_table(&noisy).len(), 7);
        let spinner_on_header = format!("  \\\r  |\rName{}", &WINGET_UPGRADE[4..]);
        assert_eq!(parse_winget_table(&spinner_on_header).len(), 7);
    }

    #[test]
    fn empty_and_unrecognised_winget_output_gives_no_rows() {
        assert!(parse_winget_table("").is_empty());
        assert!(
            parse_winget_table("No installed package found matching input criteria.").is_empty()
        );
        assert!(parse_winget_table("\r\n\r\n   \r\n").is_empty());
        assert!(parse_winget_table("Name\r\n----\r\n").is_empty());
    }

    #[test]
    fn choco_list_and_search_are_name_pipe_version() {
        let list = parse_choco(CHOCO_LIST, false);
        assert_eq!(list.len(), 8);
        assert_eq!(list[0].id, "chocolatey");
        assert_eq!(list[0].version, "2.2.2");
        assert_eq!(list[0].manager, Manager::Choco);
        assert_eq!(list[0].available, None);
        assert_eq!(parse_choco(CHOCO_SEARCH, false).len(), 8);
    }

    #[test]
    fn choco_outdated_carries_the_available_version() {
        let rows = parse_choco(CHOCO_OUTDATED, true);
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].version, "2.2.2");
        assert_eq!(rows[0].available.as_deref(), Some("2.7.5"));
    }

    #[test]
    fn choco_noise_lines_are_skipped() {
        let rows = parse_choco(
            "Chocolatey v2.2.2\r\n\r\ngit|2.40.0\r\nnot a row\r\n",
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "git");
        assert!(parse_choco("", false).is_empty());
        assert!(
            parse_choco("a|", false).is_empty() || parse_choco("a|", false)[0].version.is_empty()
        );
    }

    // Scoop is not installed on the machine the fixtures were captured on, so these use its
    // documented formats and have not been checked against a real run.
    #[test]
    fn scoop_export_json_lists_apps_with_their_bucket() {
        let json = r#"{"buckets":[{"Name":"main"}],"apps":[
            {"Info":"","Source":"main","Name":"git","Version":"2.45.2.windows.1","Updated":"2024-06-01"},
            {"Info":"Global install","Source":"extras","Name":"vlc","Version":"3.0.20","Updated":"2024-05-01"}]}"#;
        let rows = parse_scoop_export(json);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "git");
        assert_eq!(rows[0].source.as_deref(), Some("main"));
        assert_eq!(rows[1].version, "3.0.20");
        assert_eq!(rows[0].manager, Manager::Scoop);
        assert!(parse_scoop_export("not json").is_empty());
        assert!(parse_scoop_export("{}").is_empty());
    }

    #[test]
    fn scoop_status_table_gives_installed_and_latest() {
        let text = "Name Installed Version Latest Version Missing Dependencies Info\r\n---- ----------------- -------------- -------------------- ----\r\ngit  2.44.0            2.45.2\r\n";
        let rows = parse_scoop_status(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "git");
        assert_eq!(rows[0].version, "2.44.0");
        assert_eq!(rows[0].available.as_deref(), Some("2.45.2"));
        assert!(parse_scoop_status("Scoop is up to date.\r\nEverything is ok!\r\n").is_empty());
    }

    #[test]
    fn scoop_search_table_gives_name_version_bucket() {
        let text = "Results from local buckets...\r\n\r\nName Version Source Binaries\r\n---- ------- ------ --------\r\ngit  2.45.2  main\r\ngit-lfs 3.5.1 main\r\n";
        let rows = parse_scoop_search(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].id, "git-lfs");
        assert_eq!(rows[1].source.as_deref(), Some("main"));
    }
}
