//! Автозапуск на Linux — файл в `~/.config/autostart` по спецификации XDG: его читают
//! GNOME, KDE, Xfce и прочие окружения (D-173).
//!
//! Состояние, как и на Windows, живёт **в системе** (D-049): есть файл — автозапуск включён.
//! Убрал его человек сам — окно покажет «выключено». «Всегда от администратора» на Linux
//! нет: права даются ядру на каждый запуск, а не клиенту (D-174).

use std::path::PathBuf;

use crate::error::{AppError, Result};

/// Флаг, с которым клиента поднимает автозапуск: по нему `smart` узнаёт, что окно
/// показывать не надо (D-129).
pub const AT_LOGON: &str = "--autostart";

/// Флаг записи, которая при входе только возвращает сеть: прокси и запрет, оставленные
/// клиентом, умершим вместе с сеансом (D-175).
pub const RESTORE: &str = "--restore";

pub struct Autostart;

impl Autostart {
    pub fn by_system() -> bool {
        std::env::args().any(|arg| arg == AT_LOGON)
    }

    /// Переписывать нечего: файл заводит только эта сборка, флаг в нём с первого дня.
    pub fn refresh() -> Result<()> {
        Ok(())
    }

    pub fn enabled() -> bool {
        file("").is_file()
    }

    pub fn always_admin() -> bool {
        false
    }

    pub fn set_always_admin(_on: bool) -> Result<()> {
        Err(AppError::invalid(
            "На Linux права даются ядру при каждом запуске — отдельной настройки нет",
        ))
    }

    pub fn set(on: bool) -> Result<()> {
        put("", on, AT_LOGON)
    }

    /// Запись «вернуть сеть при входе» (D-175): лежит, пока в системе стоит наше.
    pub fn set_restore(on: bool) -> Result<()> {
        put("-restore", on, RESTORE)
    }
}

fn put(suffix: &str, on: bool, flag: &str) -> Result<()> {
    let path = &file(suffix);
    if !on {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let exe = std::env::current_exe()?;
    std::fs::write(
        path,
        entry(
            &format!("{}{suffix}", crate::paths::APP_NAME),
            &exe.to_string_lossy(),
            flag,
        ),
    )?;
    Ok(())
}

/// `~/.config/autostart/<имя><suffix>.desktop`; своё имя у отладочной сборки (D-150).
fn file(suffix: &str) -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    config
        .join("autostart")
        .join(format!("{}{suffix}.desktop", crate::paths::APP_NAME))
}

/// Запись автозапуска. Путь — в кавычках: спецификация XDG делит `Exec` по пробелам.
fn entry(name: &str, exe: &str, flag: &str) -> String {
    let exe = exe.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec=\"{exe}\" {flag}\n\
         X-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_starts_the_client_quietly() {
        let text = entry("umiray", "/opt/my apps/umiray", AT_LOGON);
        assert!(text.contains("Exec=\"/opt/my apps/umiray\" --autostart\n"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        let restore = entry("umiray-restore", "/usr/bin/umiray", RESTORE);
        assert!(restore.contains("Exec=\"/usr/bin/umiray\" --restore\n"));
    }
}
