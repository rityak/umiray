//! `config.json` от usque — в запись узла `masque` (D-164).
//!
//! MASQUE в ядре — это Cloudflare WARP, а регистрирует устройство usque: его файл документация
//! ядра и советует. Поля ложатся почти напрямую; ключ сервера у usque в PEM, а ядро хочет
//! голый base64, адресам ядро ждёт маску.

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::yaml::Yaml;

pub struct Usque;

impl Usque {
    /// Похож ли текст на файл usque: без ключа сервера это кто-то другой.
    pub fn looks_like(text: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .is_some_and(|value| value.get("endpoint_pub_key").is_some())
    }

    /// Разобрать файл в запись узла. ponytail: только QUIC-вход `endpoint_v4`; вход HTTP/2
    /// и IPv6-адрес сервера — вторым узлом, когда попросят.
    pub fn to_proxy(text: &str, name: &str) -> Result<Mapping> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| AppError::invalid(format!("Это не файл usque: {e}")))?;
        let field = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let need = |key: &str| {
            field(key).ok_or_else(|| AppError::invalid(format!("В файле usque нет поля {key}")))
        };
        let public: String = need("endpoint_pub_key")?
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();

        let mut proxy = Mapping::new();
        Yaml::set(&mut proxy, "name", Value::from(name));
        Yaml::set(&mut proxy, "type", Value::from("masque"));
        Yaml::set(&mut proxy, "server", Value::from(need("endpoint_v4")?));
        Yaml::set(&mut proxy, "port", Value::from(443));
        Yaml::set(&mut proxy, "private-key", Value::from(need("private_key")?));
        Yaml::set(&mut proxy, "public-key", Value::from(public));
        if let Some(ip) = field("ipv4") {
            Yaml::set(&mut proxy, "ip", Value::from(format!("{ip}/32")));
        }
        if let Some(ip) = field("ipv6") {
            Yaml::set(&mut proxy, "ipv6", Value::from(format!("{ip}/128")));
        }
        Yaml::set(&mut proxy, "udp", Value::from(true));
        Ok(proxy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usque_file_becomes_a_masque_entry() {
        let text = r#"{"private_key":"PRIV","endpoint_v4":"162.159.198.1","endpoint_v6":"2606:4700:103::1",
            "endpoint_pub_key":"-----BEGIN PUBLIC KEY-----\nMFkw\nEwYH\n-----END PUBLIC KEY-----\n",
            "id":"x","access_token":"t","ipv4":"172.16.0.2","ipv6":"2606:4700:110:8::2"}"#;
        assert!(Usque::looks_like(text));
        let entry = Usque::to_proxy(text, "warp").unwrap();
        let field = |key: &str| entry.get(Value::from(key)).and_then(Value::as_str);
        assert_eq!(field("type"), Some("masque"));
        assert_eq!(field("server"), Some("162.159.198.1"));
        assert_eq!(field("public-key"), Some("MFkwEwYH"), "без строк PEM");
        assert_eq!(field("ip"), Some("172.16.0.2/32"));
        assert_eq!(field("ipv6"), Some("2606:4700:110:8::2/128"));
        assert!(
            field("access_token").is_none() && field("id").is_none(),
            "учётные поля Cloudflare ядру не нужны"
        );
    }

    #[test]
    fn other_json_is_not_usque() {
        assert!(!Usque::looks_like(r#"{"proxies": []}"#));
        assert!(!Usque::looks_like("[Interface]"));
        assert!(Usque::to_proxy(r#"{"endpoint_pub_key":"K"}"#, "n").is_err());
    }
}
