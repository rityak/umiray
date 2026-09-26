//! Команды про ядро: запуск, остановка, лог, трафик, установка.

use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::app::status::{look, status, Status};
use crate::app::tray;
use crate::core::controller::Traffic;
use crate::core::download;
use crate::error::AppError;
use crate::error::Result;

#[tauri::command]
pub fn core_status(app: tauri::AppHandle, state: State<AppState>) -> Status {
    let current = status(&state);
    // Значок догоняет здесь же: ядро может упасть само, и об этом никто больше не скажет.
    tray::refresh(&app, look(&current));
    current
}

#[tauri::command]
pub fn core_logs(state: State<AppState>) -> Vec<String> {
    state.supervisor.logs()
}

#[tauri::command]
pub async fn core_start(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    connect::start(&app, &state).await
}

/// Отказа не отдаёт: фаза `stop` не отменяется ничем (D-101). `Result` здесь — требование
/// границы, а не признак того, что остановка может не выйти.
#[tauri::command]
pub async fn core_stop(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    Ok(connect::stop(&app, &state).await)
}

/// Перезапустить ядро — чтобы доехало то, что оно читает на старте (D-010): сменённый
/// режим (D-060) или правка конфига.
#[tauri::command]
pub async fn core_restart(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    connect::restart(&app, &state).await
}

/// Забыть карту подменных адресов (S-021).
///
/// Только по нажатию: сброс раздаёт пул заново, то есть делает ровно то, от чего
/// `store-fake-ip` бережёт. Осознанное действие — можно, автоматическое — нет.
#[tauri::command]
pub async fn core_flush_fake_ip(state: State<'_, AppState>) -> Result<()> {
    state.supervisor.flush_fake_ip().await
}

/// Сколько прошло трафика. Пусто — ядро не запущено, и это не ошибка.
#[tauri::command]
pub async fn core_traffic(state: State<'_, AppState>) -> Result<Option<Traffic>> {
    state.supervisor.traffic().await
}

/// Скачать ядро. Пока оно запущено, файл занят — сначала отключаемся.
#[tauri::command]
pub async fn core_install(state: State<'_, AppState>) -> Result<String> {
    let _transition = state.transition().await;
    if state.supervisor.status().running {
        return Err(AppError::invalid(
            "Сначала отключитесь: работающее ядро нельзя заменить",
        ));
    }
    let version = download::install().await?;
    Ok(format!("Ядро установлено: {version}"))
}
