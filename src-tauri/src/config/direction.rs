//! Куда идёт трафик — одно решение из четырёх (D-056).
//!
//! Раньше это были две вещи сразу: выбранный узел и негласный режим. Пользователь при этом
//! не мог ответить на вопрос «что будет с выбором, если я перепишу маршрутизацию». Теперь
//! понятие одно, и от него зависит и цель псевдонима, и то, чьи конфиги лежат в разделах.
//!
//! Здесь только правила перехода — без диска и без ядра, поэтому всё проверяется
//! обычным `cargo test`.

use serde::{Deserialize, Serialize};

/// Служебные цели принадлежат модели направления, а не конкретному рендереру ядра.
pub const SELECTOR: &str = "umiray";
pub const AUTO: &str = "AUTO";
pub const DIRECT: &str = "DIRECT";
pub const UDP: &str = "umiray-udp";
pub const PROBE: &str = "probe";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Из коробки. Источников ещё нет, и обещать «трафик идёт через VPN» нельзя.
    #[default]
    Direct,
    /// Клиент сам выбирает сервер из всех источников.
    Auto,
    /// Конкретный узел. Какой именно — в `Settings::selected`.
    Manual,
    /// Решают правила пользователя. Псевдоним при этом ведёт на автовыбор, но в этом
    /// режиме человек волен переписать и группы, и правила: контроль его.
    Rules,
}

impl Direction {
    /// Направление с поправкой на то, что есть на самом деле.
    ///
    /// Подписка обновилась, выбранного сервера в ней больше нет — направление уходит
    /// в `Auto`, а не остаётся указывать в пустоту. Тот же принцип, что у D-039: выбор
    /// помним, но не притворяемся, что он ещё существует.
    pub fn resolve(direction: Direction, selected: Option<&str>, nodes: &[String]) -> Direction {
        let vanished = |name: &str| !nodes.iter().any(|node| node == name);
        match direction {
            // Выбирать не из чего: и автовыбор, и ручной выбор при пустых источниках — это
            // прямое соединение, как бы они ни назывались.
            Direction::Auto | Direction::Manual if nodes.is_empty() => Direction::Direct,
            // Узел был назван и исчез. Отсутствие выбора — не тот случай: там `target`
            // просто берёт первый сервер.
            Direction::Manual if selected.is_some_and(vanished) => Direction::Auto,
            other => other,
        }
    }

    /// На что должен указывать псевдоним `umiray`.
    ///
    /// `Manual` без выбранного узла берёт первый: пользователь нажал «вручную», значит хочет
    /// конкретный сервер, а не отказ. Какой именно — он поменяет следующим нажатием.
    pub fn target(direction: Direction, selected: Option<&str>, nodes: &[String]) -> String {
        match Direction::resolve(direction, selected, nodes) {
            Direction::Direct => DIRECT.to_string(),
            Direction::Auto | Direction::Rules => AUTO.to_string(),
            Direction::Manual => selected
                .filter(|name| nodes.iter().any(|node| node == name))
                .map(str::to_string)
                .or_else(|| nodes.first().cloned())
                .unwrap_or_else(|| AUTO.to_string()),
        }
    }

    /// Первый источник переводит из `Direct` в `Auto`.
    ///
    /// Признака «пользователь уже выбирал» отдельно нет и не нужно: переход происходит ровно
    /// на переходе «источников не было — появился первый». Если человек сам поставил `Direct`,
    /// уже имея источники, второй источник его выбор не тронет.
    pub fn on_source_added(direction: Direction, had_sources: bool) -> Direction {
        if !had_sources && direction == Direction::Direct {
            Direction::Auto
        } else {
            direction
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn out_of_the_box_everything_goes_direct() {
        assert_eq!(Direction::default(), Direction::Direct);
        assert_eq!(Direction::target(Direction::default(), None, &[]), DIRECT);
    }

    /// «Первый источник переводит в auto» — но выбор пользователя остаётся за ним.
    #[test]
    fn the_first_source_switches_to_auto_and_nothing_else_does() {
        assert_eq!(
            Direction::on_source_added(Direction::Direct, false),
            Direction::Auto,
            "первый источник обязан включить автовыбор"
        );
        assert_eq!(
            Direction::on_source_added(Direction::Direct, true),
            Direction::Direct,
            "источники уже были — значит DIRECT выбрал человек, и это его дело"
        );
        for chosen in [Direction::Manual, Direction::Rules, Direction::Auto] {
            assert_eq!(
                Direction::on_source_added(chosen, false),
                chosen,
                "{chosen:?}"
            );
        }
    }

    /// Узел исчез из подписки — направление обязано это признать, а не указывать в пустоту.
    #[test]
    fn a_vanished_node_falls_back_to_auto() {
        let live = nodes(&["Poland", "Sweden 0"]);
        assert_eq!(
            Direction::resolve(Direction::Manual, Some("Netherlands"), &live),
            Direction::Auto
        );
        assert_eq!(
            Direction::target(Direction::Manual, Some("Netherlands"), &live),
            AUTO,
            "псевдоним не должен вести на несуществующий узел"
        );
        assert_eq!(
            Direction::resolve(Direction::Manual, Some("Poland"), &live),
            Direction::Manual,
            "живой выбор трогать незачем"
        );
    }

    /// Без источников выбирать не из чего — и автовыбор тоже пустой.
    #[test]
    fn without_sources_there_is_only_direct() {
        for direction in [Direction::Auto, Direction::Manual] {
            assert_eq!(
                Direction::resolve(direction, Some("Poland"), &[]),
                Direction::Direct
            );
            assert_eq!(Direction::target(direction, Some("Poland"), &[]), DIRECT);
        }
        assert_eq!(
            Direction::resolve(Direction::Rules, None, &[]),
            Direction::Rules,
            "свои правила работают и без источников: там может быть один DIRECT"
        );
    }

    /// «При выборе manual берётся первый сервер, если не выбрали другой».
    #[test]
    fn manual_without_a_choice_takes_the_first_server() {
        let live = nodes(&["Poland", "Sweden 0"]);
        assert_eq!(Direction::target(Direction::Manual, None, &live), "Poland");
        assert_eq!(
            Direction::target(Direction::Manual, Some("Sweden 0"), &live),
            "Sweden 0"
        );
    }

    #[test]
    fn auto_and_rules_both_aim_at_the_auto_group() {
        let live = nodes(&["Poland"]);
        assert_eq!(Direction::target(Direction::Auto, None, &live), AUTO);
        assert_eq!(
            Direction::target(Direction::Rules, Some("Poland"), &live),
            AUTO,
            "в rules псевдоним ведёт на автовыбор, а дальше решают правила"
        );
    }
}
