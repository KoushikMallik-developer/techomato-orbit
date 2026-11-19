# Techomato Orbit — Plan 1: Foundation and Service Engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A running Tauri 2 Windows app with the Techomato look, a SQLite store, and a real service engine: the user can create a custom service, start it, see its real PID, status and live logs, stop and restart it, and find the configuration intact after closing and reopening Orbit.

**Architecture:** Rust owns everything that touches Windows (Job Objects, process identity, console signals, Credential Manager) and SQLite. The React frontend talks only through one typed IPC module whose replies are validated with zod. Service state changes and log lines are pushed to the frontend as Tauri events; the frontend keeps server data in React Query and UI state in a small Redux store.

**Tech Stack:** Tauri 2, Rust (`windows`, `rusqlite` bundled, `sysinfo`, `serde`, `thiserror`, `uuid`), React 19, TypeScript, Vite, Tailwind v4, TanStack Query, Redux Toolkit, zod, Vitest, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-11-19-techomato-orbit-design.md`

**Later plans (not in this one):** 2) Ports, Projects and dependencies, Notifications, command palette. 3) Packages, Terminal. 4) Doctor, VSCode cleanup, Claude Chats, updater, installer, release CI. Windows Services (SCM) listing and the elevated helper are in plan 2.

## Global Constraints

- Windows 10 22H2+ and 11, x64 only. WebView2 required.
- Tauri 2, React 19, TypeScript, Vite, Tailwind v4. No Electron, no .NET.
- App identifier `com.techomato.orbit`; product name `Techomato Orbit`; data under `%APPDATA%\Techomato Orbit`.
- No outbound network from the app in this plan (the update check arrives in plan 4).
- App runs unelevated; custom services always start unelevated.
- Never build shell strings: a service stores an executable and an argument list. "Run through shell" is explicit, per service, default off.
- A process is identified by PID plus creation time, never PID alone. Orbit only ever stops processes it started.
- Stop is graceful first, forced only after the shutdown timeout, and covers the whole process tree (Job Object).
- Secret environment values live in Windows Credential Manager, never in SQLite, and are masked in the UI and logs.
- Every destructive command confirms from Rust (native dialog); the frontend cannot pass a "confirmed" flag.
- Colours only from theme tokens; tomato accent `#C8381F`; Bricolage Grotesque headings, Instrument Sans body, bundled locally (no Google Fonts request); light and dark; pill buttons.
- No fabricated status, PID or metric. A button either works or is not rendered.
- Clean-room: no code, files or assets copied from Nexus Console or any other project.
- Tests: Vitest for frontend logic, `cargo test` for Rust. Real-machine tests that need a built helper binary are `#[ignore]` by default and run with `--ignored` in CI.
- Commits: one per task, no attribution lines.

## Review Focus

1. A service whose executable path or working directory contains spaces or non-ASCII characters starts correctly. (Task 7 test)
2. A service that prints a flood of output (hundreds of MB, or one multi-megabyte line) never grows memory without bound. (Task 6 test)
3. Double-clicking Start, or Start while Stopping, never launches a second instance. (Task 7 test)
4. A process that exits immediately (before Orbit records its PID) ends as `failed` with its exit code, not stuck in `starting`. (Task 7 test)
5. `%APPDATA%\Techomato Orbit` missing, or the database file corrupted or opened by a newer schema version, gives a clear startup error and does not wipe data. (Task 3 test)

## File Structure

```
package.json, vite.config.ts, tsconfig.json, index.html
src/
  main.tsx, app/{App.tsx,Bootstrap.tsx,store.ts,queryClient.ts}
  styles/{theme.css,fonts.css,index.css}   public/fonts/*.woff2
  shared/ipc/{invoke.ts,commands.ts,schemas.ts,events.ts}
  shared/ui/{Button,Pill,Card,Dialog,Field,StatusDot,Toast}/ (folder per component)
  shared/layout/{AppShell,Sidebar}/
  features/services/{api,components,pages}/
  features/logs/{api,components,pages}/
src-tauri/
  Cargo.toml, tauri.conf.json, capabilities/default.json
  src/{main.rs,lib.rs,error.rs,tray.rs}
  src/db/{mod.rs,migrations/0001_init.sql}
  src/models/service.rs
  src/repo/services.rs
  src/windows/{procs.rs,job.rs,console.rs,creds.rs}
  src/engine/{mod.rs,supervisor.rs,logs.rs,health.rs,preflight.rs,state.rs}
  src/commands/{mod.rs,services.rs,logs.rs,settings.rs}
  src/bin/orbit-testsvc.rs     (test helper: serve, exit, crash, ignore-stop, handle-stop, spawn-child, spam)
.github/workflows/ci.yml
docs/{architecture.md,database_schema.md}
```

---

### Task 0: Toolchain and scaffold

**Files:** Create everything under repo root listed above that is configuration: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `.gitignore`, `.github/workflows/ci.yml`, `README.md` (replace stub; credit Nexus Console as inspiration only).

**Interfaces:**
- Produces: `npm start` (= `tauri dev`), `npm test`, `npm run typecheck`, `npm run lint`, `cargo test --manifest-path src-tauri/Cargo.toml`, and a window titled "Techomato Orbit" showing a placeholder.

- [ ] **Step 1: Check prerequisites.** Run `node -v` (≥ 22.12), `rustc --version`, `cargo --version`, and look for the MSVC C++ build tools. Expected today: Rust missing; Visual Studio 2019 present but C++ workload unconfirmed (the `link.exe` on PATH is Git's, not MSVC's).
- [ ] **Step 2: Install what is missing, with the user's go-ahead.** rustup with the `stable-x86_64-pc-windows-msvc` toolchain plus `rustfmt` and `clippy`; "Desktop development with C++" (MSVC + Windows SDK) via Visual Studio Build Tools if absent. Stop and tell the user if an installer needs a reboot or a manual click.
- [ ] **Step 3: Scaffold** Tauri 2 + React + TypeScript + Vite in the repo root, then add Tailwind v4 (`@tailwindcss/vite`), `@tanstack/react-query`, `@reduxjs/toolkit`, `react-redux`, `zod`, `@tauri-apps/api`, `vitest`, `oxlint`, `prettier`. Window config: `title "Techomato Orbit"`, `visible false`, native decorations, min size 1000×660, identifier `com.techomato.orbit`, `csp` set to a strict policy (`default-src 'self'`; inline styles allowed only if Tailwind needs it), bundle targets `nsis` and `msi`.
- [ ] **Step 4: Add `.github/workflows/ci.yml`** on `windows-latest`: `npm ci`, lint, typecheck, `npm test`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, then `cargo test -- --ignored`.
- [ ] **Step 5: Verify.** `npm run typecheck`, `npm test` (a trivial passing test), `cargo test`, and `npm start` opens the window. Expected: all pass, window appears.
- [ ] **Step 6: Commit** `chore: scaffold Tauri 2 + React app`.

---

### Task 1: Techomato design tokens, fonts and app shell

**Files:**
- Create: `src/styles/{theme.css,fonts.css,index.css}`, `public/fonts/*.woff2`, `src/shared/ui/{Button,Pill,Card,StatusDot}/*`, `src/shared/layout/{AppShell,Sidebar}/*`, `src/app/{App.tsx,Bootstrap.tsx}`
- Modify: `index.html` (set `data-theme` in a pre-module script from stored preference or `prefers-color-scheme`)
- Test: `src/shared/ui/StatusDot/StatusDot.test.ts`

**Interfaces:**
- Produces: tokens `--color-canvas`, `--color-paper`, `--color-fg`, `--color-fg-muted`, `--color-border`, `--color-accent` (`#C8381F`), status ramp `running|stopped|warn|danger|starting` each with `-soft`; Tailwind utilities `bg-canvas text-fg border-border text-accent`; `<Button variant="primary|ghost|danger" size="sm|md">` (pill); `<StatusDot status={ServiceStatus} />` where `type ServiceStatus = 'stopped'|'starting'|'running'|'stopping'|'failed'|'unresponsive'|'unknown'`; `statusLabel(status): string`; `<AppShell>` with `<Sidebar>` rail (Services, Logs, Settings for now).

- [ ] **Step 1: Write failing test** `statusLabel` returns exactly: `"Stopped","Starting","Running","Stopping","Failed","Unresponsive","Unknown"` for the seven statuses, and `StatusDot` maps `failed` and `unresponsive` to a danger/warn token plus a text label (colour is never the only signal).
- [ ] **Step 2: Run** `npx vitest run src/shared/ui/StatusDot` — expect FAIL.
- [ ] **Step 3: Implement tokens and components.** Take values from `../techomato-software-hub/src/styles.css` and `src/styles/techomato.css` (paper background, ink, tomato `#C8381F`, pill buttons, 22px card radius); dark variants under `[data-theme='dark']`. Fonts: download Bricolage Grotesque and Instrument Sans (OFL) once, commit the `.woff2` files, declare with `@font-face`, `font-display: swap`. Never hardcode a hex outside `theme.css`.
- [ ] **Step 4: `Bootstrap.tsx`** renders after fonts and theme are ready, then calls the window `show()` (add `core:window:allow-show` to `capabilities/default.json`) so there is no white flash.
- [ ] **Step 5: Run tests, typecheck, and `npm start`.** Expected: shell renders in light and dark; window shows only after first paint.
- [ ] **Step 6: Commit** `feat(ui): Techomato tokens, fonts, app shell`.

---

### Task 2: Typed IPC boundary and error type

**Files:**
- Create: `src/shared/ipc/{invoke.ts,commands.ts,schemas.ts,events.ts}`, `src-tauri/src/error.rs`, `src-tauri/src/commands/mod.rs`
- Test: `src/shared/ipc/invoke.test.ts`, `src-tauri/src/error.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces (Rust): `enum ErrorKind { Validation, NotFound, Conflict, Io, Db, Os, Cancelled, Internal }`; `struct AppError { kind, message, detail: Option<String> }` implementing `serde::Serialize` as `{kind,message,detail}` and `From` for `rusqlite::Error` and `std::io::Error`; `type AppResult<T> = Result<T, AppError>`.
- Produces (TS): `class IpcError extends Error { kind: string; detail?: string }`; `ipc<T>(cmd: CmdName, args: unknown, schema: z.ZodType<T>): Promise<T>`; `ipcVoid(cmd: CmdName, args?: unknown): Promise<void>`; `CMD` const object (no raw command strings elsewhere); `useIpcEvent<T>(name: string, schema: z.ZodType<T>, handler: (v: T) => void)`.
- Rule: `invoke.ts` is the only file that imports `@tauri-apps/api/core`.

- [ ] **Step 1: Write failing tests.** TS: with `invoke` mocked, a reply that fails the schema rejects with `IpcError` of kind `"schema"`; a Rust-shaped error `{kind:"conflict",message:"x"}` becomes `IpcError` with `kind==="conflict"`. Rust: `AppError` serializes to JSON with exactly keys `kind, message, detail`.
- [ ] **Step 2: Run** `npx vitest run src/shared/ipc` and `cargo test error` — expect FAIL.
- [ ] **Step 3: Implement** both sides. Add an oxlint rule or a test that greps `src` and fails if any other file imports `@tauri-apps/api/core`.
- [ ] **Step 4: Run both test suites** — expect PASS.
- [ ] **Step 5: Commit** `feat(ipc): typed IPC and AppError`.

---

### Task 3: SQLite store and migrations

**Files:**
- Create: `src-tauri/src/db/mod.rs`, `src-tauri/src/db/migrations/0001_init.sql`, `docs/database_schema.md`
- Modify: `src-tauri/src/lib.rs` (open DB at startup, manage as Tauri state)
- Test: tests in `db/mod.rs`

**Interfaces:**
- Produces: `struct Db(Mutex<Connection>)`; `fn open(path: &Path) -> AppResult<Db>` (creates parent dir, `PRAGMA foreign_keys=ON`, WAL, applies migrations by `PRAGMA user_version`); `fn open_in_memory() -> AppResult<Db>` for tests; `const SCHEMA_VERSION: u32`.
- Schema `0001`: `services`, `service_env` (service_id FK, key, value NULL when `is_secret=1`, is_secret), `service_processes` (service_id FK, pid, created_at_ms, started_at, adopted), `activity_history` (id, ts, service_id NULL, kind, message), `settings` (key PK, value). Indexes on every FK and `activity_history(ts)`. Projects, dependencies and notifications tables arrive in plan 2.

- [ ] **Step 1: Write failing tests.** (a) `open_in_memory()` ends with `user_version == SCHEMA_VERSION` and all five tables exist. (b) Re-running migrations is a no-op. (c) Deleting a service cascades to `service_env` and `service_processes`. (d) **Review Focus 5:** opening a file whose `user_version` is greater than `SCHEMA_VERSION` returns `AppError{kind: Db}` with a message naming both versions and leaves the file byte-for-byte unchanged; opening a non-SQLite file returns an error and does not truncate it; a missing parent directory is created.
- [ ] **Step 2: Run** `cargo test db::` — expect FAIL.
- [ ] **Step 3: Implement** with a small in-house migration runner (ordered `(version, sql)` slice applied in one transaction each); no extra migration crate.
- [ ] **Step 4: Run** — expect PASS. Write `docs/database_schema.md` from the migration.
- [ ] **Step 5: Commit** `feat(db): SQLite store with migrations`.

---

### Task 4: Service model, validation and repository

**Files:**
- Create: `src-tauri/src/models/service.rs`, `src-tauri/src/repo/services.rs`, `src-tauri/src/commands/services.rs` (CRUD only), `src/features/services/api/services.schema.ts`, `src/features/services/api/useServices.ts`
- Modify: `commands/mod.rs`, `lib.rs` (register), `shared/ipc/commands.ts`
- Test: in `models/service.rs`, `repo/services.rs`; `src/features/services/api/services.schema.test.ts`

**Interfaces:**
- Produces (Rust, all `#[serde(rename_all="camelCase")]`):
  - `struct ServiceSpec { id: String, name: String, description: String, category: Category, project_id: Option<String>, working_dir: String, executable: String, args: Vec<String>, run_through_shell: bool, env: Vec<EnvVar>, port: Option<u16>, startup_timeout_secs: u32, shutdown_timeout_secs: u32, restart: RestartPolicy, health: Option<HealthCheck>, keep_running_on_quit: bool, allow_multiple: bool, enabled: bool }`
  - `enum Category { Database, Cache, Backend, Frontend, Worker, Queue, Custom }`
  - `struct EnvVar { key: String, value: Option<String>, is_secret: bool }`
  - `enum RestartPolicy { Never, OnFailure{max_retries:u32,backoff_secs:u32}, Always{max_retries:u32,backoff_secs:u32} }`
  - `enum HealthCheck { Tcp{port:u16,interval_secs:u32,timeout_secs:u32}, Http{url:String,min_status:u16,max_status:u16,interval_secs:u32,timeout_secs:u32}, Command{exe:String,args:Vec<String>,interval_secs:u32,timeout_secs:u32} }`
  - `fn ServiceSpec::validate(&self) -> AppResult<()>`
  - Repo: `create(&Db,&ServiceSpec)->AppResult<ServiceSpec>` (assigns uuid), `update`, `get`, `list`, `delete`.
  - Commands: `service_create`, `service_update`, `service_list`, `service_get`, `service_delete` (delete confirms from Rust with a native dialog, and refuses if the service is running).
- Produces (TS): zod `ServiceSpecSchema` mirroring the DTO field-for-field; `useServices()` query; `CMD.serviceCreate|Update|List|Get|Delete`.

- [ ] **Step 1: Write failing tests.** Validation rejects: empty name; empty executable; `args` containing a NUL byte; port 0; `shutdown_timeout_secs` 0; env key empty or containing `=`; duplicate env keys; HTTP health URL not starting with `http://` or `https://`. Repo: create then get round-trips every field including non-ASCII names and paths with spaces (**Review Focus 1**); a secret env var is stored with `value NULL`; list is ordered by name case-insensitively; two services with the same name are allowed but ids differ.
- [ ] **Step 2: Run** `cargo test models:: repo::` and the vitest schema test — expect FAIL.
- [ ] **Step 3: Implement** model, validation, repo and CRUD commands; zod schema and query hook.
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(services): service model, validation, repository`.

---

### Task 5: Process identity and stale detection

**Files:**
- Create: `src-tauri/src/windows/procs.rs`
- Test: in `procs.rs`

**Interfaces:**
- Produces: `struct ProcId { pid: u32, created_at_ms: u64 }`; `fn identify(pid: u32) -> Option<ProcId>` (None if no such process or access denied); `fn is_alive(id: &ProcId) -> bool` (true only if a process with that PID exists **and** its creation time equals `created_at_ms`); `fn children_of(pid: u32) -> Vec<u32>`.
- Uses `OpenProcess` + `GetProcessTimes` (via the `windows` crate); `sysinfo` only for the child list.

- [ ] **Step 1: Write failing tests.** `identify(std::process::id())` is `Some`, and `is_alive` on it is true. `is_alive(&ProcId{pid: std::process::id(), created_at_ms: 1})` is false (PID reuse is detected). A spawned-then-reaped child's `ProcId` is not alive. `identify(0xFFFF_FFF0)` is `None`.
- [ ] **Step 2: Run** `cargo test windows::procs` — expect FAIL.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(windows): process identity by pid + creation time`.

---

### Task 6: Job Objects, graceful stop and log capture

**Files:**
- Create: `src-tauri/src/windows/{job.rs,console.rs}`, `src-tauri/src/engine/logs.rs`, `src-tauri/src/bin/orbit-testsvc.rs`
- Test: `src-tauri/tests/engine_process.rs` (`#[ignore]` real-process tests), unit tests in `logs.rs`

**Interfaces:**
- Produces (testsvc): `orbit-testsvc serve <port>` (HTTP 200 on `/`, exits 0 on Ctrl+Break/Ctrl+C); `exit <code>`; `crash` (panics, exit 101); `ignore-stop <port>` (serves, ignores Ctrl+Break/Ctrl+C and window close); `handle-stop` (prints `STOPPING` on stop signal and exits 0); `spawn-child` (spawns a child `serve` and prints its PID); `spam <mb>` (writes `<mb>` MB of lines to stdout, plus one 4 MB single line to stderr).
- Produces (Rust):
  - `struct ManagedProcess { id: ProcId, job: Job, stdout: ChildStdout, stderr: ChildStderr }`
  - `fn job::spawn(spec: &SpawnSpec) -> AppResult<ManagedProcess>` where `SpawnSpec { exe: PathBuf, args: Vec<String>, cwd: PathBuf, env: Vec<(String,String)>, through_shell: bool }`. Creates a kill-on-close-disabled Job Object (children tracked, but Orbit exiting must not kill services marked keep-running), `CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`, piped stdout and stderr.
  - `fn console::send_graceful(pid: u32) -> AppResult<()>` — sends Ctrl+Break to the child's process group; because Orbit has no console, implement by running a short-lived helper mode of the Orbit binary (`orbit.exe --stop-helper <pid>`) that attaches to the child's console, ignores the signal itself, and calls `GenerateConsoleCtrlEvent`; fall back to `WM_CLOSE` for processes with a top-level window.
  - `fn job::terminate_tree(job: &Job) -> AppResult<()>`.
  - `engine::logs::LogSink`: `fn new(dir: PathBuf, service_id: &str, max_file_bytes: u64, max_files: usize, ring_lines: usize) -> LogSink`; `fn push(&self, stream: Stream, line: &str)`; `fn recent(&self, n: usize) -> Vec<LogLine>`; `fn read_history(&self, offset: usize, limit: usize, filter: Option<&str>) -> Vec<LogLine>`. `LogLine { ts_ms: u64, stream: Stream, text: String }`, `enum Stream { Stdout, Stderr }`.
- Behavior: lines longer than 64 KiB are truncated with a `… [truncated N bytes]` marker; the in-memory ring holds at most `ring_lines` lines; files rotate at `max_file_bytes` keeping `max_files`; secret values are replaced with `••••` in anything written or emitted.

- [ ] **Step 1: Write failing unit tests for `LogSink`.** Ring never exceeds `ring_lines`; rotation produces at most `max_files` files each ≤ `max_file_bytes` plus one line; a 4 MB line is truncated to 64 KiB plus marker; a registered secret value never appears in `recent()` or on disk; `read_history` with a filter returns only matching lines.
- [ ] **Step 2: Write failing process tests (ignored by default).** (a) `serve` starts, responds on its port, `send_graceful` stops it within 3 s with exit 0. (b) `ignore-stop` survives `send_graceful`, then `terminate_tree` kills it. (c) `spawn-child`: after `terminate_tree`, neither parent nor child PID is alive. (d) **Review Focus 2:** `spam 300` runs to completion through `LogSink` with the process's peak working-set growth under 50 MB over baseline, and the ring still holds ≤ `ring_lines`. (e) A path like `C:\Temp\orbit test ü\orbit-testsvc.exe` (copied there by the test) and a working directory with spaces both start correctly.
- [ ] **Step 3: Run** `cargo test` (unit) and `cargo test -- --ignored` — expect FAIL.
- [ ] **Step 4: Implement** `orbit-testsvc`, `job.rs`, `console.rs` (including the `--stop-helper` mode wired in `main.rs`), and `logs.rs`. Read stdout and stderr on separate threads, line-based with the 64 KiB cap enforced while reading, not after.
- [ ] **Step 5: Run both suites** — expect PASS. If graceful stop does not work for console children, fix the mechanism before moving on; Task 7 depends on it.
- [ ] **Step 6: Commit** `feat(engine): job objects, graceful stop, bounded log capture`.

---

### Task 7: Service supervisor (lifecycle, preflight, restart policy)

**Files:**
- Create: `src-tauri/src/engine/{mod.rs,state.rs,supervisor.rs,preflight.rs}`, `src-tauri/src/commands/services.rs` (extend with lifecycle)
- Modify: `lib.rs` (manage `Supervisor`, adopt stale records at startup)
- Test: unit tests in `state.rs` and `preflight.rs`; `src-tauri/tests/engine_lifecycle.rs` (`#[ignore]`)

**Interfaces:**
- Consumes: `ServiceSpec`, `repo::services`, `job::spawn`, `console::send_graceful`, `job::terminate_tree`, `LogSink`, `ProcId`.
- Produces:
  - `enum Status` (same seven values, `serde(rename_all="lowercase")`) and `struct ServiceRuntime { id: String, status: Status, pid: Option<u32>, parent_pid: Option<u32>, started_at_ms: Option<u64>, exit_code: Option<i32>, restart_count: u32, last_error: Option<String> }` (camelCase).
  - `fn state::next(current: Status, event: Event) -> Result<Status, IllegalTransition>` — pure transition table; illegal: Start while `Starting|Running|Stopping`; Stop while `Stopped`.
  - `fn preflight::check(spec: &ServiceSpec, running_ports: &dyn Fn(u16)->bool) -> Vec<PreflightIssue>` — reports `MissingExecutable`, `InvalidWorkingDir`, `PortInUse{port}`; resolves a bare executable name via PATH.
  - `Supervisor` methods: `start(&self, id: &str) -> AppResult<()>`, `stop(&self, id: &str, force: bool) -> AppResult<()>`, `restart(&self, id: &str) -> AppResult<()>`, `runtime(&self, id: &str) -> ServiceRuntime`, `all_runtime(&self) -> Vec<ServiceRuntime>`, `adopt_on_startup(&self)`.
  - Commands: `service_start`, `service_stop`, `service_force_stop` (confirms natively), `service_restart`, `service_runtime_list`.
  - Event `service://status` with payload `ServiceRuntime`.
- Behavior: a service becomes `running` when its process is alive after startup (or, with a health check, when the check first passes within `startup_timeout_secs`, else `failed`); a pre-start issue returns `AppError{kind: Conflict|Validation}` with a plain-language message and leaves status `stopped`; a process exit while `running` → `failed` (non-zero) or `stopped` (zero) and applies `RestartPolicy` with backoff and max retries, emitting events; `service_processes` is written on start and cleared on exit; at startup each stored record is checked with `is_alive` — matching ones are re-adopted (status `running`, flagged `adopted`), non-matching ones are deleted and logged as stale, never signalled; on app quit services with `keep_running_on_quit=false` are stopped gracefully, the rest are left running.

- [ ] **Step 1: Write failing unit tests.** `state::next` covers every legal transition and rejects Start-while-Starting/Running/Stopping (**Review Focus 3**). `preflight::check` returns the right issue for a nonexistent executable, a file given as working directory, and an occupied port.
- [ ] **Step 2: Write failing lifecycle tests (ignored).** start `serve` → `running` with a real PID that `is_alive`; stop → `stopped`, PID gone; `ignore-stop` with `shutdown_timeout_secs=2` → forced stop after ~2 s; restart gives a new PID and `restart_count` unchanged; two services on different ports run together; a service on an occupied port is refused with a `PortInUse` message and never spawned; `exit 3` immediately → `failed` with `exit_code 3` within 2 s (**Review Focus 4**); 20 rapid `start` calls on one service spawn exactly one process (**Review Focus 3**); `crash` with `OnFailure{max_retries:2,backoff_secs:1}` restarts twice then stays `failed`; a stale record (valid PID, wrong creation time) is deleted at startup and that other process is untouched; a live record is re-adopted.
- [ ] **Step 3: Run** unit and ignored tests — expect FAIL.
- [ ] **Step 4: Implement.** One supervisor task per running service owns its `ManagedProcess`, reads exits, and sends status events; a per-service mutex plus the transition table prevents duplicate starts.
- [ ] **Step 5: Run both suites** — expect PASS.
- [ ] **Step 6: Commit** `feat(engine): service supervisor with preflight, restart policy, stale recovery`.

---

### Task 8: Health checks

**Files:**
- Create: `src-tauri/src/engine/health.rs`
- Modify: `supervisor.rs` (use it)
- Test: `health.rs` unit tests

**Interfaces:**
- Produces: `async fn run_once(check: &HealthCheck) -> HealthResult` where `enum HealthResult { Healthy, Unhealthy(String) }`; `Tcp` connects to `127.0.0.1:port`; `Http` does a GET with a timeout and accepts `min_status..=max_status`; `Command` runs the exe with args and no shell, healthy on exit 0. An `Unhealthy` result for 3 consecutive intervals moves a `running` service to `unresponsive`; a later `Healthy` returns it to `running`.
- HTTP uses a minimal client over `std`/`tokio` TCP only if no HTTP crate is already a dependency (no new network-facing dependency beyond localhost use).

- [ ] **Step 1: Write failing tests** against a local listener the test opens itself: Tcp healthy when open, unhealthy when closed; Http healthy on 200, unhealthy on 500 with range 200–399, honours timeout; Command healthy on exit 0 and unhealthy on exit 1 or a missing executable.
- [ ] **Step 2: Run** `cargo test health` — expect FAIL.
- [ ] **Step 3: Implement** and wire the 3-strikes rule with a unit-tested pure function `next_health_status(consecutive_failures: u32, current: Status) -> Status`.
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(engine): health checks`.

---

### Task 9: Secrets and environment handling

**Files:**
- Create: `src-tauri/src/windows/creds.rs`
- Modify: `repo/services.rs`, `supervisor.rs`
- Test: `creds.rs` (ignored real-store test), repo tests

**Interfaces:**
- Produces: `fn set_secret(service_id: &str, key: &str, value: &str) -> AppResult<()>`, `fn get_secret(service_id: &str, key: &str) -> AppResult<Option<String>>`, `fn delete_secrets(service_id: &str) -> AppResult<()>` using `CredWriteW/CredReadW/CredDeleteW` with target `TechomatoOrbit/<service_id>/<key>`.
- Behavior: `create`/`update` move secret values into Credential Manager and store `NULL` in SQLite; the supervisor reads them only when building the child environment; `service_get` returns secret values as `null` with `isSecret: true` and a separate `service_secret_is_set` boolean command, so the UI can show "set" without ever receiving the value; deleting a service removes its credentials.

- [ ] **Step 1: Write failing tests.** (ignored) set/get/delete round-trips, including a non-ASCII value. (unit, with an injected fake secret store trait `SecretStore`) after `create`, the SQLite row has `value IS NULL`, and no log line or event payload built for a service containing the secret includes it.
- [ ] **Step 2: Run** — expect FAIL.
- [ ] **Step 3: Implement** behind a `SecretStore` trait so repo tests use an in-memory fake.
- [ ] **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(services): secret env values in Credential Manager`.

---

### Task 10: Services and Logs UI

**Files:**
- Create: `src/features/services/components/{ServiceRow,ServiceForm,ServiceActions}/*`, `src/features/services/pages/ServicesPage/*`, `src/features/logs/{api,components,pages}/*`, `src-tauri/src/commands/logs.rs`
- Modify: `AppShell`/`Sidebar` routes, `shared/ipc/commands.ts`
- Test: `src/features/services/components/ServiceActions/powerState.test.ts`, `src/features/services/components/ServiceForm/formToSpec.test.ts`, `src/features/logs/logFilter.test.ts`

**Interfaces:**
- Consumes: all service commands and `service://status` events from Tasks 4 and 7.
- Produces (Rust commands): `log_recent(service_id: String, n: usize) -> Vec<LogLine>`, `log_history(service_id: String, offset: usize, limit: usize, filter: Option<String>) -> Vec<LogLine>`, `log_clear(service_id)` (confirms natively), `log_export(service_id, path)` via the native save dialog; event `log://line` payload `{serviceId, tsMs, stream, text}` (batched at most every 100 ms).
- Produces (TS): `availableActions(status: ServiceStatus): {start:boolean, stop:boolean, restart:boolean, forceStop:boolean}`; `formToSpec(form: ServiceFormValues): ServiceSpec`; `filterLines(lines: LogLine[], query: string, streams: Set<Stream>): LogLine[]`.
- UI: Services page lists services with `StatusDot`, name, category, real PID, uptime and port; an Add/Edit form (name, executable with file picker, arguments one per line, working directory picker, environment variables with a secret toggle, port, timeouts, restart policy, health check, keep-running-on-quit); Start / Stop / Restart / Force stop buttons enabled per `availableActions`; the Logs page shows live output with separate stderr colouring, search, pause/resume, auto-scroll, copy, clear, export, capped to the last 5 000 rendered lines (virtualised). Errors from preflight appear as a toast with the plain-language message.

- [ ] **Step 1: Write failing tests.** `availableActions`: `stopped`→start only; `running`→stop, restart, forceStop; `starting|stopping`→forceStop only; `failed|unresponsive`→start, restart, forceStop. `formToSpec` splits the argument textarea into one arg per non-empty line (quotes are kept literally, not shell-parsed) and rejects a secret var without a key. `filterLines` is case-insensitive and honours the stream set.
- [ ] **Step 2: Run** `npx vitest run src/features` — expect FAIL.
- [ ] **Step 3: Implement** pure helpers first, then components and pages; wire `useIpcEvent('service://status')` to update the query cache and `useIpcEvent('log://line')` to a bounded buffer per service.
- [ ] **Step 4: Run tests, typecheck, lint.** Expected: all pass.
- [ ] **Step 5: Manual check** (`npm start`): create a service pointing at the built `orbit-testsvc.exe serve 8765`, start it, open `http://localhost:8765`, see PID and live logs, stop it.
- [ ] **Step 6: Commit** `feat(ui): services and logs pages`.

---

### Task 11: Tray, close-to-tray, persistence and milestone acceptance

**Files:**
- Create: `src-tauri/src/tray.rs`, `docs/architecture.md`
- Modify: `lib.rs`, `README.md`
- Test: `src-tauri/tests/acceptance.rs` (`#[ignore]`)

**Interfaces:**
- Produces: tray icon with menu "Open Orbit", "Running services: N" (disabled item, refreshed on `service://status`), "Quit Orbit"; closing the window hides it; "Quit Orbit" stops services per `keep_running_on_quit`, then exits. A scripted acceptance test that drives the Rust API without the UI.

- [ ] **Step 1: Write the failing acceptance test (ignored).** Using a temp data directory: create a service for `orbit-testsvc serve <port>` with an env var and a non-default working directory, start it, assert real PID alive and HTTP 200, read logs, stop, restart, stop; drop the supervisor and DB handle, reopen from the same directory, and assert the service and all its fields (including secret-set flag) are intact and no process remains.
- [ ] **Step 2: Run** `cargo test --test acceptance -- --ignored` — expect FAIL.
- [ ] **Step 3: Implement tray and quit behaviour** as specified; write `docs/architecture.md` (modules, event names, data flow, the process-identity and stop rules).
- [ ] **Step 4: Run all suites:** `npm test`, `npm run typecheck`, `npm run lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test -- --ignored`. Expected: all pass.
- [ ] **Step 5: Manual milestone check:** create a service, start it, view logs, stop, restart, quit Orbit, reopen — configuration preserved.
- [ ] **Step 6: Commit** `feat: tray, persistence, milestone 1 acceptance`.

---

## Self-review notes

- Spec coverage for this plan's scope: custom services (T4, T7), process identity and stale recovery (T5, T7), Job Object trees and graceful-then-forced stop (T6, T7), separate stdout/stderr and bounded, rotated logs (T6, T10), health checks and auto-restart (T7, T8), secrets (T9), SQLite with migrations (T3), Techomato look and no white flash (T1), tray and keep-running policy (T11), strict CSP and typed IPC (T0, T2). Deferred to later plans per the header: Ports, Windows Services/SCM and elevated helper, Projects and dependencies, Notifications, command palette, Packages, Terminal, Doctor, VSCode cleanup, Claude Chats, updater, installer.
- Known risk: graceful stop of console children from a GUI process (Task 6). The plan fixes the mechanism (stop-helper mode) and makes Task 7 wait on its passing tests.
