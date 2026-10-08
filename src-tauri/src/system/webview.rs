//! Вебвью окна: видно ли страницу (WebView2) и как его готовить к запуску (WebKitGTK).
//!
//! Окно прячется в трей (D-046) через `hide`, а страница этого не замечает:
//! `document.hidden` остаётся `false`, и она часами крутит опрос бэкенда, таймеры
//! и отрисовку для окна, которого нет на экране (замерено через CDP). WebView2 на такой
//! случай даёт `IsVisible`: с `false` браузер сам притормаживает таймеры и отрисовку,
//! а страница получает `visibilitychange` и останавливает опрос (`usePoll`).

use tauri::{Runtime, WebviewWindow};

pub struct Webview;

impl Webview {
    /// До первого окна. WebKitGTK с драйвером NVIDIA рисует пустое окно или падает
    /// (Error 71 на Wayland): его быстрый путь отрисовки просит у драйвера форматы буферов,
    /// которых тот не даёт. Выключаем этот путь только там, где стоит NVIDIA, и только
    /// если человек не решил сам. Остальным он не мешает и быстрее.
    pub fn prepare() {
        #[cfg(target_os = "linux")]
        if std::path::Path::new("/proc/driver/nvidia").exists()
            && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        {
            // Первым делом в `main`, пока других нитей нет: менять окружение безопасно.
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

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
