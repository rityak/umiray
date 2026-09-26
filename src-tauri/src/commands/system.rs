//! Команды про систему: сброс, автозапуск, системный прокси, права администратора.

use tauri::State;

use crate::app::settings;
use crate::app::state::AppState;
use crate::app::status::{
    engage_kill_switch, look, release_kill_switch, release_system_proxy, status, Status,
};
use crate::app::{reset, tray};
use crate::error::Result;
use crate::nodes::device;
use crate::system::{autostart, elevation};

/// Перезапустить приложение с правами администратора: только так включается TUN.
/// Текущее окно закрываем сами — две копии одновременно ни к чему.
/// Сброс всего, кроме скачанного ядра и идентификатора устройства (см. `reset.rs`).
///
/// Ядро останавливаем сами: оно держит рабочий каталог и читает файлы источников,
/// а требовать «сначала отключитесь» — это перекладывать на пользователя то,
/// что мы и так знаем.
#[tauri::command]
pub async fn system_reset(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Status> {
    let _transition = state.transition().await;
    state.supervisor.stop();
    release_kill_switch(&state)?;
    release_system_proxy(&state)?;
    reset::run()?;
    state.reload_settings();
    let current = status(&state);
    tray::refresh(&app, look(&current));
    Ok(current)
}

/// «Всегда от администратора» (D-087). Как и автозапуск, в настройках не хранится:
/// это наличие задачи в планировщике, и она же — источник истины.
///
/// Отдельная команда, а не поле в `settings_update`: завести задачу можно только
/// с правами, и отказ обязан приехать как `NeedsElevation` — с кнопкой, а не текстом.
#[tauri::command]
pub fn system_always_admin_set(on: bool, state: State<AppState>) -> Result<Status> {
    autostart::set_always_admin(on)?;
    Ok(status(&state))
}

/// Автозапуск в настройках не храним: он и есть запись в реестре (см. `autostart.rs`).
#[tauri::command]
pub fn system_autostart_set(on: bool, state: State<AppState>) -> Result<Status> {
    autostart::set(on)?;
    Ok(status(&state))
}

/// Тумблер kill switch (D-073). Отдельная команда по той же причине, что и у прокси:
/// он меняет состояние машины, и сработать обязан сейчас, а не при следующем подключении.
///
/// Выключение снимает запрет **всегда**, не спрашивая, работает ли ядро: если правило
/// осталось от прошлой жизни клиента, тумблер — самый естественный способ его убрать.
#[tauri::command]
pub async fn system_kill_switch_set(
    on: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    let _transition = state.transition().await;
    state.patch(settings::Patch {
        kill_switch: Some(on),
        ..Default::default()
    })?;
    if on {
        engage_kill_switch(&state)?;
    } else {
        release_kill_switch(&state)?;
    }
    let current = status(&state);
    tray::refresh(&app, look(&current));
    Ok(current)
}

#[tauri::command]
pub async fn system_relaunch_elevated(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    let _transition = state.transition().await;
    elevation::relaunch_as_admin()?;
    state.supervisor.stop();
    app.exit(0);
    Ok(())
}

/// Идентификатор устройства — справочно. Каждый новый съедает слот в подписке (GOTCHAS),
/// поэтому знать, какой именно держит клиент, полезно; до сих пор он лежал только в файле.
#[tauri::command]
pub fn system_device() -> Result<String> {
    device::hwid()
}

/// The interface language, picked from the installed keyboard layouts.
#[tauri::command]
pub fn system_language() -> crate::system::lang::Lang {
    crate::system::lang::detect()
}
