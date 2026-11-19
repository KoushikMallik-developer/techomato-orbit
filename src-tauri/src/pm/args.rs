//! Turns an operation into an argument array. Values are validated first, and always travel as
//! single array elements: nothing here is ever joined into a command string for a shell.

use super::{Manager, Op};

/// Longest accepted package id or search text.
pub const MAX_TERM: usize = 200;

/// A package id: letters, digits and `. _ - + @ / :`, no leading dash.
pub fn check_id(id: &str) -> Result<&str, String> {
    let ok = !id.is_empty()
        && id.chars().count() <= MAX_TERM
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-+@/:".contains(c));
    if ok {
        Ok(id)
    } else {
        Err(format!("'{}' is not a valid package id", id.escape_debug()))
    }
}

/// Search text: anything printable, no leading dash, at most `MAX_TERM` characters.
pub fn check_query(q: &str) -> Result<&str, String> {
    let ok = !q.trim().is_empty()
        && q.chars().count() <= MAX_TERM
        && !q.starts_with('-')
        && !q.chars().any(char::is_control);
    if ok {
        Ok(q)
    } else {
        Err("Search text must be 1-200 printable characters and not start with '-'".into())
    }
}

fn strs(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// The arguments (after the executable) for `op` on `manager`.
pub fn build(manager: Manager, op: &Op) -> Result<Vec<String>, String> {
    // Validate first, whatever the manager.
    let id = match op {
        Op::Install(i) | Op::Upgrade(i) | Op::Uninstall(i) => Some(check_id(i)?),
        _ => None,
    };
    let query = match op {
        Op::Search(q) => Some(check_query(q)?),
        _ => None,
    };
    let (id, query) = (id.unwrap_or(""), query.unwrap_or(""));
    let wg = ["--accept-source-agreements", "--disable-interactivity"];
    let wg_run = [
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "--disable-interactivity",
    ];
    let mut v = match (manager, op) {
        (Manager::Winget, Op::Installed) => strs(&["list"]),
        (Manager::Winget, Op::Outdated) => strs(&["upgrade"]),
        (Manager::Winget, Op::Search(_)) => strs(&["search", "--query", query]),
        (Manager::Winget, Op::Install(_)) => strs(&["install", "--id", id, "--exact"]),
        (Manager::Winget, Op::Upgrade(_)) => strs(&["upgrade", "--id", id, "--exact"]),
        (Manager::Winget, Op::UpgradeAll) => strs(&["upgrade", "--all"]),
        (Manager::Winget, Op::Uninstall(_)) => strs(&["uninstall", "--id", id, "--exact"]),
        (Manager::Choco, Op::Installed) => strs(&["list", "--limit-output"]),
        (Manager::Choco, Op::Outdated) => strs(&["outdated", "--limit-output"]),
        (Manager::Choco, Op::Search(_)) => strs(&["search", query, "--limit-output"]),
        (Manager::Choco, Op::Install(_)) => strs(&["install", id, "-y", "--no-progress"]),
        (Manager::Choco, Op::Upgrade(_)) => strs(&["upgrade", id, "-y", "--no-progress"]),
        (Manager::Choco, Op::UpgradeAll) => strs(&["upgrade", "all", "-y", "--no-progress"]),
        (Manager::Choco, Op::Uninstall(_)) => strs(&["uninstall", id, "-y", "--no-progress"]),
        (Manager::Scoop, Op::Installed) => strs(&["export"]),
        (Manager::Scoop, Op::Outdated) => strs(&["status"]),
        (Manager::Scoop, Op::Search(_)) => strs(&["search", query]),
        (Manager::Scoop, Op::Install(_)) => strs(&["install", id]),
        (Manager::Scoop, Op::Upgrade(_)) => strs(&["update", id]),
        (Manager::Scoop, Op::UpgradeAll) => strs(&["update", "*"]),
        (Manager::Scoop, Op::Uninstall(_)) => strs(&["uninstall", id]),
    };
    if manager == Manager::Winget {
        let extra: &[&str] = match op {
            Op::Install(_) | Op::Upgrade(_) | Op::UpgradeAll | Op::Uninstall(_) => &wg_run,
            _ => &wg,
        };
        v.extend(strs(extra));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(m: Manager, op: Op) -> Vec<String> {
        build(m, &op).unwrap()
    }

    #[test]
    fn ids_allow_real_package_ids_and_nothing_shell_like() {
        for ok in [
            "Git.Git",
            "7zip.7zip",
            "Microsoft.VisualStudioCode",
            "nodejs-lts",
            "git-lfs",
            "a+b",
            "pkg@1.2",
            "bucket/app",
            "x:y",
        ] {
            assert_eq!(check_id(ok), Ok(ok), "{ok}");
        }
        for bad in [
            "", "  ", "a b", "a&calc", "a|b", "a;b", "a\"b", "a'b", "a`b", "a$b", "a^b", "a%b",
            "a<b", "a>b", "a\nb", "a\0b", "-rf", "--source", r"a\b", "(a)", "a!b", "*",
        ] {
            assert!(check_id(bad).is_err(), "{bad:?}");
        }
        assert!(check_id(&"a".repeat(201)).is_err());
        assert!(check_id(&"a".repeat(200)).is_ok());
    }

    #[test]
    fn queries_may_contain_spaces_and_symbols_but_not_control_characters_or_a_leading_dash() {
        for ok in [
            "git",
            "visual studio",
            "c++ redist",
            "a & b; \"c\" | d",
            "naïve",
        ] {
            assert_eq!(check_query(ok), Ok(ok), "{ok}");
        }
        for bad in ["", "   ", "-x", "--accept", "a\nb", "a\0b", "a\tb"] {
            assert!(check_query(bad).is_err(), "{bad:?}");
        }
        assert!(check_query(&"a".repeat(201)).is_err());
    }

    #[test]
    fn hostile_search_text_stays_one_argument() {
        let hostile = "a & calc; \"b\" | c";
        for m in [Manager::Winget, Manager::Choco, Manager::Scoop] {
            let args = b(m, Op::Search(hostile.into()));
            assert_eq!(
                args.iter().filter(|a| a.as_str() == hostile).count(),
                1,
                "{m:?}: {args:?}"
            );
        }
    }

    #[test]
    fn a_hostile_id_never_reaches_an_argument_array() {
        for m in [Manager::Winget, Manager::Choco, Manager::Scoop] {
            for op in [
                Op::Install("x & calc".into()),
                Op::Upgrade("x|y".into()),
                Op::Uninstall("--force".into()),
            ] {
                assert!(build(m, &op).is_err(), "{m:?} {op:?}");
            }
        }
    }

    #[test]
    fn winget_arguments() {
        let flags = ["--accept-source-agreements", "--disable-interactivity"];
        let installed = b(Manager::Winget, Op::Installed);
        assert_eq!(installed[0], "list");
        assert!(flags.iter().all(|f| installed.contains(&f.to_string())));
        assert_eq!(b(Manager::Winget, Op::Outdated)[0], "upgrade");
        let search = b(Manager::Winget, Op::Search("git".into()));
        assert_eq!(&search[..3], ["search", "--query", "git"]);
        let install = b(Manager::Winget, Op::Install("Git.Git".into()));
        assert_eq!(&install[..4], ["install", "--id", "Git.Git", "--exact"]);
        assert!(install.contains(&"--silent".to_string()));
        assert!(install.contains(&"--accept-package-agreements".to_string()));
        assert_eq!(
            &b(Manager::Winget, Op::Upgrade("Git.Git".into()))[..2],
            ["upgrade", "--id"]
        );
        assert_eq!(
            &b(Manager::Winget, Op::UpgradeAll)[..2],
            ["upgrade", "--all"]
        );
        assert_eq!(
            &b(Manager::Winget, Op::Uninstall("Git.Git".into()))[..2],
            ["uninstall", "--id"]
        );
    }

    #[test]
    fn choco_arguments() {
        assert_eq!(b(Manager::Choco, Op::Installed), ["list", "--limit-output"]);
        assert_eq!(
            b(Manager::Choco, Op::Outdated),
            ["outdated", "--limit-output"]
        );
        assert_eq!(
            b(Manager::Choco, Op::Search("git".into())),
            ["search", "git", "--limit-output"]
        );
        assert_eq!(
            b(Manager::Choco, Op::Install("git".into())),
            ["install", "git", "-y", "--no-progress"]
        );
        assert_eq!(
            b(Manager::Choco, Op::Upgrade("git".into())),
            ["upgrade", "git", "-y", "--no-progress"]
        );
        assert_eq!(
            b(Manager::Choco, Op::UpgradeAll),
            ["upgrade", "all", "-y", "--no-progress"]
        );
        assert_eq!(
            b(Manager::Choco, Op::Uninstall("git".into())),
            ["uninstall", "git", "-y", "--no-progress"]
        );
    }

    #[test]
    fn scoop_arguments() {
        assert_eq!(b(Manager::Scoop, Op::Installed), ["export"]);
        assert_eq!(b(Manager::Scoop, Op::Outdated), ["status"]);
        assert_eq!(
            b(Manager::Scoop, Op::Search("git".into())),
            ["search", "git"]
        );
        assert_eq!(
            b(Manager::Scoop, Op::Install("git".into())),
            ["install", "git"]
        );
        assert_eq!(
            b(Manager::Scoop, Op::Upgrade("git".into())),
            ["update", "git"]
        );
        assert_eq!(b(Manager::Scoop, Op::UpgradeAll), ["update", "*"]);
        assert_eq!(
            b(Manager::Scoop, Op::Uninstall("git".into())),
            ["uninstall", "git"]
        );
    }
}
