//! Parsers for the text Windows tools print. Odd input gives fewer rows, never a panic.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegValue {
    pub name: String,
    pub kind: String,
    pub data: String,
}

/// `reg query <key>` output: value lines are indented four spaces and split by four spaces.
pub fn parse_reg_query(text: &str) -> Vec<RegValue> {
    text.lines()
        .filter(|l| l.starts_with("    "))
        .filter_map(|l| {
            let mut p = l.trim_start().splitn(3, "    ");
            let (name, kind) = (p.next()?, p.next()?);
            if !kind.starts_with("REG_") || name.is_empty() {
                return None;
            }
            Some(RegValue {
                name: name.to_string(),
                kind: kind.to_string(),
                data: p.next().unwrap_or("").trim_end().to_string(),
            })
        })
        .collect()
}

/// The Run key value names that Windows has been told not to start, from the matching
/// `StartupApproved\Run` key: its first byte is even (02, 06) when enabled and odd (03, 07)
/// when disabled. Names with no entry are enabled.
pub fn startup_enabled(approved: &[RegValue]) -> HashMap<String, bool> {
    approved
        .iter()
        .filter(|v| v.kind == "REG_BINARY")
        .filter_map(|v| {
            let first = u8::from_str_radix(v.data.get(0..2)?, 16).ok()?;
            Some((v.name.clone(), first & 1 == 0))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

pub fn startup_items(run: &[RegValue], approved: &[RegValue]) -> Vec<StartupItem> {
    let state = startup_enabled(approved);
    run.iter()
        .map(|v| StartupItem {
            name: v.name.clone(),
            command: v.data.clone(),
            enabled: state.get(&v.name).copied().unwrap_or(true),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerScheme {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

pub fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `powercfg /list`: `Power Scheme GUID: <guid>  (<name>) [*]`.
pub fn parse_power_list(text: &str) -> Vec<PowerScheme> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Power Scheme GUID:")?.trim();
            let (guid, tail) = rest.split_once(' ')?;
            let tail = tail.trim();
            let name = tail.strip_prefix('(')?.split(')').next()?.to_string();
            is_guid(guid).then(|| PowerScheme {
                guid: guid.to_lowercase(),
                name,
                active: tail.ends_with('*'),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = include_str!("../../tests/fixtures/doctor/run_hkcu.txt");
    const HKLM: &str = include_str!("../../tests/fixtures/doctor/run_hklm.txt");
    const APPROVED: &str = include_str!("../../tests/fixtures/doctor/approved_hkcu.txt");
    const POWER: &str = include_str!("../../tests/fixtures/doctor/power_list.txt");

    #[test]
    fn reg_query_gives_name_kind_and_data() {
        let rows = parse_reg_query(RUN);
        assert_eq!(rows.len(), 8, "{rows:#?}");
        let od = rows.iter().find(|r| r.name == "OneDrive").unwrap();
        assert_eq!(od.kind, "REG_SZ");
        assert!(od.data.ends_with("/background"));
        // Names with a space, and other value types, survive.
        let hklm = parse_reg_query(HKLM);
        assert!(hklm.iter().any(|r| r.name == "Example Agent"));
        assert!(hklm.iter().any(|r| r.kind == "REG_EXPAND_SZ"));
    }

    #[test]
    fn reg_query_ignores_headers_blank_lines_and_errors() {
        assert!(parse_reg_query("").is_empty());
        assert!(parse_reg_query("\r\nHKEY_CURRENT_USER\\Software\\X\r\n").is_empty());
        assert!(parse_reg_query(
            "ERROR: The system was unable to find the specified registry key."
        )
        .is_empty());
        let empty_data = parse_reg_query("    Flag    REG_SZ\r\n");
        assert_eq!(empty_data[0].data, "");
    }

    #[test]
    fn startup_state_comes_from_the_first_byte() {
        let state = startup_enabled(&parse_reg_query(APPROVED));
        assert_eq!(state.get("Discord"), Some(&false));
        let enabled = parse_reg_query("    X    REG_BINARY    020000000000000000000000\r\n");
        assert_eq!(startup_enabled(&enabled).get("X"), Some(&true));
        let six = parse_reg_query("    Y    REG_BINARY    06000000\r\n");
        assert_eq!(startup_enabled(&six).get("Y"), Some(&true));
    }

    #[test]
    fn an_item_with_no_approval_entry_is_enabled() {
        let run = parse_reg_query("    New App    REG_SZ    C:\\x.exe\r\n");
        let items = startup_items(&run, &parse_reg_query(APPROVED));
        assert!(items[0].enabled);
        let all = startup_items(&parse_reg_query(RUN), &parse_reg_query(APPROVED));
        assert!(!all.iter().find(|i| i.name == "Discord").unwrap().enabled);
    }

    #[test]
    fn power_schemes_include_the_active_one() {
        let s = parse_power_list(POWER);
        assert_eq!(s.len(), 3, "{s:#?}");
        let active: Vec<_> = s.iter().filter(|p| p.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Balanced");
        assert_eq!(s[1].guid, "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
        assert!(parse_power_list("Invalid Parameters -- try \"/?\" for help").is_empty());
    }

    #[test]
    fn guids_are_checked_strictly() {
        assert!(is_guid("381b4222-f694-41f0-9685-ff5bb260df2e"));
        for bad in [
            "",
            "381b4222f69441f09685ff5bb260df2e",
            "381b4222-f694-41f0-9685-ff5bb260df2",
            "x81b4222-f694-41f0-9685-ff5bb260df2e",
            "381b4222-f694-41f0-9685-ff5bb260df2e & calc",
        ] {
            assert!(!is_guid(bad), "{bad}");
        }
    }
}
