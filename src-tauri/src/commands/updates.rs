use tauri::{ipc::Channel, AppHandle};

use crate::app::updates::Updates;
use crate::app::updates::{Info, Progress};
use crate::error::Result;

#[tauri::command]
pub async fn updates_check(app: AppHandle) -> Result<Info> {
    Updates::check(&app).await
}

#[tauri::command]
pub async fn updates_install(app: AppHandle, progress: Channel<Progress>) -> Result<()> {
    Updates::install(&app, progress).await
}
