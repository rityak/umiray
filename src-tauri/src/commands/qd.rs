//! Команды того, что есть только у qd (`app/qd.rs`, `QdPanel`). Питание, лог и установка
//! qd — общие `core_*` (D-154).

use serde_json::Value;
use tauri::State;

use crate::app::qd::{Adopted, QdStatus};
use crate::app::state::AppState;
use crate::error::Result;

#[tauri::command]
pub async fn qd_status(state: State<'_, AppState>) -> Result<QdStatus> {
    Ok(state.qd_panel.status(&state).await)
}

/// Прокси к API qd: окно ходит в него только так — токен остаётся в Rust (QD.md).
#[tauri::command]
pub async fn qd_call(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value> {
    state.qd.call(&method, &path, body).await
}

/// Ссылка `qd://` из поля добавления (D-161): qd скачивается, если его нет.
#[tauri::command]
pub async fn qd_adopt(state: State<'_, AppState>, link: String) -> Result<Adopted> {
    state.qd_panel.adopt(&state, &link).await
}

/// Тумблер «qd» выключили — удалить бинарь (D-161).
#[tauri::command]
pub async fn qd_remove(state: State<'_, AppState>) -> Result<()> {
    state.qd_panel.remove(&state).await
}

#[tauri::command]
pub async fn qd_rules_export(state: State<'_, AppState>) -> Result<Option<String>> {
    state.qd_panel.export_rules(&state).await
}

#[tauri::command]
pub async fn qd_rules_import(state: State<'_, AppState>) -> Result<Option<Value>> {
    state.qd_panel.import_rules(&state).await
}
