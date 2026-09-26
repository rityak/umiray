//! Системное окно выбора файла (D-120).
//!
//! Своего окна не рисуем и плагина ради одного диалога не берём: `GetOpenFileNameW`
//! лежит в `comdlg32`, а `windows-sys` у нас и так стоит. Это то самое окно, которое
//! человек видит во всех остальных программах, — со своими «Недавними», сетью и вводом
//! пути с клавиатуры.
//!
//! Зовётся из команды, а та — из `spawn_blocking`: окно модальное и держит поток,
//! пока человек не ответит.

#[cfg(windows)]
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

/// Строка фильтра: пары «подпись\0маска\0», и весь список закрыт вторым нулём.
#[cfg(windows)]
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Спросить файл. `None` — человек закрыл окно; это не ошибка и сообщением не является.
#[cfg(windows)]
pub fn file(title: &str, filter: &[(&str, &str)]) -> Option<std::path::PathBuf> {
    let mut patterns: Vec<u16> = Vec::new();
    for (label, mask) in filter {
        patterns.extend(utf16(label));
        patterns.extend(utf16(mask));
    }
    patterns.push(0);
    let title = utf16(title);

    // MAX_PATH мало: у пути с русскими папками и OneDrive он кончается. Буфер берём
    // с запасом — цена ему четыре килобайта на одно нажатие.
    let mut buffer = vec![0u16; 2048];
    let mut open: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    open.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    open.lpstrFilter = patterns.as_ptr();
    open.lpstrFile = buffer.as_mut_ptr();
    open.nMaxFile = buffer.len() as u32;
    open.lpstrTitle = title.as_ptr();
    // `NOCHANGEDIR` обязателен: без него окно меняет текущий каталог процесса, и все
    // относительные пути клиента после одного нажатия указывают в чужое место.
    open.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;

    if unsafe { GetOpenFileNameW(&mut open) } == 0 {
        return None;
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(std::path::PathBuf::from(String::from_utf16_lossy(
        &buffer[..end],
    )))
}

#[cfg(not(windows))]
pub fn file(_title: &str, _filter: &[(&str, &str)]) -> Option<std::path::PathBuf> {
    None
}
