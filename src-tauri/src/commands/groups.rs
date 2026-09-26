//! Команды раздела «Группы»: форма и код правят один документ (D-074).
//!
//! Диска здесь нет намеренно. Форма работает с тем же черновиком, что открыт в коде,
//! а записывает его `config_write` — та же кнопка «Сохранить» и тот же Ctrl+S.

use crate::config::groups::{self, Group};
use crate::error::Result;

/// Разобрать документ в группы. Текст приходит из окна — это его собственный черновик.
#[tauri::command]
pub fn groups_parse(text: String) -> Result<Vec<Group>> {
    groups::parse(&text)
}

/// Собрать документ заново. Всё, чего форма не знает, остаётся на месте.
#[tauri::command]
pub fn groups_render(text: String, groups: Vec<Group>) -> Result<String> {
    groups::render(&text, &groups)
}
