//! Команды про ядро: запуск, остановка, лог, трафик, установка.
//!
//! Питание, лог и установка — для любого ядра (D-154): запуск поднимает выбранное в шапке,
//! остановка гасит работающее, лог и установка берут ядро аргументом. Трафик и fake-ip —
//! у mihomo.

use tauri::State;

use crate::app::state::AppState;
use crate::app::status::Status;
use crate::app::tray::Tray;
use crate::core::mihomo::controller::Traffic;
use crate::core::EngineId;
use crate::error::Result;

#[tauri::command]
pub fn core_status(app: tauri::AppHandle, state: State<AppState>) -> Status {
    let current = Status::gather(&state);
    // Значок догоняет здесь же: ядро может упасть само, и об этом никто больше не скажет.
    Tray::refresh(&app, current.look());
    current
}

#[tauri::command]
pub fn core_logs(engine: EngineId, state: State<AppState>) -> Vec<String> {
    state.engine(engine).log().lines()
}

#[tauri::command]
pub async fn core_start(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    state.connection.start(&app, &state).await
}

/// Отказа не отдаёт: фаза `stop` не отменяется ничем (D-101). `Result` здесь — требование
/// границы, а не признак того, что остановка может не выйти.
#[tauri::command]
pub async fn core_stop(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    Ok(state.connection.stop(&app, &state).await)
}

/// Перезапустить ядро — чтобы доехало то, что оно читает на старте (D-010): сменённый
/// режим (D-060) или правка конфига.
#[tauri::command]
pub async fn core_restart(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    state.connection.restart(&app, &state).await
}

/// Забыть карту подменных адресов (S-021).
///
/// Только по нажатию: сброс раздаёт пул заново, то есть делает ровно то, от чего
/// `store-fake-ip` бережёт. Осознанное действие — можно, автоматическое — нет.
#[tauri::command]
pub async fn core_flush_fake_ip(state: State<'_, AppState>) -> Result<()> {
    state.mihomo.flush_fake_ip().await
}

/// Сколько прошло трафика. Пусто — ядро не запущено, и это не ошибка.
#[tauri::command]
pub async fn core_traffic(state: State<'_, AppState>) -> Result<Option<Traffic>> {
    state.mihomo.traffic().await
}

/// Скачать ядро. Отдаёт версию. Пока ядро держит трафик, файл занят — сначала отключаемся.
#[tauri::command]
pub async fn core_install(engine: EngineId, state: State<'_, AppState>) -> Result<String> {
    state.connection.install(&state, engine).await
}
