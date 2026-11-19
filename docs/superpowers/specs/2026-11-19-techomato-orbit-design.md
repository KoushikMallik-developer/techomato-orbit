# Techomato Orbit — Design

Status: draft for approval (revision 2). Date: 2026-11-19.

## Intent

Techomato Orbit is a free Windows desktop app that puts a developer's local machine in one window. It manages the services a developer runs (Windows Services and any command the user registers), shows what is listening on which port, manages installed software, and checks that the machine is healthy. It sits in the system tray, opens in seconds, and sends nothing about you anywhere. The only network call the app makes itself is the update check.

It is for every developer on Windows (10 22H2 and 11), not only the Techomato team. It is a Techomato product: it must look and feel like the rest of the family (techomato-software-hub), and we own all of its code.

Inspiration only: the macOS app "Nexus Console" (MIT) showed which jobs matter, and a "DevStation" feature brief showed what a local dev-environment manager needs. **No code, files, tests, fixtures or assets are copied from either.** Everything is written fresh here. (Because nothing is copied, no third-party licence notice is required; we still credit Nexus as inspiration in the README.)

## Goals

1. **Custom services.** Register any command (executable, arguments, working directory, environment variables) as a service that Orbit starts, stops, restarts and monitors. Most dev servers are not Windows Services, so this is the core.
2. **Honest service control.** Real PIDs, real state (stopped, starting, running, stopping, failed, unresponsive), never simulated. Windows Services are shown and controlled alongside Orbit's own services.
3. **Safe process handling.** Orbit only ever stops processes it started. A process is identified by PID plus creation time, never PID alone. Stop is graceful first, forced after a timeout, and covers the whole process tree.
4. **Projects and dependencies.** Group services into projects, define dependencies, start in dependency order, stop in reverse, wait for health, reject cycles.
5. **Logs.** Live, separate stdout and stderr, searchable, bounded in memory, rotated on disk.
6. **Ports.** See what is listening and who owns it, detect conflicts before start, end a process safely. Never auto-kill a port holder.
7. **Packages.** winget, Scoop and Chocolatey: list, search, install, upgrade, uninstall.
8. **Doctor.** Read-only probes with honest speed / storage / info tags, plus VSCode cleanup.
9. **Claude Chats archiver.**
10. **Terminal.** PowerShell by default, plus cmd, Git Bash and WSL when present.
11. **Notifications and quick access.** Native alerts, tray, command palette (Ctrl+K) and global shortcuts.
12. Lives in the tray, opens fast without a white flash, installs from a normal installer, small and light on memory.
13. Techomato look: paper background, ink text, tomato accent `#C8381F`, Bricolage Grotesque headings, Instrument Sans body, pill buttons, light and dark.

## Non-goals (v1)

- No accounts, cloud, analytics or crash reporting. The only network code of our own is the signed update check (see Updates).
- No macOS or Linux build, no systemd or launchd.
- Not a remote-management or multi-machine tool.
- Not an installer for runtimes (Node, Python) beyond what the package managers already do.
- No per-engine database integrations (PostgreSQL, MySQL, MongoDB and so on). A database is a Windows Service or a custom service like any other.
- Planned for v1.1, not designed here: Docker and Compose management, startup profiles, a full environment-variable manager (multiple `.env` files, environment switching), editable service templates (Django, Vite, Celery and similar), configuration export/import and backups, system monitoring (CPU, RAM, disk, network, GPU), a process explorer with a process tree, and per-project package-manager selection.

## Anti-goals

- Anything that looks like a generic Electron web page in a window, or a marketing page inside the app.
- Claiming a cleanup makes the machine faster when it only frees disk.
- Faking a metric, a status or a feature: a button either works or does not exist.
- Running the whole app as Administrator, or running custom services elevated.
- Killing a process Orbit did not start without an explicit, backend-enforced confirm, and never killing a process just because it holds a port.
- Deleting user data directories (database data, chats) as part of any action.
- A destructive action that the UI could trigger without a confirm that the backend enforces.

## Constraints

- Platform: Windows 10 22H2+ and 11, x64 (ARM64 is later). WebView2 is required (bundled by the installer if missing).
- Stack: Tauri 2 (Rust) + React 19 + TypeScript + Vite + Tailwind v4. Chosen for a ~10 MB installer and low idle memory.
- Private: no outbound network from the app except the update check, which sends only the current version and Windows architecture. winget, Scoop and Chocolatey do their own downloading when the user installs something.
- Privilege: the app runs unelevated. Actions that need admin (starting or stopping most Windows Services, machine-wide installs) trigger a per-action UAC prompt through a short-lived elevated helper. The app never stays elevated, and user-defined services are always started unelevated.
- Never build shell strings from user input. A custom service stores an executable and an argument list; a "run through shell" option is explicit, per service, and off by default.
- Doctor probes must be read-only and must never start a service or daemon just to inspect it.
- Protected system services are shown read-only.
- Secrets: an environment variable can be marked secret. Its value is stored in Windows Credential Manager, not in the database, and is masked in the UI and logs.

## Approaches considered

- **A. Tauri 2 + React, Windows-native Rust (chosen).** Small, fast, the Techomato web look is reusable, and Rust talks to the Windows APIs directly. Cost: we write the Windows layer ourselves; Windows 10 needs the WebView2 runtime (the installer handles it).
- **B. Electron + Node.** Quickest to a first screen, but ~150 MB installer and heavy idle RAM, which fights a tray utility.
- **C. Native .NET / WinUI.** Best OS feel, but loses the React and Techomato look and is a different skill set from the rest of the family.
- Also decided: fresh codebase rather than a port of the macOS app, so we own every line and shape it for Windows.

## Design

### Shell

One main window (`main`), hidden until the first paint is ready to avoid a white flash. Native Windows decorations themed light or dark. Left navigation rail with modules, a content pane, an optional bottom terminal drawer, and a Ctrl+K command palette. Closing the window hides to tray; Quit in the tray menu really exits.

```
┌ Techomato Orbit ─────────────────────────── – □ ✕ ┐
│ ◉ Orbit     │  Services   [Ctrl+K search]   [+ Add]│
│ Projects    │  ┌────────────────────────────────┐  │
│ Services    │  │ ● api         Running  :8000   │  │
│ Ports       │  │ ● postgresql  Running  :5432   │  │
│ Logs        │  │ ○ worker      Stopped          │  │
│ Packages    │  └────────────────────────────────┘  │
│ Doctor      │  ▸ Terminal (Ctrl+`)                 │
│ Settings    │                                      │
└─────────────┴──────────────────────────────────────┘
```

### Modules (v1, in build order)

1. **Shell, theme, tray, settings, command palette** — window, nav, Techomato tokens, light/dark, per-module on/off, tray with running-service count, global shortcuts (start/stop/restart project, open Orbit), start with Windows.
2. **Ports** — listening TCP/UDP endpoints with owning process (name, PID, path), mapped to Orbit services and projects where they match. Check whether a port is free. End a process: graceful first, force second, with a backend-enforced confirm. Watch chosen ports and notify on conflict.
3. **Services** — two sources in one list:
   - **Windows Services** through the Service Control Manager: state, start type, start/stop/restart, a filter to "developer" services (built-in name/pattern list plus user pins, toggle for all). Protected services are read-only.
   - **Custom services** managed by Orbit: see "Service engine" below.
4. **Logs** — one view across services and projects: live stdout and stderr, timestamps, search, filter, error and warning highlighting, pause/resume, auto-scroll, copy, clear, export, retention settings. Crash output is kept.
5. **Projects** — name, description, icon, root directory; contains services. Start all, stop all, restart all, start or stop selected. Create, edit, duplicate, archive, delete. Dependencies and start order (see below).
6. **Notifications** — native alerts for service started/stopped/crashed, start failed, health check failed, port conflict, auto-restart, and sustained high CPU/RAM or low disk for services Orbit manages. History and preferences in Settings.
7. **Packages** — a manager adapter per tool behind one interface: winget (always), Scoop and Chocolatey (shown only if installed). List installed, outdated, search, install, upgrade one/all, uninstall. Output is parsed from each tool's machine-friendly output where available.
8. **Terminal** — ConPTY-backed terminal (xterm.js), multiple tabs. Profiles detected at startup: PowerShell 7, Windows PowerShell, cmd, Git Bash, WSL distros. Working directory selection, copy/paste, command history, open external terminal.
9. **Doctor** — read-only probes, each tagged speed / storage / info with severity: disk space and large caches (npm, NuGet, pip, Docker, temp), startup items, power plan, memory pressure, pending reboot, WSL and Docker state, VSCode and browser caches, Ollama. Fixes only on explicit button press, each confirmed.
10. **VSCode cleanup** — inside Doctor's VSCode scope. Reads `%APPDATA%\Code\User` (stable build; Insiders and VSCodium later): finds stale workspace storage for folders that no longer exist, large caches and old extension versions, and can compact `state.vscdb` (only when VSCode is not running). Every cleanup is explicit, confirmed, tagged storage (never speed), and can be scheduled daily from the tray (default off).
11. **Claude Chats (archiver)** — shown only if Claude Code is installed (`%USERPROFILE%\.claude\projects` exists). Lists chat history per project (sessions, size, last active). Archive old chats to a folder the user chooses, restore them, and optionally archive daily from the tray on a user-set retention period (default off). Chats are only moved, never deleted, and an index file records what was archived.

### Service engine (custom services)

A service has: name, description, category (database, cache, backend, frontend, worker, queue, custom), icon, project, working directory, executable, argument list, environment variables (with a secret flag), configured port, startup and shutdown timeouts, optional pre/post start and stop commands, health check, auto-restart policy (never, on failure, always, with a max retry count and backoff), dependencies, "allow multiple instances" (default off) and enabled flag.

- **Identity.** A running process is recorded as PID plus process creation time. After an Orbit crash or restart, stored records are checked against the live process list and any that no longer match are marked stale, not killed.
- **Process trees.** Each service runs inside a Windows Job Object so its children are tracked and stopped with it. Stop sends a graceful signal first (Ctrl+Break for console apps, window close otherwise), waits the shutdown timeout, then terminates the job.
- **Output.** stdout and stderr are captured separately and streamed as events.
- **Health.** A check is a TCP port, an HTTP URL (status code range) or a command exit code, with interval and timeout. A running process that fails its check becomes "unresponsive".
- **Pre-start checks.** Missing executable, invalid working directory and (when a port is configured) port conflicts are reported before start, with a clear message.
- **On Orbit quit.** Each service has a policy: stop it, or keep it running. The default is stop. Kept services are detached from the job and re-adopted on next launch by identity. Databases the user marked "keep running" are never stopped by Orbit exiting.
- **Statuses.** stopped, starting, running, stopping, failed, unresponsive, unknown. Tracked: PID, parent PID, start time, uptime, exit code, restart count, CPU and memory of the service's tree.
- **Dependencies.** A directed acyclic graph per project. Start waits until each dependency is healthy (or running, if it has no health check). Stop runs in reverse order. Cycles are rejected when saved. Failure behaviour per dependency: abort the start, or continue with a warning. A simple graph view shows the order.

### Boundaries and data flow

- The frontend never calls OS tools. Every call goes through one typed IPC module; Rust replies are validated against schemas on the JS side, so a mismatch fails loudly in one place.
- Rust owns all Windows access: the `windows` crate for the Service Control Manager, Job Objects and network tables, `sysinfo` for process data, `portable-pty` for ConPTY, `rusqlite` for storage and for reading VSCode's `state.vscdb`. Package managers and PowerShell are invoked with argument arrays and timeouts, off the UI thread.
- Streaming work (logs, status changes, Doctor scan, terminal output, long installs) is emitted as events, not returned in one reply.
- **Storage.** One SQLite database under `%APPDATA%\Techomato Orbit` with versioned migrations, foreign keys and indexes. It holds projects, services, dependencies, environment variables (secret values excepted), process records, activity history, notifications and settings. Log files live on disk beside it, one rotating file set per service, with a capped in-memory buffer for the live view; the database keeps only log metadata.
- **UI state.** Server data (services, ports, packages, findings) lives in a query cache with polling and refetch-on-focus; UI state and user intent (selected module, theme, enabled modules, terminal tabs) live in a small store.
- Errors carry a kind, a message and detail, and are shown centrally as a toast, except a user-cancelled confirm.

### Repo layout

```
src/            React app: app/, features/<module>/, shared/{ui,ipc,state,lib}, styles/
src-tauri/      Rust: commands/<module>.rs, windows/ (scm, jobs, net, procs), engine/ (services, deps, health, logs),
                pm/ (winget, scoop, choco), doctor/, db/ (migrations), pty.rs, tray.rs
docs/           specs and plans
```

Folder-per-component, named exports, one place for design tokens, no hardcoded colours.

### Testing

- Pure logic (parsers, severity/tag rules, adapters, dependency ordering and cycle detection, protected-service rule, health evaluation, restart policy) is unit tested in Vitest and `cargo test`.
- Parsers are tested against output samples we capture ourselves from real winget / Scoop / Chocolatey / netstat runs on Windows, kept in a fixtures folder.
- The service engine is tested against a small harmless local HTTP server and a few scripted test programs (exits cleanly, crashes, ignores the stop signal, spawns children, holds a port). Scenarios: start, graceful stop, forced stop of an unresponsive process, restart, two services on different ports, occupied port, missing executable, invalid working directory, crash and auto-restart, stale record after Orbit restart, project start in dependency order and stop in reverse, configuration persisting across restart.
- Tests that touch the real machine beyond that are marked ignored by default.
- CI on `windows-latest`: lint, typecheck, tests, clippy, build. A tagged release builds the installer.

### Updates

Orbit checks a Techomato-hosted update manifest using Tauri's updater. Releases are signed with a key we hold, and the app refuses unsigned or mismatched updates. In Settings the user chooses manual check (the default) or an automatic check at launch. Only the version and architecture are sent. Updates install only on the user's go-ahead. The manifest host and release pipeline belong in the build plan.

### Distribution

NSIS (setup .exe) and MSI via Tauri bundling, WebView2 bootstrapper included. Code signing is required for a public launch to avoid SmartScreen warnings; development builds are unsigned. Signing certificate procurement is outside this design.

## Open choices I made (flag if you disagree)

- Custom services and the process engine are the core of v1, ahead of Packages and Doctor.
- v1 package managers: winget + Scoop + Chocolatey, with Scoop/Chocolatey hidden when not installed.
- Per-action UAC elevation, never a permanently elevated app; custom services are never run elevated.
- Archiver and VSCode cleanup are in v1, as you asked.
- Auto-update is in v1, signed and opt-in: manual check by default. It is the one network call the app makes.
- SQLite for application data (replacing the earlier settings-file idea), because services, projects and history are relational.
- Secret environment values go to Windows Credential Manager.
- On quit, services stop by default; a per-service "keep running" overrides it.
- Docker management, profiles, the full env manager, templates, export/import, system monitoring and the process explorer are v1.1.
- x64 only for v1. Product folder is this repo (`techomato-orbit`), app identifier `com.techomato.orbit`.

## Calls settled after review (mine, flag if you disagree)

- **Elevated helper.** It accepts only a closed set of typed operations (Windows service start/stop/restart by validated name, end process by PID, machine-wide package install/upgrade/uninstall by validated id, a short list of named Doctor fixes). It never accepts generic argv or script text. The main app launches it with a UAC prompt per action, passes one typed request, and it exits when done. It is a second mode of the same signed binary, not a separate tool.
- **Backend-enforced confirms.** Every destructive command shows a native Windows dialog from Rust before acting. The frontend cannot pass a "confirmed" flag to skip it.
- **Dependents warnings** are best effort: shown where the manager exposes reverse dependencies (Scoop, Chocolatey); winget shows none.
- **Protected services.** Read-only if the service type is a driver, or its image path is under `%SystemRoot%`, or it is on a short built-in list of core Windows services. The rule lives in one tested function in Rust.
- **Doctor fixes in v1.** Clear named caches (npm, NuGet, pip, temp, browser), disable a user-level startup item, and switch power plan. No WSL or Docker pruning. Fixes that free disk are tagged storage, never speed.
- **Title bar.** Native Windows decorations, themed light/dark. No Mica and no custom caption buttons, so Windows 10 and 11 behave the same.
- **Refresh.** Services and ports poll every 2 s while the window is visible; the tray count polls every 15 s while hidden, and that can be switched off in Settings.
- **Targets.** Installer under 15 MB, cold start to first paint under 1.5 s on a mid-range laptop, hidden-in-tray idle memory under 100 MB (Orbit itself, not the services it runs). Terminal, Doctor and Logs are lazy-loaded.
- **Design tokens.** Taken from techomato-software-hub (`src/styles.css` tokens and `src/styles/techomato.css`), with the two fonts bundled locally for offline use.

## Left to the builder

Exact crate versions, the IPC helper shape, component kit details, the elevated-helper mechanism (sub-process vs. scheduled task), the database table layout, log rotation sizes, Doctor probe thresholds and ordering, and the exact build order of modules within the order given, so long as the principles above hold.
