//! Команда раздела «Соединение»: всё, что он опрашивает, одним ответом (D-145).
//!
//! Раньше это были четыре команды на каждый такт опроса — список узлов, выход,
//! направление и способ замера. Спрашивают их всегда вместе, и четыре перехода через
//! границу вместо одного были чистой тратой.

use serde::Serialize;
use tauri::State;

use crate::app::state::AppState;
use crate::config::direction::Direction;
use crate::error::Result;
use crate::nodes::ping::Method;
use crate::nodes::Node;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Все узлы всех источников — с диска, при живом и остановленном ядре (D-061).
    nodes: Vec<Node>,
    /// Куда идёт трафик — с поправкой на исчезнувший выбранный узел (D-056).
    direction: Direction,
    /// Чем мерить задержку (D-069): колонка таблицы называет его.
    ping: Method,
    /// Выход от выбранного до узла: `["AUTO", "Poland 1"]`. Пусто — выхода нет.
    route: Vec<String>,
}

#[tauri::command]
pub async fn connection_snapshot(state: State<'_, AppState>) -> Result<Snapshot> {
    Ok(Snapshot {
        nodes: state.nodes(),
        direction: state.direction(),
        ping: crate::app::client::ping()?,
        route: state.route().await,
    })
}
