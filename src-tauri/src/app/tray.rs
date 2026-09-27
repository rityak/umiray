//! Значок в трее (D-046).
//!
//! Существует ради одного: закрытие окна больше не выключает VPN. Раньше крестик означал
//! `RunEvent::Exit`, а тот гасит ядро — то есть закрыть окно клиента значило отключиться.
//! Теперь крестик прячет окно, а выход — отдельный пункт меню. Питание тоже здесь
//! и одним пунктом на оба направления (D-091): у спрятанного клиента это то же основное
//! действие, что и кнопка в шапке.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

const TRAY_ID: &str = "main";
const POWER: &str = "power";
const SHOW: &str = "show";
const QUIT: &str = "quit";

/// Что показывает значок. Не «работает ли ядро», а режим целиком: пока окно спрятано,
/// значок — единственный источник этой информации (D-051).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Off,
    Local,
    Tun,
    /// Local proxy, прописанный в систему: для пользователя это отдельное состояние —
    /// от него зависит, идёт трафик браузера через нас или нет.
    System,
}

impl Look {
    /// Цветом одним состояние не показываем: подсказка называет режим словами (STYLEGUIDE).
    ///
    /// Имена те же, что в шапке (D-066): значок и переключатель говорят об одном, и звать
    /// один и тот же режим в двух местах по-разному нельзя.
    fn tooltip(self) -> String {
        let state = match self {
            Look::Off => "отключено",
            Look::Local => "Proxy",
            Look::Tun => "TUN",
            Look::System => "System: прокси прописан в Windows",
        };
        format!("{} — {state}", crate::paths::APP_NAME)
    }

    /// Значки различаются только цветом луча — форма одна, иначе в трее они читались бы
    /// как разные приложения.
    fn png(self, light: bool) -> &'static [u8] {
        match (self, light) {
            (Look::Off, false) => include_bytes!("../../icons/tray/dark/off.png"),
            (Look::Local, false) => include_bytes!("../../icons/tray/dark/local.png"),
            (Look::Tun, false) => include_bytes!("../../icons/tray/dark/tun.png"),
            (Look::System, false) => include_bytes!("../../icons/tray/dark/system.png"),
            (Look::Off, true) => include_bytes!("../../icons/tray/light/off.png"),
            (Look::Local, true) => include_bytes!("../../icons/tray/light/local.png"),
            (Look::Tun, true) => include_bytes!("../../icons/tray/light/tun.png"),
            (Look::System, true) => include_bytes!("../../icons/tray/light/system.png"),
        }
    }
}

/// Последнее показанное состояние. Статус опрашивается раз в 1.5 с, и перерисовывать
/// значок на каждый такт незачем — меняем, только когда действительно изменилось.
static SHOWN: Mutex<Option<(Look, bool)>> = Mutex::new(None);

// The taskbar follows the Windows system theme, not the application's theme.
fn light_taskbar() -> bool {
    crate::system::registry::read_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "SystemUsesLightTheme",
    )
    .ok()
    .flatten()
        == Some(1)
}

/// Меню трея. Пересобирается целиком при смене состояния, а не правится по частям:
/// пункт питания один на оба направления (D-060, как и кнопка в шапке), и его подпись —
/// это состояние. Пересборка трёх пунктов раз в несколько минут дешевле, чем хранить
/// ссылку на пункт в ещё одном глобальном месте.
fn menu<R: Runtime>(app: &AppHandle<R>, running: bool) -> tauri::Result<Menu<R>> {
    let power = MenuItem::with_id(
        app,
        POWER,
        if running {
            "Отключить"
        } else {
            "Подключить"
        },
        true,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, SHOW, "Показать окно", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Выход", true, None::<&str>)?;
    Menu::with_items(app, &[&power, &show, &quit])
}

/// Что делает пункт питания, приходит снаружи: значок — это индикатор, которым правит
/// `connect`, и знать про `connect` в ответ он не должен (иначе модули ссылаются друг
/// на друга по кругу). Указатель на функцию, а не абстракция: реализация
/// ровно одна, и живёт она там, где собирается приложение.
pub fn build<R: Runtime>(app: &AppHandle<R>, power: fn(&AppHandle<R>)) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(Look::Off.tooltip())
        .menu(&menu(app, false)?)
        // Левая кнопка показывает окно, меню — по правой. Иначе основное действие
        // («покажи окно») требовало бы двух нажатий вместо одного.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            // Питание доступно, не открывая окна: закрытый крестиком клиент живёт
            // в трее (D-046), и включать VPN из него — то же основное действие.
            POWER => power(app),
            SHOW => show(app),
            // Выход именно здесь: `RunEvent::Exit` по-прежнему гасит ядро, иначе
            // осиротевший mihomo держал бы порт (GOTCHAS).
            QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        });

    // Свой значок, а не иконка окна: он меняется по режиму, и форма у всех четырёх
    // одна — различается только цвет луча (D-051).
    let light = light_taskbar();
    tray = tray.icon(Image::from_bytes(Look::Off.png(light))?);

    tray.build(app)?;
    *SHOWN.lock().unwrap() = Some((Look::Off, light));
    Ok(())
}

/// Показать окно и поднять его наверх. Спрятанное окно ещё и свёрнутым может быть —
/// одного `show` тогда мало.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        crate::system::webview::set_visible(&window, true);
    }
}

/// Привести значок к текущему состоянию.
///
/// Ошибки глотаем: значок в трее — это индикация, и её пропажа не повод завалить
/// запуск или остановку ядра.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, look: Look) {
    let mut shown = SHOWN.lock().unwrap();
    let light = light_taskbar();
    if *shown == Some((look, light)) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(icon) = Image::from_bytes(look.png(light)) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(look.tooltip()));
    // Подпись питания — часть состояния: «Подключить» при работающем ядре врала бы
    // ровно там, где окна на экране может не быть вовсе.
    if let Ok(next) = menu(app, look != Look::Off) {
        let _ = tray.set_menu(Some(next));
    }
    *shown = Some((look, light));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tray_variant_decodes() {
        for look in [Look::Off, Look::Local, Look::System, Look::Tun] {
            for light in [false, true] {
                let icon = Image::from_bytes(look.png(light)).unwrap();
                assert_eq!((icon.width(), icon.height()), (32, 32));
            }
        }
    }
}
