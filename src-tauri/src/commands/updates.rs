use tauri::{ipc::Channel, AppHandle};

use crate::app::updates::{self, Info, Progress};
use crate::error::Result;

#[tauri::command]
pub async fn updates_check(app: AppHandle) -> Result<Info> {
    updates::check(&app).await
}

#[tauri::command]
pub async fn updates_install(app: AppHandle, progress: Channel<Progress>) -> Result<()> {
    updates::install(&app, progress).await
}
