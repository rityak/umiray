//! Команда про UDP через свои узлы (D-113).
//!
//! Домен свой, а не `client`: там настройки окна и замеров, а здесь — то, что меняет
//! собранный конфиг и потому доезжает до ядра.

use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::error::Result;

/// Состояние тумблера и то, есть ли вообще из чего собирать группу.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Udp {
    pub on: bool,
    /// Сколько узлов с нативным UDP нашлось. Ноль — включать нечего, и окно гасит галку:
    /// пустую группу ядро не примет и не стартует вовсе.
    pub nodes: usize,
}

#[tauri::command]
pub fn udp_get() -> Udp {
    Udp {
        on: crate::config::udp::on(),
        nodes: crate::render::effective::udp_nodes(),
    }
}

/// Переключить — и довести до живого ядра: группа и правило появляются в собранном
/// конфиге, а его ядро читает на старте (D-064).
#[tauri::command]
pub async fn udp_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    crate::config::udp::write(on)?;
    connect::apply(&app, &state).await
}
