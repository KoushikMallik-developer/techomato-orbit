//! Package managers (winget, Scoop, Chocolatey) behind one interface. This module is pure:
//! parsers turn captured tool output into `Package` rows, and builders turn an operation into an
//! argument array. Running processes lives in `run.rs`.

pub mod args;
pub mod parse;
pub mod run;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Manager {
    Winget,
    Scoop,
    Choco,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The newer version, when one is known.
    pub available: Option<String>,
    /// winget source or Scoop bucket.
    pub source: Option<String>,
    pub manager: Manager,
}

/// What to ask a manager to do. Text values are validated by `args::build`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Installed,
    Outdated,
    Search(String),
    Install(String),
    Upgrade(String),
    UpgradeAll,
    Uninstall(String),
}

/// Parses the output of a read-only `op` for `manager`.
pub fn parse(manager: Manager, op: &Op, text: &str) -> Vec<Package> {
    use parse::*;
    match (manager, op) {
        (Manager::Winget, _) => parse_winget_table(text),
        (Manager::Choco, Op::Outdated) => parse_choco(text, true),
        (Manager::Choco, _) => parse_choco(text, false),
        (Manager::Scoop, Op::Installed) => parse_scoop_export(text),
        (Manager::Scoop, Op::Outdated) => parse_scoop_status(text),
        (Manager::Scoop, _) => parse_scoop_search(text),
    }
}

/// Turns a finished read-only run into rows. A failing run that printed nothing recognisable is an
/// error; one that printed a "No ... found" style message (winget exits non-zero then) is empty.
pub fn interpret(manager: Manager, op: &Op, out: &run::Output) -> Result<Vec<Package>, String> {
    let rows = parse(manager, op, &out.stdout);
    if out.code == 0 || !rows.is_empty() {
        return Ok(rows);
    }
    let said = format!("{}\n{}", out.stdout, out.stderr);
    if said.to_lowercase().contains("no ") && out.stderr.trim().is_empty() {
        return Ok(rows);
    }
    Err(last_line(&said).unwrap_or_else(|| format!("exited with code {}", out.code)))
}

/// The last non-empty line of `text`, for a one-line failure message.
pub fn last_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32, stdout: &str, stderr: &str) -> run::Output {
        run::Output {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    #[test]
    fn a_no_results_message_is_an_empty_list_not_an_error() {
        let o = out(
            1,
            "No installed package found matching input criteria.\r\n",
            "",
        );
        assert_eq!(interpret(Manager::Winget, &Op::Installed, &o), Ok(vec![]));
    }

    #[test]
    fn a_failing_run_with_nothing_recognisable_is_an_error_with_its_last_line() {
        let o = out(2, "", "boom\r\nThe source is unreachable\r\n");
        assert_eq!(
            interpret(Manager::Choco, &Op::Installed, &o),
            Err("The source is unreachable".to_string())
        );
        let empty = out(5, "", "");
        assert!(interpret(Manager::Scoop, &Op::Outdated, &empty).is_err());
    }

    #[test]
    fn rows_win_over_a_nonzero_exit() {
        let o = out(1, "git|2.0\r\n", "");
        assert_eq!(
            interpret(Manager::Choco, &Op::Installed, &o).unwrap().len(),
            1
        );
    }
}
