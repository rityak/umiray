//! Команды про источники узлов: подписки и ручные ссылки (D-032). Сценарии — у сервиса
//! `app/sources.rs`; здесь только граница.

use tauri::State;

use crate::app::sources::Import;
use crate::app::state::AppState;
use crate::error::{AppError, Result};
use crate::nodes::sources::Source;
use crate::nodes::sources::SourceStore;

#[tauri::command]
pub fn sources_list() -> Vec<Source> {
    SourceStore::list()
}

/// Одно поле на всё: ссылка на сервер дописывается в «мои ссылки», остальное считается
/// подпиской и заводит отдельный источник (D-032).
#[tauri::command]
pub async fn sources_add(
    input: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    state.sources.add(&app, &state, &input).await
}

#[tauri::command]
pub async fn sources_add_proxy(
    entry: serde_yaml::Mapping,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    state.sources.add_proxy(&app, &state, entry).await
}

/// Показать запись узла кодом до того, как её добавили.
#[tauri::command]
pub fn sources_proxy_yaml(entry: serde_yaml::Mapping) -> Result<String> {
    // Порядок ключей тот же, что у записанного узла: объект приезжает из окна через JSON
    // и по дороге теряет его — иначе код открывался бы алфавитным списком.
    serde_yaml::to_string(&serde_yaml::Value::Mapping(SourceStore::ordered(entry)))
        .map_err(|e| AppError::invalid(e.to_string()))
}

#[tauri::command]
pub async fn sources_add_proxy_text(
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    state.sources.add_proxy_text(&app, &state, &text).await
}

#[tauri::command]
pub async fn sources_add_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Import>> {
    state.sources.add_file(&app, &state).await
}

#[tauri::command]
pub async fn sources_refresh_all(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(state.sources.refresh_all(&state).await)
}

#[tauri::command]
pub async fn sources_refresh(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    state.sources.refresh(&app, &state, &id).await
}

#[tauri::command]
pub fn sources_read(id: String) -> String {
    SourceStore::raw(&id)
}

#[tauri::command]
pub async fn sources_write(
    id: String,
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    state.sources.write(&app, &state, &id, &text).await
}

/// Удалить источник. Отдаёт предупреждение, если на него ссылается написанное человеком.
#[tauri::command]
pub async fn sources_delete(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    state.sources.delete(&app, &state, &id).await
}
