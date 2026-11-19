//! SQLite store. Versioned migrations keyed on `PRAGMA user_version`.

use crate::error::{AppError, AppResult, ErrorKind};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

/// Ordered `(version, sql)`; each is applied in its own transaction.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("migrations/0001_init.sql")),
    (
        2,
        include_str!("migrations/0002_projects_notifications.sql"),
    ),
];

pub const SCHEMA_VERSION: u32 = MIGRATIONS[MIGRATIONS.len() - 1].0;

fn user_version(conn: &Connection) -> AppResult<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    let current = user_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(AppError::new(
            ErrorKind::Db,
            format!(
                "This data was written by a newer Techomato Orbit (database version {current}, \
                 this app understands up to {SCHEMA_VERSION}). Update Orbit; your data was not changed."
            ),
        ));
    }
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn open(path: &Path) -> AppResult<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    // Check the version before any pragma that writes (WAL rewrites the file header), so a
    // newer or foreign file is never modified.
    let found = user_version(&conn).map_err(|e| {
        AppError::new(
            ErrorKind::Db,
            format!(
                "{} is not a usable Orbit database; it was left untouched.",
                path.display()
            ),
        )
        .with_detail(e.message)
    })?;
    if found > SCHEMA_VERSION {
        migrate(&mut conn)?; // returns the "newer version" error
    }
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut conn)?;
    Ok(Db(Mutex::new(conn)))
}

pub fn open_in_memory() -> AppResult<Db> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut conn)?;
    Ok(Db(Mutex::new(conn)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: [&str; 8] = [
        "services",
        "service_env",
        "service_processes",
        "activity_history",
        "settings",
        "projects",
        "service_deps",
        "notifications",
    ];

    fn version(db: &Db) -> u32 {
        db.0.lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn in_memory_db_is_migrated_with_all_tables() {
        let db = open_in_memory().unwrap();
        assert_eq!(version(&db), SCHEMA_VERSION);
        let conn = db.0.lock().unwrap();
        for t in TABLES {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "missing table {t}");
        }
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let db = open_in_memory().unwrap();
        let mut conn = db.0.lock().unwrap();
        migrate(&mut conn).unwrap();
        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn deleting_a_service_cascades_to_env_and_processes() {
        let db = open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO services (id,name,category,working_dir,executable,startup_timeout_secs,
             shutdown_timeout_secs,restart_json,created_at,updated_at)
             VALUES ('s1','n','custom','.','x',30,10,'\"never\"',0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO service_env (service_id,position,key,value,is_secret) VALUES ('s1',0,'A','1',0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO service_processes (service_id,pid,created_at_ms,started_at) VALUES ('s1',1,1,1)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM services WHERE id='s1'", [])
            .unwrap();
        for t in ["service_env", "service_processes"] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{t} not cascaded");
        }
    }

    #[test]
    fn newer_schema_is_refused_and_the_file_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orbit.db");
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE keep (x); PRAGMA user_version = 999;")
                .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let err = open(&path).err().expect("must refuse a newer schema");
        assert_eq!(err.kind, crate::error::ErrorKind::Db);
        assert!(err.message.contains("999"), "{}", err.message);
        assert!(
            err.message.contains(&SCHEMA_VERSION.to_string()),
            "{}",
            err.message
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_non_sqlite_file_is_refused_and_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orbit.db");
        std::fs::write(&path, b"this is not a database, it is precious").unwrap();
        assert!(open(&path).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"this is not a database, it is precious"
        );
    }

    #[test]
    fn a_missing_parent_directory_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("orbit.db");
        let db = open(&path).unwrap();
        assert_eq!(version(&db), SCHEMA_VERSION);
        assert!(path.exists());
    }
}
