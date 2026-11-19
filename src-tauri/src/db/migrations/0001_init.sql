CREATE TABLE services (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    description           TEXT NOT NULL DEFAULT '',
    category              TEXT NOT NULL,
    project_id            TEXT,
    working_dir           TEXT NOT NULL,
    executable            TEXT NOT NULL,
    args_json             TEXT NOT NULL DEFAULT '[]',
    run_through_shell     INTEGER NOT NULL DEFAULT 0,
    port                  INTEGER,
    startup_timeout_secs  INTEGER NOT NULL,
    shutdown_timeout_secs INTEGER NOT NULL,
    restart_json          TEXT NOT NULL,
    health_json           TEXT,
    keep_running_on_quit  INTEGER NOT NULL DEFAULT 0,
    allow_multiple        INTEGER NOT NULL DEFAULT 0,
    enabled               INTEGER NOT NULL DEFAULT 1,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL
);
CREATE INDEX idx_services_name ON services (name COLLATE NOCASE);
CREATE INDEX idx_services_project ON services (project_id);

CREATE TABLE service_env (
    service_id TEXT NOT NULL REFERENCES services (id) ON DELETE CASCADE,
    position   INTEGER NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT,
    is_secret  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (service_id, key)
);
CREATE INDEX idx_service_env_service ON service_env (service_id);

CREATE TABLE service_processes (
    service_id    TEXT PRIMARY KEY REFERENCES services (id) ON DELETE CASCADE,
    pid           INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    started_at    INTEGER NOT NULL,
    adopted       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE activity_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         INTEGER NOT NULL,
    service_id TEXT,
    kind       TEXT NOT NULL,
    message    TEXT NOT NULL
);
CREATE INDEX idx_activity_ts ON activity_history (ts);
CREATE INDEX idx_activity_service ON activity_history (service_id);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
