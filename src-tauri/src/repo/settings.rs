use crate::db::Db;
use crate::error::AppResult;
use crate::models::prefs::Prefs;
use rusqlite::OptionalExtension;
use std::collections::HashMap;

const PREFS_KEY: &str = "prefs";

pub fn get(db: &Db, key: &str) -> AppResult<Option<String>> {
    let conn = db.0.lock().unwrap();
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set(db: &Db, key: &str, value: &str) -> AppResult<()> {
    db.0.lock().unwrap().execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn get_all(db: &Db) -> AppResult<HashMap<String, String>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Missing or unreadable preferences fall back to the defaults, field by field.
pub fn prefs(db: &Db) -> AppResult<Prefs> {
    Ok(get(db, PREFS_KEY)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub fn set_prefs(db: &Db, prefs: &Prefs) -> AppResult<()> {
    let json = serde_json::to_string(prefs).expect("prefs serialize");
    set(db, PREFS_KEY, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn a_setting_round_trips_and_overwrites() {
        let db = open_in_memory().unwrap();
        assert_eq!(get(&db, "a").unwrap(), None);
        set(&db, "a", "1").unwrap();
        set(&db, "a", "2").unwrap();
        set(&db, "b", "x").unwrap();
        assert_eq!(get(&db, "a").unwrap().as_deref(), Some("2"));
        let all = get_all(&db).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["b"], "x");
    }

    #[test]
    fn prefs_default_when_nothing_is_stored() {
        let db = open_in_memory().unwrap();
        let p = prefs(&db).unwrap();
        assert_eq!(p, Prefs::default());
        assert!(p.notify.crashed && !p.notify.started);
    }

    #[test]
    fn prefs_round_trip() {
        let db = open_in_memory().unwrap();
        let mut p = Prefs::default();
        p.notify.started = true;
        p.modules.doctor = false;
        p.start_with_windows = true;
        set_prefs(&db, &p).unwrap();
        assert_eq!(prefs(&db).unwrap(), p);
    }

    #[test]
    fn corrupt_or_partial_prefs_fall_back_to_defaults_per_field() {
        let db = open_in_memory().unwrap();
        set(&db, PREFS_KEY, "{not json").unwrap();
        assert_eq!(prefs(&db).unwrap(), Prefs::default());
        // A document from an older version that lacks newer fields keeps what it has.
        set(&db, PREFS_KEY, r#"{"notify":{"started":true}}"#).unwrap();
        let p = prefs(&db).unwrap();
        assert!(p.notify.started && p.notify.crashed);
    }
}
