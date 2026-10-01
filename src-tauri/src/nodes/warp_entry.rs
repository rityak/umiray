//! Ответ Cloudflare об устройстве WARP — в запись узла (D-165).
//!
//! Отдельно от разговора с API: сборка записи проверяется без сети, а ответ, в котором нет
//! адресов или ключа сервера, становится отказом, а не узлом, который не встанет.

use serde::Deserialize;
use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::nodes::codec::Base64;
use crate::nodes::warp::WgKeys;
use crate::yaml::Yaml;

/// API выдаёт `162.159.198.1`, но по TCP это обычный край CDN; MASQUE слушает `.2` (Throne).
const MASQUE_ENDPOINT: &str = "162.159.198.2";
const WIREGUARD_ENDPOINT: (&str, u16) = ("engage.cloudflareclient.com", 2408);
/// Туннель WARP узкий: MTU сверх 1280 в нём дробится (usque, Throne пишут то же).
const MTU: u16 = 1280;

/// Устройство так, как его отдаёт API. `id` и `token` приходят только на регистрацию.
#[derive(Deserialize)]
pub(super) struct Device {
    #[serde(default)]
    pub(super) id: String,
    #[serde(default)]
    pub(super) token: String,
    config: Config,
}

#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    client_id: String,
    interface: Interface,
    peers: Vec<Peer>,
}

#[derive(Deserialize)]
struct Interface {
    addresses: Addresses,
}

#[derive(Deserialize)]
struct Addresses {
    #[serde(default)]
    v4: String,
    #[serde(default)]
    v6: String,
}

#[derive(Deserialize)]
struct Peer {
    public_key: String,
    #[serde(default)]
    endpoint: Option<Endpoint>,
}

#[derive(Deserialize)]
struct Endpoint {
    #[serde(default)]
    host: String,
}

/// Общее у обоих туннелей: адреса в туннеле и ключ сервера. Без них узел не встанет.
fn peer_and_addresses(device: &Device) -> Result<(&Peer, &Addresses)> {
    let broken = || AppError::network("В ответе Cloudflare нет адресов или ключа сервера");
    let peer = device.config.peers.first().ok_or_else(broken)?;
    let addresses = &device.config.interface.addresses;
    if peer.public_key.is_empty() || (addresses.v4.is_empty() && addresses.v6.is_empty()) {
        return Err(broken());
    }
    Ok((peer, addresses))
}

fn common(name: &str, kind: &str, server: &str, port: u16) -> Mapping {
    let mut entry = Mapping::new();
    Yaml::set(&mut entry, "name", Value::from(name));
    Yaml::set(&mut entry, "type", Value::from(kind));
    Yaml::set(&mut entry, "server", Value::from(server));
    Yaml::set(&mut entry, "port", Value::from(port));
    entry
}

/// WireGuard: `reserved` — три байта `client_id`, без них WARP молчит (S-010). `mask` —
/// маскировка рукопожатия (D-118): кладётся там, где узел рождается, как у ссылки.
pub(super) fn wireguard_entry(
    device: &Device,
    keys: &WgKeys,
    mask: Option<Value>,
) -> Result<Mapping> {
    let (peer, addresses) = peer_and_addresses(device)?;
    let reserved = Base64::decode(&device.config.client_id)
        .filter(|bytes| bytes.len() == 3)
        .ok_or_else(|| AppError::network("В ответе Cloudflare нет client_id для reserved"))?;
    // `engage.cloudflareclient.com:2408`; без него — известный адрес.
    let (server, port) = peer
        .endpoint
        .as_ref()
        .and_then(|endpoint| endpoint.host.rsplit_once(':'))
        .and_then(|(host, port)| Some((host.to_string(), port.parse().ok()?)))
        .unwrap_or((WIREGUARD_ENDPOINT.0.to_string(), WIREGUARD_ENDPOINT.1));
    let mut entry = common("Cloudflare WARP", "wireguard", &server, port);
    Yaml::set(&mut entry, "private-key", Value::from(keys.private.clone()));
    Yaml::set(
        &mut entry,
        "public-key",
        Value::from(peer.public_key.clone()),
    );
    let mut outward = Vec::new();
    if !addresses.v4.is_empty() {
        Yaml::set(&mut entry, "ip", Value::from(addresses.v4.clone()));
        outward.push(Value::from("0.0.0.0/0"));
    }
    if !addresses.v6.is_empty() {
        Yaml::set(&mut entry, "ipv6", Value::from(addresses.v6.clone()));
        outward.push(Value::from("::/0"));
    }
    let reserved = reserved.into_iter().map(Value::from).collect();
    Yaml::set(&mut entry, "reserved", Value::Sequence(reserved));
    Yaml::set(&mut entry, "mtu", Value::from(MTU));
    Yaml::set(&mut entry, "allowed-ips", Value::Sequence(outward));
    Yaml::set(&mut entry, "udp", Value::from(true));
    if let Some(mask) = mask {
        Yaml::set(&mut entry, "amnezia-wg-option", mask);
    }
    Ok(entry)
}

/// MASQUE: ключ сервера приходит PEM, ядро ждёт голый base64 (`x509.ParsePKIXPublicKey`).
pub(super) fn masque_entry(device: &Device, private: &str) -> Result<Mapping> {
    let (peer, addresses) = peer_and_addresses(device)?;
    let public: String = peer
        .public_key
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let mut entry = common("Cloudflare WARP MASQUE", "masque", MASQUE_ENDPOINT, 443);
    Yaml::set(&mut entry, "private-key", Value::from(private));
    Yaml::set(&mut entry, "public-key", Value::from(public));
    if !addresses.v4.is_empty() {
        let ip = format!("{}/32", addresses.v4);
        Yaml::set(&mut entry, "ip", Value::from(ip));
    }
    if !addresses.v6.is_empty() {
        let ip = format!("{}/128", addresses.v6);
        Yaml::set(&mut entry, "ipv6", Value::from(ip));
    }
    Yaml::set(&mut entry, "mtu", Value::from(MTU));
    Yaml::set(&mut entry, "udp", Value::from(true));
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(entry: &'a Mapping, key: &str) -> Option<&'a Value> {
        entry.get(Value::from(key))
    }

    /// Ответ `POST /reg` в том виде, в каком его показывают wgcf и usque.
    fn registered(peer_key: &str) -> Device {
        serde_json::from_str(&format!(
            r#"{{"id":"dev","token":"tok","account":{{"license":"L"}},
               "config":{{"client_id":"AQID",
                 "interface":{{"addresses":{{"v4":"172.16.0.2","v6":"2606:4700:110:8::2"}}}},
                 "peers":[{{"public_key":{peer_key},
                   "endpoint":{{"host":"engage.cloudflareclient.com:2408","v4":"162.159.192.1:0"}}}}]}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn a_wireguard_device_becomes_a_node_with_reserved_and_the_mask() {
        let keys = WgKeys {
            private: "PRIV".into(),
            public: "PUB".into(),
        };
        let mask = Some(Value::from("маска"));
        let entry = wireguard_entry(&registered(r#""cGVlcg==""#), &keys, mask).unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "wireguard");
        assert_eq!(
            field(&entry, "server").unwrap(),
            "engage.cloudflareclient.com"
        );
        assert_eq!(field(&entry, "port").unwrap(), 2408);
        assert_eq!(field(&entry, "private-key").unwrap(), "PRIV");
        assert_eq!(field(&entry, "public-key").unwrap(), "cGVlcg==");
        let reserved = field(&entry, "reserved").unwrap().as_sequence().unwrap();
        assert_eq!(reserved, &[Value::from(1), Value::from(2), Value::from(3)]);
        assert_eq!(field(&entry, "ipv6").unwrap(), "2606:4700:110:8::2");
        let outward = field(&entry, "allowed-ips").unwrap().as_sequence().unwrap();
        assert_eq!(outward.len(), 2);
        assert!(field(&entry, "amnezia-wg-option").is_some(), "маска D-118");
    }

    #[test]
    fn a_masque_device_gets_the_bare_server_key_and_masks_on_addresses() {
        let pem = r#""-----BEGIN PUBLIC KEY-----\nMFkwEwYH\nKoZIzj0C\n-----END PUBLIC KEY-----\n""#;
        let entry = masque_entry(&registered(pem), "SEC1").unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "masque");
        assert_eq!(field(&entry, "server").unwrap(), MASQUE_ENDPOINT);
        assert_eq!(field(&entry, "public-key").unwrap(), "MFkwEwYHKoZIzj0C");
        assert_eq!(field(&entry, "ip").unwrap(), "172.16.0.2/32");
        assert_eq!(field(&entry, "ipv6").unwrap(), "2606:4700:110:8::2/128");
        assert!(field(&entry, "token").is_none(), "токен узлу не нужен");
    }

    /// Ответ без адресов или ключа — отказ, а не узел, который не встанет.
    #[test]
    fn a_device_without_keys_or_addresses_is_refused() {
        let empty: Device = serde_json::from_str(
            r#"{"config":{"interface":{"addresses":{}},"peers":[{"public_key":"K"}]}}"#,
        )
        .unwrap();
        assert!(masque_entry(&empty, "k").is_err());
        let keys = WgKeys {
            private: "a".into(),
            public: "b".into(),
        };
        let mut no_client = registered(r#""K""#);
        no_client.config.client_id = String::new();
        assert!(wireguard_entry(&no_client, &keys, None).is_err());
    }
}
