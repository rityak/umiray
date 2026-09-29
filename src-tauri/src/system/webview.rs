//! Видно ли страницу окна — сказать об этом самому WebView2.
//!
//! Окно прячется в трей (D-046) через `hide`, а страница этого не замечает:
//! `document.hidden` остаётся `false`, и она часами крутит опрос бэкенда, таймеры
//! и отрисовку для окна, которого нет на экране (замерено через CDP). WebView2 на такой
//! случай даёт `IsVisible`: с `false` браузер сам притормаживает таймеры и отрисовку,
//! а страница получает `visibilitychange` и останавливает опрос (`usePoll`).

use tauri::{Runtime, WebviewWindow};

pub struct Webview;

impl Webview {
    /// Показать или спрятать страницу вместе с окном. Неудача не страшна — страница просто
    /// продолжит работать как видимая, то есть как до этого модуля.
    pub fn set_visible<R: Runtime>(window: &WebviewWindow<R>, visible: bool) {
        #[cfg(windows)]
        let _ = window.with_webview(move |webview| {
            // SAFETY: вызов COM-метода контроллера на потоке окна, куда `with_webview`
            // и доставляет замыкание.
            let _ = unsafe { webview.controller().SetIsVisible(visible) };
        });
        #[cfg(not(windows))]
        let _ = (window, visible);
    }
}
