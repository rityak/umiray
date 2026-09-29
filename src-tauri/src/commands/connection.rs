//! Команда раздела «Соединение»: всё, что он опрашивает, одним ответом (D-145).

use tauri::State;

use crate::app::routing::Snapshot;
use crate::app::state::AppState;
use crate::error::Result;

#[tauri::command]
pub async fn connection_snapshot(state: State<'_, AppState>) -> Result<Snapshot> {
    state.routing.snapshot(&state).await
}
