# Techomato Orbit — architecture (Plan 1: foundation and service engine)

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

## Known gaps (tracked for later plans)

- Only one Orbit instance should run; a second would supervise the same services. No
  single-instance guard yet.
- `https://` health URLs are only checked as a TCP connect (no TLS stack).
- Adopted services cannot be stopped gracefully, only force-ended.
- `allow_multiple` is stored but the supervisor runs one process per service.
