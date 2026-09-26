//! Права администратора: есть ли они у нас и как перезапуститься с ними.
//!
//! Нужны только для TUN: настройка виртуального адаптера без них падает с «Access is denied».
//! Режим local proxy работает от обычного пользователя, поэтому поднимать права по умолчанию
//! нельзя — это разовое действие по явной команде.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;

use crate::error::{AppError, Result};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// ShellExecuteW отдаёт код <= 32, если запустить не удалось.
const SHELL_EXECUTE_MIN_SUCCESS: isize = 32;

/// Права процесса не меняются на ходу, поэтому считаем один раз: статус опрашивается часто.
pub fn is_elevated() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(query_elevation)
}

/// Запускает вторую копию приложения через UAC и оставляет вызывающему решение о выходе.
///
/// Права нельзя добавить работающему процессу — можно только стартовать новый с нужным токеном.
pub fn relaunch_as_admin() -> Result<()> {
    let exe = std::env::current_exe()?;

    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if (result as isize) <= SHELL_EXECUTE_MIN_SUCCESS {
        return Err(AppError::NeedsElevation {
            message: "Запуск с правами администратора отклонён".into(),
        });
    }
    Ok(())
}

fn query_elevation() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);

        ok != 0 && elevation.TokenIsElevated != 0
    }
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
