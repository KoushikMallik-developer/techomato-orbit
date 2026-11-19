// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Second mode of this binary: deliver a graceful stop to another process's console, then exit.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--stop-helper") {
        let pid = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
        std::process::exit(orbit_lib::windows::console::stop_helper(pid));
    }
    orbit_lib::run()
}
