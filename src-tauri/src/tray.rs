//! Tray icon: Open, a live "Running services: N" line, Quit. Closing the window only hides it;
//! Quit stops services per their keep-running policy and then exits.

use crate::engine::state::Status;
use crate::engine::supervisor::{ServiceRuntime, Supervisor};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Listener, Manager};

/// A service counts as running while it is up, even if its health check is failing.
pub fn count_running(all: &[ServiceRuntime]) -> usize {
    all.iter()
        .filter(|r| matches!(r.status, Status::Running | Status::Unresponsive))
        .count()
}

pub fn running_label(n: usize) -> String {
    format!("Running services: {n}")
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Orbit", true, None::<&str>)?;
    let running = MenuItem::with_id(app, "running", running_label(0), false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Orbit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &running,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("orbit")
        .tooltip("Techomato Orbit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => {
                let app = app.clone();
                // Stopping can take a shutdown timeout; never block the tray's event loop.
                std::thread::spawn(move || {
                    app.state::<Supervisor>().stop_all_on_quit();
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    let handle = app.handle().clone();
    app.listen("service://status", move |_| {
        if let Ok(all) = handle.state::<Supervisor>().all_runtime() {
            let _ = running.set_text(running_label(count_running(&all)));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(status: Status) -> ServiceRuntime {
        ServiceRuntime {
            id: "x".into(),
            status,
            pid: None,
            parent_pid: None,
            started_at_ms: None,
            exit_code: None,
            restart_count: 0,
            last_error: None,
        }
    }

    #[test]
    fn only_up_services_are_counted() {
        use Status::*;
        let all: Vec<_> = [
            Running,
            Unresponsive,
            Starting,
            Stopping,
            Stopped,
            Failed,
            Unknown,
        ]
        .into_iter()
        .map(rt)
        .collect();
        assert_eq!(count_running(&all), 2);
        assert_eq!(count_running(&[]), 0);
    }

    #[test]
    fn the_label_reads_plainly() {
        assert_eq!(running_label(0), "Running services: 0");
        assert_eq!(running_label(3), "Running services: 3");
    }
}
