use tauri::{AppHandle, State};

use crate::app::state::AppState;
use crate::config::volt::Options;
use crate::core::volt::Snapshot;
use crate::error::Result;

#[tauri::command]
pub fn volt_get(state: State<'_, AppState>) -> Result<Snapshot> {
    state.volt.snapshot()
}

#[tauri::command]
pub async fn volt_set(
    app: AppHandle,
    state: State<'_, AppState>,
    options: Options,
) -> Result<Snapshot> {
    crate::app::volt::update(&app, &state, options).await
}

#[tauri::command]
pub fn volt_strategy_parse(yaml: String) -> Result<serde_json::Value> {
    crate::config::volt::strategy_parse(&yaml)
}

#[tauri::command]
pub fn volt_strategy_render(strategy: serde_json::Value) -> Result<String> {
    crate::config::volt::strategy_render(strategy)
}

#[tauri::command]
pub fn volt_strategy_preset(yaml: String, id: String) -> Result<String> {
    crate::core::volt_tune::selected_yaml(&yaml, &id)
}

#[tauri::command]
pub async fn volt_tune(state: State<'_, AppState>) -> Result<crate::core::volt_tune::TuneReport> {
    let _transition = state.connection.lock().await;
    state.volt.tune_now().await
}

#[tauri::command]
pub async fn volt_check_site(
    state: State<'_, AppState>,
    url: String,
) -> Result<crate::core::volt_tune::SiteCheck> {
    state.volt.check_site(&url).await
}

#[tauri::command]
pub async fn volt_dictionary_pick() -> Result<Option<String>> {
    tokio::task::spawn_blocking(|| {
        crate::system::pick::FileDialog::file("Noise dictionary", &[("Text files", "*.txt")])
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| crate::error::AppError::io(format!("VOLT file dialog: {e}")))
}
