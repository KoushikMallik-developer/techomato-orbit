# Orbit Plan 3: UI refresh, Packages, Terminal

> Executed natively (inline), one commit per task, one whole-branch review at the end.

**Goal:** Modernise the whole UI (keeping the Techomato identity), then add the Packages module (winget, Scoop, Chocolatey) and the Terminal module (ConPTY + xterm.js).

**Spec:** `docs/superpowers/specs/2026-11-19-techomato-orbit-design.md` (items 7 and 8, UI look).

## Global Constraints

- Techomato identity stays: paper/ink/tomato `#C8381F`, Bricolage Grotesque headings, Instrument Sans body, pill buttons, light and dark. Colours only through tokens in `src/styles/theme.css`.
- Icons come from one library (`lucide-react`), one stroke weight. No emoji or glyphs as icons.
- Only `src/shared/ipc/invoke.ts` imports `@tauri-apps/api/core`. Rust errors are `{kind,message,detail}`; `cancelled` is silent.
- Package managers and shells are started with argument arrays, never a joined command string; every call has a timeout; output streams as events.
- Parsers are tested against real output captured on this machine (`src-tauri/tests/fixtures/pm/`). Scoop is not installed here, so its fixtures are written from its documented format and flagged unverified.

## Review Focus

1. A package id or search text containing spaces, quotes, `&`, `|` or `;` is passed as one argument and never interpreted by a shell. (Task 2 test)
2. winget output with spinner or progress lines before the table, with CRLF, and with an empty result, parses without panicking. (Task 2 tests)
3. A manager that is missing or hangs reports a plain message and never blocks the UI. (Task 3 test)
4. A shell profile that is not installed is not offered; closing a tab ends its process tree. (Task 5 test)
5. Terminal output with partial UTF-8 sequences split across reads is not corrupted. (Task 5 test)

## Tasks

### Task 1: UI refresh

Tokens (depth, radius 16, scrollbar, selection, tabular numerals), shared `ui/` parts (`PageHeader`, `EmptyState`, `Input`, `Select`, `Segmented`, `Badge`, `Icon` via lucide), a new sidebar (icons, running-count badge, `Ctrl K` hint, collapsed groups ready for new modules), one page-enter motion, denser tables, and every existing page moved onto them.
**Test:** vitest for any new pure helper; typecheck, lint; live visual check in both themes.
**Commit:** `feat(ui): modern shell and shared components`

### Task 2: Package parsers and adapters (Rust)

`pm/` module: `Manager` enum, `Package {id,name,version,available,source,manager}`, pure parsers `parse_winget_table`, `parse_choco_limit`, `parse_scoop_export`/`parse_scoop_status`, and command builders returning `Vec<String>` argument arrays (`list`, `outdated`, `search`, `install`, `upgrade`, `uninstall`). Ids and queries are validated (non-empty, ≤200 chars, no control characters).
**Tests first:** parsers against the real fixtures; empty/spinner/CRLF cases; argument builders keep hostile text as one argument.
**Commit:** `feat(packages): parsers and command builders`

### Task 3: Package commands and events

`pm/run.rs` runs a manager with timeout, streams lines as `pm://output`, detects installed managers (`which`-style lookup on PATH; Scoop hidden when absent). Commands: `pm_managers`, `pm_installed`, `pm_outdated`, `pm_search`, `pm_install`, `pm_upgrade`, `pm_upgrade_all`, `pm_uninstall` (native confirm for uninstall and upgrade-all).
**Tests:** missing manager and timeout produce plain errors (fake executable); real `choco list` and `winget list` smoke test (`#[ignore]`).
**Commit:** `feat(packages): commands and streaming output`

### Task 4: Packages UI

Page with manager tabs, Installed/Outdated/Search views, filter box, per-row Upgrade/Uninstall, bulk "Upgrade all", live output drawer, empty and error states, `PackagesPage` behind the module toggle and palette entries.
**Tests:** vitest for filter/sort helpers and the zod schemas; live check against the real app.
**Commit:** `feat(ui): packages`

### Task 5: Terminal backend (Rust)

`pty.rs` with `portable-pty`: `Profile` detection (PowerShell 7, Windows PowerShell, cmd, Git Bash, WSL distros; only installed ones), `open(profile, cwd, cols, rows)`, `write`, `resize`, `close` (ends the tree). Output is emitted as `term://data` with a UTF-8 decoder that carries partial sequences between reads. Commands: `term_profiles`, `term_open`, `term_write`, `term_resize`, `term_close`, `term_open_external`.
**Tests first:** profile detection filters missing shells; UTF-8 carry decoder; a real `cmd` session echo round trip (`#[ignore]`).
**Commit:** `feat(terminal): conpty sessions`

### Task 6: Terminal UI

`@xterm/xterm` + fit addon (lazy-loaded), tabbed bottom drawer toggled with Ctrl+`, profile picker, working-directory picker, copy/paste, close-tab, "open external terminal", themed from tokens in light and dark.
**Tests:** vitest for tab state helpers; live check that a command runs and output renders.
**Commit:** `feat(ui): terminal drawer`

### Task 7: Settings, palette, acceptance, docs

Module toggles for Packages and Terminal; palette entries; acceptance test (`#[ignore]`): detect managers, list installed via real choco/winget, open a cmd session, run `echo`, close; update `docs/architecture.md` and README. Full suites and a live pass.
**Commit:** `feat: plan 3 acceptance and docs`

### Final

One fresh whole-branch review (Opus), one fix pass with RED→GREEN tests, rulings and deferred minors in the final message.
