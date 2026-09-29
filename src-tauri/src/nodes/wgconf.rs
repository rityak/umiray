//! Конфиг WireGuard и AmneziaWG файлом — в запись узла для ядра (D-120).
//!
//! Формат придуман не нами: это `wg-quick`-овский ini с секциями `[Interface]` и `[Peer]`.
//! Его же отдают панели, приложения Amnezia и половина инструкций в сети — принимать его
//! файлом дешевле, чем просить человека переписать поля руками.
//!
//! Разбираем **только то, что понимаем**: чужой формат получает отказ с текстом, а не
//! узел, который молча не работает. Имена полей ядра берутся из его документации
//! (`wiki.metacubex.one/config/proxies/wg`), а не из головы.

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Поля awg в порядке, в каком их пишет сам AmneziaWG. Имя в файле — слева, ключ ядра —
/// справа; совпадают они не всегда только регистром, поэтому таблица, а не `to_lowercase`.
const AWG: [(&str, &str); 11] = [
    ("jc", "jc"),
    ("jmin", "jmin"),
    ("jmax", "jmax"),
    ("s1", "s1"),
    ("s2", "s2"),
    ("s3", "s3"),
    ("s4", "s4"),
    ("h1", "h1"),
    ("h2", "h2"),
    ("h3", "h3"),
    ("h4", "h4"),
];

/// Одна секция ini: ключи в нижнем регистре, значения как есть.
type Section = Vec<(String, String)>;

pub struct WgConf;

impl WgConf {
    /// Разобрать файл в запись узла. `name` — как назвать узел: имени в файле не бывает,
    /// а у узла оно обязано быть.
    pub fn to_proxy(text: &str, name: &str) -> Result<Mapping> {
        let (interface, peer) = sections(text);
        let field = |section: &Section, key: &str| -> Option<String> {
            section
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        let need = |section: &Section, key: &str, what: &str| -> Result<String> {
            field(section, key).ok_or_else(|| {
                AppError::invalid(format!("Это не конфиг WireGuard: в нём нет {what} ({key})"))
            })
        };

        if interface.is_empty() && peer.is_empty() {
            return Err(AppError::invalid(
                "Это не конфиг WireGuard: в файле нет ни [Interface], ни [Peer]",
            ));
        }

        let endpoint = need(&peer, "endpoint", "адреса сервера")?;
        let (server, port) = endpoint
            .rsplit_once(':')
            .ok_or_else(|| AppError::invalid(format!("Адрес сервера без порта: {endpoint}")))?;
        let port: u16 = port
            .trim()
            .parse()
            .map_err(|_| AppError::invalid(format!("Порт сервера не число: {port}")))?;

        let mut proxy = Mapping::new();
        Yaml::set(&mut proxy, "name", Value::from(name));
        Yaml::set(&mut proxy, "type", Value::from("wireguard"));
        Yaml::set(&mut proxy, "server", Value::from(server.trim()));
        Yaml::set(&mut proxy, "port", Value::from(port));
        Yaml::set(
            &mut proxy,
            "private-key",
            Value::from(need(&interface, "privatekey", "своего ключа")?),
        );
        Yaml::set(
            &mut proxy,
            "public-key",
            Value::from(need(&peer, "publickey", "ключа сервера")?),
        );

        // Адрес внутри туннеля: в файле их бывает два через запятую, ядру они называются
        // разными полями.
        let addresses = need(&interface, "address", "адреса в туннеле")?;
        let mut ipv6 = None;
        let mut ipv4 = None;
        for part in addresses.split(',') {
            let part = part
                .trim()
                .split('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if part.contains(':') {
                ipv6 = Some(part);
            } else if !part.is_empty() {
                ipv4 = Some(part);
            }
        }
        Yaml::set(
            &mut proxy,
            "ip",
            Value::from(ipv4.ok_or_else(|| AppError::invalid("В туннеле нет адреса IPv4"))?),
        );
        if let Some(ipv6) = ipv6 {
            Yaml::set(&mut proxy, "ipv6", Value::from(ipv6));
        }
        if let Some(psk) = field(&peer, "presharedkey").filter(|key| !key.is_empty()) {
            Yaml::set(&mut proxy, "pre-shared-key", Value::from(psk));
        }
        if let Some(mtu) = field(&interface, "mtu").and_then(|v| v.parse::<u32>().ok()) {
            Yaml::set(&mut proxy, "mtu", Value::from(mtu));
        }
        if let Some(keepalive) =
            field(&peer, "persistentkeepalive").and_then(|v| v.parse::<u32>().ok())
        {
            Yaml::set(&mut proxy, "persistent-keepalive", Value::from(keepalive));
        }

        // Узел здесь — выход наружу целиком, как и у ссылки (D-063): `AllowedIPs` из файла
        // описывает маршруты внутри системы, а у нас маршрут решают правила.
        let mut allowed = vec![Value::from("0.0.0.0/0")];
        if proxy.contains_key(Value::from("ipv6")) {
            allowed.push(Value::from("::/0"));
        }
        Yaml::set(&mut proxy, "allowed-ips", Value::Sequence(allowed));
        Yaml::set(&mut proxy, "udp", Value::from(true));

        // AmneziaWG: свои поля у узла сильнее общей настройки клиента (D-118) — сервер
        // с обфускацией знает только свои числа.
        let mut awg = Mapping::new();
        for (from, to) in AWG {
            if let Some(value) = field(&interface, from).and_then(|v| v.parse::<u32>().ok()) {
                Yaml::set(&mut awg, to, Value::from(value));
            }
        }
        if !awg.is_empty() {
            Yaml::set(&mut proxy, "amnezia-wg-option", Value::Mapping(awg));
        }
        Ok(proxy)
    }
}

/// Разложить ini на две секции. Незнакомая секция игнорируется целиком: в файлах
/// встречаются и `[Peer]` вторым, и комментарии панели сверху.
fn sections(text: &str) -> (Section, Section) {
    let (mut interface, mut peer) = (Section::new(), Section::new());
    let mut here: Option<&mut Section> = None;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            here = match line.to_ascii_lowercase().as_str() {
                "[interface]" => Some(&mut interface),
                "[peer]" => Some(&mut peer),
                _ => None,
            };
            continue;
        }
        let Some(section) = here.as_deref_mut() else {
            continue;
        };
        if let Some((key, value)) = line.split_once('=') {
            section.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    (interface, peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "\
[Interface]
PrivateKey = cHJpdmF0ZQ==
Address = 10.13.13.2/32, fd00::2/128
MTU = 1420
DNS = 1.1.1.1

[Peer]
PublicKey = cHVibGlj
PresharedKey = cHNr
AllowedIPs = 0.0.0.0/0, ::/0
Endpoint = 45.86.245.83:51820
PersistentKeepalive = 25
";

    #[test]
    fn a_wg_quick_file_becomes_a_node_of_the_core() {
        let proxy = WgConf::to_proxy(PLAIN, "Дом").unwrap();
        let at = |key: &str| proxy.get(Value::from(key)).cloned();
        assert_eq!(at("name"), Some(Value::from("Дом")));
        assert_eq!(at("type"), Some(Value::from("wireguard")));
        assert_eq!(at("server"), Some(Value::from("45.86.245.83")));
        assert_eq!(at("port"), Some(Value::from(51820u16)));
        assert_eq!(at("private-key"), Some(Value::from("cHJpdmF0ZQ==")));
        assert_eq!(at("public-key"), Some(Value::from("cHVibGlj")));
        assert_eq!(at("pre-shared-key"), Some(Value::from("cHNr")));
        assert_eq!(at("ip"), Some(Value::from("10.13.13.2")));
        assert_eq!(at("ipv6"), Some(Value::from("fd00::2")));
        assert_eq!(at("mtu"), Some(Value::from(1420u32)));
        assert_eq!(at("persistent-keepalive"), Some(Value::from(25u32)));
        assert!(at("amnezia-wg-option").is_none(), "обычный wg — без масок");
    }

    /// Файл AmneziaWG отличается только полями обфускации — и они обязаны доехать
    /// **к узлу**: у сервера с awg свои числа, общая настройка клиента тут не подходит.
    #[test]
    fn an_amnezia_file_brings_its_own_masking() {
        let text = PLAIN.replace(
            "MTU = 1420",
            "MTU = 1420\nJc = 4\nJmin = 40\nJmax = 70\nS1 = 15\nS2 = 30\nH1 = 100\nH2 = 200\nH3 = 300\nH4 = 400",
        );
        let proxy = WgConf::to_proxy(&text, "AWG").unwrap();
        let awg = proxy
            .get(Value::from("amnezia-wg-option"))
            .and_then(Value::as_mapping)
            .expect("маска приехала из файла");
        assert_eq!(awg.get(Value::from("jc")), Some(&Value::from(4u32)));
        assert_eq!(awg.get(Value::from("h4")), Some(&Value::from(400u32)));
        assert_eq!(awg.len(), 9, "лишнего не выдумали: {awg:?}");
    }

    #[test]
    fn a_file_we_do_not_understand_is_refused_with_a_reason() {
        assert!(WgConf::to_proxy("совсем не конфиг", "X").is_err());
        assert!(
            WgConf::to_proxy("[Interface]\nPrivateKey = k\n", "X")
                .unwrap_err()
                .to_string()
                .contains("адреса сервера"),
            "отказ называет, чего не хватило"
        );
    }

    /// Комментарии и лишние секции панели не должны ломать разбор.
    #[test]
    fn comments_and_unknown_sections_are_ignored() {
        let text = format!("# выдано панелью\n[Unknown]\nX = 1\n{PLAIN}");
        assert_eq!(
            WgConf::to_proxy(&text, "Дом")
                .unwrap()
                .get(Value::from("server")),
            Some(&Value::from("45.86.245.83"))
        );
    }
}
