//! Команды про то, куда идёт трафик: выход (D-056) и тумблер маршрутизации (D-166).

use tauri::State;

use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::direction;
use crate::error::Result;

/// Сменить выход (D-056). Нажатие по узлу — это `manual` вместе с узлом, по `DIRECT`
/// и `AUTO` — направление без узла, поэтому команда одна, а не три.
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
    state
        .connection
        .change(&app, &state, || {
            state.routing.set_direction(&state, direction, node)
        })
        .await
}

/// Выбрать выход и вернуть ему `MATCH` применённого набора (D-166): нажатие в «Соединении»,
/// когда `MATCH` смотрит мимо выбора, после «Заменить» человека. Одна правка — одна
/// перезагрузка ядра, а не две подряд.
#[tauri::command]
pub async fn direction_take_match(
    direction: direction::Direction,
    node: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || {
            state.routing.take_match(&state, direction, node)
        })
        .await
}

/// Блокировать ли рекламу (D-169): готовый набор в применённом наборе маршрута. Статус —
/// по той же причине, что и у маршрутизации: сборка доезжает до работающего ядра сразу.
#[tauri::command]
pub async fn routing_ads_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.routing.set_ads(&state, on))
        .await
}

/// Включить или выключить маршрутизацию (D-166). Отдаёт статус по той же причине:
/// сборка меняется целиком и доезжает до работающего ядра сразу.
#[tauri::command]
pub async fn routing_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.routing.set_routing(&state, on))
        .await
}
