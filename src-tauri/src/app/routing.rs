//! Куда идёт трафик (D-155): направление, выбранный узел, выход до узла, псевдоним ядра
//! и набор, чьи правила сейчас решают маршрут.
//!
//! Сам ничего не хранит: направление и выбор лежат в настройках, набор — в своём
//! хранилище, живой выход знает ядро. Сервис — единственный, кто сводит их в ответ
//! «через что идёт трафик» и умеет его поменять.

use serde::Serialize;

use crate::app::settings::Settings;
use crate::app::state::AppState;
use crate::config::direction::Direction;
use crate::config::presets::PresetStore;
use crate::error::Result;
use crate::nodes::ping::Method;
use crate::nodes::Node;

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
        Ok(Snapshot {
            nodes: state.catalog.nodes(),
            direction: self.direction(state),
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

    /// Направление с поправкой на то, что есть на самом деле: выбранный узел мог исчезнуть
    /// из подписки, и показывать `manual` в этом случае было бы враньём.
    pub fn direction(&self, state: &AppState) -> Direction {
        let settings = state.settings.get();
        Direction::resolve(
            settings.direction,
            settings.selected.as_deref(),
            &state.catalog.names(),
        )
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

    /// Первый источник переводит направление в автовыбор (D-056).
    pub fn note_source_added(&self, state: &AppState, had_sources: bool) -> Result<()> {
        state.settings.update(|settings| {
            settings.direction = Direction::on_source_added(settings.direction, had_sources);
        })
    }

    /// Через что идёт трафик. На остановленном ядре — тот, что мы запомнили сами (D-039):
    /// отметку в списке решает направление, а не живость процесса (D-092).
    ///
    /// В `RULES` спрашивать ядро бесполезно: псевдоним `umiray` там указывает на `AUTO`
    /// (D-056), а маршрут решают правила, и ответ «AUTO» — правда не про то (B-014).
    /// Правда там — цель `MATCH` применённого набора: набор с `MATCH,RU-VLESS-GROUP`
    /// именно её и назначает всему непойманному.
    pub async fn selected(&self, state: &AppState) -> Option<String> {
        if state.settings.get().direction == Direction::Rules {
            if let Some(target) = routed(self.rules(state).ok().flatten().as_deref()) {
                return Some(target);
            }
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

    /// Правила, которые уходят в сборку (D-071, D-075): текст применённого набора.
    ///
    /// Пусто — значит правила соберёт рендер сам: вне `RULES` набор пользователя
    /// не участвует. Группы к этому отношения не имеют — они общий документ и идут
    /// в сборку всегда. Набор, на который ссылается настройка, мог быть удалён мимо нас,
    /// — тогда тоже пусто, а не отказ собрать конфиг.
    pub fn rules(&self, state: &AppState) -> Result<Option<String>> {
        let Some(id) = self.applied_preset(state) else {
            return Ok(None);
        };
        if PresetStore::get(&id).is_err() {
            return Ok(None);
        }
        Ok(Some(PresetStore::content(&id)?))
    }

    /// Набор, чьи документы сейчас уходят ядру. Вне `RULES` таких нет: там маршрут
    /// собирает клиент, и подсвечивать чей-то набор применённым было бы враньём.
    pub fn applied_preset(&self, state: &AppState) -> Option<String> {
        applied(&state.settings.get())
    }
}

fn applied(settings: &Settings) -> Option<String> {
    (settings.direction == Direction::Rules)
        .then(|| settings.preset.clone())
        .flatten()
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

    /// В RULES «через какой сервер» отвечает документ, а не ядро: у ядра там всегда `AUTO`.
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

    /// Вне `RULES` набор в сборке не участвует — и применённым не считается.
    #[test]
    fn only_rules_have_an_applied_preset() {
        let mut settings = Settings {
            preset: Some("p1".into()),
            ..Settings::default()
        };
        settings.direction = Direction::Auto;
        assert_eq!(applied(&settings), None);
        settings.direction = Direction::Rules;
        assert_eq!(applied(&settings).as_deref(), Some("p1"));
    }
}
