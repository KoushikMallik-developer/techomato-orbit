//! Package managers (winget, Scoop, Chocolatey) behind one interface. This module is pure:
//! parsers turn captured tool output into `Package` rows, and builders turn an operation into an
//! argument array. Running processes lives in `run.rs`.

pub mod args;
pub mod parse;

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
