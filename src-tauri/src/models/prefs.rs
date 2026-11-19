//! User preferences, stored as one JSON document under the `prefs` settings key.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModulePrefs {
    pub ports: bool,
    pub winservices: bool,
    pub projects: bool,
    pub packages: bool,
    pub terminal: bool,
    pub doctor: bool,
    pub vscode: bool,
    pub claude_chats: bool,
}

impl Default for ModulePrefs {
    fn default() -> Self {
        Self {
            ports: true,
            winservices: true,
            projects: true,
            packages: true,
            terminal: true,
            doctor: true,
            vscode: true,
            claude_chats: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotifyPrefs {
    pub started: bool,
    pub stopped: bool,
    pub crashed: bool,
    pub start_failed: bool,
    pub health_failed: bool,
    pub port_conflict: bool,
    pub auto_restart: bool,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            started: false,
            stopped: false,
            crashed: true,
            start_failed: true,
            health_failed: true,
            port_conflict: true,
            auto_restart: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub modules: ModulePrefs,
    pub notify: NotifyPrefs,
    pub refresh_hidden: bool,
    pub start_with_windows: bool,
}
