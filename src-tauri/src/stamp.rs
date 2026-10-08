//! Чем метят файл на диске: случайный идентификатор и время эпохи.
//!
//! Отдельный модуль, потому что метят двое — источники и наборы конфигов, — и метка
//! у них должна быть одинаковой. Пока читатель был один, обе функции жили в `sources.rs`;
//! со вторым читателем это стало бы копией.

use crate::error::{AppError, Result};

pub struct Stamp;

impl Stamp {
    /// Шестнадцать шестнадцатеричных цифр: коротко, читается глазами в имени файла,
    /// и столкнуться на одной машине нечем.
    pub fn id() -> Result<String> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes)
            .map_err(|e| AppError::io(format!("Не удалось получить случайные байты: {e}")))?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// Секунды эпохи. `None`, если часы машины стоят до 1970 — форматирует всё равно окно.
    pub fn now() -> Option<u64> {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|since| since.as_secs())
    }

    /// Время UTC в RFC 3339 с миллисекундами: так Cloudflare ждёт отметку согласия
    /// с условиями WARP (D-165).
    #[cfg(windows)]
    pub fn utc() -> String {
        use windows_sys::Win32::System::SystemInformation::GetSystemTime;
        // SAFETY: GetSystemTime только заполняет переданную структуру.
        let t = unsafe {
            let mut t = std::mem::zeroed();
            GetSystemTime(&mut t);
            t
        };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
        )
    }

    /// Местное время строкой, как его пишет mihomo в `time=`: `2026-09-26T17:32:55.123`.
    /// Своим строкам в логе нужно то же время, что у строк ядра, иначе колонка времени
    /// в окне дырявая ровно там, где объясняется нажатие кнопки.
    #[cfg(windows)]
    pub fn local() -> String {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        // SAFETY: GetLocalTime только заполняет переданную структуру.
        let t = unsafe {
            let mut t = std::mem::zeroed();
            GetLocalTime(&mut t);
            t
        };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
        )
    }

    #[cfg(unix)]
    pub fn utc() -> String {
        format!("{}Z", unix(libc::gmtime_r))
    }

    #[cfg(unix)]
    pub fn local() -> String {
        unix(libc::localtime_r)
    }
}

/// Время по разбивке libc (`gmtime_r` или `localtime_r`) в том же виде, что у Windows.
#[cfg(unix)]
fn unix(
    split: unsafe extern "C" fn(*const libc::time_t, *mut libc::tm) -> *mut libc::tm,
) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as libc::time_t;
    // SAFETY: обе функции только заполняют переданную структуру.
    let t = unsafe {
        let mut t: libc::tm = std::mem::zeroed();
        split(&seconds, &mut t);
        t
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
        t.tm_year + 1900,
        t.tm_mon + 1,
        t.tm_mday,
        t.tm_hour,
        t.tm_min,
        t.tm_sec,
        now.subsec_millis()
    )
}
