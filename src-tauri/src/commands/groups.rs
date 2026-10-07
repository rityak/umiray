//! Команды раздела «Группы»: форма и код правят один документ (D-074).
//!
//! Диска здесь нет намеренно. Форма работает с тем же черновиком, что открыт в коде,
//! а записывает его `config_write` — та же кнопка «Сохранить» и тот же Ctrl+S.

use tauri::State;

use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::auto::{Exclude, Grouping};
use crate::config::groups::Group;
use crate::config::groups::GroupsCodec;
use crate::error::Result;

/// Разобрать документ в группы. Текст приходит из окна — это его собственный черновик.
#[tauri::command]
pub fn groups_parse(text: String) -> Result<Vec<Group>> {
    GroupsCodec::parse(&text)
}

/// Собрать документ заново. Всё, чего форма не знает, остаётся на месте.
#[tauri::command]
pub fn groups_render(text: String, groups: Vec<Group>) -> Result<String> {
    GroupsCodec::render(&text, &groups)
}

/// Переименовать свою группу вместе с тем, что на неё смотрит (D-172), — и довести до ядра.
#[tauri::command]
pub async fn groups_rename(
    from: String,
    to: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.groups.rename(&state, &from, &to))
        .await
}

/// Какие группы клиент собирает сам (D-172): по стране и по протоколу.
#[tauri::command]
pub fn groups_auto_get(state: State<AppState>) -> Grouping {
    state.groups.grouping()
}

#[tauri::command]
pub async fn groups_auto_set(
    grouping: Grouping,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.groups.set_grouping(grouping))
        .await
}

/// Кого нет в `AUTO` (D-172).
#[tauri::command]
pub async fn groups_auto_exclude(
    exclude: Exclude,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.groups.set_exclude(&exclude))
        .await
}
