//! Фаза `tick`: всё, что клиент делает по часам (D-101).
//!
//! Она же дом фонового обновления подписок (D-038): расписание живёт в клиенте, а не
//! таймером в окне, — иначе оно работало бы только при открытом разделе.
//!
//! «Раз в N сделай X» — одна запись в `TICKS` с собственным периодом, а не ещё одна ветка
//! в общем цикле. Задача одна на всех: будильников столько же, сколько шагов, а потоков —
//! один, и просыпается он ровно к ближайшему сроку.
//!
//! Отличие от фазы `core`: отменять здесь нечего — шаги друг от друга не зависят, и отказ
//! одного просто уезжает в лог. Зато есть **первый заход**: обновить подписки при запуске
//! и по расписанию — разные решения (D-024), и шаг узнаёт, какой из них его.
//!
//! В кольцо лога ядра уезжают только отказы. Успех — на stderr: две строки в минуту съели
//! бы пятисотстрочное кольцо за сутки, и в нём не осталось бы ничего про само ядро.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::app::lifecycle::Lifecycle;
use crate::app::state::AppState;
use crate::error::Result;

type Job<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type Run = fn(&AppState, bool) -> Job<'_>;

struct Tick {
    id: &'static str,
    label: &'static str,
    /// Как часто. Считается от конца прошлого прогона: шаг, идущий дольше своего периода,
    /// не выстраивает очередь из самого себя.
    every: Duration,
    run: Run,
}

/// Реестр периодики. Новый «раз в N сделай X» — одна запись здесь.
///
/// Шаги сегодня ходят с похожими периодами, и это не совпадение: у подписок срок
/// свой у каждой (`refresher::is_due`), у стран узлов — свой (D-084), а минута — просто
/// частота, с которой их спрашивают «не пора ли». Шагу с настоящим собственным периодом
/// — сторожу соединения, проверке часов — тут ничего менять не придётся.
const TICKS: &[Tick] = &[
    Tick {
        id: "sources",
        label: "обновление подписок",
        every: Duration::from_secs(60),
        run: sources,
    },
    Tick {
        id: "guard",
        label: "сторож соединения",
        every: Duration::from_secs(300),
        run: guard,
    },
    Tick {
        id: "routes",
        label: "чей маршрут по умолчанию",
        every: Duration::from_secs(60),
        run: routes,
    },
    Tick {
        id: "firewall",
        label: "исполняет ли кто-то запрет",
        every: Duration::from_secs(300),
        run: firewall,
    },
    Tick {
        id: "geo",
        label: "страны узлов",
        every: Duration::from_secs(60),
        run: geo,
    },
];

pub struct Clock;

impl Clock {
    /// Завести часы. Отдельная задача, а не таймер во фронтенде: периодика обязана идти
    /// и при спрятанном окне, и независимо от того, какой раздел открыт.
    pub fn spawn(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            // Первый заход — сразу и у всех: срок у шага считается от прошлого прогона,
            // а прошлого ещё не было.
            let mut due: Vec<Instant> = TICKS.iter().map(|_| Instant::now()).collect();
            let mut first = true;
            loop {
                let state = app.state::<AppState>();
                let now = Instant::now();
                for (at, tick) in due.iter_mut().zip(TICKS) {
                    if *at > now {
                        continue;
                    }
                    let began = Instant::now();
                    let outcome = (tick.run)(&state, first).await;
                    let (level, text) =
                        Lifecycle::line("tick", tick.id, tick.label, began.elapsed(), &outcome);
                    eprintln!("{text}");
                    if outcome.is_err() {
                        state.note(level, &text);
                    }
                    *at = Instant::now() + tick.every;
                }
                first = false;
                // Спим до ближайшего срока, а не фиксированную минуту: иначе шаг с периодом
                // короче общего такта получал бы не свой период, а такт.
                let next = due.iter().min().copied().unwrap_or_else(Instant::now);
                tokio::time::sleep(next.saturating_duration_since(Instant::now())).await;
            }
        });
    }
}

fn sources(state: &AppState, first: bool) -> Job<'_> {
    Box::pin(crate::app::refresher::Refresher::due(state, first))
}

/// Правда ли трафик идёт через туннель (D-107). Пять минут: чаще — это лишний запрос
/// наружу каждую минуту, реже — обрыв висит незамеченным полчаса.
fn guard(state: &AppState, _first: bool) -> Job<'_> {
    Box::pin(async move {
        crate::app::guard::Guard::look(state).await;
        Ok(())
    })
}

/// Не увёл ли кто маршрут по умолчанию у туннеля (D-109, D-115).
///
/// Самая тихая беда из всех: окно зелёное, ядро работает, сторож видит ответ — а уходит
/// всё через чужой адаптер с меньшей метрикой. Спрашиваем только в TUN: в local маршрут
/// по умолчанию и должен быть чужим. Минута, а не пять: чужой VPN поднимается когда
/// угодно, и висеть без туннеля полчаса нельзя.
fn routes(state: &AppState, _first: bool) -> Job<'_> {
    Box::pin(async move {
        let core = state.mihomo.status();
        if !core.running || core.mode != Some(crate::config::mode::Mode::Tun) {
            state.notices.set(crate::app::notice::ROUTE, None);
            return Ok(());
        }
        state.notices.set(
            crate::app::notice::ROUTE,
            crate::diag::system::SystemProbe::thief_of_the_route().map(|thief| {
                crate::app::notice::Notice::about_core(
                    format!(
                        "Трафик уходит мимо туннеля: маршрут по умолчанию держит «{thief}». Выключите его или поднимите метрику."
                    ),
                    core.started.unwrap_or_default(),
                )
            }),
        );
        Ok(())
    })
}

/// Правда ли запрет выхода мимо туннеля кто-то исполняет (B-012, D-115).
///
/// Защита включает брандмауэр сама, но выключить его после этого может кто угодно —
/// человек, политика, другая программа, — и запрет останется лежать настройкой, которую
/// никто не применяет. Снаружи это выглядит как работающая защита, и в этом вся беда.
/// Спрашиваем только пока запрет стоит: нет запрета — нечего и исполнять.
fn firewall(state: &AppState, _first: bool) -> Job<'_> {
    Box::pin(async move {
        if state.settings.get().kill_switch_backup.is_none() {
            state.notices.set(crate::app::notice::FIREWALL, None);
            return Ok(());
        }
        let off: Vec<String> = crate::system::killswitch::Firewall::profiles()?
            .into_iter()
            .filter(|profile| !profile.enabled.eq_ignore_ascii_case("true"))
            .map(|profile| profile.name)
            .collect();
        state.notices.set(
            crate::app::notice::FIREWALL,
            (!off.is_empty()).then(|| {
                crate::app::notice::Notice::about(format!(
                    "Запрет выхода мимо туннеля стоит, но исполнять его некому: брандмауэр выключен ({}).",
                    off.join(", ")
                ))
            }),
        );
        Ok(())
    })
}

/// Страны узлов (D-084): у записи свой срок в неделю, здесь только «не пора ли».
fn geo(_state: &AppState, _first: bool) -> Job<'static> {
    Box::pin(crate::nodes::geo::GeoCache::refresh())
}
