//! Команды раздела «Маршрутизация» (D-074). Диска здесь нет по той же причине,
//! что и в `commands::groups`: форма правит черновик, а пишет его `config_write`.

use crate::config::rules::Routing;
use crate::config::rules::RulesCodec;
use crate::error::Result;

#[tauri::command]
pub fn rules_parse(text: String) -> Result<Routing> {
    RulesCodec::parse(&text)
}

#[tauri::command]
pub fn rules_render(text: String, routing: Routing) -> Result<String> {
    RulesCodec::render(&text, &routing)
}

#[tauri::command]
pub fn rules_processes() -> Result<Vec<crate::system::process::Process>> {
    crate::system::process::ProcessTable::running()
}
