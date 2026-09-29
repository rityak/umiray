//! Команды про наборы правил (D-056).

use serde::Serialize;
use tauri::State;

use crate::app::state::AppState;
use crate::config::presets;
use crate::config::presets::PresetStore;
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
        presets: PresetStore::list(),
        active: state.routing.applied_preset(&state),
    }
}

/// Завести набор — копию того, что клиент собирает из ваших источников (D-071).
/// Применённым он не становится: применение рвёт связь, и решает его человек.
#[tauri::command]
pub fn presets_create(state: State<AppState>) -> Result<presets::Preset> {
    state.presets.create()
}

/// Применить набор. Его документы ядро читает на старте (D-010), поэтому работающее
/// перезапускается — как и при смене направления (D-064).
#[tauri::command]
pub async fn presets_select(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    state.presets.select(&app, &state, &id).await
}

#[tauri::command]
pub fn presets_rename(id: String, name: String) -> Result<presets::Preset> {
    PresetStore::rename(&id, &name)
}

#[tauri::command]
pub fn presets_delete(id: String, state: State<AppState>) -> Result<()> {
    state.presets.delete(&state, &id)
}
