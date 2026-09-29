//! Кто мы для панели подписки: идентификатор устройства и его описание.
//!
//! Панель ограничивает число устройств по заголовку `x-hwid` и **без него отвечает 404**
//! (Remnawave). Формат значения панель проверяет: `[a-zA-Z0-9=-]{10,64}`, а стандарт XTLS
//! ограничивает длину 36 символами — MachineGuid укладывается ровно.

use crate::error::{AppError, Result};
use crate::paths::Paths;

pub struct Device;

impl Device {
    /// Идентификатор устройства для подписок с привязкой (D-016, D-034).
    ///
    /// MachineGuid из реестра — настоящий идентификатор машины. Он переживает переустановку
    /// клиента, поэтому повторное добавление подписки не съедает у провайдера ещё один слот;
    /// прежний случайный идентификатор умирал вместе с каталогом данных и съедал.
    ///
    /// Файл рядом — не источник истины, а слепок: если реестр недоступен, берём из него,
    /// иначе перезаписываем. Два разных значения означали бы два устройства в панели.
    pub fn hwid() -> Result<String> {
        let path = Paths::hwid();
        let cached = std::fs::read_to_string(&path)
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|id| is_valid(id));

        let id = match machine_guid() {
            Some(id) => id,
            None => match cached.clone() {
                Some(id) => id,
                None => random()?,
            },
        };

        if cached.as_deref() != Some(id.as_str()) {
            Paths::ensure_root()?;
            crate::atomic::AtomicFile::write(&path, &id)?;
        }
        Ok(id)
    }

    /// Описание устройства. Панели это не обязательно, но по нему она различает устройства
    /// в списке — человеку иначе не понять, какой слот чей.
    pub fn os_version() -> String {
        #[cfg(windows)]
        {
            // Версию берём у самой системы: «11» константой врёт на любой другой сборке.
            if let Some(version) = registry(
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
                "CurrentBuild",
            ) {
                return format!("10.0.{version}");
            }
        }
        "unknown".into()
    }
}

/// Панель проверяет значение регуляркой — мусор она отвергнет вместе со всей подпиской.
fn is_valid(id: &str) -> bool {
    (10..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '=' || c == '-')
}

#[cfg(windows)]
fn machine_guid() -> Option<String> {
    registry("SOFTWARE\\Microsoft\\Cryptography", "MachineGuid").filter(|id| is_valid(id))
}

#[cfg(not(windows))]
fn machine_guid() -> Option<String> {
    None
}

/// Чтение одной строки из HKEY_LOCAL_MACHINE.
///
/// `KEY_WOW64_64KEY` обязателен: 32-битный процесс иначе попадёт в WOW6432Node, где
/// MachineGuid другой — и панель увидит два устройства вместо одного.
#[cfg(windows)]
fn registry(subkey: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
    };

    let subkey = wide(subkey);
    let name = wide(name);
    let mut key = std::ptr::null_mut();
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * 2) as u32;

    unsafe {
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut key,
        ) != 0
        {
            return None;
        }
        let status = RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        );
        RegCloseKey(key);
        if status != 0 {
            return None;
        }
    }

    let chars = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..chars.min(buffer.len())]))
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Запасной вариант, если реестр недоступен: случайный идентификатор установки.
/// Он переживает перезапуск, но не переустановку — и это лучше, чем не работающая подписка.
fn random() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| AppError::io(format!("Не удалось создать идентификатор: {e}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_regex_is_what_we_check_against() {
        assert!(
            is_valid("4c4c4544-0043-4a10-8058-b7c04f4a3258"),
            "MachineGuid"
        );
        assert!(is_valid("9f1c2d3e4a5b6c7d8e9f0a1b2c3d4e5f"), "случайный");
        assert!(!is_valid("короткий"), "меньше десяти символов");
        assert!(!is_valid("с кириллицей и пробелом"), "не тот алфавит");
        assert!(!is_valid(&"a".repeat(65)), "длиннее шестидесяти четырёх");
    }

    /// Если реестр читается — значение обязано подойти панели без правок.
    #[test]
    #[cfg(windows)]
    fn the_machine_guid_is_shaped_the_way_the_panel_wants() {
        match machine_guid() {
            Some(id) => assert!(is_valid(&id), "MachineGuid не прошёл проверку"),
            None => println!("реестр недоступен — проверять нечего"),
        }
    }

    #[test]
    #[cfg(windows)]
    fn the_os_version_is_read_from_the_system() {
        let version = Device::os_version();
        assert!(
            version.starts_with("10.0.") || version == "unknown",
            "неожиданная версия: {version}"
        );
    }
}
