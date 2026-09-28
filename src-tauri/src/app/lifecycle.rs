//! Реестр хуков и фаза `core` (D-101).
//!
//! Запуск и остановка ядра — это **список шагов**, а не последовательность вызовов внутри
//! одной функции. У шага есть `id`, своя подпись и своя неудача; порядок виден списком,
//! а не порядком `await`. Новый участник запуска — одна запись в `HOOKS`, и ни одной
//! правки в `app/connect.rs`.
//!
//! Два правила, ради которых реестр и заводился:
//!
//! - **«До» может отменить фазу, «после» — нет.** Битый конфиг обязан остановить запуск;
//!   неудача с системным прокси откатывать поднятый туннель не должна.
//! - **Изоляция отказа — свойство фазы.** У `stop` «до»-шагов нет вовсе: не погашенное
//!   ядро хуже любой причины, по которой шаг не сработал.
//!
//! Порядок задаёт сам список, а не поле `when`: в `stop` защита снимается раньше, чем
//! гаснет ядро, но отменить остановку не вправе.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use crate::app::state::AppState;
use crate::app::status::{
    engage_kill_switch, engage_system_proxy, release_kill_switch, release_system_proxy,
};
use crate::error::{AppError, Result};
use crate::render::mihomo::Effective;

/// Что происходит с ядром. `reload` появится вместе с тем, что можно перезагружать
/// (`PUT /configs`, S-019): фаза без единого участника — мёртвый код, а не задел.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Start,
    /// Остановить только процесс перед заменой, сохранив системный прокси и запрет.
    /// Полная `Stop` означает волю пользователя выключиться и снимает оба.
    Restart,
    Stop,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Restart => "restart",
            Self::Stop => "stop",
        }
    }
}

/// Где шаг стоит относительно работающего ядра — и, что важнее, чем оборачивается его
/// отказ: «до» отменяет фазу, «после» только пишется в лог (D-101).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum When {
    Before,
    After,
}

/// Что фаза передаёт от шага к шагу. Не «всё приложение»: шаг, которому дали `AppHandle`,
/// рано или поздно начнёт трогать окно — а об исходе фазы окну расскажет тот, кто её звал.
pub struct Ctx<'a> {
    state: &'a AppState,
    /// Собранный конфиг: кладёт `config`, забирает `process`. Единственное, что шаги
    /// передают друг другу, — поэтому поле, а не общее состояние.
    effective: Option<Effective>,
}

impl<'a> Ctx<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
            effective: None,
        }
    }
}

type Step<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type Run = for<'a, 'b> fn(&'a mut Ctx<'b>) -> Step<'a>;

struct Hook {
    phase: Phase,
    when: When,
    /// Кем шаг назван в логе. Уникален внутри своей фазы — это проверяется тестом,
    /// а не в рантайме: реестр константный, и ошибка в нём обязана падать сборкой тестов.
    id: &'static str,
    /// Чем шаг назвать человеку, когда он не удался. `id` для этого не годится:
    /// в строке отказа читают причину, а не имя записи.
    label: &'static str,
    run: Run,
}

/// Реестр. Новый шаг — одна запись здесь.
///
/// Порядок несущий, и каждый шаг стоит там, где стоит, по причине:
///
/// - `config` первым и «до»: собрать нечего — запускать нечего.
/// - `check` между сборкой и запуском: ядро говорит, что не так, **до** того, как
///   что-то поднялось (D-106) — иначе причина приходит строкой в логе упавшего запуска.
/// - `port` перед процессом: с занятым портом прокси ядро поднимается как ни в чём
///   не бывало и молча не слушает (D-133) — отказать можно только до него.
/// - `process` тоже «до»: не поднявшееся ядро отменяет всё остальное.
/// - `alias` после процесса: наводить псевдоним не на чем, пока ядро не работает (D-056).
/// - `system-proxy` после псевдонима: в систему прописывается адрес рабочего ядра (D-047).
/// - `kill-switch` последним: запереть выход раньше, чем туннель начал работать, значит
///   на секунду отнять сеть у самого себя (D-073).
///
/// В `stop` порядок обратный и «до»-шагов нет: сначала открыть выход, потом гасить ядро —
/// иначе машина осталась бы запертой на время остановки.
const HOOKS: &[Hook] = &[
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "config",
        label: "сборка конфига",
        run: start_config,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "check",
        label: "сухой прогон конфига",
        run: start_check,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "port",
        label: "порт локального прокси",
        run: start_port,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "process",
        label: "запуск ядра",
        run: start_process,
    },
    Hook {
        phase: Phase::Start,
        when: When::After,
        id: "alias",
        label: "наведение псевдонима",
        run: start_alias,
    },
    Hook {
        phase: Phase::Start,
        when: When::After,
        id: "system-proxy",
        label: "системный прокси",
        run: start_proxy,
    },
    Hook {
        phase: Phase::Start,
        when: When::After,
        id: "kill-switch",
        label: "запрет выхода мимо туннеля",
        run: start_lock,
    },
    Hook {
        phase: Phase::Restart,
        when: When::After,
        id: "process",
        label: "остановка ядра перед заменой",
        run: stop_process,
    },
    Hook {
        phase: Phase::Stop,
        when: When::After,
        id: "kill-switch",
        label: "снятие запрета",
        run: stop_lock,
    },
    Hook {
        phase: Phase::Stop,
        when: When::After,
        id: "system-proxy",
        label: "возврат системного прокси",
        run: stop_proxy,
    },
    Hook {
        phase: Phase::Stop,
        when: When::After,
        id: "process",
        label: "остановка ядра",
        run: stop_process,
    },
];

/// Пройти фазу. Отказ «до»-шага останавливает проход и уезжает наружу **как есть**:
/// вариант ошибки — это кнопка, которую покажет окно (D-028), и подменять его нельзя.
pub async fn run(phase: Phase, ctx: &mut Ctx<'_>) -> Result<()> {
    let mut journal = Vec::new();
    let mut cancelled = None;
    for hook in HOOKS.iter().filter(|hook| hook.phase == phase) {
        let began = Instant::now();
        let outcome = (hook.run)(ctx).await;
        journal.push(line(
            phase.name(),
            hook.id,
            hook.label,
            began.elapsed(),
            &outcome,
        ));
        if cancels(hook.when, &outcome) {
            cancelled = outcome.err();
            break;
        }
    }
    // Пишем весь проход разом и после него, а не по ходу: запуск ядра чистит кольцо лога,
    // и строки, написанные до него, исчезли бы вместе с прошлой жизнью ядра.
    for (level, text) in journal {
        ctx.state.supervisor.note(level, &text);
    }
    match cancelled {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

/// Правило «до отменяет, после — нет» целиком. Отдельной функцией, потому что проверить
/// его внутри прохода нечем: тот живёт вокруг настоящего ядра.
fn cancels(when: When, outcome: &Result<()>) -> bool {
    outcome.is_err() && when == When::Before
}

/// Строка прогона: фаза, `id`, сколько заняло, исход. Без неё реестр хуже хардкода —
/// порядок не виден, а упавший шаг молчит.
///
/// Общая на все фазы, поэтому берёт имя фазы, а не саму фазу: у `tick` свой реестр
/// со своей записью (период вместо «до/после»), а строка в логе обязана быть одна и та же.
pub fn line<E: std::fmt::Display>(
    phase: &str,
    id: &str,
    label: &str,
    took: Duration,
    outcome: &std::result::Result<(), E>,
) -> (&'static str, String) {
    let head = format!("{phase} · {id} · {} мс", took.as_millis());
    match outcome {
        Ok(()) => ("info", format!("{head} · ок")),
        Err(why) => ("error", format!("{head} · отказ — {label}: {why}")),
    }
}

/// Конфиг собираем здесь, а не в супервизоре: что в него войдёт из маршрутных документов,
/// знают настройки (D-071), а дочерний процесс — только процесс.
///
/// Служебный вход под замер (D-072) заводим на свободном порту: фиксированный номер
/// однажды оказался бы занят, и ядро не поднялось бы вовсе — причём без внятной причины.
fn start_config<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        ctx.effective = Some(crate::render::effective::effective(
            ctx.state.routing()?.as_deref(),
            Some(crate::core::free_port()?),
        )?);
        Ok(())
    })
}

/// Показать собранный конфиг ядру, не запуская VPN (D-106).
///
/// Проверяем **тот самый** текст, который сейчас уедет в запуск, а не собранный заново:
/// вторая сборка — это второй конфиг, и проверка отвечала бы не про него.
///
/// Отказ — `CoreFailed`: у него та же кнопка, что у неудачного запуска (D-028), и та же
/// природа — ядро сказало, что не может это читать. Разница в том, что сказало оно
/// до, а не после.
fn start_check<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        let Some(effective) = ctx.effective.as_ref() else {
            return Ok(());
        };
        let said = crate::diag::config::accepts(&effective.yaml)?;
        if said.ok {
            return Ok(());
        }
        Err(AppError::CoreFailed {
            message: format!("Ядро не приняло собранный конфиг: {}", said.complaint()),
            log: said.lines,
        })
    })
}

/// Порт локального прокси свободен (D-133).
///
/// Замерено: mihomo с занятым `mixed-port` не падает — пишет в лог «Start Mixed server
/// error» и работает дальше. API отвечает, готовность есть, окно пишет «Подключено»,
/// а прокси не слушает, и браузер уходит в чужую программу на том же порту.
///
/// Отказ — `CoreFailed`, как и у сухого прогона: ядро здесь не виновато, но кнопки
/// у этой беды нет, а причина от системы едет строкой рядом.
fn start_port<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        let Some(effective) = ctx.effective.as_ref() else {
            return Ok(());
        };
        // В TUN порта нет: трафик идёт адаптером, а не через прокси.
        let Some(port) = effective.port else {
            return Ok(());
        };
        // Своё работающее ядро держит порт законно: запуск всё равно гасит его первым.
        if ctx.state.supervisor.status().running {
            return Ok(());
        }
        let address = crate::core::proxy_address(&effective.yaml, port)?;
        let Err(why) = crate::core::vacant(address) else {
            return Ok(());
        };
        let holder = crate::system::net::port_owner(port)
            .map(|who| format!(" — его держит {who}"))
            .unwrap_or_default();
        Err(AppError::CoreFailed {
            message: format!(
                "Порт {port} уже занят{holder}. Закройте ту программу или смените порт \
                 в «Настройки» → «Настройки mihomo» → «Локальный прокси»."
            ),
            log: vec![format!("{address}: {why}")],
        })
    })
}

fn start_process<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        // Не «такого не бывает»: конфиг кладёт соседняя запись реестра, и переставить их
        // местами ничего не мешает. Отказ читается, паника — нет.
        let Some(effective) = ctx.effective.as_ref() else {
            return Err(AppError::invalid(
                "Конфиг не собран — запускать ядро не с чем",
            ));
        };
        ctx.state.supervisor.start(effective).await
    })
}

/// Псевдоним наводится по направлению (D-056): куда именно — знает состояние приложения,
/// а не дочерний процесс.
fn start_alias<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move { ctx.state.point_alias().await.map(|_| ()) })
}

fn start_proxy<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        if ctx.state.settings().system_proxy {
            engage_system_proxy(ctx.state)?;
        }
        Ok(())
    })
}

fn start_lock<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move { engage_kill_switch(ctx.state) })
}

fn stop_lock<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move { release_kill_switch(ctx.state) })
}

fn stop_proxy<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move { release_system_proxy(ctx.state) })
}

fn stop_process<'a, 'b>(ctx: &'a mut Ctx<'b>) -> Step<'a> {
    Box::pin(async move {
        ctx.state.supervisor.stop();
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase(phase: Phase) -> impl Iterator<Item = &'static Hook> {
        HOOKS.iter().filter(move |hook| hook.phase == phase)
    }

    /// Повторный `id` — единственная ошибка регистрации, которую не ловит компилятор:
    /// шага без фазы не бывает, неизвестной фазы не бывает тоже — это enum, а не строка.
    /// Между фазами `id` повторяться вправе: в логе он стоит рядом с именем фазы.
    #[test]
    fn every_step_has_its_own_id_within_its_phase() {
        for one in [Phase::Start, Phase::Restart, Phase::Stop] {
            let mut seen = Vec::new();
            for hook in phase(one) {
                assert!(
                    !seen.contains(&hook.id),
                    "{}: два шага с id {}",
                    one.name(),
                    hook.id
                );
                seen.push(hook.id);
            }
            assert!(!seen.is_empty(), "{}: фаза без единого шага", one.name());
        }
    }

    #[test]
    fn only_a_before_step_cancels_its_phase() {
        let bad = || Err(AppError::invalid("не вышло"));
        assert!(cancels(When::Before, &bad()), "«до» отменяет фазу");
        assert!(!cancels(When::After, &bad()), "«после» — нет");
        assert!(!cancels(When::Before, &Ok(())));
        assert!(!cancels(When::After, &Ok(())));
    }

    /// Порт проверяется до ядра, а не после: после поздно — оно уже «работает» (D-133).
    #[test]
    fn the_port_is_checked_before_the_core_starts() {
        let ids: Vec<_> = phase(Phase::Start).map(|hook| hook.id).collect();
        let at = |id| ids.iter().position(|seen| *seen == id).unwrap();
        assert!(at("port") < at("process"), "{ids:?}");
        assert!(phase(Phase::Start).any(|hook| hook.id == "port" && hook.when == When::Before));
    }

    /// Не погашенное ядро хуже любой причины, по которой шаг не сработал: ни прокси,
    /// ни брандмауэр отменить остановку не вправе.
    #[test]
    fn a_stop_cannot_be_cancelled() {
        assert!(phase(Phase::Stop).all(|hook| hook.when == When::After));
        assert!(phase(Phase::Restart).all(|hook| hook.when == When::After));
    }

    #[test]
    fn a_restart_stops_only_the_process() {
        assert_eq!(
            phase(Phase::Restart)
                .map(|hook| hook.id)
                .collect::<Vec<_>>(),
            ["process"]
        );
    }

    /// По строке видно всё, ради чего журнал заводился: какая фаза, какой шаг, сколько
    /// занял, чем кончился — и, если не вышло, почему.
    #[test]
    fn the_journal_line_names_the_step_and_its_reason() {
        let hook = phase(Phase::Start).next().unwrap();
        let (level, text) = line(
            Phase::Start.name(),
            hook.id,
            hook.label,
            Duration::from_millis(12),
            &Result::Ok(()),
        );
        assert_eq!(level, "info");
        assert_eq!(text, "start · config · 12 мс · ок");

        let (level, text) = line(
            Phase::Start.name(),
            hook.id,
            hook.label,
            Duration::from_millis(3),
            &Err(AppError::invalid("битый YAML")),
        );
        assert_eq!(level, "error");
        assert_eq!(
            text,
            "start · config · 3 мс · отказ — сборка конфига: битый YAML"
        );
    }
}
