# Techomato Orbit

A free Windows desktop app that puts your local dev machine in one window: the services you run,
the ports in use, installed software, and machine health. It lives in the system tray and works
offline.

Built with Tauri 2 (Rust) + React + TypeScript. Design: `docs/superpowers/specs/`. How it fits together: `docs/architecture.md`.

Inspired by the macOS app Nexus Console. No code or assets are shared with it.

## Develop

Needs Node 22.12+, Rust (rustup, MSVC toolchain), and the Visual Studio C++ Build Tools.

```bash
npm ci
npm start          # tauri dev
npm test           # vitest
cargo test --manifest-path src-tauri/Cargo.toml
```

## What works so far

Custom services you define: start, graceful stop, force stop, restart, restart policy, health
checks, secret environment values in Windows Credential Manager, live logs, and a tray icon.
Closing the window hides it to the tray; Quit Orbit stops services unless they are marked
keep-running.

Also: a Ports view (what is listening, who owns it, end a process, watch a port), Projects
(group services with dependencies; start in order, stop in reverse), a Windows services list with
Start/Stop/Restart through a single UAC prompt (Windows' own services are protected), desktop
notifications with history, a Ctrl+K command palette, global shortcuts, and a Settings page
(modules, notifications, start with Windows).
