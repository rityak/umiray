//! Подключение целиком: ядро плюс всё, что вокруг него, — и надзор за тем, чтобы оно
//! не исчезло само (D-057).
//!
//! Запуск ядра — это не только спавн процесса: псевдоним наводится по направлению (D-056),
//! адрес прописывается в систему (D-047), значок в трее догоняет. Автоподъём после падения
//! обязан делать ровно то же самое, иначе поднятое ядро окажется без выбранного узла
//! и без системного прокси — «работает», но не туда. Поэтому запуск живёт здесь целиком,
//! а команда его только зовёт (D-030).
//!
//! Из чего он состоит, здесь больше не написано: шаги лежат реестром в `app/lifecycle.rs`
//! (D-101), а отсюда фаза только запускается. Новый участник запуска — запись там,
//! и ни одной правки здесь.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app::lifecycle::{self, Ctx, Phase};
use crate::app::state::AppState;
use crate::app::status::{shown_look, status, Status};
use crate::app::tray;
use crate::error::Result;

/// Как часто спрашиваем, живо ли ядро. Опрос из окна для этого не годится: окно прячется
/// в трей (D-046), а надзор обязан работать и без него.
const TICK: Duration = Duration::from_secs(1);
/// Сколько подъёмов подряд. Больше — значит ядро не встаёт вообще: неудачный запуск
/// возвращается за доли секунды, и повторять его бесконечно значит прятать поломку.
const LIMIT: u32 = 3;
/// Сколько тактов ядро должно прожить, чтобы подъём считался удачным и счётчик попыток
/// обнулился. В тактах, а не в секундах: правило так проверяется целиком, не ожидая минуту.
const HEALTHY: u32 = 60;
/// Сколько строк прошлой жизни ядра вернуть в лог после подъёма. Запуск чистит кольцо,
/// а причина падения — именно в них.
const FAREWELL: usize = 3;

/// Поднять ядро и всё, что при нём. Отдаёт статус, каким его увидит окно.
///
/// Отказ «до»-шага уезжает наружу как есть: вариант ошибки — это кнопка, которую покажет
/// окно (D-028).
pub async fn start(app: &AppHandle, state: &AppState) -> Result<Status> {
    state.qd.shutdown().await;
    let _transition = state.transition().await;
    start_locked(app, state).await
}

async fn start_locked(app: &AppHandle, state: &AppState) -> Result<Status> {
    lifecycle::run(Phase::Start, &mut Ctx::new(state)).await?;
    Ok(shown(app, state))
}

/// Перезапустить: то же, что выключить и включить, но одним действием и без промежуточного
/// «Отключено» в окне.
///
/// Нужен там, где изменилось то, что ядро читает **на старте** (D-010): режим (D-060)
/// и направление вместе с наборами конфигов (D-064).
pub async fn restart(app: &AppHandle, state: &AppState) -> Result<Status> {
    let _transition = state.transition().await;
    lifecycle::run(Phase::Restart, &mut Ctx::new(state)).await?;
    start_locked(app, state).await
}

/// Довести правку до работающего ядра (D-102, D-143).
///
/// Перезагрузкой, если правка такая, что доедет (S-020): PID тот же. Если не доедет
/// (`tun.enable`, `mixed-port`) — перезапуском: без него выбранное просто не работает.
///
/// Изменилось хоть что-то, что ядро видит, — все открытые соединения обрываются
/// (D-143). Иначе они доживали бы по прежнему маршруту: человек переключил выход,
/// а загрузка, начатая до этого, всё ещё идёт через старый узел или мимо VPN.
///
/// Ядро не работает — ничего: файл лежит на диске и уедет следующим запуском.
pub async fn apply(app: &AppHandle, state: &AppState) -> Result<Status> {
    let _transition = state.transition().await;
    apply_locked(app, state).await
}

async fn apply_locked(app: &AppHandle, state: &AppState) -> Result<Status> {
    let Some(launched) = state.supervisor.launched() else {
        return Ok(shown(app, state));
    };
    // Порт служебного входа — у работающего ядра: свежий сделал бы конфиги разными
    // на ровном месте.
    let effective = crate::render::effective::effective(
        state.routing()?.as_deref(),
        state.supervisor.probe_port(),
    )?;
    let changed = match crate::core::apply::needed(&launched, &effective.yaml)? {
        Some(crate::core::apply::Apply::Restart(_)) => {
            // Права — до остановки: иначе TUN без них гасил бы VPN, который работал.
            crate::core::supervisor::check_privileges(
                effective.mode,
                crate::system::elevation::is_elevated(),
            )?;
            lifecycle::run(Phase::Restart, &mut Ctx::new(state)).await?;
            return start_locked(app, state).await;
        }
        Some(crate::core::apply::Apply::Reload) => {
            state.supervisor.apply(&effective).await?;
            true
        }
        None => false,
    };
    // Выбранный узел в конфиг не попадает: он — цель псевдонима, и меняется через API
    // ядра. Без этой строки нажатие по строке таблицы меняло настройку, сравнение
    // конфигов не находило разницы, и трафик продолжал идти через прежний узел.
    let moved = state.point_alias().await?;
    if changed || moved {
        state.supervisor.close_connections().await?;
    }
    Ok(shown(app, state))
}

/// Режим перехвата (D-060) — и довести его до живого ядра (D-143).
///
/// `TUN` против `Proxy` — это адаптер, и доезжает он перезапуском; `System` против
/// `Proxy` — запись в реестр, конфиг у них один. Во втором случае соединения рвём сами:
/// браузер, который ходил через системный прокси, иначе так и ходил бы через ядро.
pub async fn set_mode(
    app: &AppHandle,
    state: &AppState,
    choice: crate::app::mode::Choice,
) -> Result<Status> {
    let _transition = state.transition().await;
    crate::app::mode::set(state, choice)?;
    if !state.supervisor.status().running {
        return Ok(shown(app, state));
    }
    if choice == crate::app::mode::Choice::System {
        crate::app::status::engage_system_proxy(state)?;
    } else {
        crate::app::status::release_system_proxy(state)?;
    }
    let current = apply_locked(app, state).await?;
    // Между System и Proxy конфиг не меняется, и `apply` рвать не станет. Повторный
    // обрыв после перезагрузки ничего не стоит: соединений к этому моменту уже нет.
    state.supervisor.close_connections().await?;
    Ok(current)
}

/// Сменить направление — и довести это до живого ядра (D-064, D-102).
///
/// Порядок тот же, что везде: сначала файлы, потом ядро. Сорвалась запись — ядро осталось
/// работать по-старому, и это честнее, чем применить полуприменённый набор.
pub async fn set_direction(
    app: &AppHandle,
    state: &AppState,
    direction: crate::config::direction::Direction,
    node: Option<String>,
) -> Result<Status> {
    let _transition = state.transition().await;
    state.set_direction(direction, node)?;
    apply_locked(app, state).await
}

/// Включить или выключить встроенный набор правил (D-083) — и довести до живого ядра.
///
/// Правила доезжают перезагрузкой (S-020); открытые соединения при этом рвутся (D-143):
/// правило, которое их пустило, могло только что поменяться.
pub async fn set_ruleset(app: &AppHandle, state: &AppState, id: &str, on: bool) -> Result<Status> {
    let _transition = state.transition().await;
    crate::config::rulesets::toggle(id, on)?;
    apply_locked(app, state).await
}

/// Остановка по чужой воле: нажали кнопку питания, вышли из приложения, надзор сдался.
/// Для ядра это не падение — после неё оно само не поднимется.
///
/// Отказа не отдаёт, и это свойство самой фазы: «до»-шагов у `stop` нет (D-101), потому
/// что не погашенное ядро хуже любой причины, по которой шаг не сработал.
pub async fn stop(app: &AppHandle, state: &AppState) -> Status {
    let _transition = state.transition().await;
    stop_locked(app, state).await
}

async fn stop_locked(app: &AppHandle, state: &AppState) -> Status {
    let _ = lifecycle::run(Phase::Stop, &mut Ctx::new(state)).await;
    shown(app, state)
}

/// Keep start/restart and the tray blocked until the installer has taken over.
pub async fn stop_for_update<'a>(
    app: &AppHandle,
    state: &'a AppState,
) -> Result<tokio::sync::MutexGuard<'a, ()>> {
    let transition = state.transition().await;
    stop_locked(app, state).await;
    let settings = state.settings();
    update_safe(
        state.supervisor.status().running,
        settings.proxy_backup.is_some(),
        settings.kill_switch_backup.is_some(),
    )?;
    Ok(transition)
}

fn update_safe(running: bool, proxy_backup: bool, kill_switch_backup: bool) -> Result<()> {
    if running || proxy_backup || kill_switch_backup {
        return Err(crate::error::AppError::invalid(
            "VPN shutdown did not restore networking. The update was not installed.",
        ));
    }
    Ok(())
}

/// Питание из трея: одно действие в обе стороны, как и кнопка в шапке (D-060).
///
/// Своего окна у трея нет, поэтому об отказе рассказывает лог ядра — та же дорога,
/// по которой отчитывается надзор: это единственное место, которое видно и тогда,
/// когда окно спрятано.
pub fn toggle(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if state.settings().engine == crate::app::settings::Engine::Qd {
            if let Err(why) = qd_flip(&app, &state).await {
                state
                    .supervisor
                    .note("error", &format!("qd из трея не переключился: {why}"));
            }
            return;
        }
        if state.supervisor.status().running {
            stop(&app, &state).await;
        } else if let Err(why) = start(&app, &state).await {
            state
                .supervisor
                .note("error", &format!("запуск из трея не удался: {why}"));
        }
    });
}

/// Подключиться сразу при запуске клиента, если так велят настройки (D-088).
///
/// Что поднимать, помнить не нужно: направление лежит в настройках (D-056), режим
/// перехвата — в конфиге ядра (D-060). Отдельной задачей, а не прямо в `setup`:
/// запуск ядра ждёт ответа от него, и держать на этом открытие окна нельзя.
pub fn autoconnect(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if !state.settings().auto_connect {
            return;
        }
        if state.settings().engine == crate::app::settings::Engine::Qd {
            if let Err(why) = state.qd.call("POST", "/client/api/connect", None).await {
                state
                    .supervisor
                    .note("error", &format!("автоподключение qd не удалось: {why}"));
            }
            return;
        }
        if let Err(why) = start(&app, &state).await {
            state
                .supervisor
                .note("error", &format!("автоподключение не удалось: {why}"));
        }
    });
}

/// Надзор: ядро, умершее не по нашей команде, поднимается заново (D-057).
///
/// Отдельная задача, а не проверка внутри опроса статуса, ровно по той же причине, что
/// и у `refresher`: работать она обязана независимо от того, открыто ли окно.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut watch = Watch::default();
        loop {
            tokio::time::sleep(TICK).await;
            let state = app.state::<AppState>();
            if state.settings().engine == crate::app::settings::Engine::Qd
                && state.qd.running().await
            {
                let _ = state.qd.call("GET", "/client/api/state", None).await;
                shown(&app, &state);
            }
            match watch.step(state.supervisor.crashed()) {
                Step::Idle => continue,
                Step::GiveUp => {
                    state.supervisor.note(
                        "error",
                        &format!("ядро не встало с {LIMIT} попыток — автоподъём остановлен"),
                    );
                    // Штатная остановка, а не просто «перестали пробовать»: иначе в реестре
                    // остался бы наш адрес, а в трее — цветной значок при мёртвом ядре.
                    stop(&app, &state).await;
                }
                Step::Raise => {
                    // Хвост снимаем до подъёма: он чистит кольцо лога вместе с причиной падения.
                    let farewell = state.supervisor.tail(FAREWELL);
                    let attempt = format!("ядро упало — подъём {}/{LIMIT}", watch.attempts);
                    match recover(&app, &state).await {
                        Ok(_) => {
                            state
                                .supervisor
                                .note("error", &format!("{attempt}; перед падением:"));
                            state.supervisor.recall(farewell);
                        }
                        Err(why) => state.supervisor.note("error", &format!("{attempt}: {why}")),
                    }
                }
            }
        }
    });
}

/// Подъём после падения повторно проверяет намерение уже под общим замком. Если между
/// тактом сторожа и этой строкой нажали Off, устаревший подъём ничего не делает.
async fn recover(app: &AppHandle, state: &AppState) -> Result<Status> {
    let _transition = state.transition().await;
    if !state.supervisor.crashed() {
        return Ok(shown(app, state));
    }
    start_locked(app, state).await
}

/// Что надзору делать на этом такте.
#[derive(Debug, PartialEq)]
enum Step {
    /// Ядро живо или выключено по команде — оба случая не наше дело.
    Idle,
    Raise,
    /// Не встаёт: остановиться штатно и замолчать до следующего нажатия.
    GiveUp,
}

/// Память надзора между тактами. Отдельно от самого цикла, потому что правило здесь одно,
/// а проверить его внутри цикла нечем: тот живёт вокруг настоящего ядра.
#[derive(Default)]
struct Watch {
    /// Подъёмов подряд, ни один из которых не прожил `HEALTHY`.
    attempts: u32,
    /// Тактов подряд, что ядро не падало.
    alive: u32,
}

impl Watch {
    fn step(&mut self, crashed: bool) -> Step {
        if !crashed {
            self.alive += 1;
            if self.alive >= HEALTHY {
                self.attempts = 0;
            }
            return Step::Idle;
        }
        self.alive = 0;
        if self.attempts >= LIMIT {
            return Step::GiveUp;
        }
        self.attempts += 1;
        Step::Raise
    }
}

/// Статус плюс значок: они меняются вместе, и разъезжаться им нельзя. Значок правим сразу,
/// не дожидаясь следующего опроса: полторы секунды со старым после нажатия читаются
/// как «не сработало».
fn shown(app: &AppHandle, state: &AppState) -> Status {
    let current = status(state);
    tray::refresh(app, shown_look(state, &current));
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_require_a_stopped_core_and_restored_network() {
        assert!(update_safe(false, false, false).is_ok());
        for flags in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert!(update_safe(flags.0, flags.1, flags.2).is_err());
        }
    }

    fn quiet(watch: &mut Watch, ticks: u32) {
        for _ in 0..ticks {
            assert_eq!(watch.step(false), Step::Idle);
        }
    }

    #[test]
    fn a_core_that_did_not_fall_is_left_alone() {
        let mut watch = Watch::default();
        quiet(&mut watch, 5);
        assert_eq!(watch.attempts, 0, "живое ядро попыток не тратит");
    }

    #[test]
    fn a_core_that_never_comes_back_stops_being_raised() {
        let mut watch = Watch::default();
        for attempt in 1..=LIMIT {
            assert_eq!(watch.step(true), Step::Raise, "подъём {attempt}");
        }
        assert_eq!(watch.step(true), Step::GiveUp, "четвёртого подъёма нет");
        assert_eq!(watch.step(true), Step::GiveUp, "и дальше тоже нет");
    }

    /// Иначе редкое падение раз в неделю однажды исчерпало бы лимит и осталось без подъёма.
    #[test]
    fn a_core_that_lived_gets_its_attempts_back() {
        let mut watch = Watch::default();
        watch.step(true);
        watch.step(true);
        quiet(&mut watch, HEALTHY);
        assert_eq!(watch.attempts, 0, "минута жизни списывает попытки");
        for attempt in 1..=LIMIT {
            assert_eq!(
                watch.step(true),
                Step::Raise,
                "снова три попытки: {attempt}"
            );
        }
    }

    /// Сдались — но не навсегда: ядро выключено, такты идут, и через минуту счётчик чист.
    #[test]
    fn giving_up_is_not_forever() {
        let mut watch = Watch::default();
        for _ in 0..=LIMIT {
            watch.step(true);
        }
        quiet(&mut watch, HEALTHY);
        assert_eq!(watch.step(true), Step::Raise);
    }
}

async fn qd_flip(app: &AppHandle, state: &AppState) -> Result<()> {
    let connected = state
        .qd
        .call("GET", "/client/api/state", None)
        .await?
        .get("connected")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if connected {
        state
            .qd
            .call("POST", "/client/api/disconnect", None)
            .await?;
        shown(app, state);
        return Ok(());
    }
    if state.supervisor.status().running {
        stop(app, state).await;
    }
    state.qd.call("POST", "/client/api/connect", None).await?;
    shown(app, state);
    Ok(())
}
