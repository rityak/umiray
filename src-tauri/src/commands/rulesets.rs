//! Команды про встроенные наборы правил (D-083).

use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::rulesets::{self, Ruleset};
use crate::error::Result;

/// Все наборы папки и то, какие из них включены.
#[tauri::command]
pub fn rulesets_list() -> Vec<Ruleset> {
    rulesets::list()
}

/// Текст набора для редактора в окне (D-104).
#[tauri::command]
pub fn rulesets_read(id: String) -> Result<String> {
    rulesets::read(&id)
}

/// Завести свой набор. Ядру ничего не доезжает: новый набор выключен, и в сборку
/// он не входит — поэтому здесь идентификатор, а не статус.
#[tauri::command]
pub fn rulesets_create(title: String) -> Result<String> {
    rulesets::create(&title)
}

/// Удалить набор — и довести это до живого ядра: удалённый включённый набор уносит
/// свои правила из сборки.
#[tauri::command]
pub async fn rulesets_delete(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    rulesets::delete(&id)?;
    connect::apply(&app, &state).await
}

/// Записать правку набора — и довести её до живого ядра (D-102).
///
/// В отличие от документов набора маршрутизации (D-071), «Сохранить» здесь **применяет**:
/// у встроенного набора нет отдельного «применить», его роль играет тумблер. Выключенный
/// набор в сборку не входит, и правка просто ляжет на диск.
#[tauri::command]
pub async fn rulesets_write(
    id: String,
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    rulesets::write(&id, &text)?;
    connect::apply(&app, &state).await
}

/// Включить или выключить набор — и довести до живого ядра (D-102, D-143), поэтому
/// команда отдаёт статус.
#[tauri::command]
pub async fn rulesets_set(
    id: String,
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    connect::set_ruleset(&app, &state, &id, on).await
}
