//! Диагностика: список утилит и запуск одной (D-030, D-097).

use tauri::State;

use crate::app::state::AppState;
use crate::collections;
use crate::collections::Collections;
use crate::diag::Toolbox;
use crate::diag::{Args, Report, Tool};
use crate::error::Result;

#[tauri::command]
pub fn diag_tools() -> Vec<Tool> {
    Toolbox::tools()
}

/// Запустить утилиту.
#[tauri::command]
pub async fn diag_run(
    id: String,
    args: Option<Args>,
    state: State<'_, AppState>,
) -> Result<Report> {
    state.diagnostics.run(&state, &id, args).await
}

/// Сделать то, что утилита предлагает: прописать отмеченное в документ пользователя
/// и довести до живого ядра (D-105).
///
/// Отдельная команда, а не флаг у `diag_run`: запуск утилиты ничего не меняет, а это —
/// запись в конфиг, и путать их нельзя ни в коде, ни в окне.
#[tauri::command]
pub async fn diag_apply(
    id: String,
    args: Option<Args>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Report> {
    state.diagnostics.apply(&app, &state, &id, args).await
}

/// Коллекция резолверов — та же, из которой берёт кандидатов `dns-race`.
/// Нужен окну для ручного выбора DNS.
#[tauri::command]
pub fn diag_providers() -> Result<collections::Resolvers> {
    Collections::dns()
}
