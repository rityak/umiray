//! Группы, которые собирает клиент (D-172): кого нет в `AUTO` и какие группы он заводит сам.
//!
//! Оба поля — решения клиента о сборке, поэтому живут в `client.yaml` рядом с `udp-group`
//! (D-068, D-113) и правятся точечно, как режим перехвата (D-052). Мусор в поле — умолчание,
//! а не отказ собрать конфиг: из-за галки не поднять VPN было бы хуже.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::config::files::{Documents, CLIENT};
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

const EXCLUDE: &str = "auto-exclude";
const GROUPS: &str = "auto-groups";

/// Кого нет в `AUTO`. Источник целиком — и его будущие узлы тоже; узел — по имени, как его
/// режет ядро (`exclude-filter`, S-012).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exclude {
    pub sources: Vec<String>,
    pub nodes: Vec<String>,
}

/// Какие группы клиент собирает сам: по стране узла, по протоколу и `umiray-udp` — из узлов,
/// чей протокол несёт UDP сам, а не внутри TCP (D-113). Правило «весь UDP туда» — своё поле
/// (`udp-group`), и без этой группы его нет.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grouping {
    pub location: bool,
    pub protocol: bool,
    pub udp: bool,
}

pub struct AutoGroups;

impl AutoGroups {
    pub fn exclude() -> Exclude {
        read(EXCLUDE)
    }

    pub fn grouping() -> Grouping {
        read(GROUPS)
    }

    pub fn set_exclude(exclude: &Exclude) -> Result<()> {
        write(EXCLUDE, exclude)
    }

    pub fn set_grouping(grouping: Grouping) -> Result<()> {
        write(GROUPS, &grouping)
    }
}

fn read<T: Default + for<'de> Deserialize<'de>>(key: &str) -> T {
    Documents::read(CLIENT)
        .ok()
        .map(|text| of(&text, key))
        .unwrap_or_default()
}

fn of<T: Default + for<'de> Deserialize<'de>>(text: &str, key: &str) -> T {
    Yaml::top_mapping(text)
        .ok()
        .and_then(|map| map.get(Value::from(key)).cloned())
        .and_then(|value| serde_yaml::from_value(value).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(key: &str, value: &T) -> Result<()> {
    let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
    let value = serde_yaml::to_value(value).map_err(|e| AppError::invalid(e.to_string()))?;
    Yaml::set(&mut map, key, value);
    let text = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|e| AppError::invalid(e.to_string()))?;
    Documents::write(CLIENT, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_written_means_everyone_in_auto_and_no_own_groups() {
        assert_eq!(of::<Exclude>("ping: tcp", EXCLUDE), Exclude::default());
        assert_eq!(of::<Grouping>("", GROUPS), Grouping::default());
    }

    #[test]
    fn written_fields_read_back_and_a_missing_half_is_the_default() {
        let text = "auto-exclude:\n  nodes: [Germany]\nauto-groups:\n  location: true\n";
        let exclude: Exclude = of(text, EXCLUDE);
        assert_eq!(exclude.nodes, ["Germany"]);
        assert!(exclude.sources.is_empty());
        let grouping: Grouping = of(text, GROUPS);
        assert!(grouping.location && !grouping.protocol && !grouping.udp);
    }

    /// Мусор — умолчание: из-за галки не поднять VPN было бы хуже, чем собрать без неё.
    #[test]
    fn nonsense_is_the_default_instead_of_breaking_the_vpn() {
        assert_eq!(
            of::<Exclude>("auto-exclude: конечно", EXCLUDE),
            Exclude::default()
        );
        assert_eq!(
            of::<Grouping>("auto-groups: [1, 2]", GROUPS),
            Grouping::default()
        );
    }
}
