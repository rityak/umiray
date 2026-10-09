//! mihomo как ядро клиента (D-154): его собственная обвязка — хуки запуска — и контракт.
//!
//! Процесс и API — `core/mihomo`. Здесь то, что mihomo нужно от клиента, чтобы встать:
//! собрать конфиг по применённому набору (D-071), навести псевдоним по направлению (D-056).

use std::path::PathBuf;

use crate::app::engine::{Capture, Engine, EngineState, Job};
use crate::app::lifecycle::Lifecycle;
use crate::app::lifecycle::{Hook, Phase, When};
use crate::app::state::AppState;
use crate::config::mode::Mode;
use crate::core::mihomo::download::MihomoDownload;
use crate::core::mihomo::lists::{Built, ListBuild};
use crate::core::mihomo::Mihomo;
use crate::core::process::LogRing;
use crate::error::AppError;
use crate::render::mihomo::Effective;

/// Что шаги запуска передают друг другу. Собранный конфиг кладёт `config`, забирает
/// `process` — поэтому поле, а не общее состояние.
#[derive(Default)]
pub struct Plan {
    effective: Option<Effective>,
}

/// Запуск mihomo. Порядок несущий, и каждый шаг стоит там, где стоит, по причине:
///
/// - `lists` перед конфигом: `RULE-SET` на несобранный список в конфиг не войдёт (D-157).
/// - `config` и «до»: собрать нечего — запускать нечего.
/// - `check` между сборкой и запуском: ядро говорит, что не так, **до** того, как
///   что-то поднялось (D-106) — иначе причина приходит строкой в логе упавшего запуска.
/// - `port` перед процессом: с занятым портом прокси ядро поднимается как ни в чём
///   не бывало и молча не слушает (D-133) — отказать можно только до него.
/// - `process` тоже «до»: не поднявшееся ядро отменяет всё остальное.
/// - `alias` после процесса: наводить псевдоним не на чем, пока ядро не работает (D-056).
/// - `volt-vpn` до процесса: перехват VPN обязан стоять раньше первого рукопожатия.
/// - `volt` после: Relay спрашивает имена у резолвера ядра, а его нет до ядра (D-176).
///   Отказ Relay ядро не гасит — выходы VOLT ответят ошибкой, причина уйдёт в лог.
const START: &[Hook<Plan>] = &[
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "lists",
        label: "сборка rule sets",
        run: lists,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "config",
        label: "сборка конфига",
        run: config,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "check",
        label: "сухой прогон конфига",
        run: check,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "port",
        label: "порт локального прокси",
        run: port,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "volt-vpn",
        label: "перехват прокси-серверов для VOLT",
        run: volt_vpn,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "process",
        label: "запуск ядра",
        run: process,
    },
    Hook {
        phase: Phase::Start,
        when: When::After,
        id: "alias",
        label: "наведение псевдонима",
        run: alias,
    },
    Hook {
        phase: Phase::Start,
        when: When::After,
        id: "volt",
        label: "VOLT Relay",
        run: volt_relay,
    },
];

impl Engine for Mihomo {
    fn start<'a>(&'a self, state: &'a AppState) -> Job<'a> {
        Box::pin(async move {
            let result =
                Lifecycle::run(START, Phase::Start, state, &mut Plan::default(), self.log()).await;
            if result.is_err() {
                state.volt.stop();
            }
            result
        })
    }

    fn stop(&self) -> Job<'_> {
        Box::pin(async move {
            Mihomo::stop(self);
            Ok(())
        })
    }

    fn state(&self) -> EngineState {
        let status = self.status();
        let capture = match (status.running, status.mode) {
            (true, Some(Mode::Tun)) => status.device.map(|device| Capture::Tun { device }),
            (true, Some(Mode::Local)) => status.port.map(|port| Capture::LocalProxy { port }),
            _ => None,
        };
        EngineState {
            on: status.running,
            wanted: status.wanted,
            started: status.started,
            capture,
            recovering: false,
        }
    }

    fn log(&self) -> &LogRing {
        Mihomo::log(self)
    }

    fn binary(&self) -> PathBuf {
        Mihomo::binary()
    }

    fn install(&self) -> Job<'_, String> {
        Box::pin(MihomoDownload::install())
    }

    /// Собрать `.mrs` из изменившихся списков и дать работающему ядру их перечитать.
    /// Смешанный список — два провайдера (`<id>` и `<id>@ip`), перечитываем оба: какого
    /// нет, ядро ответит 404, и это не ошибка.
    fn lists_changed(&self) -> Job<'_> {
        Box::pin(async move {
            let built = build_lists().await?;
            for failure in &built.failed {
                self.log()
                    .note("warning", &format!("список не собрался — {failure}"));
            }
            for id in built.changed {
                self.reload_rules(&id).await?;
                self.reload_rules(&format!("{id}@ip")).await?;
            }
            Ok(())
        })
    }
}

/// Сборка `.mrs` зовёт ядро процессом — секунды на большом списке (S-028), поэтому
/// не в потоке асинхронных задач.
async fn build_lists() -> crate::error::Result<Built> {
    tokio::task::spawn_blocking(ListBuild::prepare)
        .await
        .map_err(|e| AppError::io(format!("Сборка списков оборвалась: {e}")))
}

/// Списки, пришедшие, пока ядра не было, собираются перед сборкой конфига: иначе в него
/// не попадёт ни один `RULE-SET` на них (D-157). Несобравшийся список запуск не отменяет —
/// он просто не войдёт в конфиг, а причина уедет в лог.
fn lists<'a>(state: &'a AppState, _plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move {
        for failure in build_lists().await?.failed {
            state
                .mihomo
                .log()
                .note("warning", &format!("список не собрался — {failure}"));
        }
        Ok(())
    })
}

/// Конфиг собираем здесь, а не в ядре: что в него войдёт из маршрутных документов,
/// знают настройки (D-071), а дочерний процесс — только процесс.
///
/// Служебный вход под замер (D-072) заводим на свободном порту: фиксированный номер
/// однажды оказался бы занят, и ядро не поднялось бы вовсе — причём без внятной причины.
fn config<'a>(state: &'a AppState, plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move {
        plan.effective = Some(crate::render::effective::ConfigRenderer::effective(
            &state.routing.document(state)?,
            Some(crate::core::Ports::free_port()?),
            state.volt.route(&crate::config::volt::Options::get()?)?,
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
fn check<'a>(_state: &'a AppState, plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move {
        let Some(effective) = plan.effective.as_ref() else {
            return Ok(());
        };
        let said = crate::diag::config::DryRun::accepts(&effective.yaml)?;
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
fn port<'a>(state: &'a AppState, plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move {
        let Some(effective) = plan.effective.as_ref() else {
            return Ok(());
        };
        // В TUN порта нет: трафик идёт адаптером, а не через прокси.
        let Some(port) = effective.port else {
            return Ok(());
        };
        // Своё работающее ядро держит порт законно: запуск всё равно гасит его первым.
        if state.mihomo.status().running {
            return Ok(());
        }
        let address = crate::core::Ports::proxy_address(&effective.yaml, port)?;
        let Err(why) = crate::core::Ports::vacant(address) else {
            return Ok(());
        };
        let holder = crate::system::net::NetInfo::port_owner(port)
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

fn process<'a>(state: &'a AppState, plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move {
        // Не «такого не бывает»: конфиг кладёт соседняя запись реестра, и переставить их
        // местами ничего не мешает. Отказ читается, паника — нет.
        let Some(effective) = plan.effective.as_ref() else {
            return Err(AppError::invalid(
                "Конфиг не собран — запускать ядро не с чем",
            ));
        };
        state.mihomo.start(effective).await
    })
}

/// Псевдоним наводится по направлению (D-056): куда именно — знает состояние приложения,
/// а не дочерний процесс.
fn alias<'a>(state: &'a AppState, _plan: &'a mut Plan) -> Job<'a> {
    Box::pin(async move { state.routing.point_alias(state).await.map(|_| ()) })
}

fn volt_vpn<'a>(state: &'a AppState, _plan: &'a mut Plan) -> Job<'a> {
    Box::pin(crate::app::volt::prepare_vpn(state))
}

fn volt_relay<'a>(state: &'a AppState, _plan: &'a mut Plan) -> Job<'a> {
    Box::pin(crate::app::volt::start_relay(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lifecycle::tests::{ids_are_unique, phase_of};

    #[test]
    fn every_step_has_its_own_id() {
        ids_are_unique(START);
    }

    /// Порт проверяется до ядра, а не после: после поздно — оно уже «работает» (D-133).
    #[test]
    fn the_port_is_checked_before_the_core_starts() {
        let ids: Vec<_> = phase_of(START, Phase::Start).map(|hook| hook.id).collect();
        let at = |id| ids.iter().position(|seen| *seen == id).unwrap();
        assert!(at("port") < at("process"), "{ids:?}");
        assert!(START
            .iter()
            .any(|hook| hook.id == "port" && hook.when == When::Before));
    }

    /// Что прописывать в систему и что запирать, обвязка клиента узнаёт отсюда (D-154).
    #[test]
    fn a_stopped_core_captures_nothing() {
        let state = Mihomo::new().state();
        assert!(!state.on && state.capture.is_none());
    }
}
