//! Куда идёт трафик (D-155): выбранный выход, выход до узла, псевдоним ядра, тумблер
//! маршрутизации и набор, чьи правила решают маршрут (D-166).
//!
//! Сам ничего не хранит: направление и выбор лежат в настройках, набор — в своём
//! хранилище, живой выход знает ядро. Сервис — единственный, кто сводит их в ответ
//! «через что идёт трафик» и умеет его поменять.

use serde::Serialize;

use crate::app::settings::Settings;
use crate::app::state::AppState;
use crate::config::direction::Direction;
use crate::config::presets::{Preset, PresetStore, RULES};
use crate::config::route::{Priority, ReadyUse};
use crate::config::rules::RulesCodec;
use crate::error::{AppError, Result};
use crate::nodes::ping::Method;
use crate::nodes::Node;
use crate::render::plan::Route;

pub struct Routing;

/// Всё, что опрашивает раздел «Соединение», одним ответом (D-145): спрашивают это всегда
/// вместе, и четыре перехода через границу вместо одного были чистой тратой.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Все узлы всех источников — с диска, при живом и остановленном ядре (D-061).
    nodes: Vec<Node>,
    /// Куда идёт трафик — с поправкой на исчезнувший выбранный узел (D-056).
    direction: Direction,
    /// Узел, на который наведён псевдоним в `manual`. Вне него пусто: выбран `DIRECT`
    /// или `AUTO`, и отметку в списке ставит направление (D-166).
    node: Option<String>,
    /// Куда маршрутизация шлёт непойманное, когда `MATCH` набора не за выбором (D-166):
    /// выбор в «Соединении» тогда действует только на правила в `umiray`.
    fallback: Option<String>,
    /// Чем мерить задержку (D-069): колонка таблицы называет его.
    ping: Method,
    /// Выход от выбранного до узла: `["AUTO", "Poland 1"]`. Пусто — выхода нет.
    route: Vec<String>,
}

/// UDP через свои узлы (D-113): состояние тумблера и то, есть ли из чего собирать группу.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Udp {
    pub on: bool,
    /// Сколько узлов с нативным UDP нашлось. Ноль — включать нечего, и окно гасит галку:
    /// пустую группу ядро не примет и не стартует вовсе.
    pub nodes: usize,
}

impl Routing {
    pub async fn snapshot(&self, state: &AppState) -> Result<Snapshot> {
        let settings = state.settings.get();
        let names = state.catalog.names();
        // С поправкой на то, что есть: выбранный узел мог исчезнуть из подписки,
        // и показывать `manual` тогда было бы враньём.
        let direction =
            Direction::resolve(settings.direction, settings.selected.as_deref(), &names);
        Ok(Snapshot {
            node: (direction == Direction::Manual)
                .then(|| Direction::target(direction, settings.selected.as_deref(), &names)),
            fallback: self.fallback(state),
            nodes: state.catalog.nodes(),
            direction,
            ping: crate::app::client::ClientConfig::ping()?,
            route: self.route(state).await,
        })
    }

    pub fn udp(&self) -> Udp {
        Udp {
            on: crate::config::udp::UdpGroup::on(),
            nodes: crate::render::effective::ConfigRenderer::udp_nodes(),
        }
    }

    /// Сменить направление (D-056). Узел приходит вместе с ним: нажатие по строке таблицы —
    /// одно действие, а не два. До живого ядра правку доводит `Connection::change`.
    ///
    /// Это чистая настройка (D-071): файлы никуда не перекладываются. «Прямое» гасит и
    /// UDP-группу (D-113): правило `NETWORK,udp` идёт мимо псевдонима, и без этого UDP
    /// продолжал бы уходить в туннель при выключенном VPN. Гасим галку, а не обходим её
    /// при сборке: снятая галка в окне честнее включённой, которая ничего не делает.
    pub fn set_direction(
        &self,
        state: &AppState,
        direction: Direction,
        node: Option<String>,
    ) -> Result<()> {
        if direction == Direction::Direct && crate::config::udp::UdpGroup::on() {
            crate::config::udp::UdpGroup::write(false)?;
        }
        state.settings.update(|settings| {
            settings.direction = direction;
            if let Some(node) = &node {
                settings.selected = Some(node.clone());
            }
        })
    }

    /// Включить или выключить маршрутизацию (D-166). До живого ядра правку доводит
    /// `Connection::change`: сборка меняется целиком, соединения рвутся (D-143).
    pub fn set_routing(&self, state: &AppState, on: bool) -> Result<()> {
        state.settings.update(|settings| settings.routing = on)
    }

    /// Блокировать ли рекламу (D-169): готовый набор «Блокировка рекламы» в применённом
    /// наборе маршрута. Включённый вместе с ним включает и маршрутизацию — набор, лежащий
    /// в стороне от сборки, рекламу не режет (D-166). Отдельного места для этой галки нет:
    /// в «Маршрутизации» её видно и снимают той же строкой готовых наборов.
    pub fn set_ads(&self, state: &AppState, on: bool) -> Result<()> {
        let id = self
            .applied_preset(state)
            .ok_or_else(|| AppError::invalid("Набора маршрута нет — блокировать рекламу негде"))?;
        let text = PresetStore::content(&id)?;
        PresetStore::write(&id, RULES, &with_ads(&text, on)?)?;
        if on {
            state.settings.update(|settings| {
                settings.preset = Some(id.clone());
                settings.routing = true;
            })?;
        }
        Ok(())
    }

    /// Первый источник переводит направление в автовыбор (D-056).
    pub fn note_source_added(&self, state: &AppState, had_sources: bool) -> Result<()> {
        state.settings.update(|settings| {
            settings.direction = Direction::on_source_added(settings.direction, had_sources);
        })
    }

    /// Через что идёт трафик. На остановленном ядре — тот, что мы запомнили сами (D-039):
    /// отметку в списке решает выбор, а не живость процесса (D-166).
    ///
    /// `MATCH` набора не в псевдоним — спрашивать ядро бесполезно: ответ про псевдоним
    /// правда не про то (B-014). Правда — цель `MATCH`: набор с `MATCH,RU-VLESS-GROUP`
    /// именно её и назначает всему непойманному.
    pub async fn selected(&self, state: &AppState) -> Option<String> {
        if let Some(target) = self.fallback(state) {
            return Some(target);
        }
        if state.mihomo.status().running {
            return state.mihomo.selected().await;
        }
        state.settings.get().selected
    }

    /// Выход целиком: что выбрано и куда оно ведёт — `AUTO → Poland 1` (D-145). Пусто —
    /// выхода нет. На остановленном ядре цепочка из одного звена: разворачивать группы
    /// умеет только живое ядро.
    pub async fn route(&self, state: &AppState) -> Vec<String> {
        match self.selected(state).await {
            Some(start) => state.mihomo.route(&start).await,
            None => Vec::new(),
        }
    }

    /// Навести псевдоним туда, куда указывает направление.
    ///
    /// На остановленном ядре молча ничего не делаем: наводить нечего, а цель всё равно
    /// вычислится заново при следующем запуске.
    ///
    /// Отдаёт, сдвинулся ли выход: соединения, открытые через прежний, живут дальше,
    /// и рвать их — забота того, кто сдвигал (D-143).
    pub async fn point_alias(&self, state: &AppState) -> Result<bool> {
        if !state.mihomo.status().running {
            return Ok(false);
        }
        let settings = state.settings.get();
        let target = Direction::target(
            settings.direction,
            settings.selected.as_deref(),
            &state.catalog.names(),
        );
        let moved = state.mihomo.selected().await.as_deref() != Some(target.as_str());
        state.mihomo.select(&target).await?;
        Ok(moved)
    }

    /// Маршрут, который уходит в сборку (D-071, D-158): документ выбранного набора целиком —
    /// свои правила, rule sets, готовые наборы и `MATCH`. Выключенная маршрутизация не отдаёт
    /// ничего, и дно маршрута — `MATCH,umiray` в выбранный выход (D-166). Группы к этому
    /// отношения не имеют — они общий документ и идут в сборку всегда.
    pub fn document(&self, state: &AppState) -> Result<Route> {
        let text = match self.applied_preset(state) {
            Some(id) if state.settings.get().routing => Some(PresetStore::content(&id)?),
            _ => None,
        };
        Ok(Route { text })
    }

    /// Куда маршрутизация шлёт непойманное мимо выбора: цель `MATCH`, если она не псевдоним.
    /// Выключенная маршрутизация `MATCH` набора не берёт — там выбор решает всё.
    fn fallback(&self, state: &AppState) -> Option<String> {
        routed(
            self.document(state)
                .ok()
                .and_then(|route| route.text)
                .as_deref(),
        )
    }

    /// Набор, чей маршрут действует при включённой маршрутизации (D-166). Выбор набора
    /// и тумблер — разные вещи: выключили и включили — вернулся тот же набор.
    ///
    /// Выбранного нет или он удалён мимо нас — первый: наборов не бывает меньше одного
    /// (D-071), а маршрут без документа молча потерял бы списки.
    pub fn applied_preset(&self, state: &AppState) -> Option<String> {
        applied(&state.settings.get(), &PresetStore::list())
    }
}

/// Готовый набор блокировки рекламы — по имени коллекции (`collections/rules/block-ads.yaml`).
const ADS: &str = "block-ads";

/// Документ набора с готовым набором блокировки рекламы или без него. Остальное как было:
/// повторное включение не заводит второй строки, выключение не трогает соседей.
fn with_ads(text: &str, on: bool) -> Result<String> {
    let mut routing = RulesCodec::parse(text)?;
    let had = routing.ready.iter().any(|ready| ready.id == ADS);
    if on && !had {
        routing.ready.push(ReadyUse {
            id: ADS.into(),
            target: None,
            priority: Priority::default(),
        });
    }
    if !on {
        routing.ready.retain(|ready| ready.id != ADS);
    }
    RulesCodec::render(text, &routing)
}

fn applied(settings: &Settings, presets: &[Preset]) -> Option<String> {
    settings
        .preset
        .clone()
        .filter(|id| presets.iter().any(|preset| &preset.id == id))
        .or_else(|| presets.first().map(|preset| preset.id.clone()))
}

/// Куда набор отправляет всё непойманное — его `MATCH` (B-014).
///
/// Набора нет вовсе — пусто: тогда правила собирает клиент, и там `MATCH` целится
/// в псевдоним, про который честнее спросить ядро. Документ, который не разбирается,
/// — тоже пусто: соврать имя группы хуже, чем промолчать.
fn routed(routing: Option<&str>) -> Option<String> {
    let text = routing?;
    crate::config::rules::RulesCodec::parse(text)
        .ok()
        .map(|routing| routing.fallback)
        // `MATCH` в псевдоним — это «спроси у ядра»: псевдоним не сервер, а указатель
        // на него, и разворачивает его ядро.
        .filter(|target| target != crate::config::direction::SELECTOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Галка мастера — ровно одна строка готовых наборов: повтор её не удваивает, снятие
    /// не трогает соседей.
    #[test]
    fn ad_blocking_is_one_ready_set_line() {
        let base = "rules:
  - DOMAIN,a.ru,DIRECT
ready:
  - id: direct-ru
";
        let on = with_ads(&with_ads(base, true).unwrap(), true).unwrap();
        let routing = RulesCodec::parse(&on).unwrap();
        let ids: Vec<&str> = routing
            .ready
            .iter()
            .map(|ready| ready.id.as_str())
            .collect();
        assert_eq!(ids, ["direct-ru", "block-ads"]);
        assert_eq!(routing.rules.len(), 1, "свои правила на месте");

        let off = RulesCodec::parse(&with_ads(&on, false).unwrap()).unwrap();
        let ids: Vec<&str> = off.ready.iter().map(|ready| ready.id.as_str()).collect();
        assert_eq!(ids, ["direct-ru"]);
    }

    /// При `MATCH` мимо псевдонима «через какой сервер» отвечает документ, а не ядро.
    #[test]
    fn the_set_says_where_everything_unmatched_goes() {
        assert_eq!(
            routed(Some(
                "rules:
  - DOMAIN,a.ru,DIRECT
  - MATCH,RU-VLESS-GROUP
"
            )),
            Some("RU-VLESS-GROUP".to_string())
        );
        assert_eq!(
            routed(Some(
                "rules:
  - DOMAIN,a.ru,DIRECT
  - MATCH,umiray
"
            )),
            None,
            "MATCH в псевдоним — не ответ: куда он ведёт, знает ядро"
        );
        assert_eq!(routed(None), None, "набора нет — спрашиваем ядро");
        assert_eq!(
            routed(Some(
                "rules: 12
"
            )),
            None,
            "документ не разобрался — молчим, а не выдумываем имя"
        );
    }

    /// Выбранный набор действует в любом направлении (D-158, D-166). Выбора нет или он
    /// указывает в никуда — первый.
    #[test]
    fn the_chosen_preset_is_in_use_in_every_direction() {
        let presets = ["p1", "p2"].map(|id| Preset {
            id: id.into(),
            name: id.into(),
            created: None,
        });
        let mut settings = Settings {
            preset: Some("p2".into()),
            ..Settings::default()
        };
        for direction in [Direction::Auto, Direction::Direct, Direction::Manual] {
            settings.direction = direction;
            assert_eq!(applied(&settings, &presets).as_deref(), Some("p2"));
        }
        settings.preset = Some("gone".into());
        assert_eq!(applied(&settings, &presets).as_deref(), Some("p1"));
        settings.preset = None;
        assert_eq!(applied(&settings, &presets).as_deref(), Some("p1"));
        assert_eq!(applied(&settings, &[]), None);
    }
}
