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
//!
//! Ядер несколько, трафик держит одно, и следит за этим клиент, а не ядра (D-154): перед
//! подъёмом выбранного остальные гасятся штатно, под тем же замком перехода (D-134).

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::app::lifecycle::Lifecycle;
use crate::app::lifecycle::Phase;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::app::tray::Tray;
use crate::core::EngineId;
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

/// Подключение (D-155). Владеет замком перехода: один переход за раз, и замок охватывает
/// не только процесс, но и обвязку вокруг него — иначе параллельные запуск и остановка
/// успевают рассинхронизировать ядро, системный прокси и брандмауэр (D-134).
#[derive(Default)]
pub struct Connection {
    transition: tokio::sync::Mutex<()>,
}

pub type Transition<'a> = tokio::sync::MutexGuard<'a, ()>;

impl Connection {
    /// Замок перехода — для тех, кто меняет то же, что и переход: брандмауэр, файл ядра,
    /// каталог данных при сбросе.
    pub async fn lock(&self) -> Transition<'_> {
        self.transition.lock().await
    }

    /// Поднять выбранное ядро и всё, что при нём. Отдаёт статус, каким его увидит окно.
    ///
    /// Отказ «до»-шага уезжает наружу как есть: вариант ошибки — это кнопка, которую покажет
    /// окно (D-028).
    pub async fn start(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        let _transition = self.lock().await;
        self.start_locked(app, state, state.settings.get().engine)
            .await
    }

    pub(crate) async fn start_locked(
        &self,
        app: &AppHandle,
        state: &AppState,
        engine: EngineId,
    ) -> Result<Status> {
        for other in others(&state.busy(), engine) {
            let _ = Lifecycle::phase(Phase::Stop, state, other).await;
        }
        Lifecycle::phase(Phase::Start, state, engine).await?;
        Ok(self.shown(app, state))
    }

    /// Перезапустить: то же, что выключить и включить, но одним действием и без промежуточного
    /// «Отключено» в окне. Перезапускается работающее ядро, а не выбранное в шапке.
    ///
    /// Нужен там, где изменилось то, что ядро читает **на старте** (D-010): режим (D-060)
    /// и направление вместе с наборами конфигов (D-064).
    pub async fn restart(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        let _transition = self.lock().await;
        let engine = state
            .running()
            .map_or(state.settings.get().engine, |(id, _)| id);
        self.restart_locked(app, state, engine).await
    }

    async fn restart_locked(
        &self,
        app: &AppHandle,
        state: &AppState,
        engine: EngineId,
    ) -> Result<Status> {
        Lifecycle::phase(Phase::Restart, state, engine).await?;
        self.start_locked(app, state, engine).await
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
    pub async fn apply(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        self.change(app, state, || Ok(())).await
    }

    /// Изменить то, что читает mihomo, — и довести до живого ядра. Одно действие под одним
    /// замком: правка документа, направления, встроенного набора, UDP или источника. Порядок
    /// тот же везде — сначала файлы, потом ядро: сорвалась запись — ядро осталось работать
    /// по-старому, и это честнее, чем применить полуприменённое.
    pub async fn change(
        &self,
        app: &AppHandle,
        state: &AppState,
        change: impl FnOnce() -> Result<()>,
    ) -> Result<Status> {
        let _transition = self.lock().await;
        change()?;
        self.apply_locked(app, state).await
    }

    /// Помощники VOLT обходят перезагрузку с двух сторон (D-177): нужные поднимаются до неё —
    /// ядро направит в них трафик, — ненужные гаснут после, когда маршрутов к ним уже нет.
    /// Отказ помощника здесь не останавливает правку чужого документа: он уходит в лог,
    /// а строгий ответ даёт окно VOLT (`app::volt::update`).
    pub(crate) async fn apply_locked(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        if let Err(why) = crate::app::volt::prepare(state).await {
            state.note("error", &format!("VOLT не поднялся: {why}"));
        }
        let status = self.reload_locked(app, state).await?;
        crate::app::volt::settle(state)?;
        Ok(status)
    }

    async fn reload_locked(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        let Some(launched) = state.mihomo.launched() else {
            return Ok(self.shown(app, state));
        };
        // Порт служебного входа — у работающего ядра: свежий сделал бы конфиги разными
        // на ровном месте.
        let effective = crate::render::effective::ConfigRenderer::effective(
            &state.routing.document(state)?,
            state.mihomo.probe_port(),
            state.volt.route(&crate::config::volt::Options::get()?)?,
        )?;
        let changed = match crate::core::mihomo::apply::Apply::needed(&launched, &effective.yaml)? {
            Some(crate::core::mihomo::apply::Apply::Restart(_)) => {
                // Права — до остановки: иначе TUN без них гасил бы VPN, который работал.
                crate::core::mihomo::Mihomo::check_privileges(
                    effective.mode,
                    crate::system::elevation::Elevation::is_elevated(),
                )?;
                return self.restart_locked(app, state, EngineId::Mihomo).await;
            }
            Some(crate::core::mihomo::apply::Apply::Reload) => {
                state.mihomo.apply(&effective).await?;
                true
            }
            None => false,
        };
        // Выбранный узел в конфиг не попадает: он — цель псевдонима, и меняется через API
        // ядра. Без этой строки нажатие по строке таблицы меняло настройку, сравнение
        // конфигов не находило разницы, и трафик продолжал идти через прежний узел.
        let moved = state.routing.point_alias(state).await?;
        if changed || moved {
            state.mihomo.close_connections().await?;
        }
        Ok(self.shown(app, state))
    }

    /// Режим перехвата (D-060) — и довести его до живого ядра (D-143).
    ///
    /// `TUN` против `Proxy` — это адаптер, и доезжает он перезапуском; `System` против
    /// `Proxy` — запись в реестр, конфиг у них один. Во втором случае соединения рвём сами:
    /// браузер, который ходил через системный прокси, иначе так и ходил бы через ядро.
    pub async fn set_mode(
        &self,
        app: &AppHandle,
        state: &AppState,
        choice: crate::app::mode::Choice,
    ) -> Result<Status> {
        let _transition = self.lock().await;
        let running = state.mihomo.status().running;
        // Права — до записи режима и до снятия системного прокси (B-041): отказ на полпути
        // оставлял ядро в Proxy без прокси в Windows, TUN в конфиге и зелёное окно —
        // браузер ходил мимо VPN. Остановленному ядру права не нужны: их спросит запуск.
        if running {
            crate::core::mihomo::Mihomo::check_privileges(
                choice.core(),
                crate::system::elevation::Elevation::is_elevated(),
            )?;
        }
        crate::app::mode::Choice::set(state, choice)?;
        if !running {
            return Ok(self.shown(app, state));
        }
        if choice == crate::app::mode::Choice::System {
            state.proxy.engage(state, &state.mihomo)?;
        } else {
            state.proxy.release(state)?;
        }
        let current = self.apply_locked(app, state).await?;
        // Между System и Proxy конфиг не меняется, и `apply` рвать не станет. Повторный
        // обрыв после перезагрузки ничего не стоит: соединений к этому моменту уже нет.
        state.mihomo.close_connections().await?;
        Ok(current)
    }

    /// Остановка по чужой воле: нажали кнопку питания, вышли из приложения, надзор сдался.
    /// Для ядра это не падение — после неё оно само не поднимется.
    ///
    /// Гасятся все, кто держит трафик или должен. Никого — всё равно снять обвязку
    /// выбранного: залипший прокси или запрет снимаются тем же нажатием.
    ///
    /// Отказа не отдаёт, и это свойство самой фазы: «до»-шагов у `stop` нет (D-101), потому
    /// что не погашенное ядро хуже любой причины, по которой шаг не сработал.
    pub async fn stop(&self, app: &AppHandle, state: &AppState) -> Status {
        let _transition = self.lock().await;
        self.stop_locked(app, state).await
    }

    pub(crate) async fn stop_locked(&self, app: &AppHandle, state: &AppState) -> Status {
        let mut busy = state.busy();
        if busy.is_empty() {
            busy.push(state.settings.get().engine);
        }
        for engine in busy {
            let _ = Lifecycle::phase(Phase::Stop, state, engine).await;
        }
        self.shown(app, state)
    }

    /// Keep start/restart and the tray blocked until the installer has taken over.
    pub async fn stop_for_update<'a>(
        &'a self,
        app: &AppHandle,
        state: &AppState,
    ) -> Result<Transition<'a>> {
        let transition = self.lock().await;
        self.stop_locked(app, state).await;
        let settings = state.settings.get();
        update_safe(
            !state.busy().is_empty(),
            settings.proxy_backup.is_some(),
            settings.kill_switch_backup.is_some(),
        )?;
        Ok(transition)
    }

    /// Скачать ядро. Отдаёт версию. Пока ядро держит трафик, файл занят — сначала отключаемся.
    pub async fn install(&self, state: &AppState, engine: EngineId) -> Result<String> {
        let _transition = self.lock().await;
        let engine = state.engine(engine);
        if engine.state().on {
            return Err(crate::error::AppError::invalid(
                "Сначала отключитесь: работающее ядро нельзя заменить",
            ));
        }
        engine.install().await
    }

    /// Статус плюс значок: они меняются вместе, и разъезжаться им нельзя. Значок правим сразу,
    /// не дожидаясь следующего опроса: полторы секунды со старым после нажатия читаются
    /// как «не сработало».
    pub fn shown(&self, app: &AppHandle, state: &AppState) -> Status {
        let current = Status::gather(state);
        Tray::refresh(app, current.look());
        current
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
            if state.running().is_some() {
                state.connection.stop(&app, &state).await;
            } else if let Err(why) = state.connection.start(&app, &state).await {
                state.note("error", &format!("запуск из трея не удался: {why}"));
            }
        });
    }

    /// Подключиться сразу при запуске клиента, если так велят настройки (D-088).
    ///
    /// Что поднимать, помнить не нужно: ядро и направление лежат в настройках (D-056, D-154),
    /// режим перехвата — в конфиге ядра (D-060). Отдельной задачей, а не прямо в `setup`:
    /// запуск ядра ждёт ответа от него, и держать на этом открытие окна нельзя.
    pub fn autoconnect(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            if !state.settings.get().auto_connect {
                return;
            }
            if let Err(why) = state.connection.start(&app, &state).await {
                state.note("error", &format!("автоподключение не удалось: {why}"));
            }
        });
    }

    /// Надзор: ядро, умершее не по нашей команде, поднимается заново (D-057) — любое (D-154).
    ///
    /// Отдельная задача, а не проверка внутри опроса статуса, ровно по той же причине, что
    /// и у `refresher`: работать она обязана независимо от того, открыто ли окно.
    pub fn watch(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            let mut watch = Watch::default();
            let mut volt = Watch::default();
            let mut seen = Vec::new();
            loop {
                tokio::time::sleep(TICK).await;
                let state = app.state::<AppState>();
                let connection = &state.connection;
                for engine in EngineId::ALL {
                    let _ = state.engine(engine).refresh().await;
                }
                revive_volt(&state, &mut volt).await;
                let now = EngineId::ALL.map(|engine| (engine, state.engine(engine).state()));
                // Ядро сменило состояние само — значок догоняет, даже если окно спрятано.
                let on: Vec<_> = now.iter().map(|(_, engine)| engine.on).collect();
                if on != seen {
                    connection.shown(&app, &state);
                    seen = on;
                }
                let crashed = now
                    .iter()
                    .find(|(_, engine)| engine.crashed())
                    .map(|(id, _)| *id);
                match (watch.step(crashed.is_some()), crashed) {
                    (Step::Idle, _) | (_, None) => continue,
                    (Step::GiveUp, Some(engine)) => {
                        state.engine(engine).log().note(
                            "error",
                            &format!("ядро не встало с {LIMIT} попыток — автоподъём остановлен"),
                        );
                        // Штатная остановка, а не просто «перестали пробовать»: иначе в реестре
                        // остался бы наш адрес, а в трее — цветной значок при мёртвом ядре.
                        connection.stop(&app, &state).await;
                    }
                    (Step::Raise, Some(engine)) => {
                        let log = state.engine(engine).log();
                        // Хвост снимаем до подъёма: он чистит кольцо лога вместе с причиной
                        // падения.
                        let farewell = log.tail(FAREWELL);
                        let attempt = format!("ядро упало — подъём {}/{LIMIT}", watch.attempts);
                        match connection.recover(&app, &state, engine).await {
                            Ok(_) => {
                                log.note("error", &format!("{attempt}; перед падением:"));
                                log.recall(farewell);
                            }
                            Err(why) => log.note("error", &format!("{attempt}: {why}")),
                        }
                    }
                }
            }
        });
    }

    /// Подъём после падения повторно проверяет намерение уже под общим замком. Если между
    /// тактом сторожа и этой строкой нажали Off, устаревший подъём ничего не делает.
    async fn recover(&self, app: &AppHandle, state: &AppState, engine: EngineId) -> Result<Status> {
        let _transition = self.lock().await;
        if !state.engine(engine).state().crashed() {
            return Ok(self.shown(app, state));
        }
        self.start_locked(app, state, engine).await
    }
}

/// Упавший помощник VOLT поднимается так же, как ядро (D-057, D-178): три попытки, потом
/// VOLT выключается до следующего подключения. VPN не рвётся: без Relay перестают работать
/// только выходы VOLT, а молчать об этом нельзя.
async fn revive_volt(state: &AppState, watch: &mut Watch) {
    match watch.step(state.volt.crashed()) {
        Step::Idle => {}
        Step::GiveUp => {
            let why =
                format!("VOLT не встал с {LIMIT} попыток — выключен до следующего подключения");
            state.note("error", &why);
            state.volt.stop();
            state.volt.fail(&why);
        }
        Step::Raise => {
            let _transition = state.connection.lock().await;
            let attempt = format!("VOLT упал — подъём {}/{LIMIT}", watch.attempts);
            match state.volt.revive().await {
                Ok(()) => state.note("warning", &attempt),
                Err(why) => state.note("error", &format!("{attempt}: {why}")),
            }
        }
    }
}

/// Кого погасить, прежде чем поднять `engine`: всех, кто держит трафик или должен держать.
/// «Должен» тоже: упавшее ядро с `wanted` надзор поднял бы через секунду рядом с новым.
fn others(busy: &[EngineId], engine: EngineId) -> Vec<EngineId> {
    busy.iter().copied().filter(|id| *id != engine).collect()
}

fn update_safe(running: bool, proxy_backup: bool, kill_switch_backup: bool) -> Result<()> {
    if running || proxy_backup || kill_switch_backup {
        return Err(crate::error::AppError::invalid(
            "Stopping the core did not restore networking. The update was not installed.",
        ));
    }
    Ok(())
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
///
/// Одна на все ядра: трафик держит одно, и падать по очереди им не с чего.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Трафик держит одно ядро: поднимая одно, гасим всех остальных — и только их.
    #[test]
    fn starting_one_engine_stops_every_other() {
        use EngineId::{Mihomo, Qd};
        assert_eq!(others(&[Mihomo], Qd), [Mihomo]);
        assert_eq!(
            others(&[Qd], Qd),
            [],
            "своё ядро не гасим — его поднимает запуск"
        );
        assert_eq!(others(&[], Mihomo), []);
        assert_eq!(others(&[Mihomo, Qd], Qd), [Mihomo]);
    }

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
