//! Signed automatic updates (FR-BEA-03, NFR-PLT-04).
//!
//! The update server URL comes from `tauri.conf.json` (set at build time from
//! the UPDATER_ENDPOINT repository variable) and can be overridden at run time
//! in Beállítások → Névjegy és frissítés (setting `update.endpoint`).
//! Packages are verified against the public key in `tauri.conf.json`.

use std::sync::Mutex;

use lunda_core::auth::Session;
use lunda_core::{backup, settings, CoreError};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;

#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

fn err(e: impl std::fmt::Display) -> CoreError {
    CoreError::new("update", e.to_string())
}

#[tauri::command]
pub async fn update_check(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<Option<UpdateInfo>, CoreError> {
    let endpoint = {
        let state = app.state::<AppState>();
        let a = state.0.lock().unwrap();
        a.store.as_ref().and_then(|s| settings::get(&s.conn, "update.endpoint", None).ok().flatten()).filter(|e| !e.trim().is_empty())
    };
    let mut builder = app.updater_builder();
    if let Some(e) = endpoint {
        let url = Url::parse(e.trim()).map_err(err)?;
        builder = builder.endpoints(vec![url]).map_err(err)?;
    }
    let update = builder.build().map_err(err)?.check().await.map_err(err)?;
    let info = update.as_ref().map(|u| UpdateInfo { version: u.version.clone(), current_version: u.current_version.clone(), notes: u.body.clone() });
    *pending.0.lock().unwrap() = update;
    Ok(info)
}

/// Takes a backup first (FR-BEA-03), then downloads, verifies and installs; restarts the app.
#[tauri::command]
pub async fn update_install(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<(), CoreError> {
    let update = pending.0.lock().unwrap().take().ok_or_else(|| CoreError::new("update", "no pending update"))?;
    {
        let state = app.state::<AppState>();
        let a = state.0.lock().unwrap();
        if let Some(store) = &a.store {
            backup::create(&store.conn, &Session::system(), &a.data_dir, "pre-update")?;
        }
    }
    update.download_and_install(|_, _| {}, || {}).await.map_err(err)?;
    app.restart();
}
