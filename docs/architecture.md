# Techomato Orbit — architecture (Plans 1-2: service engine, ports, projects, Windows services, shell)

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

| Area                  | Where                                                                                                | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ports                 | `windows/net.rs`, `engine/ports.rs`, `commands/ports.rs`, `features/ports`                           | Reads TCP/UDP endpoints (IP Helper), joins them to Orbit services by PID tree, flags a foreign listener on a service's port as a conflict. Ending a process re-reads identity, refuses Orbit itself and core Windows processes, asks a native confirm, then `taskkill` close followed by `/T /F`. The page polls every 2 s while visible; the watch list lives in the browser profile.                                                                                                                                                 |
| Projects              | `models/project.rs`, `repo/projects.rs`, `engine/deps.rs`, `engine/projects.rs`, `features/projects` | A project groups services. `service_deps` edges (cycle-checked, cascade-deleted) give a start order; stop runs in reverse. Per-edge `abort` or `continue` on failure. Progress is streamed as `project://progress`.                                                                                                                                                                                                                                                                                                                    |
| Windows services      | `windows/scm.rs`, `commands/winservices.rs`, `features/winservices`                                  | Lists SCM services (opened with query rights only). A service is **protected** if it is a driver, in the core list, or its image is under `%SystemRoot%`; protected rows get no controls. "Developer" services match name patterns or the user's pins.                                                                                                                                                                                                                                                                                 |
| Elevated helper       | `elevated.rs`, `windows/elevate.rs`, `main.rs`                                                       | Start/stop/restart of a Windows service needs admin. Orbit relaunches itself with `runas` and `--elevated-op <base64 json> --result <file>`. The request is a closed enum (`ServiceStart/Stop/Restart`, `EndProcess`), max 1 KiB, `deny_unknown_fields`; the helper decodes, re-validates (name shape, exists, not protected, identity) and only then acts. There is no "run this command" operation. The caller creates the result file; the helper writes only to an existing file. One UAC prompt per action; cancelling is silent. |
| Notifications         | `engine/notify.rs`, `repo/notifications.rs`, `commands/notifications.rs`                             | Status changes become notices (crash, start failed, health failed, auto-restart, optional started/stopped) filtered by `Prefs.notify`, shown as native toasts and kept as history (last 500). A watched-port conflict is raised by the Ports page.                                                                                                                                                                                                                                                                                     |
| Settings              | `models/prefs.rs`, `repo/settings.rs`, `features/settings`                                           | One JSON document (`prefs`) in the `settings` table: module toggles, notification toggles, refresh policy, start-with-Windows. Autostart goes through the autostart plugin and launches with `--background`, which keeps the window in the tray.                                                                                                                                                                                                                                                                                       |
| Shortcuts and palette | `shortcuts.rs`, `shared/palette`                                                                     | `Ctrl+K` opens an in-app palette (`rankCommands`: prefix, then contains, then subsequence; title beats keyword). Global `Win+Alt+O` open, `Win+Alt+S` / `Win+Alt+X` (not Ctrl+Alt, which is AltGr and would swallow typed characters) start / stop the last project started from Orbit. A shortcut another program owns is not registered and is reported in Settings.                                                                                                                                                                                                                                |

Migration `0002` adds `projects`, `service_deps` and `notifications`.

Events added: `project://progress` (`{projectId, serviceId, step}`).

**Not covered by automated tests:** the real UAC prompt and elevated service control, native toast
display, tray menu clicks, and pressing the global shortcuts (registration is checked).

## Known gaps (tracked for later plans)

- `https://` health URLs are only checked as a TCP connect (no TLS stack).
- Keep-running services lose the stdout/stderr distinction, and their raw file is only truncated at
  each start, not rotated while running.
- `allow_multiple` is stored but the supervisor runs one process per service.
