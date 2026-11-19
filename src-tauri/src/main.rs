// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Second mode of this binary: deliver a graceful stop to another process's console, then exit.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--stop-helper") {
        let pid = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
        std::process::exit(orbit_lib::windows::console::stop_helper(pid));
    }
    // Third mode: perform one typed operation with administrator rights, then exit.
    if args.get(1).map(String::as_str) == Some("--elevated-op") {
        let request = args.get(2).map(String::as_str).unwrap_or_default();
        let result = args
            .get(4)
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        std::process::exit(orbit_lib::elevated::run_helper(request, &result));
    }
    orbit_lib::run()
}
