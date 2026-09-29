//! UDP через свои узлы: тумблер в `client.yaml` (D-113).
//!
//! UDP через TCP-транспорт (VLESS, Trojan, WS) едет внутри надёжного потока: потерянный
//! пакет тормозит всё, что за ним, — это и есть лаг-спайк в звонке и в игре. Hysteria2
//! и TUIC несут его датаграммой QUIC, где потеря остаётся потерей.
//!
//! Клиент умеет собрать группу из таких узлов сам и увести в неё весь UDP одной строкой
//! правил. Включается это здесь: поле клиентское, ядру файл не уходит (D-068), у поля
//! один хозяин (D-052).
//!
//! **Снимается вместе с «Прямым».** Правило с именной целью идёт мимо псевдонима, и
//! переключение направления в «Прямое» такой UDP не остановило бы (D-056) — поэтому
//! `state::set_direction` тумблер гасит, а не обходит его при сборке: выключенная галка
//! в окне честнее, чем включённая, которая ничего не делает.

use serde_yaml::Value;

use crate::config::files::Documents;
use crate::config::files::CLIENT;
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

const KEY: &str = "udp-group";

pub struct UdpGroup;

impl UdpGroup {
    /// Включён ли тумблер. Мусор в поле — это «выключено», а не отказ собрать конфиг:
    /// из-за галки не поднять VPN было бы хуже, чем не собрать группу.
    pub fn on() -> bool {
        Documents::read(CLIENT)
            .ok()
            .and_then(|text| Yaml::top_mapping(&text).ok())
            .and_then(|map| map.get(Value::from(KEY)).and_then(Value::as_bool))
            .unwrap_or(false)
    }

    pub fn write(on: bool) -> Result<()> {
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        Yaml::set(&mut map, KEY, Value::from(on));
        let text = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(CLIENT, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Разбор отделён от диска — правило проверяется обычным `cargo test`.
    fn of(text: &str) -> bool {
        Yaml::top_mapping(text)
            .ok()
            .and_then(|map| map.get(Value::from(KEY)).and_then(Value::as_bool))
            .unwrap_or(false)
    }

    #[test]
    fn the_switch_is_off_until_it_is_written() {
        assert!(!of(""));
        assert!(!of("ping: tcp"));
        assert!(of("udp-group: true"));
        assert!(!of("udp-group: false"));
    }

    /// Мусор в поле — «выключено»: из-за галки не поднять VPN было бы хуже.
    #[test]
    fn nonsense_turns_it_off_instead_of_breaking_the_vpn() {
        assert!(!of("udp-group: конечно"));
        assert!(!of("udp-group: 1"));
    }
}
