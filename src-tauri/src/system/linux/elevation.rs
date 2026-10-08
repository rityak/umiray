//! Права на TUN под Linux (D-173): не у клиента, а у ядра, и на один его запуск.
//!
//! Клиент работает от пользователя всегда. Ядро в TUN запускает помощник через pkexec:
//! он отдаёт ему `CAP_NET_ADMIN` от имени того же пользователя (`helper::run_core`).
//! «Есть права» поэтому значит «есть чем их получить»: pkexec на месте или клиент сам root.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::error::{AppError, Result};
use crate::system::helper::Helper;

pub struct Elevation;

impl Elevation {
    pub fn is_elevated() -> bool {
        Helper::available()
    }

    /// Перезапуска с правами на Linux нет: права даются ядру, а не клиенту. Сюда приходят,
    /// только когда их не дать нечем.
    pub fn relaunch_as_admin() -> Result<()> {
        Err(AppError::NeedsElevation {
            message: Elevation::missing(),
        })
    }

    /// Почему TUN не поднять, если прав нет.
    pub fn missing() -> String {
        "Для TUN нужен pkexec (пакет polkit) — установите его и подключитесь снова".into()
    }

    /// Ядро с правами завершилось этим кодом — не отказал ли в правах pkexec.
    pub fn refused(status: std::process::ExitStatus) -> Option<String> {
        Helper::refused(status.code()).map(str::to_string)
    }

    /// Команда, которая запустит ядро с правами на адаптер `device`. Аргументы ядра
    /// дописывает вызывающий.
    pub fn privileged(binary: &Path, device: &str) -> Command {
        Helper::command(&[OsStr::new("core"), OsStr::new(device), binary.as_os_str()])
    }
}
