# Database schema

SQLite file: `%APPDATA%\Techomato Orbit\orbit.db` (WAL mode, foreign keys on).
Migrations live in `src-tauri/src/db/migrations/` and are applied in order, each in its own
transaction, tracked by `PRAGMA user_version`. A database with a newer version than the app
understands is refused and left untouched.

## Version 1

| Table               | Purpose                                                                                                                                                                     |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `services`          | One row per user-defined service: executable, args (JSON array), working dir, timeouts, restart policy (JSON), optional health check (JSON), flags.                         |
| `service_env`       | Environment variables per service, ordered by `position`. `value` is NULL when `is_secret = 1`; the secret lives in Windows Credential Manager. Cascades on service delete. |
| `service_processes` | The live process Orbit started for a service: `pid` + `created_at_ms` (identity is both, never PID alone). Cleared on exit; checked at startup for stale records.           |
| `activity_history`  | Append-only log of lifecycle events (started, stopped, crashed, stale record removed).                                                                                      |
| `settings`          | Key/value application settings.                                                                                                                                             |

Projects, dependencies and notifications tables arrive in plan 2.
