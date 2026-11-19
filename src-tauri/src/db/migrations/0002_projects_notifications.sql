CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    icon        TEXT NOT NULL DEFAULT '',
    root_dir    TEXT NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_projects_name ON projects (name COLLATE NOCASE);

CREATE TABLE service_deps (
    service_id    TEXT NOT NULL REFERENCES services (id) ON DELETE CASCADE,
    depends_on_id TEXT NOT NULL REFERENCES services (id) ON DELETE CASCADE,
    on_failure    TEXT NOT NULL DEFAULT 'abort',
    PRIMARY KEY (service_id, depends_on_id),
    CHECK (service_id <> depends_on_id)
);
CREATE INDEX idx_service_deps_dep ON service_deps (depends_on_id);

CREATE TABLE notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL,
    service_id TEXT
);
CREATE INDEX idx_notifications_ts ON notifications (ts);
