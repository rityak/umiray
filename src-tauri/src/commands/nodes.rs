//! Команды про узлы.
//!
//! Домен `nodes`, а не `core` (D-030): на остановленном ядре узлы читаются с диска,
//! и «core» в имени врал бы про половину случаев.

use tauri::State;

use crate::app::node_check::NodeCheck;
use crate::app::state::AppState;
use crate::error::Result;
use crate::nodes::source_editor;
use crate::nodes::source_editor::SourceEditor;
use crate::nodes::Node;

/// Код узла — запись, которую читает ядро, и она же объектом для формы (D-121, D-122).
///
/// Отдельной командой, а не полем у каждого узла: он нужен одному открытому узлу,
/// а список опрашивается каждую секунду.
#[tauri::command]
pub fn nodes_code(source: String, node: String) -> Result<source_editor::Code> {
    SourceEditor::node_code(&source, &node)
}

/// Переписать запись узла целиком. Храним разницу от собранного, а не текст (D-119).
#[tauri::command]
pub async fn nodes_code_set(
    source: String,
    node: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<()> {
    state
        .sources
        .edit(&state, &source, || {
            NodeCheck::text(&text)?;
            SourceEditor::edit_node_code(&source, &node, &text)
        })
        .await
}

/// Записать узел, собранный формой (D-121). Объектом, а не текстом: YAML разбирает
/// бэкенд, окно его не знает.
#[tauri::command]
pub async fn nodes_entry_set(
    source: String,
    node: String,
    entry: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<()> {
    state
        .sources
        .edit(&state, &source, || {
            NodeCheck::object(&entry)?;
            SourceEditor::set_node_entry(&source, &node, entry)
        })
        .await
}

/// Убрать узел, который клиент написал сам (D-121).
#[tauri::command]
pub async fn nodes_delete(source: String, node: String, state: State<'_, AppState>) -> Result<()> {
    state
        .sources
        .edit(&state, &source, || {
            SourceEditor::delete_node(&source, &node)
        })
        .await
}

/// Вернуть узел к тому, что прислала панель.
#[tauri::command]
pub async fn nodes_reset(source: String, node: String, state: State<'_, AppState>) -> Result<()> {
    state
        .sources
        .edit(&state, &source, || SourceEditor::reset_node(&source, &node))
        .await
}

/// Все узлы всех источников. Состав приходит с диска, поэтому список одинаков при живом
/// и остановленном ядре (D-061).
#[tauri::command]
pub fn nodes_list(state: State<AppState>) -> Vec<Node> {
    state.catalog.nodes()
}

/// Померить, сколько до каждого сервера (D-062). Числа остаются в состоянии приложения —
/// следующий опрос таблицы покажет их сам.
#[tauri::command]
pub async fn nodes_ping(state: State<'_, AppState>) -> Result<()> {
    state.catalog.measure(&state).await
}
