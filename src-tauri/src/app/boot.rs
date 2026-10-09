//! Фаза `app`: запуск клиента (D-101).
//!
//! Порядок этих шагов несущий, а снаружи был невидим: восемь вызовов в одном замыкании,
//! у большинства — комментарий «почему именно здесь». Теперь это список, и причина стоит
//! при записи, а не при строке кода.
//!
//! Отменить фазу не может ни один шаг, и это её свойство, а не поблажка: клиент обязан
//! подняться. Не построился значок в трее — окно всё равно открыто; не убрался залипший
//! прокси — тем более не повод не запускаться. Раньше `tray` уносил с собой весь `setup`.
//!
//! `main.rs` от этого остаётся тем, чем должен быть: сборкой приложения и списком команд.

use std::time::Instant;

use tauri::{AppHandle, Manager};

use crate::app::connect::Connection;
use crate::app::data::DataDir;
use crate::app::lifecycle::Lifecycle;
use crate::app::migrate::Migration;
use crate::app::settings;
use crate::app::state::AppState;
use crate::app::tick::Clock;
use crate::app::tray::Tray;
use crate::paths;

pub struct Boot;

impl Boot {
    /// У debug отдельны mutex одиночного запуска, хранилище WebView и имя окна (D-150).
    pub fn context() -> tauri::Context<tauri::Wry> {
        let mut context = tauri::generate_context!();
        if cfg!(debug_assertions) {
            let config = context.config_mut();
            config.identifier = "com.umiray.client.dev".into();
            config.product_name = Some(paths::APP_NAME.into());
            // Повышенный процесс WebView2 эту переменную не слушает (защита от правки
            // пользователем), а флаги из кода — слушает: так проверки по CDP достают и окно
            // от администратора (GOTCHAS). Умолчания wry при своих флагах пропадают — вернуть.
            let flags = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")
                .ok()
                .map(|own| {
                    format!("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {own}")
                });
            for window in &mut config.app.windows {
                window.title = paths::APP_NAME.into();
                window.additional_browser_args = flags.clone();
            }
            if let Some(updater) = config.plugins.0.get_mut("updater") {
                updater["pubkey"] = "".into();
            }
        }
        context
    }

    /// Пройти запуск. Отказ никого не отменяет — только уезжает в лог.
    pub fn run(app: &AppHandle) -> crate::error::Result<()> {
        // После single-instance и до чтения настроек. Ошибка копирования отменяет запуск:
        // пустой клиент поверх недокопированной подписки выглядел бы потерей данных.
        DataDir::migrate()?;
        if let Err(why) = Migration::run() {
            eprintln!("миграция не удалась: {why}");
        }
        app.manage(AppState::new());
        // Первая строка журнала — номер версии. Без него разбор чужого лога начинается
        // с вопроса «а какая это сборка»; строка стоит ровно один `println!`.
        eprintln!(
            "{}: запуск · версия {}",
            paths::APP_NAME,
            env!("CARGO_PKG_VERSION")
        );
        // Вторая — что эта ОС умеет (D-174): «почему у меня нет кнопки» на Linux решается
        // этой строкой в «Логах», а не перепиской о дистрибутиве.
        let features = format!(
            "возможности системы: {:?}",
            crate::system::features::Features::supported()
        );
        eprintln!("{}: {features}", paths::APP_NAME);
        app.state::<AppState>().note("info", &features);
        for step in STEPS {
            let began = Instant::now();
            let outcome = (step.run)(app);
            let (level, text) =
                Lifecycle::line("app", step.id, step.label, began.elapsed(), &outcome);
            eprintln!("{text}");
            if outcome.is_err() {
                app.state::<AppState>().note(level, &text);
            }
        }
        Ok(())
    }
}

/// Неудача шага — строка для человека. Своего варианта `AppError` тут не заводим:
/// вариант существует под действие, которое предложит окно (D-028), а окну на запуске
/// предлагать нечего — оно про эти шаги не знает вовсе.
type Done = Result<(), String>;

struct Step {
    id: &'static str,
    label: &'static str,
    run: fn(&AppHandle) -> Done,
}

/// Список запуска. Новый шаг — одна запись здесь, и место в списке — это и есть решение
/// о том, когда он выполнится.
const STEPS: &[Step] = &[
    Step {
        id: "window",
        label: "окно",
        run: window,
    },
    Step {
        id: "replace",
        label: "приём просьбы уступить место",
        run: replace,
    },
    Step {
        id: "sweep",
        label: "уборка осиротевшего ядра",
        run: sweep,
    },
    Step {
        id: "sources",
        label: "восстановление источников",
        run: sources,
    },
    Step {
        id: "tick",
        label: "часы",
        run: clock,
    },
    Step {
        id: "watch",
        label: "надзор за ядром",
        run: watch,
    },
    Step {
        id: "wake",
        label: "наблюдатель за сетью",
        run: netwatch,
    },
    Step {
        id: "session",
        label: "выход по просьбе системы",
        run: session,
    },
    Step {
        id: "tray",
        label: "значок в трее",
        run: icon,
    },
    Step {
        id: "autostart",
        label: "флаг записи автозапуска",
        run: autostart_flag,
    },
    Step {
        id: "system-proxy",
        label: "возврат системного прокси",
        run: proxy,
    },
    Step {
        id: "kill-switch",
        label: "снятие залипшего запрета",
        run: unlock,
    },
    Step {
        id: "autoconnect",
        label: "автоподключение",
        run: autoconnect,
    },
];

/// Окно показываем сами и первым делом: в конфиге оно объявлено скрытым, чтобы «тихий
/// запуск» не мигал им по дороге в трей (D-088). Раньше всего остального: шаг, упавший
/// до этого, оставил бы пользователя вообще без окна.
fn window(app: &AppHandle) -> Done {
    let shown = match app.state::<AppState>().settings.get().launch {
        settings::Launch::Window => true,
        settings::Launch::Tray => false,
        // Ярлык — окном, вход в систему — в трей (D-129).
        settings::Launch::Smart => !crate::system::autostart::Autostart::by_system(),
    };
    let Some(window) = app.get_webview_window("main") else {
        return Err("окна main нет в конфиге приложения".into());
    };
    // Запуск в трей — страница тоже невидима: иначе она опрашивала бы бэкенд до первого
    // показа окна (`system::webview`).
    crate::system::webview::Webview::set_visible(&window, shown);
    if !shown {
        return Ok(());
    }
    window.show().map_err(|why| why.to_string())
}

/// Прибираемся после падения, и ядро — первым: осиротевшее держит порт, а в TUN ещё
/// и весь трафик машины (D-059). Место обязано быть именно здесь: вторую копию приложения
/// плагин одиночного запуска гасит раньше `setup`, иначе она убила бы ядро первой.
/// Повышенный клиент обязан слышать новую версию, запущенную без прав (B-030).
fn replace(app: &AppHandle) -> Done {
    if crate::system::instance::Instance::hear_lower(&app.config().identifier) {
        Ok(())
    } else {
        Err(
            "окно одиночного запуска не нашлось — новая версия не сможет попросить уступить место"
                .into(),
        )
    }
}

fn sweep(app: &AppHandle) -> Done {
    let state = app.state::<AppState>();
    for id in crate::core::EngineId::ALL {
        crate::core::process::CoreProcess::sweep(&state.engine(id).binary());
    }
    Ok(())
}

fn clock(app: &AppHandle) -> Done {
    Clock::spawn(app.clone());
    Ok(())
}

/// Ядро, умершее не по нашей команде, поднимается заново (D-057).
fn watch(app: &AppHandle) -> Done {
    Connection::watch(app.clone());
    Ok(())
}

/// Сеть сменилась под ногами — узлы перепроверяются сами (D-112). Рядом с надзором
/// за ядром: оба про то, что случилось без нашего ведома.
fn netwatch(app: &AppHandle) -> Done {
    crate::app::wake::Wake::watch(app.clone());
    Ok(())
}

/// Выключение машины и `kill` — тот же выход, что из трея: ядро гасится, прокси
/// и запрет снимаются (`RunEvent::Exit` в `main`).
fn session(app: &AppHandle) -> Done {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::system::session::Session::ended().await;
        app.exit(0);
    });
    Ok(())
}

/// Что делает пункт питания, трей не знает: действие приходит отсюда, из места сборки
/// приложения.
fn icon(app: &AppHandle) -> Done {
    Tray::build(app, Connection::toggle, crate::app::volt::toggle_from_tray)
        .map_err(|why| why.to_string())?;
    crate::app::volt::show_in_tray(app);
    Ok(())
}

/// Запись в `Run` из прошлой сборки получает флаг, по которому `smart` узнаёт вход
/// в систему (D-129). Без этого у включивших автозапуск раньше окно всплывало бы при входе.
fn autostart_flag(_app: &AppHandle) -> Done {
    crate::system::autostart::Autostart::refresh().map_err(|why| why.to_string())
}

/// Подключаемся сами, если так велят настройки (D-088). После трея: значок должен уже
/// существовать, чтобы догнать состояние.
fn autoconnect(app: &AppHandle) -> Done {
    Connection::autoconnect(app.clone());
    Ok(())
}

/// Снимок прежней настройки остаётся в файле только в одном случае: мы включили прокси
/// и не успели его вернуть. Это и есть лекарство от «прокси залип», которого боялся D-008;
/// снимка нет — шаг ничего не делает.
fn proxy(app: &AppHandle) -> Done {
    let state = app.state::<AppState>();
    state.proxy.release(&state).map_err(|why| why.to_string())
}

/// То же лекарство для брандмауэра, и оно важнее: с залипшим запретом машина остаётся
/// без интернета вовсе, и «починить» для пользователя означает просто открыть umiray
/// ещё раз (D-073).
fn unlock(app: &AppHandle) -> Done {
    let state = app.state::<AppState>();
    let released = state.kill_switch.release(&state);
    // Запись возврата при входе могла пережить свои снимки (D-175).
    crate::app::leftovers::Leftovers::mark(&state);
    released.map_err(|why| why.to_string())
}

fn sources(_app: &AppHandle) -> Done {
    crate::nodes::sources::SourceStore::repair_all().map_err(|why| why.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тот же разбор регистрации, что у фазы `core`: повторный `id` — единственное,
    /// что не ловится типами.
    #[test]
    fn every_step_has_its_own_id() {
        let mut seen = Vec::new();
        for step in STEPS {
            assert!(!seen.contains(&step.id), "два шага с id {}", step.id);
            seen.push(step.id);
        }
    }

    /// Окно — первое: шаг, упавший раньше него, оставил бы пользователя без окна, а именно
    /// этого фаза и не должна допускать.
    #[test]
    fn the_window_comes_first() {
        assert_eq!(STEPS.first().map(|step| step.id), Some("window"));
    }

    #[test]
    fn stale_os_state_is_cleaned_before_autoconnect() {
        let ids = STEPS.iter().map(|step| step.id).collect::<Vec<_>>();
        let at = |id| ids.iter().position(|seen| *seen == id).unwrap();
        assert!(at("sources") < at("autoconnect"));
        assert!(at("system-proxy") < at("autoconnect"));
        assert!(at("kill-switch") < at("autoconnect"));
    }
    #[test]
    fn debug_has_its_own_instance_and_cannot_install_stable_updates() {
        let context = Boot::context();
        let config = context.config();
        if cfg!(debug_assertions) {
            assert_eq!(config.identifier, "com.umiray.client.dev");
            assert_eq!(config.app.windows[0].title, "umiray-dev");
            assert_eq!(config.plugins.0["updater"]["pubkey"], "");
            assert_eq!(paths::CORE_NAME.trim_end_matches(".exe"), "mihomo-dev");
        } else {
            assert_eq!(config.identifier, "com.umiray.client");
            assert_ne!(config.plugins.0["updater"]["pubkey"], "");
            assert_eq!(paths::CORE_NAME.trim_end_matches(".exe"), "mihomo");
        }
        #[cfg(windows)]
        assert_eq!(crate::system::task::NAME, paths::APP_NAME);
    }
}
