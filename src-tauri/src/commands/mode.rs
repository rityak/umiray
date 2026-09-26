//! Команда про режим перехвата (D-060).

use tauri::State;

use crate::app::mode;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::error::Result;

/// Выбрать режим — и довести его до живого ядра: перезапуском, если сменился TUN,
/// иначе реестром. Старые соединения рвутся в обоих случаях (D-143).
#[tauri::command]
pub async fn mode_set(
    mode: mode::Choice,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    crate::app::connect::set_mode(&app, &state, mode).await
}
