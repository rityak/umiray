//! Interface language: Russian when a Russian keyboard layout is installed, English
//! otherwise.
//!
//! The layout list rather than the display language: plenty of Russian speakers run an
//! English Windows, but nobody who does not type Russian keeps its layout around.

/// Primary language id of Russian (`LANG_RUSSIAN`), the low byte of a layout's language.
const LANG_RUSSIAN: u16 = 0x19;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    pub fn detect() -> Lang {
        if layouts().into_iter().any(is_russian) {
            Lang::Ru
        } else {
            Lang::En
        }
    }
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

#[cfg(not(windows))]
fn layouts() -> Vec<u16> {
    Vec::new()
}

fn is_russian(lang_id: u16) -> bool {
    lang_id & 0x3FF == LANG_RUSSIAN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_primary_language_decides() {
        assert!(is_russian(0x0419), "ru-RU");
        assert!(!is_russian(0x0409), "en-US");
        assert!(!is_russian(0x0422), "uk-UA");
    }
}
