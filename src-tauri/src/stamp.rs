//! Чем метят файл на диске: случайный идентификатор и время эпохи.
//!
//! Отдельный модуль, потому что метят двое — источники и наборы конфигов, — и метка
//! у них должна быть одинаковой. Пока читатель был один, обе функции жили в `sources.rs`;
//! со вторым читателем это стало бы копией.

use crate::error::{AppError, Result};

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
