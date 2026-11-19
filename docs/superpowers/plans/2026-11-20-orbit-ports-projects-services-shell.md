# Techomato Orbit — Plan 2: Ports, Windows Services, Projects, Notifications, Palette

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Orbit shows what is listening on every port and who owns it, lists and controls Windows Services (with per-action UAC through a typed elevated helper), groups custom services into projects with dependency-ordered start and reverse stop, raises native notifications, and has a Ctrl+K command palette, global shortcuts, settings and start-with-Windows.

**Architecture:** Same as Plan 1: Rust owns Windows access and SQLite; the frontend goes through the one typed IPC module and zod. New Windows access lives in `windows/net.rs` (IP Helper tables) and `windows/scm.rs` (Service Control Manager). Anything needing admin runs in a second mode of the same binary (`--elevated-op`) that accepts only a closed enum of typed operations.

**Tech Stack:** as Plan 1, plus `tauri-plugin-notification`, `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`; `windows` features `Win32_NetworkManagement_IpHelper`, `Win32_Networking_WinSock`, `Win32_System_Services`, `Win32_UI_Shell`.

**Spec:** `docs/superpowers/specs/2026-11-19-techomato-orbit-design.md`

**Builds on:** Plan 1 (`build/plan-1`): `Supervisor`, `ServiceRuntime`, `repo::services`, `ProcId`, `confirm_action`, typed IPC, `uiSlice`, Toast.

**Later plans:** 3) Packages, Terminal. 4) Doctor, VSCode cleanup, Claude Chats, updater, installer, release CI.

## Global Constraints

All of Plan 1's constraints still apply, in particular:

- App runs unelevated; custom services always start unelevated; never a permanently elevated app.
- Never kill a process Orbit did not start without a backend-enforced native confirm; **never kill a process just because it holds a port**; a PID is trusted only together with its creation time.
- The elevated helper accepts only a closed set of typed operations (Windows service start/stop/restart by validated name, end process by PID+creation time); never argv or script text.
- Protected services (driver type, image path under `%SystemRoot%`, or on the short core list) are read-only; the rule is one tested Rust function used by both the UI listing and the helper.
- No outbound network. No fabricated status or metric: a button works or is not rendered.
- Services and ports poll every 2 s while the window is visible; tray count every 15 s while hidden (switchable off).
- Clean-room code only. Commits: one per task, no attribution lines.

## Review Focus

1. Ending a process whose PID was reused since the Ports list was fetched is refused (PID + creation time re-checked at the moment of the kill). (Task 2 test)
2. Orbit's own PID, PID 0/4 and critical system processes can never be ended; a non-Orbit process is ended only after the native confirm. (Task 2 test)
3. A dependency cycle (self, two-node, cross-project) is rejected when saved; a dependency that fails obeys its abort/continue setting. (Tasks 5–6 tests)
4. A malformed or hostile elevated-helper request (unknown operation, protected service, oversize input, path-like or wildcard names) is refused without acting. (Task 9 test)
5. UDP and IPv6 rows, and processes whose path cannot be read (access denied), render without breaking the Ports table. (Tasks 1 and 3 tests)

## File Structure

```
src-tauri/src/
  windows/{net.rs,scm.rs,elevate.rs}
  models/{project.rs}
  repo/{projects.rs,settings.rs,notifications.rs}
  engine/{ports.rs,projects.rs,notify.rs,deps.rs}
  elevated.rs                       (--elevated-op mode: closed op enum, validation, executor)
  commands/{ports.rs,winservices.rs,projects.rs,settings.rs,notifications.rs}
  db/migrations/0002_projects_notifications.sql
src/features/{ports,winservices,projects,settings,notifications}/{api,components,pages}
src/shared/{palette,shortcuts}/
```

---

### Task 1: Ports backend — listening endpoints with owners

**Files:** Create `src-tauri/src/windows/net.rs`, `src-tauri/src/commands/ports.rs`; modify `Cargo.toml` (windows features), `lib.rs`.

**Interfaces:**

- Produces `net::list_endpoints() -> AppResult<Vec<Endpoint>>` where `Endpoint { proto: Proto(Tcp|Udp), local_addr: String, port: u16, state: Option<String>, pid: u32, created_at_ms: Option<u64>, process_name: Option<String>, path: Option<String> }` (serde camelCase). TCP rows are `LISTEN` only (plus established is out of scope); IPv4 and IPv6; UDP rows have no state. Owner details come from `procs::identify`/`sysinfo`; an unreadable process yields `None` name/path, never an error.
- Produces `fn port_is_free(port: u16) -> bool` (reuses `preflight::port_in_use`) and commands `ports_list() -> Vec<PortRow>`, `port_check(port) -> bool`. `PortRow` adds `service_id: Option<String>` and `project_id: Option<String>`, matched by PID against running services' `ServiceRuntime.pid` (tree members included via `procs::children_of`) and otherwise by the spec's configured `port`.
- [ ] **Step 1: Write failing tests** (`net.rs`): bind a TCP listener on `127.0.0.1:0` and a UDP socket; `list_endpoints` contains each with this process's pid, a non-empty `process_name`, and `created_at_ms` matching `identify(pid)`; an IPv6 `[::1]` listener appears when the machine supports it; rows for pids that cannot be opened (System, PID 4) have `path: None` and do not error.
- [ ] **Step 2: Run** `cargo test net::` — expect FAIL.
- [ ] **Step 3: Implement** with `GetExtendedTcpTable` (`TCP_TABLE_OWNER_PID_LISTENER`) and `GetExtendedUdpTable` (`UDP_TABLE_OWNER_PID`) for `AF_INET` and `AF_INET6`, growing the buffer on `ERROR_INSUFFICIENT_BUFFER`; convert the network-order port.
- [ ] **Step 4: Run** — expect PASS. Add the service/project matching as a pure function `annotate(endpoints, runtimes, specs, children) -> Vec<PortRow>` with unit tests (pid match, child pid match, configured-port match, no match).
- [ ] **Step 5: Commit** `feat(ports): listening endpoints with owners`.

### Task 2: End a process safely

**Files:** Create `src-tauri/src/engine/ports.rs`; modify `commands/ports.rs`.

**Interfaces:**

- Consumes: `ProcId`, `identify`, `is_alive`, `console::send_graceful_with`, `job::terminate_tree`-style kill, `confirm_action`.
- Produces `pub fn refusal(target: ProcId, own_pid: u32, name: Option<&str>) -> Option<String>` — `Some(plain-language reason)` for: Orbit's own PID, PID 0 or 4, and a short list of critical image names (`system`, `csrss.exe`, `wininit.exe`, `winlogon.exe`, `services.exe`, `lsass.exe`, `smss.exe`, `svchost.exe` is **allowed** only with the confirm). Produces `end_process(helper: &Path, target: ProcId, force: bool) -> AppResult<()>` that re-checks identity immediately before acting (`identify(pid) == Some(target)` else `Conflict` "That process has changed or already ended"), signals gracefully, waits up to 5 s, and only then (or when `force`) kills the tree with `taskkill /PID /T /F` after one more identity check. Command `port_end_process(pid, created_at_ms, force)` shows `confirm_action` first (native, naming the process, pid and port), and never runs when `refusal` is `Some`.
- [ ] **Step 1: Write failing tests:** `refusal` for own pid, 0, 4, `lsass.exe`; allowed for a normal name. Real-process tests (`#[ignore]`): spawn `orbit-testsvc serve <port>`, `end_process` ends it and the port frees; a **reused-PID** simulation (pass a `ProcId` with a wrong `created_at_ms`) is refused and the live process is untouched; an `ignore-stop` process is ended only after the graceful wait, by the forced path.
- [ ] **Step 2: Run** — expect FAIL. **Step 3: Implement.** **Step 4: Run** — expect PASS.
- [ ] **Step 5: Commit** `feat(ports): safe process end with identity re-check`.

### Task 3: Ports UI and port watch

**Files:** Create `src/features/ports/{api/usePorts.ts,components/PortTable.tsx,pages/PortsPage.tsx}`, `src/features/ports/portFilter.ts` + test; modify nav (`uiSlice.ModuleKey`), `commands.ts`.

**Interfaces:**

- Produces `filterPorts(rows, query, proto: 'all'|'tcp'|'udp', onlyMine: boolean): PortRow[]` (query matches port, process name, path, service name, case-insensitive literal). The table shows proto, address, port, process, PID, path, owning service/project, and an **End** button per row (hidden for refused pids; the backend still enforces). A "Check a port" input calls `port_check`. A watch list (stored in settings key `ports.watch`) is polled; a watched port becoming occupied by a process that is not its configured service raises a notification (Task 4 hook `notify::port_conflict`).
- [ ] **Step 1: Write failing tests** for `filterPorts` (each filter; unreadable path rows still match by port/pid; empty query keeps all). **Step 2: Run** — FAIL. **Step 3: Implement** helpers, then components with 2 s polling while visible. **Step 4: Run tests, typecheck, lint.**
- [ ] **Step 5: Commit** `feat(ui): ports page`.

### Task 4: Settings store and notifications

**Files:** Create `repo/settings.rs`, `repo/notifications.rs`, `engine/notify.rs`, `commands/settings.rs`, `commands/notifications.rs`, `db/migrations/0002_projects_notifications.sql` (also creates `projects`, `service_deps`: see Task 5); add `tauri-plugin-notification`.

**Interfaces:**

- Produces `settings::{get(db,key)->Option<String>, set(db,key,&str), get_all(db)->HashMap}` (existing `settings` table) and typed `Prefs { modules: {ports,winservices,projects,...: bool}, notify: {started,stopped,crashed,startFailed,healthFailed,portConflict,autoRestart: bool}, refreshHidden: bool, startWithWindows: bool }` with defaults.
- Produces pure `notice_for(prev: Option<&ServiceRuntime>, next: &ServiceRuntime, name: &str, prefs: &NotifyPrefs) -> Option<Notice{kind, title, body}>`: Running after Starting → `started`; Stopped after Stopping → `stopped`; Failed after Running/Starting → `crashed` (body carries the exit code); Failed with a `last_error` and no prior pid → `startFailed`; Unresponsive → `healthFailed`; `restart_count` increase → `autoRestart`; a user-initiated stop never reads as a crash. Notices are stored in `notifications` (id, ts, kind, title, body, service_id) and shown natively; commands `notification_list(limit)`, `notification_clear`, `prefs_get/prefs_set`.
- [ ] **Step 1: Write failing tests** for `notice_for` (each transition, prefs off ⇒ `None`, manual stop ⇒ no crash notice, repeated identical state ⇒ `None`) and repo round-trips. **Step 2: Run** — FAIL. **Step 3: Implement**; hook the supervisor's emit closure so each `ServiceRuntime` change passes through `notice_for` (keeping the previous runtime per id). **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(notifications): native alerts, history and prefs`.

### Task 5: Projects, dependencies and cycle detection

**Files:** Create `models/project.rs`, `repo/projects.rs`, `engine/deps.rs`; the migration from Task 4 adds `projects(id,name,description,icon,root_dir,archived,created_at)` and `service_deps(service_id,depends_on_id,on_failure)` with foreign keys and `ON DELETE CASCADE`.

**Interfaces:**

- Produces `Project { id, name, description, icon, root_dir, archived }`, repo `create/update/get/list/duplicate/archive/delete` (delete never deletes services: it sets their `project_id` NULL), `set_dependencies(db, service_id, Vec<Dep{depends_on_id, on_failure: Abort|Continue}>)` which rejects any cycle with a `Validation` error naming the cycle; pure `deps::order(nodes: &[(Id, Vec<Id>)]) -> Result<Vec<Vec<Id>>, Cycle(Vec<Id>)>` returning start **levels** (level 0 has no dependencies; services within a level may start together); `deps::stop_order` is the reverse.
- [ ] **Step 1: Write failing tests:** `order` on a chain, a diamond (levels), independent nodes, a self-dependency, a 2-cycle, a 3-cycle, a cross-project edge; `set_dependencies` rejects each cycle and leaves the previous edges intact; duplicating a project copies services and edges with new ids; deleting a project keeps its services. **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(projects): model, dependencies, cycle detection`.

### Task 6: Project orchestration

**Files:** Create/extend `engine/projects.rs`; `commands/projects.rs`.

**Interfaces:**

- Consumes: `Supervisor::{start,stop,runtime}`, `deps::order`, `ServiceRuntime.status`.
- Produces `start_project(sup, db, project_id, selection: Option<Vec<Id>>) -> AppResult<ProjectRunReport>` that starts level by level; within a level starts concurrently; a service counts as ready when `Running` (which already means health passed when a health check exists) within its `startup_timeout_secs`; a dependency that does not become ready obeys its edge's `on_failure`: **Abort** stops starting dependents (they are reported `skipped`), **Continue** proceeds with a warning in the report. `stop_project` stops in reverse level order, skipping services marked keep-running-on-quit only when invoked from quit. `restart_project`. Report: per service `started|already_running|failed(msg)|skipped(reason)`. Events `project://progress` `{projectId, serviceId, step}`.
- [ ] **Step 1: Write failing tests** (`#[ignore]`, real processes via `orbit-testsvc`): a chain A←B←C starts in order (assert order from recorded `service://status` events, not sleeps) and stops C,B,A; a failing dependency with **Abort** skips its dependents and reports why; with **Continue** dependents still start; an already-running dependency is reused; a diamond starts the two middle services concurrently (both Starting before either is Running when each has a slow health check); selection starts a subset plus its required dependencies. **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(projects): dependency-ordered start and reverse stop`.

### Task 7: Projects UI

**Files:** Create `src/features/projects/{api,components,pages}`, `dependencyGraph.ts` + test; modify nav, `ServiceForm` (project select), `commands.ts`.

**Interfaces:**

- Produces `layoutLevels(services, deps): string[][]` mirroring the Rust levels for the simple order view (tested to agree on the same fixtures), a Projects list (status roll-up, Start all / Stop all / Restart all, per-service checkboxes for start/stop selected), a project editor (name, description, icon, root directory picker, member services, dependency editor with abort/continue), duplicate, archive, delete (native confirm), and a read-only order view. Cycle errors from Rust show as the toast message.
- [ ] **Step 1: Write failing tests** for `layoutLevels` and the roll-up (`projectStatus(runtimes)`: all running ⇒ running, some ⇒ partial, any failed ⇒ failed, none ⇒ stopped). **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run tests, typecheck, lint.**
- [ ] **Step 5: Commit** `feat(ui): projects`.

### Task 8: Windows Services (SCM), read side and the protected rule

**Files:** Create `windows/scm.rs`, `commands/winservices.rs`; `windows` feature `Win32_System_Services`.

**Interfaces:**

- Produces `WinService { name, display_name, state: Stopped|StartPending|StopPending|Running|ContinuePending|PausePending|Paused, start_type: Auto|AutoDelayed|Manual|Disabled|Boot|System, service_type: String, binary_path: Option<String>, pid: Option<u32>, protected: bool, developer: bool }`; `scm::list() -> AppResult<Vec<WinService>>` via `OpenSCManagerW(SC_MANAGER_ENUMERATE_SERVICE)` + `EnumServicesStatusExW` (`SC_ENUM_PROCESS_INFO`) + `QueryServiceConfigW`/`QueryServiceConfig2W` per service opened with `SERVICE_QUERY_CONFIG` only (no access beyond read). Pure `is_protected(service_type_flags, binary_path, name, system_root) -> bool` (driver types, image path under `%SystemRoot%` after normalising quotes/arguments/case, or name on a built-in list: `RpcSs, DcomLaunch, LSM, SamSs, EventLog, Winmgmt, BFE, mpssvc, CryptSvc, Dhcp, Dnscache, PlugPlay, Power, ProfSvc, SENS, Schedule, TrustedInstaller, WinDefend`); pure `is_developer(name, display, patterns, pins) -> bool` with a built-in pattern list (`postgres*, mysql*, mariadb*, mongo*, redis*, memurai*, mssql*, sqlserver*, docker*, com.docker*, wsl*, vmcompute, hns, rabbitmq*, elastic*, kafka*, nginx*, apache*, httpd*, iis*, w3svc, was, mosquitto*, neo4j*, couchdb*, minio*, consul*, vault*`) plus user pins (settings `winservices.pins`). Command `winservice_list(all: bool)`.
- [ ] **Step 1: Write failing tests:** `is_protected` table (driver, `C:\Windows\System32\svchost.exe -k x`, quoted paths, a `C:\Program Files\PostgreSQL\bin\postgres.exe` service is **not** protected, listed core names protected, case/`%SystemRoot%` forms); `is_developer` (patterns, pins, case). `#[ignore]` real test: `list()` returns >20 services including `EventLog` marked protected and running with a pid. **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(services): windows services listing and protected rule`.

### Task 9: Elevated helper (closed typed operations)

**Files:** Create `src-tauri/src/elevated.rs`, `windows/elevate.rs`; modify `main.rs` (new mode), `windows/scm.rs` (control calls).

**Interfaces:**

- Produces `enum ElevatedOp { ServiceStart{name}, ServiceStop{name}, ServiceRestart{name}, EndProcess{pid, created_at_ms} }` (serde, `deny_unknown_fields`, tagged), `fn validate(op: &ElevatedOp) -> Result<(), String>` (service names: 1–256 chars, no `\ / * ? " < > |` or control characters, must exist in the SCM and **not** be protected; `EndProcess` re-checks `engine::ports::refusal` and identity), `fn execute(op) -> OpResult{ok, message}`, and `windows::elevate::run_elevated(op) -> AppResult<OpResult>`: builds a base64 JSON request (max 1 KiB, rejected above), launches `<own exe> --elevated-op <base64> --result <temp file path>` with `ShellExecuteExW` verb `runas` (one UAC prompt, `SW_HIDE`), waits for exit, reads the result file (created by the caller with a random name, size-capped), deletes it. `main.rs` handles `--elevated-op`: decode, `validate`, `execute`, write result, exit code 0/1; any decode or validation failure writes a refusal and acts on nothing. UAC cancel maps to `ErrorKind::Cancelled` (silent).
- [ ] **Step 1: Write failing tests:** decode rejects oversize, bad base64, unknown op tags, unknown fields, empty/path-like/wildcard names (`..\x`, `a*`, `a/b`, names with NUL); `validate` refuses protected names and a PID/creation mismatch; `execute` for `EndProcess` on a real spawned `orbit-testsvc` works **unelevated** (it is the same process-owner case), proving the executor path without UAC. `#[ignore]` manual-only: a real `run_elevated(ServiceStop)` against a throwaway service (documented, needs a UAC click).
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(elevate): typed elevated helper`.

### Task 10: Windows Services UI and control

**Files:** Create `src/features/winservices/{api,components,pages}`, `winFilter.ts` + test; modify `commands.ts`, nav; `commands/winservices.rs` gains `winservice_start/stop/restart(name)` which call `run_elevated` after a `confirm_action` that states a UAC prompt will follow.

**Interfaces:**

- Produces `filterWinServices(list, query, state, startType, showAll): WinService[]`; a Services page with two sections: **Orbit services** (Plan 1) and **Windows services** (developer filter on by default, "show all" toggle, pin/unpin, state pill, start type, PID); protected rows show a lock badge and no action buttons; Start/Stop/Restart enabled per state; the UAC cancellation is silent; state refreshes after the call.
- [ ] **Step 1: Write failing tests** for `filterWinServices` and `winActions(state, protected): {start,stop,restart}` (protected ⇒ all false; running ⇒ stop, restart; stopped ⇒ start; pending states ⇒ none). **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run tests, typecheck, lint.**
- [ ] **Step 5: Commit** `feat(ui): windows services`.

### Task 11: Command palette, global shortcuts, settings page, start with Windows

**Files:** Create `src/shared/palette/{rankCommands.ts,Palette.tsx}` + test, `src/features/settings/pages/SettingsPage.tsx`, `src-tauri/src/shortcuts.rs`; add `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`; capabilities.

**Interfaces:**

- Produces `rankCommands(commands: {id,title,keywords?}[], query): Command[]` (subsequence + prefix scoring, stable, title match beats keyword match, empty query keeps registration order). Ctrl+K opens the palette; commands: go to each enabled module, start/stop/restart any service or project, check a port, toggle theme, open settings. Settings page: per-module on/off (disabled modules disappear from the nav), notification toggles, refresh options, start-with-Windows (autostart plugin), shortcut display. Global shortcuts (defaults `Ctrl+Alt+O` open Orbit, `Ctrl+Alt+S` start all in the last-used project, `Ctrl+Alt+X` stop it) registered in Rust; a conflict registers nothing and reports it in Settings. Refresh policy: `useVisible()` hook pauses 2 s polling when hidden; tray count polls every 15 s unless disabled.
- [ ] **Step 1: Write failing tests** for `rankCommands` (prefix > subsequence > none, keyword hit, empty query, ties keep order). **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run tests, typecheck, lint.**
- [ ] **Step 5: Commit** `feat: command palette, shortcuts, settings, autostart`.

### Task 12: Acceptance and docs

**Files:** `src-tauri/tests/acceptance_plan2.rs` (`#[ignore]`), `docs/architecture.md`, `README.md`.

- [ ] **Step 1: Write the acceptance test:** temp data dir; create two projects' worth of services (`orbit-testsvc`) with a dependency chain; start the project (order asserted); `list_endpoints` + `annotate` shows both ports mapped to their service and project; end one port holder through `end_process`; stop the project (reverse order); reopen the database and find projects, edges and settings intact.
- [ ] **Step 2: Run** `cargo test --test acceptance_plan2 -- --ignored` — expect PASS once Tasks 1–11 are in. **Step 3:** update `docs/architecture.md` (Ports, SCM, elevated helper, projects, notifications, settings) and the README feature list.
- [ ] **Step 4: Run all suites:** `npm test`, `npm run typecheck`, `npm run lint`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test -- --ignored`.
- [ ] **Step 5: Drive the real app** through the CDP harness for Ports, Projects and the Windows Services list (read-only), then commit `feat: plan 2 acceptance and docs`.
