//! Interface language: Russian when a Russian keyboard layout is installed, English
//! otherwise.
//!
//! The layout list rather than the display language: plenty of Russian speakers run an
//! English Windows, but nobody who does not type Russian keeps its layout around.

/// Primary language id of Russian (`LANG_RUSSIAN`), the low byte of a layout's language.
#[cfg(windows)]
const LANG_RUSSIAN: u16 = 0x19;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    #[cfg(windows)]
    pub fn detect() -> Lang {
        if layouts().into_iter().any(is_russian) {
            Lang::Ru
        } else {
            Lang::En
        }
    }

    /// Linux: раскладки рабочего стола (GNOME), системные (`localectl`) и язык сеанса.
    #[cfg(not(windows))]
    pub fn detect() -> Lang {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        let said = |program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .args(args)
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default()
        };
        let gnome = said(
            "gsettings",
            &["get", "org.gnome.desktop.input-sources", "sources"],
        );
        let system = said("localectl", &["status"]);
        if russian_unix(&locale, &gnome, &system) {
            Lang::Ru
        } else {
            Lang::En
        }
    }
}

/// Русская раскладка у GNOME — `('xkb', 'ru')`, у `localectl` — `ru` в списке
/// `X11 Layout: us,ru`.
#[cfg(not(windows))]
fn russian_unix(locale: &str, gnome: &str, system: &str) -> bool {
    locale.starts_with("ru")
        || gnome.contains("'ru'")
        || gnome.contains("'ru+")
        || system
            .lines()
            .filter_map(|line| line.trim().strip_prefix("X11 Layout:"))
            .any(|layouts| layouts.split(',').any(|layout| layout.trim() == "ru"))
}

/// Language ids of the installed keyboard layouts (the low word of each `HKL`).
#[cfg(windows)]
fn layouts() -> Vec<u16> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;
    // SAFETY: a zero-sized call only returns the count; the second call writes at most
    // `count` handles into a buffer of exactly that size.
    unsafe {
        let count = GetKeyboardLayoutList(0, std::ptr::null_mut());
        let mut list = vec![std::ptr::null_mut(); count.max(0) as usize];
        let got = GetKeyboardLayoutList(count, list.as_mut_ptr());
        list.truncate(got.max(0) as usize);
        list.into_iter()
            .map(|hkl| (hkl as usize & 0xFFFF) as u16)
            .collect()
    }
}

#[cfg(windows)]
fn is_russian(lang_id: u16) -> bool {
    lang_id & 0x3FF == LANG_RUSSIAN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn a_russian_layout_or_locale_means_russian() {
        assert!(russian_unix("ru_RU.UTF-8", "", ""));
        assert!(russian_unix(
            "en_US.UTF-8",
            "[('xkb', 'us'), ('xkb', 'ru')]",
            ""
        ));
        assert!(russian_unix("", "", "   X11 Layout: us,ru\n"));
        assert!(!russian_unix(
            "en_US.UTF-8",
            "[('xkb', 'us')]",
            "X11 Layout: us,ua"
        ));
    }

    #[test]
    #[cfg(windows)]
    fn only_the_primary_language_decides() {
        assert!(is_russian(0x0419), "ru-RU");
        assert!(!is_russian(0x0409), "en-US");
        assert!(!is_russian(0x0422), "uk-UA");
    }
}
