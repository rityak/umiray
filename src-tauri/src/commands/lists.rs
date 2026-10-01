//! Команды про rule sets и geo-базы ядра (D-157, D-158).

use tauri::State;

use crate::app::lists::Offer;
use crate::app::state::AppState;
use crate::core::mihomo::{GeoFile, Mihomo};
use crate::error::Result;
use crate::lists::store::RuleList;

/// Всё скачанное: окно сводит это со строками маршрута.
#[tauri::command]
pub fn lists_list(state: State<AppState>) -> Vec<RuleList> {
    state.lists.list()
}

#[tauri::command]
pub fn lists_catalog(state: State<AppState>) -> Result<Vec<Offer>> {
    state.lists.catalog()
}

/// Скачать список для строки маршрута: из каталога или по адресу из документа.
#[tauri::command]
pub async fn lists_fetch(
    id: String,
    url: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RuleList> {
    state.lists.fetch(&app, &state, &id, url.as_deref()).await
}

/// Свой список по адресу. Пустое имя — имя файла из адреса.
#[tauri::command]
pub async fn lists_add_url(
    url: String,
    title: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RuleList> {
    state.lists.add_url(&app, &state, &url, &title).await
}

/// Обновить один список или, без `id`, все.
#[tauri::command]
pub async fn lists_refresh(
    id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    state.lists.refresh(&app, &state, id.as_deref()).await
}

/// Скачать всё, на что ссылаются наборы и чего ещё нет, — после записи кода.
#[tauri::command]
pub async fn lists_ensure(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.lists.ensure(&app, &state).await
}

#[tauri::command]
pub fn geo_files() -> Vec<GeoFile> {
    Mihomo::geo_files()
}

/// Обновить geo-базы руками работающего ядра.
#[tauri::command]
pub async fn geo_update(state: State<'_, AppState>) -> Result<Vec<GeoFile>> {
    state.mihomo.update_geo().await
}
