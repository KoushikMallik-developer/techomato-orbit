use crate::db::Db;
use crate::error::AppResult;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredNotice {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub service_id: Option<String>,
}

/// History is capped so it cannot grow without bound.
pub const KEEP: i64 = 500;

pub fn add(
    db: &Db,
    ts: i64,
    kind: &str,
    title: &str,
    body: &str,
    service_id: Option<&str>,
) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO notifications (ts, kind, title, body, service_id) VALUES (?1,?2,?3,?4,?5)",
        rusqlite::params![ts, kind, title, body, service_id],
    )?;
    conn.execute(
        "DELETE FROM notifications WHERE id NOT IN
         (SELECT id FROM notifications ORDER BY id DESC LIMIT ?1)",
        [KEEP],
    )?;
    Ok(())
}

/// Newest first.
pub fn list(db: &Db, limit: i64) -> AppResult<Vec<StoredNotice>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, ts, kind, title, body, service_id FROM notifications ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(StoredNotice {
            id: r.get(0)?,
            ts: r.get(1)?,
            kind: r.get(2)?,
            title: r.get(3)?,
            body: r.get(4)?,
            service_id: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn clear(db: &Db) -> AppResult<()> {
    db.0.lock()
        .unwrap()
        .execute("DELETE FROM notifications", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn notices_are_listed_newest_first_and_can_be_cleared() {
        let db = open_in_memory().unwrap();
        add(&db, 1, "crashed", "API crashed", "exit 1", Some("s1")).unwrap();
        add(&db, 2, "started", "API started", "", None).unwrap();
        let l = list(&db, 10).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].kind, "started");
        assert_eq!(l[1].service_id.as_deref(), Some("s1"));
        assert_eq!(list(&db, 1).unwrap().len(), 1);
        clear(&db).unwrap();
        assert!(list(&db, 10).unwrap().is_empty());
    }

    #[test]
    fn history_keeps_only_the_most_recent_entries() {
        let db = open_in_memory().unwrap();
        for i in 0..(KEEP + 20) {
            add(&db, i, "started", "t", "b", None).unwrap();
        }
        let all = list(&db, KEEP + 100).unwrap();
        assert_eq!(all.len() as i64, KEEP);
        assert_eq!(all[0].ts, KEEP + 19);
    }
}
