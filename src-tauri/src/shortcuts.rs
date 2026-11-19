//! Global shortcuts: open Orbit, and start / stop the last-used project from anywhere.
//! A shortcut another program already owns is not registered; the reason is kept so Settings can
//! show it. The others still work.

use crate::db::Db;
use crate::engine::projects::{start_project, stop_project, Progress};
use crate::engine::supervisor::Supervisor;
use crate::repo::settings;
use serde::Serialize;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Settings key holding the id of the project most recently started from Orbit.
pub const LAST_PROJECT_KEY: &str = "lastProject";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    StartProject,
    StopProject,
}

pub const DEFAULTS: [(Action, &str, &str); 3] = [
    (Action::Open, "Ctrl+Alt+O", "Open Orbit"),
    (
        Action::StartProject,
        "Ctrl+Alt+S",
        "Start the last-used project",
    ),
    (
        Action::StopProject,
        "Ctrl+Alt+X",
        "Stop the last-used project",
    ),
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub label: String,
    pub accelerator: String,
    /// Why it is not active, in plain words; `None` when registered.
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Shortcuts(pub Mutex<Vec<ShortcutStatus>>);

fn action_for(shortcut: &Shortcut) -> Option<Action> {
    DEFAULTS
        .iter()
        .find(|(_, acc, _)| Shortcut::from_str(acc).is_ok_and(|s| s == *shortcut))
        .map(|(a, _, _)| *a)
}

fn run_on_last_project(app: &AppHandle, start: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        let (Some(db), Some(sup)) = (app.try_state::<Arc<Db>>(), app.try_state::<Supervisor>())
        else {
            return;
        };
        let Ok(Some(id)) = settings::get(&db, LAST_PROJECT_KEY) else {
            return;
        };
        let emit = |p: &Progress| {
            let _ = app.emit("project://progress", p);
        };
        let _ = if start {
            start_project(&sup, &db, &id, None, &emit)
        } else {
            stop_project(&sup, &db, &id, false, &emit)
        };
    });
}

/// The plugin, with the key handler.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            match action_for(shortcut) {
                Some(Action::Open) => crate::tray::show_main_window(app),
                Some(Action::StartProject) => run_on_last_project(app, true),
                Some(Action::StopProject) => run_on_last_project(app, false),
                None => {}
            }
        })
        .build()
}

/// Registers every default; failures are recorded, never fatal.
pub fn register_all(app: &AppHandle) -> Vec<ShortcutStatus> {
    DEFAULTS
        .iter()
        .map(|(_, acc, label)| {
            let error = match Shortcut::from_str(acc) {
                Err(e) => Some(format!("Not a valid shortcut: {e}")),
                Ok(s) => app
                    .global_shortcut()
                    .register(s)
                    .err()
                    .map(|_| "Another program is already using this shortcut.".to_string()),
            };
            ShortcutStatus {
                label: (*label).into(),
                accelerator: (*acc).into(),
                error,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_a_valid_distinct_shortcut() {
        let parsed: Vec<Shortcut> = DEFAULTS
            .iter()
            .map(|(_, acc, _)| Shortcut::from_str(acc).unwrap_or_else(|e| panic!("{acc}: {e}")))
            .collect();
        for (i, a) in parsed.iter().enumerate() {
            assert!(parsed[i + 1..].iter().all(|b| b != a), "duplicate default");
        }
    }

    #[test]
    fn each_shortcut_maps_to_its_own_action() {
        for (action, acc, _) in DEFAULTS {
            let s = Shortcut::from_str(acc).unwrap();
            assert_eq!(action_for(&s), Some(action), "{acc}");
        }
        assert_eq!(action_for(&Shortcut::from_str("Ctrl+Alt+Q").unwrap()), None);
    }
}
