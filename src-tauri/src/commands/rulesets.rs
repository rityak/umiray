//! Команды про готовые наборы правил (D-083): сами файлы. Какие из них в маршруте,
//! решает раздел `ready` документа набора (D-158) — его пишет `config_write`.

use tauri::State;

use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::rulesets::Ruleset;
use crate::config::rulesets::RulesetStore;
use crate::error::Result;

/// Все наборы папки с их выходом по умолчанию.
#[tauri::command]
pub fn rulesets_list() -> Vec<Ruleset> {
    RulesetStore::list()
}

/// Текст набора для редактора в окне (D-104).
#[tauri::command]
pub fn rulesets_read(id: String) -> Result<String> {
    RulesetStore::read(&id)
}

/// Завести свой набор. Ядру ничего не доезжает: в маршрут новый набор не входит, пока
/// его не выберут, — поэтому здесь идентификатор, а не статус.
#[tauri::command]
pub fn rulesets_create(title: String) -> Result<String> {
    RulesetStore::create(&title)
}

/// Удалить набор — и довести это до живого ядра: выбранный в маршруте уносит свои
/// правила из сборки.
#[tauri::command]
pub async fn rulesets_delete(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || RulesetStore::delete(&id))
        .await
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
    state
        .connection
        .change(&app, &state, || RulesetStore::write(&id, &text))
        .await
}
