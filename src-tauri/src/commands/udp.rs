//! Команда про UDP через свои узлы (D-113).
//!
//! Домен свой, а не `client`: там настройки окна и замеров, а здесь — то, что меняет
//! собранный конфиг и потому доезжает до ядра.

use tauri::State;

use crate::app::routing::Udp;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::error::Result;

#[tauri::command]
pub fn udp_get(state: State<AppState>) -> Udp {
    state.routing.udp()
}

/// Переключить — и довести до живого ядра: группа и правило появляются в собранном
/// конфиге, а его ядро читает на старте (D-064).
#[tauri::command]
pub async fn udp_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state
        .connection
        .change(&app, &state, || state.groups.set_udp_rule(on))
        .await
}
