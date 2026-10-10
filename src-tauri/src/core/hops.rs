//! Авто-TTL для приманок VOLT (#5). Перед запуском VPN-пути мы прикидываем число
//! хопов до endpoint'ов и выдаём decoy'ям TTL, при котором они истекают в сети,
//! не доходя до сервера пользователя (иначе tls-auto-приманки собирают на нём
//! challenge-ACK — и нагрузка, и сигнатура). Делается на стороне клиента,
//! потому что ядро захватывает только исходящее и до прихода ответа уже
//! отправило приманки — оценить обратный путь ему не из чего.

use std::net::SocketAddr;
use std::time::Duration;

use serde_yaml::Value;
use socket2::{Domain, Socket, Type};

/// Сколько хопов не доходя до сервера истекает приманка.
const DELTA: u8 = 1;
/// Потолок развёртки: дальше 32 хопов считаем путь неизмеримым.
const MAX_TTL: u32 = 32;
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

enum Reach {
    Reached,
    NotReached,
    Error,
}

/// Достигает ли SYN с данным IP TTL хоста: соединение открылось или сервер
/// ответил RST (порт закрыт, но хост достигнут). Таймаут или unreachable —
/// пакет умер в сети раньше, пробуем больший TTL. Блокирующий connect с
/// таймаутом, поэтому зовётся с рабочего потока, не с async-рантайма.
fn reaches(addr: SocketAddr, ttl: u32) -> Reach {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = match Socket::new(domain, Type::STREAM, None) {
        Ok(socket) => socket,
        Err(_) => return Reach::Error,
    };
    let set = if addr.is_ipv4() {
        socket.set_ttl_v4(ttl)
    } else {
        socket.set_unicast_hops_v6(ttl)
    };
    if set.is_err() {
        return Reach::Error;
    }
    match socket.connect_timeout(&addr.into(), PROBE_TIMEOUT) {
        Ok(_) => Reach::Reached,
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => Reach::Reached,
        Err(_) => Reach::NotReached,
    }
}

/// Наименьший IP TTL, при котором SYN доходит до хоста, — оценка числа хопов.
/// None — путь измерить не удалось (ни один TTL не достиг, либо ошибка сокета).
fn hop_count(addr: SocketAddr) -> Option<u8> {
    for ttl in 1..=MAX_TTL {
        match reaches(addr, ttl) {
            Reach::Reached => return Some(ttl as u8),
            Reach::NotReached => continue,
            Reach::Error => return None,
        }
    }
    None
}

/// TTL приманки по ближайшему endpoint'у: на DELTA меньше числа хопов, но не ноль.
/// Берём минимум по всем endpoint'ам, чтобы приманка умирала раньше самого
/// близкого сервера (а значит, раньше всех).
fn decoy_ttl(min_hops: u8, delta: u8) -> u8 {
    min_hops.saturating_sub(delta).max(1)
}

/// Зондирует хопы до всех разобранных endpoint'ов и возвращает TTL для приманок,
/// либо None, если ни один endpoint не удалось измерить (тогда TTL не трогаем).
/// Развёртка блокирующая, поэтому уезжает на рабочий поток.
pub async fn probe_decoy_ttl(endpoints: &[String]) -> Option<u8> {
    let endpoints = endpoints.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut min_hops: Option<u8> = None;
        for endpoint in &endpoints {
            let addr: SocketAddr = match endpoint.parse() {
                Ok(addr) => addr,
                Err(_) => continue,
            };
            if let Some(hops) = hop_count(addr) {
                min_hops = Some(min_hops.map_or(hops, |current| current.min(hops)));
            }
        }
        min_hops.map(|hops| decoy_ttl(hops, DELTA))
    })
    .await
    .ok()
    .flatten()
}

/// Проставляет `ttl` каждой приманке `fake` в YAML-стратегии. Непарсимый YAML
/// или неожиданная форма возвращаются как есть — авто-TTL только дополняет.
pub fn inject_ttl(yaml: &str, ttl: u8) -> String {
    let mut root: Value = match serde_yaml::from_str(yaml) {
        Ok(value) => value,
        Err(_) => return yaml.to_owned(),
    };
    let Some(profiles) = root.get_mut("profiles").and_then(Value::as_sequence_mut) else {
        return yaml.to_owned();
    };
    for profile in profiles {
        if let Some(transform) = profile.get_mut("transform") {
            set_fake_ttl(transform, ttl);
        }
        if let Some(stages) = profile.get_mut("stages").and_then(Value::as_sequence_mut) {
            for stage in stages {
                set_fake_ttl(stage, ttl);
            }
        }
    }
    serde_yaml::to_string(&root).unwrap_or_else(|_| yaml.to_owned())
}

fn set_fake_ttl(stage: &mut Value, ttl: u8) {
    if let Some(fake) = stage.get_mut("fake").and_then(Value::as_mapping_mut) {
        fake.insert(Value::from("ttl"), Value::from(ttl));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoy_ttl_stays_below_hops_and_above_zero() {
        assert_eq!(decoy_ttl(8, 1), 7);
        assert_eq!(decoy_ttl(1, 1), 1);
        assert_eq!(decoy_ttl(0, 1), 1);
        assert_eq!(decoy_ttl(5, 2), 3);
    }

    #[test]
    fn inject_ttl_sets_every_fake() {
        let yaml = "profiles:\n  - name: a\n    transform:\n      action: fake\n      fake:\n        kind: tls-auto\n  - name: b\n    stages:\n      - action: fake\n        fake:\n          kind: quic\n      - action: split\n        positions: ['1']\n";
        let out = inject_ttl(yaml, 6);
        let value: Value = serde_yaml::from_str(&out).unwrap();
        let profiles = value["profiles"].as_sequence().unwrap();
        assert_eq!(profiles[0]["transform"]["fake"]["ttl"], Value::from(6u8));
        assert_eq!(profiles[1]["stages"][0]["fake"]["ttl"], Value::from(6u8));
        // The split stage has no fake, so nothing is added there.
        assert!(profiles[1]["stages"][1].get("fake").is_none());
    }

    #[test]
    fn inject_ttl_leaves_unparsable_or_shapeless_yaml_alone() {
        assert_eq!(inject_ttl("not: [valid", 6), "not: [valid");
        assert_eq!(inject_ttl("version: 1\n", 6), "version: 1\n");
    }
}
