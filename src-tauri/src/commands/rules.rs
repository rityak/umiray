//! Команды раздела «Маршрутизация» (D-074). Диска здесь нет по той же причине,
//! что и в `commands::groups`: форма правит черновик, а пишет его `config_write`.

use crate::config::rules::{self, Routing};
use crate::error::Result;

#[tauri::command]
pub fn rules_parse(text: String) -> Result<Routing> {
    rules::parse(&text)
}

#[tauri::command]
pub fn rules_render(text: String, routing: Routing) -> Result<String> {
    rules::render(&text, &routing)
}
