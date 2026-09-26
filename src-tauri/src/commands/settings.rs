//! Команды про настройки клиента (D-037).

use tauri::State;

use crate::app::settings::{self, Settings};
use crate::app::state::AppState;
use crate::error::Result;

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> Settings {
    state.settings()
}

/// Одна команда на все настройки: приходит только то, что меняется (D-037).
#[tauri::command]
pub fn settings_update(patch: settings::Patch, state: State<AppState>) -> Result<Settings> {
    state.patch(patch)?;
    Ok(state.settings())
}
