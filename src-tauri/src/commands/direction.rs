//! Команды про направление трафика (D-056).

use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::direction;
use crate::error::Result;

/// Сменить направление (D-056). Нажатие по строке таблицы — это `manual` вместе с узлом,
/// поэтому команда одна, а не две.
///
/// Работающее ядро при этом перезапускается (D-064), поэтому команда отдаёт статус:
/// окно обязано показать результат сразу, а не через опрос.
#[tauri::command]
pub async fn direction_set(
    direction: direction::Direction,
    node: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    connect::set_direction(&app, &state, direction, node).await
}
