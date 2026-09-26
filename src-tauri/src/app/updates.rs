//! Signed client updates from GitHub Releases (D-149). The webview never supplies a URL.

use std::time::Duration;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::app::{connect, state::AppState};
use crate::error::{AppError, Result};

#[derive(Default)]
pub struct Updates(pub tokio::sync::Mutex<Option<Update>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub enabled: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

pub async fn check(app: &AppHandle) -> Result<Info> {
    let config = app.config().plugins.0.get("updater");
    let enabled = config
        .and_then(|config| config.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty());
    if !enabled {
        return Ok(Info {
            enabled,
            version: None,
            notes: None,
        });
    }
    let updates = app.state::<Updates>();
    let mut pending = updates.0.lock().await;
    let mut update = app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| AppError::network(error.to_string()))?
        .check()
        .await
        .map_err(|error| AppError::network(error.to_string()))?;
    if let Some(update) = &mut update {
        update.timeout = Some(Duration::from_secs(600));
    }
    let info = Info {
        enabled,
        version: update.as_ref().map(|update| update.version.clone()),
        notes: update.as_ref().and_then(|update| update.body.clone()),
    };
    *pending = update;
    Ok(info)
}

pub async fn install(app: &AppHandle, progress: Channel<Progress>) -> Result<()> {
    let updates = app.state::<Updates>();
    // One check or installation at a time. A check cannot replace the verified update.
    let pending = updates.0.lock().await;
    let update = pending
        .as_ref()
        .ok_or_else(|| AppError::invalid("Check for updates first."))?;
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(Progress {
                    phase: "download",
                    downloaded,
                    total,
                });
            },
            || {},
        )
        .await
        .map_err(|error| AppError::network(error.to_string()))?;
    // download() verifies the signature. Failed downloads leave the VPN untouched.
    let state = app.state::<AppState>();
    let _transition = connect::stop_for_update(app, &state).await?;
    let _ = progress.send(Progress {
        phase: "install",
        downloaded,
        total: Some(downloaded),
    });
    update
        .install(bytes)
        .map_err(|error| AppError::io(error.to_string()))
}
