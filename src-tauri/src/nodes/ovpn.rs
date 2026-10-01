//! Конфиг OpenVPN файлом (`.ovpn`) — в запись узла для ядра (D-120, D-164, S-029).
//!
//! Формат — директивы по строке плюс встроенные блоки `<ca>…</ca>`: так файлы отдают
//! панели, провайдеры и `easy-rsa`. Как и с WireGuard, разбираем только то, что понимаем:
//! ключ, вынесенный в соседний файл, или логин с паролем вне файла — отказ с текстом,
//! а не узел, который ядро не поднимет (без `ca` и без учётки конфиг не соберётся вовсе).
//! Имена полей ядра — из `wiki.metacubex.one/config/proxies/openvpn`.

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Встроенные блоки, которые ядро принимает текстом, — имя блока совпадает с ключом ядра.
const BLOCKS: [&str; 6] = ["ca", "cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2"];

pub struct Ovpn;

impl Ovpn {
    /// Похож ли текст на `.ovpn`: у WireGuard вместо директив секции `[Interface]`.
    pub fn looks_like(text: &str) -> bool {
        text.lines()
            .map(str::trim)
            .any(|line| line.starts_with("remote ") || line == "<ca>" || line == "client")
    }

    /// Разобрать файл в запись узла. `name` — имя файла: своего имени у `.ovpn` нет.
    pub fn to_proxy(text: &str, name: &str) -> Result<Mapping> {
        let (directives, blocks) = parse(text);
        let arg = |key: &str| -> Option<&Vec<String>> {
            directives
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, args)| args)
        };
        let first = |key: &str| arg(key).and_then(|args| args.first()).cloned();

        // Первый `remote` — сервер. ponytail: запасные `remote` теряются; развести их
        // узлами, когда попросят.
        let remote = arg("remote").ok_or_else(|| {
            AppError::invalid("Это не конфиг OpenVPN: в нём нет адреса сервера (remote)")
        })?;
        let server = remote.first().cloned().unwrap_or_default();
        let port = remote
            .get(1)
            .cloned()
            .or_else(|| first("port"))
            .unwrap_or_else(|| "1194".into());
        let port: u16 = port
            .parse()
            .map_err(|_| AppError::invalid(format!("Порт сервера не число: {port}")))?;
        let proto = remote
            .get(2)
            .cloned()
            .or_else(|| first("proto"))
            .unwrap_or_else(|| "udp".into());
        // `tcp-client`, `udp6` и прочие написания — у ядра их два.
        let proto = if proto.starts_with("tcp") {
            "tcp"
        } else {
            "udp"
        };

        let ca = blocks.iter().find(|(name, _)| name == "ca");
        if ca.is_none() {
            return Err(AppError::invalid(match arg("ca") {
                Some(_) => "Сертификат сервера (ca) вынесен в отдельный файл — встройте его в .ovpn блоком <ca>",
                None => "Это не конфиг OpenVPN для клиента: в нём нет сертификата сервера (ca)",
            }));
        }
        for block in ["cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2"] {
            if arg(block).is_some() && !blocks.iter().any(|(name, _)| name == block) {
                return Err(AppError::invalid(format!(
                    "Ключ {block} вынесен в отдельный файл — встройте его в .ovpn блоком <{block}>"
                )));
            }
        }

        let mut proxy = Mapping::new();
        Yaml::set(&mut proxy, "name", Value::from(name));
        Yaml::set(&mut proxy, "type", Value::from("openvpn"));
        Yaml::set(&mut proxy, "server", Value::from(server));
        Yaml::set(&mut proxy, "port", Value::from(port));
        Yaml::set(&mut proxy, "proto", Value::from(proto));
        Yaml::set(&mut proxy, "udp", Value::from(true));

        if arg("auth-user-pass").is_some() {
            let pair = blocks
                .iter()
                .find(|(name, _)| name == "auth-user-pass")
                .and_then(|(_, text)| {
                    let mut lines = text.lines().map(str::trim);
                    Some((lines.next()?.to_string(), lines.next()?.to_string()))
                })
                .ok_or_else(|| {
                    AppError::invalid(
                        "Серверу нужны логин и пароль, а в файле их нет — добавьте узел вручную \
                         и вставьте сертификаты из файла",
                    )
                })?;
            Yaml::set(&mut proxy, "username", Value::from(pair.0));
            Yaml::set(&mut proxy, "password", Value::from(pair.1));
        }
        for (name, text) in blocks
            .iter()
            .filter(|(name, _)| BLOCKS.contains(&name.as_str()))
        {
            Yaml::set(&mut proxy, name, Value::from(text.clone()));
        }
        // `tls-auth` пишет направление вторым аргументом или отдельной директивой.
        if let Some(direction) =
            first("key-direction").or_else(|| arg("tls-auth").and_then(|args| args.get(1)).cloned())
        {
            Yaml::set(&mut proxy, "key-direction", Value::from(direction));
        }
        if let Some(cipher) = first("cipher") {
            Yaml::set(&mut proxy, "cipher", Value::from(cipher));
        }
        if let Some(ciphers) = first("data-ciphers").or_else(|| first("ncp-ciphers")) {
            let list = ciphers.split(':').map(Value::from).collect();
            Yaml::set(&mut proxy, "data-ciphers", Value::Sequence(list));
        }
        if let Some(fallback) = first("data-ciphers-fallback") {
            Yaml::set(&mut proxy, "data-ciphers-fallback", Value::from(fallback));
        }
        if let Some(auth) = first("auth") {
            Yaml::set(&mut proxy, "auth", Value::from(auth));
        }
        // `comp-lzo` без аргумента у OpenVPN значит `adaptive`.
        if let Some(args) = arg("comp-lzo") {
            let mode = args.first().cloned().unwrap_or_else(|| "adaptive".into());
            Yaml::set(&mut proxy, "comp-lzo", Value::from(mode));
        }
        Ok(proxy)
    }
}

/// Директивы (имя и аргументы, в порядке файла) и встроенные блоки (имя и текст).
type Parsed = (Vec<(String, Vec<String>)>, Vec<(String, String)>);

fn parse(text: &str) -> Parsed {
    let mut directives = Vec::new();
    let mut blocks = Vec::new();
    let mut open: Option<(String, Vec<&str>)> = None;
    for line in text.lines().map(str::trim) {
        if let Some((name, lines)) = open.as_mut() {
            if line == format!("</{name}>") {
                blocks.push((name.clone(), lines.join("\n")));
                open = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('<')
            .and_then(|rest| rest.strip_suffix('>'))
        {
            open = Some((name.to_string(), Vec::new()));
            continue;
        }
        let mut words = line.split_whitespace().map(str::to_string);
        if let Some(name) = words.next() {
            directives.push((name, words.collect()));
        }
    }
    (directives, blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(entry: &'a Mapping, key: &str) -> Option<&'a Value> {
        entry.get(Value::from(key))
    }

    const CA: &str = "<ca>\n-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n</ca>\n";

    #[test]
    fn a_client_file_becomes_the_entry() {
        let text = format!(
            "client\ndev tun\nproto tcp-client\nremote vpn.example 443\nremote other.example 1194\n\
             cipher AES-256-GCM\ndata-ciphers AES-256-GCM:AES-128-GCM\ncomp-lzo\n\
             {CA}<cert>\nC\n</cert>\n<key>\nK\n</key>\n<tls-auth>\nT\n</tls-auth>\nkey-direction 1\n"
        );
        assert!(Ovpn::looks_like(&text));
        let entry = Ovpn::to_proxy(&text, "Мой").unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "openvpn");
        assert_eq!(field(&entry, "server").unwrap(), "vpn.example");
        assert_eq!(field(&entry, "port").unwrap(), 443);
        assert_eq!(field(&entry, "proto").unwrap(), "tcp");
        assert!(field(&entry, "ca")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("BEGIN"));
        assert_eq!(field(&entry, "cert").unwrap(), "C");
        assert_eq!(field(&entry, "tls-auth").unwrap(), "T");
        assert_eq!(field(&entry, "key-direction").unwrap(), "1");
        assert_eq!(field(&entry, "comp-lzo").unwrap(), "adaptive");
        assert_eq!(
            field(&entry, "data-ciphers")
                .unwrap()
                .as_sequence()
                .unwrap()
                .len(),
            2
        );
    }

    /// Порт и протокол могут прийти отдельными директивами; по умолчанию — 1194 и UDP.
    #[test]
    fn port_and_proto_fall_back_to_their_directives() {
        let entry = Ovpn::to_proxy(&format!("remote 1.2.3.4\nport 30005\n{CA}"), "N").unwrap();
        assert_eq!(field(&entry, "port").unwrap(), 30005);
        assert_eq!(field(&entry, "proto").unwrap(), "udp");
        let bare = Ovpn::to_proxy(&format!("remote 1.2.3.4\n{CA}"), "N").unwrap();
        assert_eq!(field(&bare, "port").unwrap(), 1194);
    }

    /// Логин и пароль берём только встроенными; вне файла — отказ с тем, что делать.
    #[test]
    fn credentials_come_from_the_file_or_not_at_all() {
        let inline = format!(
            "remote a 1\nauth-user-pass\n<auth-user-pass>\nuser\npass\n</auth-user-pass>\n{CA}"
        );
        let entry = Ovpn::to_proxy(&inline, "N").unwrap();
        assert_eq!(field(&entry, "username").unwrap(), "user");
        assert_eq!(field(&entry, "password").unwrap(), "pass");

        let outside = format!("remote a 1\nauth-user-pass creds.txt\n{CA}");
        let error = Ovpn::to_proxy(&outside, "N").unwrap_err();
        assert!(error.to_string().contains("логин"), "{error}");
    }

    /// Ключ в соседнем файле ядро не прочтёт — узел без него не встанет.
    #[test]
    fn a_key_in_another_file_is_refused_by_name() {
        let error = Ovpn::to_proxy("remote a 1\nca ca.crt\n", "N").unwrap_err();
        assert!(error.to_string().contains("<ca>"), "{error}");
        let error =
            Ovpn::to_proxy(&format!("remote a 1\ntls-crypt tc.key\n{CA}"), "N").unwrap_err();
        assert!(error.to_string().contains("<tls-crypt>"), "{error}");
    }

    #[test]
    fn a_wireguard_file_does_not_look_like_openvpn() {
        let wg = "[Interface]\nPrivateKey = x\n[Peer]\nEndpoint = a:1\n";
        assert!(!Ovpn::looks_like(wg));
    }
}
