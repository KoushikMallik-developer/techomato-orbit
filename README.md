# Techomato Orbit

A free Windows desktop app that puts your local dev machine in one window: the services you run,
one-click databases, and your machine's health. It lives in the system tray and works offline,
except for downloading a database or an update. Windows 10 or 11, 64-bit.

Built with Tauri 2 (Rust) + React + TypeScript. Design: `docs/superpowers/specs/`. How it fits together: `docs/architecture.md`.

Inspired by the macOS app Nexus Console. No code or assets are shared with it.

## Install

Build the installers with `npm run release` (see `docs/releasing.md`). The setup `.exe` installs
for you or for all users; the `.msi` is machine-wide.

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

Databases: add PostgreSQL, MariaDB, Redis or MongoDB with one click. Orbit downloads the program
once (checked against a fixed checksum), keeps it and its data in
`%LOCALAPPDATA%\Techomato Orbit`, and adds a service that listens on this PC only. No Docker. They
have no password, so use them for development only. Redis is a community Windows port (version 5).
Deleting the service deletes its data. Uninstalling Orbit does not, so delete that folder to remove
every database.

Already-running databases: any Redis, PostgreSQL, MySQL/MariaDB or MongoDB that is already running
(started by hand, or installed as a Windows service) is added to the Services page by itself, as
an ordinary card with Start, Stop, Force stop, Restart, Logs, Edit and Delete. They keep running
when Orbit quits. One you started by hand is stopped by ending it at once. One that runs as a
Windows service is started and stopped through Windows, which asks for administrator rights (a UAC
prompt) each time. A database another account runs that Orbit cannot inspect is only listed. Delete removes the card; it comes back the next time
that database is found running.

Add service has templates (Django, Flask, FastAPI, React with Vite, Express, Node, Celery, Docker)
that only fill in the form.

Also: Projects (group services with dependencies; start in order, stop in reverse), desktop
notifications with history, a Ctrl+K command palette, global shortcuts, and a Settings page
(modules, notifications, start with Windows). The Services page shows the port each running service
is actually listening on.

Doctor: a read-only health check of this PC (drives, caches, memory, startup programs, power plan,
pending restart, WSL, Docker, Ollama). Every finding is tagged Speed, Storage or Info, and clearing
disk space is never presented as a speed-up. Fixes (clear a cache, disable a startup program, switch
power plan) run only when you press a button and confirm.

Updates: Settings can check for a new version on request or at launch. Updates are signed and
install only when you press Install.
