//! Persistence for `ServiceSpec`. Secret env values are routed to the `SecretStore`;
//! SQLite only ever stores NULL for them.

use crate::db::Db;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::models::service::{Category, EnvVar, HealthCheck, RestartPolicy, ServiceSpec};
use crate::secrets::SecretStore;
use rusqlite::{params, Connection, Row};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn category_to_str(c: Category) -> AppResult<String> {
    match serde_json::to_value(c)? {
        serde_json::Value::String(s) => Ok(s),
        _ => Err(AppError::new(ErrorKind::Internal, "bad category")),
    }
}

fn category_from_str(s: &str) -> AppResult<Category> {
    Ok(serde_json::from_value(serde_json::Value::String(s.into()))?)
}

const COLUMNS: &str = "id,name,description,category,project_id,working_dir,executable,args_json,\
    run_through_shell,port,startup_timeout_secs,shutdown_timeout_secs,restart_json,health_json,\
    keep_running_on_quit,allow_multiple,enabled";

fn row_to_spec(row: &Row) -> rusqlite::Result<(ServiceSpec, String)> {
    // JSON/category parse errors are surfaced as conversion failures.
    let conv = |e: AppError| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    };
    let category: String = row.get(3)?;
    let args_json: String = row.get(7)?;
    let restart_json: String = row.get(12)?;
    let health_json: Option<String> = row.get(13)?;
    let id: String = row.get(0)?;
    let spec = ServiceSpec {
        id: id.clone(),
        name: row.get(1)?,
        description: row.get(2)?,
        category: category_from_str(&category).map_err(conv)?,
        project_id: row.get(4)?,
        working_dir: row.get(5)?,
        executable: row.get(6)?,
        args: serde_json::from_str(&args_json).map_err(|e| conv(e.into()))?,
        run_through_shell: row.get::<_, i64>(8)? != 0,
        env: vec![],
        port: row.get::<_, Option<i64>>(9)?.map(|p| p as u16),
        startup_timeout_secs: row.get::<_, i64>(10)? as u32,
        shutdown_timeout_secs: row.get::<_, i64>(11)? as u32,
        restart: serde_json::from_str::<RestartPolicy>(&restart_json)
            .map_err(|e| conv(e.into()))?,
        health: health_json
            .map(|j| serde_json::from_str::<HealthCheck>(&j))
            .transpose()
            .map_err(|e| conv(e.into()))?,
        keep_running_on_quit: row.get::<_, i64>(14)? != 0,
        allow_multiple: row.get::<_, i64>(15)? != 0,
        enabled: row.get::<_, i64>(16)? != 0,
    };
    Ok((spec, id))
}

fn load_env(conn: &Connection, id: &str) -> AppResult<Vec<EnvVar>> {
    let mut stmt = conn.prepare(
        "SELECT key,value,is_secret FROM service_env WHERE service_id=?1 ORDER BY position",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(EnvVar {
            key: r.get(0)?,
            value: r.get(1)?,
            is_secret: r.get::<_, i64>(2)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn not_found(id: &str) -> AppError {
    AppError::new(ErrorKind::NotFound, format!("No service with id {id}."))
}

pub fn get(db: &Db, id: &str) -> AppResult<ServiceSpec> {
    let conn = db.0.lock().unwrap();
    get_with(&conn, id)
}

fn get_with(conn: &Connection, id: &str) -> AppResult<ServiceSpec> {
    let found = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM services WHERE id=?1"),
            [id],
            row_to_spec,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => not_found(id),
            other => other.into(),
        })?;
    let mut spec = found.0;
    spec.env = load_env(conn, id)?;
    Ok(spec)
}

pub fn list(db: &Db) -> AppResult<Vec<ServiceSpec>> {
    let conn = db.0.lock().unwrap();
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM services ORDER BY name COLLATE NOCASE, id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    ids.iter().map(|id| get_with(&conn, id)).collect()
}

fn write_env(conn: &Connection, id: &str, env: &[EnvVar]) -> AppResult<()> {
    conn.execute("DELETE FROM service_env WHERE service_id=?1", [id])?;
    for (i, v) in env.iter().enumerate() {
        let stored = if v.is_secret {
            None
        } else {
            v.value.as_deref()
        };
        conn.execute(
            "INSERT INTO service_env (service_id,position,key,value,is_secret) VALUES (?1,?2,?3,?4,?5)",
            params![id, i as i64, v.key, stored, v.is_secret as i64],
        )?;
    }
    Ok(())
}

fn args_restart_health(s: &ServiceSpec) -> AppResult<(String, String, Option<String>)> {
    Ok((
        serde_json::to_string(&s.args)?,
        serde_json::to_string(&s.restart)?,
        s.health.as_ref().map(serde_json::to_string).transpose()?,
    ))
}

pub fn create(db: &Db, secrets: &dyn SecretStore, spec: &ServiceSpec) -> AppResult<ServiceSpec> {
    spec.validate()?;
    let mut s = spec.clone();
    s.id = uuid::Uuid::new_v4().to_string();
    let (args, restart, health) = args_restart_health(&s)?;
    {
        let mut conn = db.0.lock().unwrap();
        let tx = conn.transaction()?;
        let now = now_ms();
        tx.execute(
            "INSERT INTO services (id,name,description,category,project_id,working_dir,executable,
             args_json,run_through_shell,port,startup_timeout_secs,shutdown_timeout_secs,restart_json,
             health_json,keep_running_on_quit,allow_multiple,enabled,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18)",
            params![
                s.id,
                s.name,
                s.description,
                category_to_str(s.category)?,
                s.project_id,
                s.working_dir,
                s.executable,
                args,
                s.run_through_shell as i64,
                s.port.map(i64::from),
                s.startup_timeout_secs,
                s.shutdown_timeout_secs,
                restart,
                health,
                s.keep_running_on_quit as i64,
                s.allow_multiple as i64,
                s.enabled as i64,
                now
            ],
        )?;
        write_env(&tx, &s.id, &s.env)?;
        tx.commit()?;
    }
    if let Err(e) = store_secrets(secrets, &s) {
        // Don't leave a half-created service behind.
        let _ = delete(db, secrets, &s.id);
        return Err(e);
    }
    get(db, &s.id)
}

fn store_secrets(secrets: &dyn SecretStore, s: &ServiceSpec) -> AppResult<()> {
    for v in s.env.iter().filter(|v| v.is_secret) {
        if let Some(value) = &v.value {
            secrets.set(&s.id, &v.key, value)?;
        }
    }
    Ok(())
}

pub fn update(db: &Db, secrets: &dyn SecretStore, spec: &ServiceSpec) -> AppResult<ServiceSpec> {
    spec.validate()?;
    let (args, restart, health) = args_restart_health(spec)?;
    let previous_secret_keys: Vec<String> = get(db, &spec.id)?
        .env
        .into_iter()
        .filter(|v| v.is_secret)
        .map(|v| v.key)
        .collect();
    {
        let mut conn = db.0.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE services SET name=?2,description=?3,category=?4,project_id=?5,working_dir=?6,
             executable=?7,args_json=?8,run_through_shell=?9,port=?10,startup_timeout_secs=?11,
             shutdown_timeout_secs=?12,restart_json=?13,health_json=?14,keep_running_on_quit=?15,
             allow_multiple=?16,enabled=?17,updated_at=?18 WHERE id=?1",
            params![
                spec.id,
                spec.name,
                spec.description,
                category_to_str(spec.category)?,
                spec.project_id,
                spec.working_dir,
                spec.executable,
                args,
                spec.run_through_shell as i64,
                spec.port.map(i64::from),
                spec.startup_timeout_secs,
                spec.shutdown_timeout_secs,
                restart,
                health,
                spec.keep_running_on_quit as i64,
                spec.allow_multiple as i64,
                spec.enabled as i64,
                now_ms()
            ],
        )?;
        write_env(&tx, &spec.id, &spec.env)?;
        tx.commit()?;
    }
    store_secrets(secrets, spec)?;
    for key in previous_secret_keys {
        let still_secret = spec.env.iter().any(|v| v.is_secret && v.key == key);
        if !still_secret {
            secrets.delete(&spec.id, &key)?;
        }
    }
    get(db, &spec.id)
}

pub fn delete(db: &Db, secrets: &dyn SecretStore, id: &str) -> AppResult<()> {
    let removed =
        db.0.lock()
            .unwrap()
            .execute("DELETE FROM services WHERE id=?1", [id])?;
    secrets.delete_all(id)?;
    if removed == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::error::ErrorKind;
    use crate::models::service::fixtures::sample;
    use crate::models::service::*;
    use crate::secrets::{InMemorySecretStore, SecretStore};

    fn env(key: &str, value: Option<&str>, secret: bool) -> EnvVar {
        EnvVar {
            key: key.into(),
            value: value.map(Into::into),
            is_secret: secret,
        }
    }

    #[test]
    fn create_then_get_round_trips_every_field() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.name = "Ünïcode サービス".into();
        s.working_dir = "C:/Users/Jörg Müller/my project".into();
        s.executable = "C:/Program Files/My App/app.exe".into();
        s.args = vec!["--name".into(), "with space".into(), "\"quoted\"".into()];
        s.env = vec![env("PLAIN", Some("1"), false)];
        s.restart = RestartPolicy::OnFailure {
            max_retries: 3,
            backoff_secs: 2,
        };
        s.health = Some(HealthCheck::Http {
            url: "http://localhost:8000/health".into(),
            min_status: 200,
            max_status: 399,
            interval_secs: 5,
            timeout_secs: 2,
        });
        s.project_id = Some("p1".into());
        s.run_through_shell = true;
        s.keep_running_on_quit = true;
        s.allow_multiple = true;
        s.enabled = false;
        let created = create(&db, &secrets, &s).unwrap();
        assert!(!created.id.is_empty());
        s.id = created.id.clone();
        assert_eq!(created, s);
        assert_eq!(get(&db, &created.id).unwrap(), s);
    }

    #[test]
    fn secret_values_go_to_the_secret_store_and_never_into_sqlite() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.env = vec![env("TOKEN", Some("hunter2"), true)];
        let created = create(&db, &secrets, &s).unwrap();
        let stored: Option<String> =
            db.0.lock()
                .unwrap()
                .query_row("SELECT value FROM service_env WHERE key='TOKEN'", [], |r| {
                    r.get(0)
                })
                .unwrap();
        assert_eq!(stored, None);
        assert_eq!(
            secrets.get(&created.id, "TOKEN").unwrap().as_deref(),
            Some("hunter2")
        );
        assert_eq!(get(&db, &created.id).unwrap().env[0].value, None);
        assert!(get(&db, &created.id).unwrap().env[0].is_secret);
    }

    #[test]
    fn list_is_ordered_by_name_ignoring_case() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        for n in ["beta", "Alpha", "gamma"] {
            let mut s = sample();
            s.name = n.into();
            create(&db, &secrets, &s).unwrap();
        }
        let names: Vec<_> = list(&db).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Alpha", "beta", "gamma"]);
    }

    #[test]
    fn two_services_may_share_a_name_but_not_an_id() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let a = create(&db, &secrets, &sample()).unwrap();
        let b = create(&db, &secrets, &sample()).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn an_invalid_spec_is_rejected_and_nothing_is_stored() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.executable = String::new();
        let err = create(&db, &secrets, &s).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Validation);
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn update_replaces_fields_and_env_and_keeps_an_unsent_secret() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.env = vec![
            env("TOKEN", Some("old"), true),
            env("GONE", Some("x"), false),
        ];
        let mut created = create(&db, &secrets, &s).unwrap();
        created.name = "Renamed".into();
        created.env = vec![env("TOKEN", None, true), env("NEW", Some("y"), false)];
        let updated = update(&db, &secrets, &created).unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.env.len(), 2);
        assert_eq!(
            secrets.get(&created.id, "TOKEN").unwrap().as_deref(),
            Some("old")
        );
    }

    #[test]
    fn update_of_a_missing_service_is_not_found() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.id = "nope".into();
        assert_eq!(
            update(&db, &secrets, &s).unwrap_err().kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn delete_removes_the_row_and_its_secrets() {
        let db = open_in_memory().unwrap();
        let secrets = InMemorySecretStore::default();
        let mut s = sample();
        s.env = vec![env("TOKEN", Some("v"), true)];
        let created = create(&db, &secrets, &s).unwrap();
        delete(&db, &secrets, &created.id).unwrap();
        assert_eq!(get(&db, &created.id).unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(secrets.get(&created.id, "TOKEN").unwrap(), None);
    }
}
