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

pub struct FileDialog;

impl FileDialog {
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

    /// Linux: окно рабочего стола — `kdialog` в KDE, `zenity` в остальных (пакет тянет
    /// его зависимостью, D-173).
    #[cfg(not(windows))]
    pub fn file(title: &str, filter: &[(&str, &str)]) -> Option<std::path::PathBuf> {
        if kde() {
            return ask(
                "kdialog",
                &[
                    "--title",
                    title,
                    "--getopenfilename",
                    ".",
                    &kde_filter(filter),
                ],
            );
        }
        let mut args = vec!["--file-selection".to_string(), format!("--title={title}")];
        args.extend(zenity_filter(filter));
        ask(
            "zenity",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }

    #[cfg(windows)]
    pub fn save(
        title: &str,
        filter: &[(&str, &str)],
        name: &str,
        extension: &str,
    ) -> Option<std::path::PathBuf> {
        use windows_sys::Win32::UI::Controls::Dialogs::{GetSaveFileNameW, OFN_OVERWRITEPROMPT};

        let mut patterns: Vec<u16> = Vec::new();
        for (label, mask) in filter {
            patterns.extend(utf16(label));
            patterns.extend(utf16(mask));
        }
        patterns.push(0);
        let title = utf16(title);
        let extension = utf16(extension);

        let mut buffer = vec![0u16; 2048];
        for (at, unit) in name.encode_utf16().take(buffer.len() - 1).enumerate() {
            buffer[at] = unit;
        }
        let mut open: OPENFILENAMEW = unsafe { std::mem::zeroed() };
        open.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        open.lpstrFilter = patterns.as_ptr();
        open.lpstrFile = buffer.as_mut_ptr();
        open.nMaxFile = buffer.len() as u32;
        open.lpstrTitle = title.as_ptr();
        open.lpstrDefExt = extension.as_ptr();
        open.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;

        if unsafe { GetSaveFileNameW(&mut open) } == 0 {
            return None;
        }
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(std::path::PathBuf::from(String::from_utf16_lossy(
            &buffer[..end],
        )))
    }

    #[cfg(not(windows))]
    pub fn save(
        title: &str,
        filter: &[(&str, &str)],
        name: &str,
        extension: &str,
    ) -> Option<std::path::PathBuf> {
        let picked = if kde() {
            ask(
                "kdialog",
                &[
                    "--title",
                    title,
                    "--getsavefilename",
                    name,
                    &kde_filter(filter),
                ],
            )
        } else {
            let mut args = vec![
                "--file-selection".to_string(),
                "--save".to_string(),
                "--confirm-overwrite".to_string(),
                format!("--title={title}"),
                format!("--filename={name}"),
            ];
            args.extend(zenity_filter(filter));
            ask(
                "zenity",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        }?;
        // Расширение дописываем сами, как `lpstrDefExt` на Windows.
        Some(if picked.extension().is_none() {
            picked.with_extension(extension)
        } else {
            picked
        })
    }
}

#[cfg(not(windows))]
/// KDE и в нём есть `kdialog`. Без него и в KDE — `zenity`: пакеты тянут его зависимостью.
fn kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"))
        && std::process::Command::new("kdialog")
            .arg("--version")
            .output()
            .is_ok()
}

/// Ответ окна — путь строкой; закрыли окно — пусто и ненулевой код.
#[cfg(not(windows))]
fn ask(program: &str, args: &[&str]) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string();
    (out.status.success() && !path.is_empty()).then(|| path.into())
}

/// Маски Windows `*.yaml;*.yml` у zenity — `подпись | *.yaml *.yml`.
#[cfg(not(windows))]
fn zenity_filter(filter: &[(&str, &str)]) -> Vec<String> {
    filter
        .iter()
        .map(|(label, mask)| format!("--file-filter={label} | {}", mask.replace(';', " ")))
        .collect()
}

/// У kdialog — `*.yaml *.yml|подпись`, фильтры через перевод строки.
#[cfg(not(windows))]
fn kde_filter(filter: &[(&str, &str)]) -> String {
    filter
        .iter()
        .map(|(label, mask)| format!("{}|{label}", mask.replace(';', " ")))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn windows_masks_become_desktop_filters() {
        let filter = [("Подписка (*.yaml)", "*.yaml;*.yml")];
        assert_eq!(
            zenity_filter(&filter),
            ["--file-filter=Подписка (*.yaml) | *.yaml *.yml"]
        );
        assert_eq!(kde_filter(&filter), "*.yaml *.yml|Подписка (*.yaml)");
    }
}
