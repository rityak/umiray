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

use crate::app::lifecycle::line;
use crate::app::state::AppState;
use crate::app::{connect, settings, status, tick, tray};

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

/// Пройти запуск. Отказ никого не отменяет — только уезжает в лог.
pub fn run(app: &AppHandle) {
    // Первая строка журнала — номер версии. Без него разбор чужого лога начинается
    // с вопроса «а какая это сборка»; строка стоит ровно один `println!`.
    eprintln!("umiray: запуск · версия {}", env!("CARGO_PKG_VERSION"));
    for step in STEPS {
        let began = Instant::now();
        let outcome = (step.run)(app);
        let (level, text) = line("app", step.id, step.label, began.elapsed(), &outcome);
        eprintln!("{text}");
        if outcome.is_err() {
            app.state::<AppState>().supervisor.note(level, &text);
        }
    }
}

/// Окно показываем сами и первым делом: в конфиге оно объявлено скрытым, чтобы «тихий
/// запуск» не мигал им по дороге в трей (D-088). Раньше всего остального: шаг, упавший
/// до этого, оставил бы пользователя вообще без окна.
fn window(app: &AppHandle) -> Done {
    let shown = match app.state::<AppState>().settings().launch {
        settings::Launch::Window => true,
        settings::Launch::Tray => false,
        // Ярлык — окном, вход в систему — в трей (D-129).
        settings::Launch::Smart => !crate::system::autostart::by_system(),
    };
    let Some(window) = app.get_webview_window("main") else {
        return Err("окна main нет в конфиге приложения".into());
    };
    // Запуск в трей — страница тоже невидима: иначе она опрашивала бы бэкенд до первого
    // показа окна (`system::webview`).
    crate::system::webview::set_visible(&window, shown);
    if !shown {
        return Ok(());
    }
    window.show().map_err(|why| why.to_string())
}

/// Прибираемся после падения, и ядро — первым: осиротевшее держит порт, а в TUN ещё
/// и весь трафик машины (D-059). Место обязано быть именно здесь: вторую копию приложения
/// плагин одиночного запуска гасит раньше `setup`, иначе она убила бы ядро первой.
fn sweep(_app: &AppHandle) -> Done {
    crate::core::supervisor::sweep();
    Ok(())
}

fn clock(app: &AppHandle) -> Done {
    tick::spawn(app.clone());
    Ok(())
}

/// Ядро, умершее не по нашей команде, поднимается заново (D-057).
fn watch(app: &AppHandle) -> Done {
    connect::watch(app.clone());
    Ok(())
}

/// Сеть сменилась под ногами — узлы перепроверяются сами (D-112). Рядом с надзором
/// за ядром: оба про то, что случилось без нашего ведома.
fn netwatch(app: &AppHandle) -> Done {
    crate::app::wake::watch(app.clone());
    Ok(())
}

/// Что делает пункт питания, трей не знает: действие приходит отсюда, из места сборки
/// приложения.
fn icon(app: &AppHandle) -> Done {
    tray::build(app, connect::toggle).map_err(|why| why.to_string())
}

/// Запись в `Run` из прошлой сборки получает флаг, по которому `smart` узнаёт вход
/// в систему (D-129). Без этого у включивших автозапуск раньше окно всплывало бы при входе.
fn autostart_flag(_app: &AppHandle) -> Done {
    crate::system::autostart::refresh().map_err(|why| why.to_string())
}

/// Подключаемся сами, если так велят настройки (D-088). После трея: значок должен уже
/// существовать, чтобы догнать состояние.
fn autoconnect(app: &AppHandle) -> Done {
    connect::autoconnect(app.clone());
    Ok(())
}

/// Снимок прежней настройки остаётся в файле только в одном случае: мы включили прокси
/// и не успели его вернуть. Это и есть лекарство от «прокси залип», которого боялся D-008;
/// снимка нет — шаг ничего не делает.
fn proxy(app: &AppHandle) -> Done {
    status::release_system_proxy(&app.state::<AppState>()).map_err(|why| why.to_string())
}

/// То же лекарство для брандмауэра, и оно важнее: с залипшим запретом машина остаётся
/// без интернета вовсе, и «починить» для пользователя означает просто открыть umiray
/// ещё раз (D-073).
fn unlock(app: &AppHandle) -> Done {
    status::release_kill_switch(&app.state::<AppState>()).map_err(|why| why.to_string())
}

fn sources(_app: &AppHandle) -> Done {
    crate::nodes::sources::repair_all().map_err(|why| why.to_string())
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
}
