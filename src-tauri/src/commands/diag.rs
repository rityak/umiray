//! Диагностика: список утилит и запуск одной (D-030, D-097).

use tauri::State;

use crate::app::state::AppState;
use crate::collections;
use crate::diag::{self, Args, Report, Tool};
use crate::error::Result;

#[tauri::command]
pub fn diag_tools() -> Vec<Tool> {
    diag::tools()
}

/// Запустить утилиту.
#[tauri::command]
pub async fn diag_run(
    id: String,
    args: Option<Args>,
    state: State<'_, AppState>,
) -> Result<Report> {
    diag::run(&id, filled(args, &state)?).await
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
    let report = diag::smart::apply(&id, filled(args, &state)?).await?;
    crate::app::connect::apply(&app, &state).await?;
    Ok(report)
}

/// Чего окно не знает и знать не должно: какой набор применён и на каком порту ядро.
///
/// Маршрутизация подставляется здесь, а не приходит из окна: какой набор применён —
/// дело состояния приложения, и спрашивать об этом вебвью значило бы завести второй
/// источник истины (D-071). Порт работающего ядра — оттуда же: окно знает его только
/// как число в статусе, а пробам он нужен как адрес прокси.
fn filled(args: Option<Args>, state: &State<'_, AppState>) -> Result<Args> {
    let mut args = args.unwrap_or_default();
    args.rules = state.routing()?;
    let status = state.supervisor.status();
    args.proxy = status.port;
    args.mode = status.mode.map(|mode| format!("{mode:?}").to_lowercase());
    Ok(args)
}

/// Коллекция резолверов — та же, из которой берёт кандидатов `dns-race`.
/// Нужен окну для ручного выбора DNS.
#[tauri::command]
pub fn diag_providers() -> Result<collections::Resolvers> {
    collections::dns()
}
