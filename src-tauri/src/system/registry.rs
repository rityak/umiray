//! Мелкая работа с реестром Windows.
//!
//! Отдельный модуль, потому что ключей стало два: настройки прокси (D-047) и автозапуск
//! (D-050).
//! Пока читатель был один, это жило внутри `sysproxy.rs`; появился второй — вынесли.
//!
//! Всё под `HKEY_CURRENT_USER`: оба наших ключа там, и права администратора не нужны.
//! Чтение `HKLM` живёт в `device.rs` — ему нужен `KEY_WOW64_64KEY`, и это его забота.

use crate::error::{AppError, Result};

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn open(subkey: &str, write: bool) -> Result<windows_sys::Win32::System::Registry::HKEY> {
    use windows_sys::Win32::System::Registry::{
        RegOpenKeyExW, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE,
    };
    let path = wide(subkey);
    let mut key = std::ptr::null_mut();
    let access = if write {
        KEY_READ | KEY_WRITE
    } else {
        KEY_READ
    };
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
    if status != 0 {
        return Err(AppError::io(format!(
            "Не удалось открыть ветку реестра {subkey} (код {status})"
        )));
    }
    Ok(key)
}

pub struct Registry;

impl Registry {
    /// Строковое значение. `None` — значения нет, и это не поломка: его просто не заводили.
    #[cfg(windows)]
    pub fn read_string(subkey: &str, name: &str) -> Result<Option<String>> {
        use windows_sys::Win32::System::Registry::{RegCloseKey, RegQueryValueExW};
        let key = open(subkey, false)?;
        let name = wide(name);
        let mut buffer = [0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        unsafe { RegCloseKey(key) };
        if status != 0 {
            return Ok(None);
        }
        let chars = (size as usize / 2).saturating_sub(1);
        Ok(Some(String::from_utf16_lossy(
            &buffer[..chars.min(buffer.len())],
        )))
    }

    #[cfg(windows)]
    pub fn read_dword(subkey: &str, name: &str) -> Result<Option<u32>> {
        use windows_sys::Win32::System::Registry::{RegCloseKey, RegQueryValueExW};
        let key = open(subkey, false)?;
        let name = wide(name);
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::addr_of_mut!(value).cast(),
                &mut size,
            )
        };
        unsafe { RegCloseKey(key) };
        if status != 0 {
            return Ok(None);
        }
        Ok(Some(value))
    }

    #[cfg(windows)]
    pub fn write_string(subkey: &str, name: &str, value: &str) -> Result<()> {
        use windows_sys::Win32::System::Registry::{RegCloseKey, RegSetValueExW, REG_SZ};
        let key = open(subkey, true)?;
        let name = wide(name);
        let data = wide(value);
        let status = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        unsafe { RegCloseKey(key) };
        failed(subkey, status)
    }

    #[cfg(windows)]
    pub fn write_dword(subkey: &str, name: &str, value: u32) -> Result<()> {
        use windows_sys::Win32::System::Registry::{RegCloseKey, RegSetValueExW, REG_DWORD};
        let key = open(subkey, true)?;
        let name = wide(name);
        let status = unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_DWORD,
                std::ptr::addr_of!(value).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        unsafe { RegCloseKey(key) };
        failed(subkey, status)
    }

    /// Убрать значение. Отсутствие — это успех: мы добивались именно того, чтобы его не было.
    #[cfg(windows)]
    pub fn delete_value(subkey: &str, name: &str) -> Result<()> {
        use windows_sys::Win32::System::Registry::{RegCloseKey, RegDeleteValueW};
        let key = open(subkey, true)?;
        let name = wide(name);
        let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
        unsafe { RegCloseKey(key) };
        // ERROR_FILE_NOT_FOUND — значения и так нет.
        if status == 2 {
            return Ok(());
        }
        failed(subkey, status)
    }

    #[cfg(not(windows))]
    pub fn read_string(_subkey: &str, _name: &str) -> Result<Option<String>> {
        Ok(None)
    }

    #[cfg(not(windows))]
    pub fn read_dword(_subkey: &str, _name: &str) -> Result<Option<u32>> {
        Ok(None)
    }

    #[cfg(not(windows))]
    pub fn write_string(_subkey: &str, _name: &str, _value: &str) -> Result<()> {
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn write_dword(_subkey: &str, _name: &str, _value: u32) -> Result<()> {
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn delete_value(_subkey: &str, _name: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn failed(subkey: &str, status: u32) -> Result<()> {
    if status != 0 {
        return Err(AppError::io(format!(
            "Не удалось записать в реестр {subkey} (код {status})"
        )));
    }
    Ok(())
}
