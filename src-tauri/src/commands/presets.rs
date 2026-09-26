//! Команды про наборы правил (D-056).

use serde::Serialize;
use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::config::presets;
use crate::error::Result;

/// Наборы вместе с тем, какой из них сейчас применён: список без этого ответа
/// не отвечает на первый же вопрос пользователя — «а какой работает?».
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetList {
    presets: Vec<presets::Preset>,
    active: Option<String>,
}

#[tauri::command]
pub fn presets_list(state: State<AppState>) -> PresetList {
    PresetList {
        presets: presets::list(),
        active: state.applied_preset(),
    }
}

/// Завести набор — копию того, что клиент собирает из ваших источников (D-071).
/// Применённым он не становится: применение рвёт связь, и решает его человек.
#[tauri::command]
pub fn presets_create(state: State<AppState>) -> Result<presets::Preset> {
    state.new_preset()
}

/// Применить набор. Его документы ядро читает на старте (D-010), поэтому работающее
/// перезапускается — как и при смене направления (D-064).
#[tauri::command]
pub async fn presets_select(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    state.select_preset(&id)?;
    if state.supervisor.status().running {
        connect::restart(&app, &state).await?;
    }
    Ok(())
}

#[tauri::command]
pub fn presets_rename(id: String, name: String) -> Result<presets::Preset> {
    presets::rename(&id, &name)
}

#[tauri::command]
pub fn presets_delete(id: String, state: State<AppState>) -> Result<()> {
    state.delete_preset(&id)
}
