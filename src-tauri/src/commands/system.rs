//! Команды про систему: сброс, автозапуск, kill switch, права администратора.

use tauri::State;

use crate::app::state::AppState;
use crate::app::status::Status;
use crate::error::Result;
use crate::nodes::device::Device;
use crate::system::autostart::Autostart;

/// Сброс всего, кроме скачанных ядер и идентификатора устройства (`app/maintenance.rs`).
#[tauri::command]
pub async fn system_reset(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    state.maintenance.reset(&app, &state).await
}

/// Настройки копией базы (D-163). Пусто — окно сохранения закрыли.
#[tauri::command]
pub async fn system_export(state: State<'_, AppState>) -> Result<Option<String>> {
    state.maintenance.export().await
}

/// «Всегда от администратора» (D-087). Как и автозапуск, в настройках не хранится:
/// это наличие задачи в планировщике, и она же — источник истины.
///
/// Отдельная команда, а не поле в `settings_update`: завести задачу можно только
/// с правами, и отказ обязан приехать как `NeedsElevation` — с кнопкой, а не текстом.
#[tauri::command]
pub fn system_always_admin_set(on: bool, state: State<AppState>) -> Result<Status> {
    Autostart::set_always_admin(on)?;
    Ok(Status::gather(&state))
}

/// Автозапуск в настройках не храним: он и есть запись в реестре (см. `autostart.rs`).
#[tauri::command]
pub fn system_autostart_set(on: bool, state: State<AppState>) -> Result<Status> {
    Autostart::set(on)?;
    Ok(Status::gather(&state))
}

/// Тумблер kill switch (D-073): отдельная команда, потому что он меняет состояние машины
/// и сработать обязан сейчас, а не при следующем подключении.
#[tauri::command]
pub async fn system_kill_switch_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    state.kill_switch.set(&app, &state, on).await
}

#[tauri::command]
pub async fn system_relaunch_elevated(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    state.maintenance.relaunch_elevated(&app, &state).await
}

/// Идентификатор устройства — справочно. Каждый новый съедает слот в подписке (GOTCHAS),
/// поэтому знать, какой именно держит клиент, полезно; до сих пор он лежал только в файле.
#[tauri::command]
pub fn system_device() -> Result<String> {
    Device::hwid()
}

/// Страница rule set на GitHub — в браузере по умолчанию. Открывается только GitHub:
/// адрес приходит из вебвью.
#[tauri::command]
pub fn system_open_github(url: String) -> Result<()> {
    crate::system::browser::Browser::github(&url)
}

/// The interface language, picked from the installed keyboard layouts.
#[tauri::command]
pub fn system_language() -> crate::system::lang::Lang {
    crate::system::lang::Lang::detect()
}

/// Что эта ОС умеет хорошо (D-174): окно прячет всё, чего нет в списке.
#[tauri::command]
pub fn system_features() -> Vec<crate::system::features::Feature> {
    crate::system::features::Features::supported()
}
