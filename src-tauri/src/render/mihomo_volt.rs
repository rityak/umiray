//! Выходы VOLT в конфиге mihomo (D-182). `DIRECT-VOLT` — всегда через Relay, `DIRECT-AUTO` —
//! сначала напрямую, Relay — если сайт не ответил. `DIRECT` остаётся прямым: правила
//! маршрута, которые ведут туда, VOLT не трогает.

use serde_yaml::{Mapping, Value};

use crate::config::direction::SELECTOR;
use crate::config::volt::{Mode, Route};
use crate::error::{AppError, Result};

pub fn apply(map: &mut Mapping, route: &Route) -> Result<()> {
    let proxies = map
        .entry("proxies".into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let proxies = proxies
        .as_sequence_mut()
        .ok_or_else(|| AppError::invalid("proxies must be a sequence"))?;
    for name in ["DIRECT-VOLT", "DIRECT-AUTO"] {
        if proxies
            .iter()
            .any(|proxy| proxy.get("name").and_then(Value::as_str) == Some(name))
        {
            return Err(AppError::invalid(format!(
                "{name} is reserved while VOLT is enabled"
            )));
        }
    }
    for (name, port) in [
        ("DIRECT-VOLT", route.relay_port),
        ("DIRECT-AUTO", route.auto_port),
    ] {
        let mut proxy = Mapping::new();
        for (key, value) in [
            ("name", name.into()),
            ("type", "socks5".into()),
            ("server", "127.0.0.1".into()),
            ("port", port.into()),
            ("username", "umiray".into()),
            ("password", route.password.clone().into()),
            ("udp", true.into()),
        ] {
            proxy.insert(key.into(), value);
        }
        proxies.push(Value::Mapping(proxy));
    }
    // Выход `DIRECT` в VOLT — это выбор в «Соединении», а не замена `DIRECT` в правилах:
    // в псевдоним добавляется ещё один член, на него и наводится выбор (`Routing`).
    if route.exit {
        let target = if route.mode == Mode::Auto {
            "DIRECT-AUTO"
        } else {
            "DIRECT-VOLT"
        };
        let alias = map
            .get_mut("proxy-groups")
            .and_then(Value::as_sequence_mut)
            .and_then(|groups| {
                groups
                    .iter_mut()
                    .find(|group| group.get("name").and_then(Value::as_str) == Some(SELECTOR))
            })
            .and_then(|group| group.get_mut("proxies"))
            .and_then(Value::as_sequence_mut);
        if let Some(members) = alias {
            if !members.iter().any(|member| member.as_str() == Some(target)) {
                members.push(target.into());
            }
        }
    }
    let rules = map
        .entry("rules".into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let rules = rules
        .as_sequence_mut()
        .ok_or_else(|| AppError::invalid("rules must be a sequence"))?;
    // Первым: сокеты самого Relay не должны вернуться в Relay через TUN (D-176).
    let head = std::iter::once("PROCESS-NAME,volt-relay.exe,DIRECT".to_owned())
        .chain(route.rules.iter().map(|rule| format!("{rule},DIRECT-VOLT")))
        .map(Value::from);
    rules.splice(0..0, head);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(exit: bool) -> Route {
        Route {
            mode: Mode::Auto,
            relay_port: 3101,
            auto_port: 3103,
            password: "test-secret".into(),
            exit,
            rules: vec![
                "DOMAIN-SUFFIX,youtube.com".into(),
                "AND,((NETWORK,UDP),(DST-PORT,50000-50100))".into(),
            ],
        }
    }

    fn config() -> Mapping {
        serde_yaml::from_str("rules:\n - DOMAIN-SUFFIX,ru,DIRECT,no-resolve\n - MATCH,umiray\nproxy-groups:\n - name: umiray\n   proxies: [DIRECT, AUTO]\n").unwrap()
    }

    /// Сервисы — первыми и в Relay; правила маршрута к `DIRECT` остаются прямыми.
    #[test]
    fn services_go_first_and_direct_stays_direct() {
        let mut map = config();
        apply(&mut map, &route(false)).unwrap();
        let rules: Vec<&str> = map["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            rules,
            [
                "PROCESS-NAME,volt-relay.exe,DIRECT",
                "DOMAIN-SUFFIX,youtube.com,DIRECT-VOLT",
                "AND,((NETWORK,UDP),(DST-PORT,50000-50100)),DIRECT-VOLT",
                "DOMAIN-SUFFIX,ru,DIRECT,no-resolve",
                "MATCH,umiray",
            ]
        );
        assert_eq!(
            map["proxy-groups"][0]["proxies"],
            serde_yaml::from_str::<Value>("[DIRECT, AUTO]").unwrap(),
            "без выхода через VOLT псевдоним не меняется"
        );
        assert!(apply(&mut map, &route(false)).is_err(), "имена заняты");
    }

    /// Выход `DIRECT` через VOLT — лишний член псевдонима, а не замена `DIRECT`.
    #[test]
    fn the_direct_exit_through_volt_joins_the_alias() {
        let mut map = config();
        apply(&mut map, &route(true)).unwrap();
        assert_eq!(
            map["proxy-groups"][0]["proxies"],
            serde_yaml::from_str::<Value>("[DIRECT, AUTO, DIRECT-AUTO]").unwrap()
        );
    }
}
