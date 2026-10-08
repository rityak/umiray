//! Панель, где живёт значок клиента: какого она цвета и есть ли в ней трей (D-046, D-051).

pub struct Taskbar;

impl Taskbar {
    /// Светлая ли панель: от этого зависит, тёмный или светлый значок на ней виден.
    /// Windows красит панель по теме системы, а не приложения.
    pub fn is_light() -> bool {
        #[cfg(windows)]
        return crate::system::registry::Registry::read_dword(
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "SystemUsesLightTheme",
        )
        .ok()
        .flatten()
            == Some(1);
        // Верхняя панель GNOME тёмная при любой теме. Панель Plasma идёт за схемой цветов:
        // по умолчанию (Breeze) — светлая, у тёмных схем в имени есть «Dark».
        #[cfg(not(windows))]
        {
            let kde = std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .any(|name| name.eq_ignore_ascii_case("KDE"));
            kde && !std::process::Command::new("kreadconfig6")
                .args([
                    "--file",
                    "kdeglobals",
                    "--group",
                    "General",
                    "--key",
                    "ColorScheme",
                ])
                .output()
                .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("Dark"))
        }
    }

    /// Есть ли куда спрятать окно. На Windows трей есть всегда; на Linux — только если
    /// кто-то держит `StatusNotifierWatcher`: GNOME без расширения AppIndicator его
    /// не держит, и окно, спрятанное в несуществующий трей, пропало бы без возврата.
    pub fn has_tray() -> bool {
        #[cfg(windows)]
        return true;
        #[cfg(not(windows))]
        {
            static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            *FOUND.get_or_init(|| {
                std::process::Command::new("gdbus")
                    .args([
                        "call",
                        "--session",
                        "--dest",
                        "org.freedesktop.DBus",
                        "--object-path",
                        "/org/freedesktop/DBus",
                        "--method",
                        "org.freedesktop.DBus.NameHasOwner",
                        "org.kde.StatusNotifierWatcher",
                    ])
                    .output()
                    .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("true"))
            })
        }
    }
}
