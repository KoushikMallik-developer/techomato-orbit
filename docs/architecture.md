# Techomato Orbit — architecture

Tauri 2 shell. The Rust side owns everything that touches the machine; the React side only
renders and sends commands. Windows 10/11 x64.

## Modules

| Path                                           | Responsibility                                                                                      |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `src-tauri/src/lib.rs`                         | Builds the app: opens the DB, builds the `Supervisor`, registers commands, tray, close-to-tray.     |
| `src-tauri/src/db`                             | SQLite (bundled), forward-only migrations, refuses a corrupt or newer database without touching it. |
| `src-tauri/src/models/service.rs`              | `ServiceSpec` and its validation.                                                                   |
| `src-tauri/src/repo`                           | `services` (CRUD), `processes` (live PID records and the activity log).                             |
| `src-tauri/src/secrets.rs`, `windows/creds.rs` | `SecretStore` trait; Windows Credential Manager implementation.                                     |
| `src-tauri/src/engine/state.rs`                | Pure lifecycle transition table.                                                                    |
| `src-tauri/src/engine/preflight.rs`            | Missing executable, bad working directory, port in use.                                             |
| `src-tauri/src/engine/supervisor.rs`           | Start, stop, restart, restart policy, adoption, quit policy. One watcher thread per process.        |
| `src-tauri/src/engine/health.rs`               | TCP, HTTP, command probes and the 3-strikes rule.                                                   |
| `src-tauri/src/engine/logs.rs`                 | Bounded ring plus rotating files per service, secret masking.                                       |
| `src-tauri/src/windows`                        | `procs` (identity), `job` (Job Objects, spawn), `console` (graceful stop).                          |
| `src-tauri/src/commands`                       | Thin Tauri command handlers. Native confirms are raised here.                                       |
| `src-tauri/src/tray.rs`                        | Tray menu and the running-services count.                                                           |
| `src/shared/ipc`                               | The only door to Rust (`invoke.ts`); zod validates every reply.                                     |
| `src/features/services`, `src/features/logs`   | Services and Logs screens.                                                                          |

## Data flow

```
React ──ipc()/ipcVoid()──▶ Tauri command ──▶ Supervisor / repo ──▶ OS (Job Object, Credential Manager, SQLite)
  ▲                                              │
  └────────── events: service://status, log://line ◀─────────────┘
```

Commands return `AppResult<T>`; an error crosses IPC as `{kind, message, detail}` and becomes an
`IpcError`. A failed mutation shows its `message` once as a toast; `kind: "cancelled"` (a declined
native confirm) is silent.

## Events

- `service://status`: one `ServiceRuntime` (`id`, `status`, `pid`, `parentPid`, `startedAtMs`,
  `exitCode`, `restartCount`, `lastError`) whenever a service changes state.
- `log://line`: an array of `{serviceId, tsMs, stream, text}`, at most one event every 100 ms.

## Rules that must not be broken

**Process identity is PID plus creation time (`ProcId`), never a PID alone.** Windows reuses PIDs.
Every liveness check and every kill compares both. A stored record whose PID now belongs to a
different process is stale, and is deleted without signalling anything.

**Each service runs in its own Job Object, without kill-on-close.** The job lets Orbit stop a whole
tree, and not killing on close is what lets a service marked keep-running outlive Orbit.

**Stop is graceful first, then forced.** The binary has a `--stop-helper <pid>` mode that attaches
to the child's console and sends CTRL_BREAK (falling back to WM_CLOSE), because a GUI process
cannot send a console signal to a child directly. After the service's shutdown timeout,
`TerminateJobObject` ends the tree. A process re-adopted after an Orbit restart has no job handle,
so it is ended with `taskkill /T /F` after an identity re-check.

**A start claims the service under its lock before doing anything slow.** Simultaneous start
requests launch exactly one process; the rest get a conflict.

**Readiness.** Without a health check a service is Running after surviving a ~500 ms grace. With
one, it stays Starting until the first pass and fails (and is terminated) if none arrives within
the startup timeout. Three consecutive failed probes make Running become Unresponsive; one pass
restores it.

**Secrets never touch SQLite or logs.** Secret env values live in Credential Manager
(`TechomatoOrbit/<service id>/<key>`); SQLite stores NULL. Values are read only when the child's
environment is built, and registered with the log sink so any echo of them is masked.

**Logs are bounded everywhere.** A line is capped at 64 KiB while being read, memory keeps a ring,
disk keeps a few rotated files, and the UI keeps at most 5 000 lines and renders only the visible
window.

**Native confirmation comes from Rust** (`confirm_action`), so a compromised frontend cannot skip
it. The IPC layer is the only importer of `@tauri-apps/api/core`, enforced by a test.

## Quit and tray

Closing the window hides it. The tray menu has Open Orbit, a disabled "Running services: N" line
(refreshed on `service://status`) and Quit Orbit. Quit runs `stop_all_on_quit`, which stops every
service in parallel except those marked keep-running-on-quit, then exits. On the next start,
`adopt_on_startup` re-attaches to survivors whose identity still matches.

## Single instance

A named mutex (`Local\TechomatoOrbit.SingleInstance`) allows one Orbit per Windows session. A
second launch brings the first window forward (even from the tray) and exits.

## Keep-running services

A pipe's read end lives in Orbit, so a service that must outlive Orbit would crash on its next
write after Orbit exits. Services marked keep-running therefore write stdout and stderr (merged)
to `<logs>/<id>.raw`, which Orbit tails into the normal log pipeline while it runs.

## Plan 2 additions

| Area                  | Where                                                                                                | What it does                                                                                                                                                                                                                                                                                                                                                           |
| --------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ports                 | `windows/net.rs`, `engine/ports.rs`, `commands/ports.rs`, `features/ports`                           | Reads TCP/UDP endpoints (IP Helper) and joins them to Orbit services by PID tree. The Services page shows the port a running service listens on, and a service's configured port is checked before start. There is no page for it and no way to end a process.                                                                                                         |
| Projects              | `models/project.rs`, `repo/projects.rs`, `engine/deps.rs`, `engine/projects.rs`, `features/projects` | A project groups services. `service_deps` edges (cycle-checked, cascade-deleted) give a start order; stop runs in reverse. Per-edge `abort` or `continue` on failure. Progress is streamed as `project://progress`.                                                                                                                                                    |
| Notifications         | `engine/notify.rs`, `repo/notifications.rs`, `commands/notifications.rs`                             | Status changes become notices (crash, start failed, health failed, auto-restart, optional started/stopped) filtered by `Prefs.notify`, shown as native toasts and kept as history (last 500).                                                                                                                                                                          |
| Settings              | `models/prefs.rs`, `repo/settings.rs`, `features/settings`                                           | One JSON document (`prefs`) in the `settings` table: Projects and Doctor toggles, notification toggles, refresh policy, start-with-Windows. Autostart goes through the autostart plugin and launches with `--background`, which keeps the window in the tray.                                                                                                          |
| Shortcuts and palette | `shortcuts.rs`, `shared/palette`                                                                     | `Ctrl+K` opens an in-app palette (`rankCommands`: prefix, then contains, then subsequence; title beats keyword). Global `Win+Alt+O` open, `Win+Alt+S` / `Win+Alt+X` (not Ctrl+Alt, which is AltGr and would swallow typed characters) start / stop the last project started from Orbit. A shortcut another program owns is not registered and is reported in Settings. |

Migration `0002` adds `projects`, `service_deps` and `notifications`.

Events added: `project://progress` (`{projectId, serviceId, step}`).

**Not covered by automated tests:** native toast
display, tray menu clicks, and pressing the global shortcuts (registration is checked).

## Databases and service templates

**One-click databases** (`src-tauri/src/engines/`, `commands/databases.rs`, `features/databases`).
`catalog.rs` pins four engines (PostgreSQL, MariaDB, Redis, MongoDB) by URL, SHA-256 and size.
Redis is a community Windows port (version 5); the others are the official builds. `install.rs`
downloads with a streaming hash, refuses a file that does not match (nothing is unpacked), unpacks
into a `.tmp` folder and renames it into `<local app data>\Techomato Orbit\engines\<id>-<version>\`.
`instance.rs` makes `...\instances\<name>-<id8>\`, runs the engine's init program when it has one,
picks a free port near the usual one, and saves an **ordinary service** bound to 127.0.0.1 (restart
never, TCP health check). From then on the Supervisor treats it like any other service. The
databases have no password, so they are for development only.

Deleting a database service also deletes its folder, but only when its working directory is
exactly a direct child of `instances\` and no other service uses the same folder (`owned_dir`); the
confirm names the folder. Duplicating a project does not copy such a database. Engines are
shared and are not deleted. Uninstalling Orbit does not remove this folder.
`db_create` emits `engine://progress` (`{engine, phase, done, total}`); `db_cancel` stops a download.
`.github/workflows/databases.yml` re-runs the real download-start-stop test weekly.

**Already-running databases** (`engine/detect.rs`, `commands/detect.rs`, `windows/scm.rs`,
`engine/winsvc.rs`). `service_detect` takes the port list (`commands::ports::current_rows`) and keeps
TCP listeners owned by `redis-server`, `postgres`, `mysqld`, `mariadbd` or `mongod` that no Orbit
service owns, then turns each into an ordinary service (the Services page asks every 10 s). It
reuses a stopped service with the same program and port instead of making a second one. Two kinds:

- Started by hand: `Supervisor::adopt` shows it as running and watches it like a program re-attached
  after an Orbit restart. Stop ends it with `taskkill`; its environment and old output are not known.
- Run by a Windows service (the program, or something above it, is a service in the Service Control
  Manager; `svchost.exe` hosts never count): the service's own command becomes the spec. Because
  `scm::find` matches a spec to a service by the exact program and arguments, `Supervisor::start`
  and `stop` send those to Windows (`sc.exe start|stop <name>` run through `ShellExecuteExW`
  "runas", one UAC prompt each; the name must have come from the service list and is checked
  again). `engine/winsvc.rs` reads the state every 3 s and calls `Supervisor::set_external`, so the
  card follows `services.msc` too. Declining the prompt changes nothing.

A program another account runs, whose command Orbit cannot read, is listed but not added. Adopted
services keep running when Orbit quits. Not covered by tests: the UAC-elevated `sc.exe` call.

**Service templates** (`features/services/components/ServiceForm/serviceTemplates.ts`) only fill the
Add service form (Django, Flask, FastAPI, React with Vite, Express, Node, Celery, Docker). Nothing is
installed or run by choosing one.

Removed since Plan 3: the Packages, Windows services, Ports and Terminal screens and the elevated
helper. `pm/run.rs` stays because Doctor uses it to run read-only commands with a timeout.

## Plan 4 additions: Doctor, updates, installer

**Doctor** (`src-tauri/src/doctor/`). `parse.rs` reads `reg query` and `powercfg /list` output,
`rules.rs` turns measurements into findings (all thresholds live there), `probes.rs` gathers the
measurements, `fixes.rs` is the closed set of fixes, `fsutil.rs` sizes and clears folders without
following links. `doctor_scan` streams `doctor://finding` events and then `doctor://done`;
`doctor_fix` asks natively first. Probes only read: nothing starts a service, VM or daemon (WSL and
Docker are detected from the registry, files and the process list, never by running `wsl` or
`docker`). Cache folders come from the enum and two Windows roots, never from the UI; the fix is
re-validated against the live machine (an enabled startup entry that exists, a listed power plan,
a browser that is not running). Disabling a startup program sets the same marker Task Manager uses
and is reversible. Cleanup findings are tagged `storage`, never `speed`.

**Updates** (`commands/updates.rs`). `tauri-plugin-updater` against a signed `latest.json` on GitHub
releases; the public key is in `tauri.conf.json`. The download is verified first, then services are
stopped as Quit does, then the installer runs. Automatic check at launch is a preference
(`autoUpdateCheck`, off by default) and only notifies. See `docs/releasing.md`.

**Installer** (`tauri.conf.json`). NSIS with a per-user or per-machine choice, an MSI, and an
embedded WebView2 bootstrapper.

The VSCode cleanup, Claude Chats and Terminal modules from the original design were dropped by the user.

## Known gaps (tracked for later plans)

- `https://` health URLs are only checked as a TCP connect (no TLS stack).
- Keep-running services lose the stdout/stderr distinction, and their raw file is only truncated at
  each start, not rotated while running.

## Brand rules

- Look: paper background, ink text, tomato accent, Bricolage Grotesque headings, Instrument Sans body,
  pill buttons, light and dark. Colours come only from `src/styles/theme.css`.
- Light tomato is exactly `#C8381F`. Dark tomato is `#c63f27` on purpose: it keeps white button text
  at 5.1:1. Do not "correct" it to `#C8381F`; the contrast notes in `theme.css` give the numbers.
- Cards use a 16 px corner (`--radius-card`). The first plan said 22 px; 16 px is what shipped.
- Names: "Techomato Orbit" for the window title, tray, installer and README. "Orbit" alone is fine in
  running text inside the app, and the sidebar mark is "Orbit."
