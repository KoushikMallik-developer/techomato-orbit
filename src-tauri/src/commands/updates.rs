//! Updates. Orbit asks a signed manifest whether a newer version exists and installs one only
//! when the user says so. The updater verifies the download against the public key in
//! `tauri.conf.json` and refuses an unsigned or mismatched file. Only the version, platform and
//! architecture are sent (they are part of the request the updater makes).

use crate::commands::confirm_action;
use crate::engine::supervisor::Supervisor;
use crate::error::{AppError, AppResult, ErrorKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

fn os_err(what: &str, e: impl ToString) -> AppError {
    AppError::new(ErrorKind::Os, what).with_detail(e.to_string())
}

/// Development builds may point the check at a local manifest with `ORBIT_UPDATE_URL`.
fn builder(app: &AppHandle) -> AppResult<tauri_plugin_updater::Updater> {
    let mut b = app.updater_builder();
    if cfg!(debug_assertions) {
        if let Ok(url) = std::env::var("ORBIT_UPDATE_URL") {
            let url: tauri::Url = url
                .parse()
                .map_err(|e| os_err("ORBIT_UPDATE_URL is not a valid URL.", e))?;
            b = b
                .endpoints(vec![url])
                .map_err(|e| os_err("Could not use ORBIT_UPDATE_URL.", e))?;
        }
    }
    b.build()
        .map_err(|e| os_err("Updates are not set up in this build.", e))
}

pub async fn find(app: &AppHandle) -> AppResult<Option<Update>> {
    builder(app)?.check().await.map_err(|e| {
        os_err(
            "Could not check for updates. Check your connection and try again.",
            e,
        )
    })
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> AppResult<Option<UpdateInfo>> {
    Ok(find(&app).await?.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
    }))
}

/// Downloads and verifies the update first, so a failed download never disturbs running
/// services; only then stops services (as Quit does) and runs the installer.
#[tauri::command]
pub async fn update_install(app: AppHandle, supervisor: State<'_, Supervisor>) -> AppResult<()> {
    let update = find(&app).await?.ok_or_else(|| {
        AppError::new(ErrorKind::NotFound, "You already have the latest version.")
    })?;
    confirm_action(
        &app,
        "Install update",
        &format!(
            "Install Orbit {}? Orbit will close, and services without keep-running will be stopped.",
            update.version
        ),
        "Install",
    )?;
    let handle = app.clone();
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = handle.emit("update://progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| {
            os_err(
                "The update could not be downloaded or did not pass verification.",
                e,
            )
        })?;
    let sup = supervisor.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sup.stop_all_on_quit())
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
    update
        .install(bytes)
        .map_err(|e| os_err("The update could not be installed. Services that were running were stopped; start them again from Services.", e))?;
    app.exit(0);
    Ok(())
}

/// At launch, when the user turned automatic checks on: tell them (never install).
pub fn announce_in_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(Some(u)) = find(&app).await else {
            return;
        };
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title("Orbit update available")
            .body(format!(
                "Version {} is ready. Open Settings to install it.",
                u.version
            ))
            .show();
        let _ = app.emit("update://available", u.version.clone());
    });
}

#[cfg(test)]
mod tests {
    use crate::models::prefs::Prefs;

    fn conf() -> serde_json::Value {
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap()
    }

    #[test]
    fn the_updater_is_configured_with_a_key_and_an_https_manifest() {
        let c = conf();
        let u = &c["plugins"]["updater"];
        let key = u["pubkey"].as_str().unwrap();
        assert!(key.len() > 60, "a real public key is required");
        let decoded = String::from_utf8(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, key).unwrap(),
        )
        .unwrap();
        assert!(decoded.starts_with("untrusted comment: minisign public key"));
        let endpoints = u["endpoints"].as_array().unwrap();
        assert!(!endpoints.is_empty());
        assert!(endpoints
            .iter()
            .all(|e| e.as_str().unwrap().starts_with("https://")));
        assert_eq!(c["bundle"]["createUpdaterArtifacts"], true);
    }

    #[test]
    fn automatic_checks_are_off_unless_chosen_and_old_prefs_still_load() {
        assert!(!Prefs::default().auto_update_check);
        let old: Prefs = serde_json::from_str(r#"{"startWithWindows":true}"#).unwrap();
        assert!(old.start_with_windows && !old.auto_update_check);
    }

    /// The updater's promise is "refuse anything not signed by our key". The plugin does the
    /// checking with this same library; this pins that the key in the config really is the one
    /// the release signature belongs to, and that a changed file is refused.
    #[test]
    fn the_shipped_key_accepts_a_signed_file_and_refuses_a_changed_one() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use minisign_verify::{PublicKey, Signature};
        let text = |b64: &str| String::from_utf8(STANDARD.decode(b64.trim()).unwrap()).unwrap();
        let key = PublicKey::decode(&text(
            conf()["plugins"]["updater"]["pubkey"].as_str().unwrap(),
        ))
        .unwrap();
        let sig = Signature::decode(&text(include_str!(
            "../../tests/fixtures/updater/sample.bin.sig"
        )))
        .unwrap();
        let file = include_bytes!("../../tests/fixtures/updater/sample.bin");
        assert!(key.verify(file, &sig, false).is_ok());
        let mut changed = file.to_vec();
        changed[0] ^= 1;
        assert!(key.verify(&changed, &sig, false).is_err());
    }
}
