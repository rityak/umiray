//! Страница в браузере по умолчанию: окно клиента ссылок не открывает — WebView2 поднял бы
//! на них своё окно без адресной строки.

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::error::{AppError, Result};

/// Куда окну можно отправить человека. Адрес приходит из вебвью: открыть «что угодно» значило
/// бы запустить по его просьбе любой файл или протокол.
const ALLOWED: &str = "https://github.com/";

pub struct Browser;

impl Browser {
    /// Открыть страницу GitHub — там лежат rule sets каталога.
    pub fn github(url: &str) -> Result<()> {
        if !allowed(url) {
            return Err(AppError::invalid(format!("Не открываю: {url}")));
        }
        let verb = wide("open");
        let file = wide(url);
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
        // ShellExecuteW отдаёт код <= 32, если открыть не удалось.
        if (result as isize) <= 32 {
            return Err(AppError::io(format!("Браузер не открылся: {url}")));
        }
        Ok(())
    }
}

fn allowed(url: &str) -> bool {
    url.starts_with(ALLOWED)
        && url.len() > ALLOWED.len()
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_github_page_is_opened() {
        assert!(allowed(
            "https://github.com/itdoginfo/allow-domains/blob/main/Russia/inside-raw.lst"
        ));
        assert!(!allowed("https://github.com/"), "пустой путь");
        assert!(!allowed("https://github.com.evil.example/x"));
        assert!(!allowed("http://github.com/x"));
        assert!(!allowed("file:///C:/Windows/System32/calc.exe"));
        assert!(!allowed("https://github.com/x\" --flag"));
    }
}
