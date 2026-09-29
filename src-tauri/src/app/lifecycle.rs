//! Реестр хуков и фаза `core` (D-101).
//!
//! Запуск и остановка ядра — это **список шагов**, а не последовательность вызовов внутри
//! одной функции. У шага есть `id`, своя подпись и своя неудача; порядок виден списком,
//! а не порядком `await`. Новый участник запуска — одна запись в реестре, и ни одной
//! правки в `app/connect.rs`.
//!
//! Реестров два уровня (D-154). Здесь — обвязка **клиента**, одна на все ядра: поднять
//! ядро, прописать прокси, запереть выход. Само ядро поднимается своим списком
//! (`app/mihomo.rs`, `app/qd.rs`), и клиент видит его одним шагом `engine`.
//!
//! Два правила, ради которых реестр и заводился, действуют на обоих уровнях:
//!
//! - **«До» может отменить фазу, «после» — нет.** Битый конфиг обязан остановить запуск;
//!   неудача с системным прокси откатывать поднятый туннель не должна.
//! - **Изоляция отказа — свойство фазы.** У `stop` «до»-шагов нет вовсе: не погашенное
//!   ядро хуже любой причины, по которой шаг не сработал.
//!
//! Порядок задаёт сам список, а не поле `when`: в `stop` защита снимается раньше, чем
//! гаснет ядро, но отменить остановку не вправе.

use std::time::{Duration, Instant};

use crate::app::engine::Job;
use crate::app::state::AppState;
use crate::core::process::LogRing;
use crate::core::EngineId;
use crate::error::Result;

/// Что происходит с ядром. `reload` появится вместе с тем, что можно перезагружать
/// (`PUT /configs`, S-019): фаза без единого участника — мёртвый код, а не задел.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Start,
    /// Остановить только ядро перед заменой, сохранив системный прокси и запрет.
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
pub enum When {
    Before,
    After,
}

/// Шаг. `C` — что шаги одного реестра передают друг другу: клиенту — какое ядро,
/// mihomo — собранный конфиг. Состояние приложения идёт отдельно и не хранится в `C`:
/// так реестр остаётся константой, а не зависит от времени жизни `AppState`.
pub struct Hook<C> {
    pub phase: Phase,
    pub when: When,
    /// Кем шаг назван в логе. Уникален внутри своей фазы — это проверяется тестом,
    /// а не в рантайме: реестр константный, и ошибка в нём обязана падать сборкой тестов.
    pub id: &'static str,
    /// Чем шаг назвать человеку, когда он не удался. `id` для этого не годится:
    /// в строке отказа читают причину, а не имя записи.
    pub label: &'static str,
    pub run: for<'a> fn(&'a AppState, &'a mut C) -> Job<'a>,
}

/// Обвязка клиента. Новый шаг вокруг любого ядра — одна запись здесь.
///
/// - `engine` первым и «до»: не вставшее ядро отменяет всё остальное. Что внутри —
///   конфиг, порт, процесс, псевдоним у mihomo — дело самого ядра.
/// - `system-proxy` после ядра: в систему прописывается адрес работающего (D-047).
/// - `kill-switch` последним: запереть выход раньше, чем туннель начал работать, значит
///   на секунду отнять сеть у самого себя (D-073).
///
/// В `stop` порядок обратный и «до»-шагов нет: сначала открыть выход, потом гасить ядро —
/// иначе машина осталась бы запертой на время остановки.
const HOOKS: &[Hook<EngineId>] = &[
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "engine",
        label: "запуск ядра",
        run: start_engine,
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
        id: "engine",
        label: "остановка ядра перед заменой",
        run: stop_engine,
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
        id: "engine",
        label: "остановка ядра",
        run: stop_engine,
    },
];

pub struct Lifecycle;

impl Lifecycle {
    /// Пройти фазу клиента над одним ядром. Журнал уходит в лог **этого** ядра: там его
    /// и будут читать, когда оно не встанет.
    pub async fn phase(phase: Phase, state: &AppState, mut engine: EngineId) -> Result<()> {
        let log = state.engine(engine).log();
        Lifecycle::run(HOOKS, phase, state, &mut engine, log).await
    }

    /// Пройти фазу реестра. Отказ «до»-шага останавливает проход и уезжает наружу **как есть**:
    /// вариант ошибки — это кнопка, которую покажет окно (D-028), и подменять его нельзя.
    pub async fn run<C: Send>(
        hooks: &[Hook<C>],
        phase: Phase,
        state: &AppState,
        ctx: &mut C,
        log: &LogRing,
    ) -> Result<()> {
        let mut journal = Vec::new();
        let mut cancelled = None;
        for hook in hooks.iter().filter(|hook| hook.phase == phase) {
            let began = Instant::now();
            let outcome = (hook.run)(state, ctx).await;
            journal.push(Lifecycle::line(
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
            log.note(level, &text);
        }
        match cancelled {
            Some(why) => Err(why),
            None => Ok(()),
        }
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
}

/// Правило «до отменяет, после — нет» целиком. Отдельной функцией, потому что проверить
/// его внутри прохода нечем: тот живёт вокруг настоящего ядра.
fn cancels(when: When, outcome: &Result<()>) -> bool {
    outcome.is_err() && when == When::Before
}

fn start_engine<'a>(state: &'a AppState, engine: &'a mut EngineId) -> Job<'a> {
    state.engine(*engine).start(state)
}

fn stop_engine<'a>(state: &'a AppState, engine: &'a mut EngineId) -> Job<'a> {
    state.engine(*engine).stop()
}

fn start_proxy<'a>(state: &'a AppState, engine: &'a mut EngineId) -> Job<'a> {
    let engine = *engine;
    Box::pin(async move {
        if state.settings.get().system_proxy {
            state.proxy.engage(state, state.engine(engine))?;
        }
        Ok(())
    })
}

fn start_lock<'a>(state: &'a AppState, engine: &'a mut EngineId) -> Job<'a> {
    let engine = *engine;
    Box::pin(async move { state.kill_switch.engage(state, state.engine(engine)) })
}

fn stop_lock<'a>(state: &'a AppState, _engine: &'a mut EngineId) -> Job<'a> {
    Box::pin(async move { state.kill_switch.release(state) })
}

fn stop_proxy<'a>(state: &'a AppState, _engine: &'a mut EngineId) -> Job<'a> {
    Box::pin(async move { state.proxy.release(state) })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn phase_of<C>(
        hooks: &'static [Hook<C>],
        phase: Phase,
    ) -> impl Iterator<Item = &'static Hook<C>> {
        hooks.iter().filter(move |hook| hook.phase == phase)
    }

    /// Повторный `id` — единственная ошибка регистрации, которую не ловит компилятор:
    /// шага без фазы не бывает, неизвестной фазы не бывает тоже — это enum, а не строка.
    /// Между фазами `id` повторяться вправе: в логе он стоит рядом с именем фазы.
    pub(crate) fn ids_are_unique<C>(hooks: &'static [Hook<C>]) {
        for one in [Phase::Start, Phase::Restart, Phase::Stop] {
            let mut seen = Vec::new();
            for hook in phase_of(hooks, one) {
                assert!(
                    !seen.contains(&hook.id),
                    "{}: два шага с id {}",
                    one.name(),
                    hook.id
                );
                seen.push(hook.id);
            }
        }
    }

    #[test]
    fn every_client_step_has_its_own_id_within_its_phase() {
        ids_are_unique(HOOKS);
        for one in [Phase::Start, Phase::Restart, Phase::Stop] {
            assert!(
                phase_of(HOOKS, one).next().is_some(),
                "{}: фаза без единого шага",
                one.name()
            );
        }
    }

    /// Обвязка клиента не знает, из чего ядро поднимается: ни одного шага mihomo здесь.
    #[test]
    fn the_client_sees_the_engine_as_one_step() {
        assert_eq!(
            phase_of(HOOKS, Phase::Start)
                .map(|hook| hook.id)
                .collect::<Vec<_>>(),
            ["engine", "system-proxy", "kill-switch"]
        );
        assert!(phase_of(HOOKS, Phase::Start)
            .any(|hook| hook.id == "engine" && hook.when == When::Before));
    }

    #[test]
    fn only_a_before_step_cancels_its_phase() {
        let bad = || Err(crate::error::AppError::invalid("не вышло"));
        assert!(cancels(When::Before, &bad()), "«до» отменяет фазу");
        assert!(!cancels(When::After, &bad()), "«после» — нет");
        assert!(!cancels(When::Before, &Ok(())));
        assert!(!cancels(When::After, &Ok(())));
    }

    /// Не погашенное ядро хуже любой причины, по которой шаг не сработал: ни прокси,
    /// ни брандмауэр отменить остановку не вправе.
    #[test]
    fn a_stop_cannot_be_cancelled() {
        assert!(phase_of(HOOKS, Phase::Stop).all(|hook| hook.when == When::After));
        assert!(phase_of(HOOKS, Phase::Restart).all(|hook| hook.when == When::After));
    }

    #[test]
    fn a_restart_stops_only_the_engine() {
        assert_eq!(
            phase_of(HOOKS, Phase::Restart)
                .map(|hook| hook.id)
                .collect::<Vec<_>>(),
            ["engine"]
        );
    }

    /// По строке видно всё, ради чего журнал заводился: какая фаза, какой шаг, сколько
    /// занял, чем кончился — и, если не вышло, почему.
    #[test]
    fn the_journal_line_names_the_step_and_its_reason() {
        let (level, text) = Lifecycle::line(
            Phase::Start.name(),
            "config",
            "сборка конфига",
            Duration::from_millis(12),
            &Result::Ok(()),
        );
        assert_eq!(level, "info");
        assert_eq!(text, "start · config · 12 мс · ок");

        let (level, text) = Lifecycle::line(
            Phase::Start.name(),
            "config",
            "сборка конфига",
            Duration::from_millis(3),
            &Err(crate::error::AppError::invalid("битый YAML")),
        );
        assert_eq!(level, "error");
        assert_eq!(
            text,
            "start · config · 3 мс · отказ — сборка конфига: битый YAML"
        );
    }
}
