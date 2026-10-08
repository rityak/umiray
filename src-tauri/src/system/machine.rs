//! Что машина говорит о себе: свой идентификатор и версию ОС (D-034). Читает их
//! `nodes::device` — для заголовков подписки.

pub struct Machine;

impl Machine {
    /// Идентификатор машины, переживающий переустановку клиента: иначе каждое повторное
    /// добавление подписки съедало бы у провайдера слот устройства.
    ///
    /// Windows — MachineGuid. Linux — производное от `/etc/machine-id`: сам он по совету
    /// systemd наружу не уходит, а хэш с именем клиента так же постоянен и с другими
    /// программами не сопоставляется.
    pub fn id() -> Option<String> {
        #[cfg(windows)]
        return hklm("SOFTWARE\\Microsoft\\Cryptography", "MachineGuid");
        #[cfg(not(windows))]
        {
            use sha2::Digest;
            let raw = ["/etc/machine-id", "/var/lib/dbus/machine-id"]
                .iter()
                .find_map(|path| std::fs::read_to_string(path).ok())?;
            let raw = raw.trim();
            if raw.is_empty() {
                return None;
            }
            let digest = sha2::Sha256::digest(format!("umiray:{raw}"));
            Some(
                digest[..16]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            )
        }
    }

    /// Имя ОС для панели (`X-Device-Os`).
    pub fn os() -> &'static str {
        if cfg!(windows) {
            "Windows"
        } else {
            "Linux"
        }
    }

    /// Версия ОС у самой системы: константа врёт на любой другой сборке.
    pub fn os_version() -> String {
        #[cfg(windows)]
        if let Some(build) = hklm(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
            "CurrentBuild",
        ) {
            return format!("10.0.{build}");
        }
        #[cfg(not(windows))]
        if let Some(version) = os_release() {
            return version;
        }
        "unknown".into()
    }
}

/// `ID VERSION_ID` из `/etc/os-release`: `fedora 43`, `ubuntu 24.04`.
#[cfg(not(windows))]
fn os_release() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(|value| value.trim_matches('"').to_string())
    };
    let id = field("ID")?;
    Some(match field("VERSION_ID") {
        Some(version) => format!("{id} {version}"),
        None => id,
    })
}

/// Чтение одной строки из HKEY_LOCAL_MACHINE.
///
/// `KEY_WOW64_64KEY` обязателен: 32-битный процесс иначе попадёт в WOW6432Node, где
/// MachineGuid другой — и панель увидит два устройства вместо одного.
#[cfg(windows)]
fn hklm(subkey: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
    };

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_os_version_is_read_from_the_system() {
        let version = Machine::os_version();
        assert!(!version.is_empty());
        if cfg!(windows) {
            assert!(
                version.starts_with("10.0.") || version == "unknown",
                "неожиданная версия: {version}"
            );
        }
    }
}
