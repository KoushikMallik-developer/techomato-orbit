//! The live process Orbit started for each service (`pid` + creation time), plus the activity
//! log. Records are written at start, cleared at exit, and checked for staleness at startup.

use crate::db::Db;
use crate::error::AppResult;
use crate::windows::procs::ProcId;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub service_id: String,
    pub id: ProcId,
}

pub fn save(db: &Db, service_id: &str, id: &ProcId, started_at_ms: i64) -> AppResult<()> {
    db.0.lock().unwrap().execute(
        "INSERT OR REPLACE INTO service_processes (service_id,pid,created_at_ms,started_at,adopted)
         VALUES (?1,?2,?3,?4,0)",
        params![service_id, id.pid, id.created_at_ms as i64, started_at_ms],
    )?;
    Ok(())
}

pub fn clear(db: &Db, service_id: &str) -> AppResult<()> {
    db.0.lock().unwrap().execute(
        "DELETE FROM service_processes WHERE service_id=?1",
        [service_id],
    )?;
    Ok(())
}

pub fn list(db: &Db) -> AppResult<Vec<ProcessRecord>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare("SELECT service_id,pid,created_at_ms FROM service_processes")?;
    let rows = stmt.query_map([], |r| {
        Ok(ProcessRecord {
            service_id: r.get(0)?,
            id: ProcId {
                pid: r.get::<_, i64>(1)? as u32,
                created_at_ms: r.get::<_, i64>(2)? as u64,
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Appends to the activity history. Failures are swallowed: history must never break a service.
pub fn note(db: &Db, service_id: Option<&str>, kind: &str, message: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let _ = db.0.lock().unwrap().execute(
        "INSERT INTO activity_history (ts,service_id,kind,message) VALUES (?1,?2,?3,?4)",
        params![ts, service_id, kind, message],
    );
}

/// Drops history older than `keep_days`. Nothing shows it yet, so it must not grow forever.
pub fn prune_history(db: &Db, keep_days: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let _ = db.0.lock().unwrap().execute(
        "DELETE FROM activity_history WHERE ts < ?1",
        [now - keep_days * 86_400_000],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::service::fixtures::sample;
    use crate::repo::services;
    use crate::secrets::InMemorySecretStore;

    #[test]
    fn save_list_clear_round_trip_and_replace() {
        let db = open_in_memory().unwrap();
        let s = services::create(&db, &InMemorySecretStore::default(), &sample()).unwrap();
        let a = ProcId {
            pid: 10,
            created_at_ms: 111,
        };
        let b = ProcId {
            pid: 11,
            created_at_ms: 222,
        };
        save(&db, &s.id, &a, 5).unwrap();
        save(&db, &s.id, &b, 6).unwrap(); // one live process per service: replaces
        let rows = list(&db).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, b);
        clear(&db, &s.id).unwrap();
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn old_history_is_pruned_and_recent_history_kept() {
        let db = open_in_memory().unwrap();
        db.0.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_history (ts,kind,message) VALUES (1,'started','old')",
                [],
            )
            .unwrap();
        note(&db, None, "started", "new");
        prune_history(&db, 30);
        let left: Vec<String> = {
            let conn = db.0.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT message FROM activity_history")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get(0)).unwrap();
            rows.map(Result::unwrap).collect()
        };
        assert_eq!(left, ["new"]);
    }

    #[test]
    fn notes_are_appended() {
        let db = open_in_memory().unwrap();
        note(&db, None, "started", "hello");
        let n: i64 =
            db.0.lock()
                .unwrap()
                .query_row("SELECT count(*) FROM activity_history", [], |r| r.get(0))
                .unwrap();
        assert_eq!(n, 1);
    }
}
