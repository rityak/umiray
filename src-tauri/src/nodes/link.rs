//! Имена узлов: нормализация и разведение дубликатов — до того, как ссылку увидит ядро.
//!
//! Правим **только имя**, не трогая остальную ссылку. Имя живёт во фрагменте после `#`,
//! и это верно для любой схемы, включая те, которых мы не знаем (D-031). Исключение одно —
//! `vmess://`, где имя лежит в поле `ps` внутри base64-JSON.
//!
//! Почему не отдать это ядру: его `override.proxy-name` чистит грубее — `·` остаётся,
//! ведущий пробел убирается только вторым паттерном (S-012).

use std::collections::HashSet;

use crate::nodes::codec::Base64;

pub struct LinkParser;

impl LinkParser {
    /// Имя без эмодзи и прочего декоративного юникода.
    ///
    /// Оставляем буквы и цифры любого алфавита (кириллица и иероглифы — нормальные имена),
    /// плюс скромный набор пунктуации. Всё прочее становится пробелом, пробелы схлопываются.
    pub fn normalize(raw: &str) -> String {
        const KEEP: &str = "-_.()[]:+/,|@#";
        raw.chars()
            .map(|symbol| {
                if symbol.is_alphanumeric() || KEEP.contains(symbol) {
                    symbol
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Список ссылок с вычищенными и уникальными именами. Порядок сохраняется.
    ///
    /// Уникальность нужна ядру: два узла с одним именем оказываются в группе выбора дважды,
    /// и переключение по имени становится неоднозначным (замерено, S-012). `taken` — имена,
    /// занятые **другими** источниками: ядро складывает все узлы в одну группу, поэтому
    /// разводить надо не внутри списка, а между всеми списками сразу.
    pub fn clean(lines: &[String], taken: &mut HashSet<String>) -> Vec<String> {
        lines
            .iter()
            .map(|line| match LinkParser::name_of(line) {
                Some(name) => {
                    LinkParser::set_name(line, &unique(taken, &LinkParser::normalize(&name)))
                }
                // Имени нет вовсе — ядро назовёт узел само, вмешиваться не во что.
                None => line.clone(),
            })
            .collect()
    }

    /// Ссылки из одного поля ввода. Однострочное поле окна склеивает вставленный список
    /// в строку через пробел, и вторая ссылка уезжала в имя первой — вместе со своим
    /// секретом. Новая ссылка начинается там, где слово начинается со схемы; пробел внутри
    /// имени («#My Node») ссылку не режет.
    pub fn lines_of(input: &str) -> Vec<String> {
        let mut links: Vec<String> = Vec::new();
        for word in input.split_whitespace() {
            match links.last_mut() {
                Some(link) if !starts_with_scheme(word) => {
                    link.push(' ');
                    link.push_str(word);
                }
                _ => links.push(word.to_string()),
            }
        }
        links
    }

    /// Одна ссылка — один узел. `mierus://` несёт несколько пар `port`+`protocol`, и каждая
    /// из них — свой узел (так их разворачивает и ядро). Разворачиваем **до** чистки имён:
    /// имена новых узлов должны развестись вместе со всеми остальными.
    pub fn split(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .flat_map(|line| mieru_split(line).unwrap_or_else(|| vec![line.clone()]))
            .collect()
    }

    /// Адрес и порт узла — то, чего API ядра не отдаёт вовсе (S-012).
    ///
    /// Разбирать протокол для этого не нужно: во всех схемах адрес стоит после `@` (или сразу
    /// после `://`, если логина нет, как у `hysteria://`) и до первого `/`, `?` или `#`.
    /// Работает и для схем, которых мы не знаем.
    ///
    /// `@` ищется только до параметров: в имени узла он законен («Node @home»), а в логине
    /// бывает `/` — base64 у `ss://` — поэтому логин отрезается раньше, чем путь.
    pub fn endpoint_of(line: &str) -> Option<String> {
        if let Some(encoded) = line.strip_prefix("vmess://") {
            let value: serde_json::Value =
                serde_json::from_str(&Base64::decode_text(encoded)?).ok()?;
            let host = value.get("add")?.as_str()?;
            let port = value.get("port")?;
            let port = port
                .as_str()
                .map(str::to_string)
                .or_else(|| Some(port.to_string()))?;
            return Some(format!("{host}:{port}"));
        }
        if let Some(encoded) = line.strip_prefix("tt://?") {
            return tt_fields(encoded)?.remove("address");
        }
        if let Some(encoded) = line.strip_prefix("ssr://") {
            let fields = ssr_fields(encoded)?;
            let (host, port) = (fields.get("server")?, fields.get("port")?);
            return Some(match host.contains(':') {
                true => format!("[{host}]:{port}"),
                false => format!("{host}:{port}"),
            });
        }
        let (_, rest) = line.split_once("://")?;
        let after = match userinfo(line) {
            Some(userinfo) => &rest[userinfo.len() + 1..],
            None => rest,
        };
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let address = Some(after[..end].to_string()).filter(|address| !address.is_empty())?;
        if !line.starts_with("mierus://") {
            return Some(address);
        }
        // Порт у mieru — параметр, а не часть адреса; у диапазона — его начало.
        let (_, port) = query(line)?.into_iter().find(|(key, _)| key == "port")?;
        Some(format!("{address}:{}", port.split('-').next()?))
    }

    /// Что стоит между `://` и `@`. У большинства схем это идентификатор пользователя,
    /// у `wireguard://` — приватный ключ клиента. Раскодировано: ключ в base64 приезжает
    /// с процентами вместо `+` и `=`.
    pub fn userinfo_of(line: &str) -> Option<String> {
        userinfo(line).map(percent_decode)
    }

    /// Логин и пароль до `@`, разделённые по **сырому** двоеточию: закодированное `%3A`
    /// принадлежит паролю. Раскодировать до разделения значило бы резать пароль `pa:ss`.
    pub fn credentials_of(line: &str) -> Option<(String, Option<String>)> {
        let userinfo = userinfo(line)?;
        Some(match userinfo.split_once(':') {
            Some((user, password)) => (percent_decode(user), Some(percent_decode(password))),
            None => (percent_decode(userinfo), None),
        })
    }

    /// Имя узла в ссылке. Нужно и снаружи: чтобы знать, какие имена уже заняты.
    pub fn name_of(line: &str) -> Option<String> {
        if let Some(encoded) = line.strip_prefix("vmess://") {
            return vmess_field(encoded).map(|(name, _)| name);
        }
        if let Some(encoded) = line.strip_prefix("ssr://") {
            return ssr_fields(encoded)?.remove("remarks");
        }
        // По первой решётке, а не по последней: фрагмент — это всё, что после неё, и решётка
        // внутри имени («Node #1») законна. `rsplit` отрезал бы от имени хвост, и таблица
        // показывала бы «1» там, где ядро видит «Node #1».
        let Some((_, fragment)) = line.split_once('#') else {
            // У `tt://?` имя может жить внутри; наше, чистое, дописывается фрагментом.
            return tt_fields(line.strip_prefix("tt://?")?)?.remove("name");
        };
        Some(percent_decode(fragment))
    }

    pub fn set_name(line: &str, name: &str) -> String {
        if let Some(encoded) = line.strip_prefix("vmess://") {
            return match vmess_field(encoded) {
                Some((_, mut value)) => {
                    value["ps"] = serde_json::Value::String(name.to_string());
                    format!("vmess://{}", Base64::encode(value.to_string().as_bytes()))
                }
                None => line.to_string(),
            };
        }
        if let Some(encoded) = line.strip_prefix("ssr://") {
            return ssr_named(encoded, name)
                .map(|body| format!("ssr://{body}"))
                .unwrap_or_else(|| line.to_string());
        }
        // Та же первая решётка, что и в `name_of`: иначе от старого имени остался бы огрызок
        // перед новым.
        let head = line.split_once('#').map_or(line, |(head, _)| head);
        format!("{head}#{}", percent_encode(name))
    }

    /// **Все** поля узла так, как они стоят в ссылке (D-114). Не наш список из шести,
    /// а то, что у узла правда есть: панель показывает его конфиг целиком, как nekoray,
    /// и половина смысла редактора — увидеть поле, о котором не знал.
    ///
    /// Ключи — как в самой ссылке (`fp`, `pbk`, `sid`, `flow`, `obfs-password`); у `vmess://`
    /// это ключи его JSON (`net`, `tls`, `aid`). Имя и адрес сюда не попадают: они не параметры,
    /// и показывает их панель отдельно.
    pub fn params(line: &str) -> std::collections::BTreeMap<String, String> {
        if let Some(encoded) = line.strip_prefix("vmess://") {
            let Some(serde_json::Value::Object(fields)) = vmess_json(encoded) else {
                return Default::default();
            };
            return fields
                .into_iter()
                .filter(|(name, _)| name != "ps" && name != "add" && name != "port")
                .filter_map(|(name, value)| {
                    let text = match value {
                        serde_json::Value::String(text) => text,
                        serde_json::Value::Null => return None,
                        other => other.to_string(),
                    };
                    (!text.is_empty()).then_some((name, text))
                })
                .collect();
        }
        if let Some(encoded) = line.strip_prefix("tt://?") {
            let mut fields = tt_fields(encoded).unwrap_or_default();
            for shown_apart in ["address", "name"] {
                fields.remove(shown_apart);
            }
            return fields;
        }
        if let Some(encoded) = line.strip_prefix("ssr://") {
            let mut fields = ssr_fields(encoded).unwrap_or_default();
            for shown_apart in ["server", "port", "remarks"] {
                fields.remove(shown_apart);
            }
            return fields;
        }
        query(line)
            .unwrap_or_default()
            .into_iter()
            .filter(|(name, value)| !name.is_empty() && !value.is_empty())
            .collect()
    }
}

/// `ssr://` — ссылка целиком в base64:
/// `host:port:protocol:method:obfs:base64(пароль)/?obfsparam=…&remarks=…`, значения
/// параметров — тоже base64. Голова режется **справа**: у IPv6 двоеточия в самом адресе
/// (ядро режет слева и такой узел теряет).
fn ssr_fields(encoded: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let plain = Base64::decode_text(encoded.split('#').next()?)?;
    let (head, query) = plain.split_once("/?").unwrap_or((plain.as_str(), ""));
    let mut parts = head.trim_end_matches('/').rsplitn(6, ':');
    let password = Base64::decode_text(parts.next()?)?;
    let obfs = parts.next()?;
    let method = parts.next()?;
    let protocol = parts.next()?;
    let port = parts.next()?;
    let server = parts.next()?;
    let mut fields: std::collections::BTreeMap<String, String> = [
        ("server", server),
        ("port", port),
        ("protocol", protocol),
        ("method", method),
        ("obfs", obfs),
        ("password", password.as_str()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = Base64::decode_text(value).unwrap_or_default();
        if !value.is_empty() {
            fields.insert(key.to_string(), value);
        }
    }
    Some(fields)
}

/// `tt://?` — TrustTunnel, `DEEP_LINK.md`: base64url от цепочки TLV, тег и длина —
/// varint из RFC 9000. Отдаёт поля под их именами из спецификации; адрес — первый
/// (ponytail: остальные адреса того же сервера теряются, развести их узлами — как mieru).
/// Сертификат — hex всей DER-цепочки: его превращает в пин разбор записи. Версия новее
/// первой — отказ, как велит спецификация.
fn tt_fields(encoded: &str) -> Option<std::collections::BTreeMap<String, String>> {
    let bytes = Base64::decode(encoded.split('#').next()?)?;
    let mut at = 0;
    let varint = |at: &mut usize| -> Option<u64> {
        let first = *bytes.get(*at)?;
        let size = 1usize << (first >> 6);
        let mut value = u64::from(first & 0x3f);
        for byte in bytes.get(*at + 1..*at + size)? {
            value = (value << 8) | u64::from(*byte);
        }
        *at += size;
        Some(value)
    };
    let mut fields = std::collections::BTreeMap::new();
    while at < bytes.len() {
        let tag = varint(&mut at)?;
        let size = usize::try_from(varint(&mut at)?).ok()?;
        let value = bytes.get(at..at.checked_add(size)?)?;
        at += size;
        let text = || String::from_utf8(value.to_vec()).ok();
        let flag = || (value == [1]).to_string();
        let (key, value) = match tag {
            0 => (
                "version",
                value
                    .iter()
                    .fold(0u64, |n, b| (n << 8) | u64::from(*b))
                    .to_string(),
            ),
            1 => ("hostname", text()?),
            // Адресов бывает несколько, берём первый.
            2 if fields.contains_key("address") => continue,
            2 => ("address", text()?),
            3 => ("custom_sni", text()?),
            5 => ("username", text()?),
            6 => ("password", text()?),
            7 => ("skip_verification", flag()),
            8 => (
                "certificate",
                value.iter().map(|b| format!("{b:02x}")).collect(),
            ),
            9 => ("upstream_protocol", value.last()?.to_string()),
            10 => ("anti_dpi", flag()),
            11 => ("client_random_prefix", text()?),
            12 => ("name", text()?),
            // has_ipv6, dns_upstreams и незнакомые теги спецификация велит пропускать.
            _ => continue,
        };
        fields.insert(key.to_string(), value);
    }
    let version: u64 = fields.get("version").map_or(Ok(0), |v| v.parse()).ok()?;
    (version <= 1).then_some(fields)
}

/// Тело `ssr://` с новым `remarks`; остальное — как было, как у прочих схем.
fn ssr_named(encoded: &str, name: &str) -> Option<String> {
    let plain = Base64::decode_text(encoded.split('#').next()?)?;
    let (head, query) = plain.split_once("/?").unwrap_or((plain.as_str(), ""));
    let mut pairs: Vec<String> = query
        .split('&')
        .filter(|pair| !pair.is_empty() && !pair.starts_with("remarks="))
        .map(str::to_string)
        .collect();
    pairs.push(format!("remarks={}", Base64::encode_url(name.as_bytes())));
    let head = head.trim_end_matches('/');
    Some(Base64::encode_url(
        format!("{head}/?{}", pairs.join("&")).as_bytes(),
    ))
}

/// `mierus://` с N парами `port`+`protocol` — N ссылок по одной паре. Имя — фрагмент,
/// иначе `profile`, иначе адрес; у нескольких пар к нему дописывается `порт/протокол`,
/// как у ядра. Пар нет или они не сходятся — ссылка остаётся как есть и честно не разберётся.
fn mieru_split(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("mierus://")?;
    let (head, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (base, raw) = head.split_once('?')?;
    let pairs: Vec<(&str, &str)> = raw
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .collect();
    let values = |name: &str| -> Vec<&str> {
        pairs
            .iter()
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| *value)
            .collect()
    };
    let (ports, protocols) = (values("port"), values("protocol"));
    if ports.is_empty() || ports.len() != protocols.len() {
        return None;
    }
    let rest: Vec<String> = pairs
        .iter()
        .filter(|(key, _)| *key != "port" && *key != "protocol")
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let name = Some(percent_decode(fragment))
        .filter(|name| !name.is_empty())
        .or_else(|| values("profile").first().map(|value| percent_decode(value)))
        .unwrap_or_else(|| base.rsplit('@').next().unwrap_or(base).to_string());
    Some(
        ports
            .iter()
            .zip(protocols)
            .map(|(port, protocol)| {
                let mut query = rest.clone();
                query.push(format!("port={port}"));
                query.push(format!("protocol={protocol}"));
                let label = match ports.len() {
                    1 => name.clone(),
                    _ => format!("{name} {}/{protocol}", percent_decode(port)),
                };
                format!(
                    "mierus://{base}?{}#{}",
                    query.join("&"),
                    percent_encode(&label)
                )
            })
            .collect(),
    )
}

/// Сырой логин: что между `://` и `@`. `@` ищется только до параметров и имени — там он
/// законен («Node @home»), а в логине его нет: там он закодирован.
fn userinfo(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("://")?;
    let head = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    head.split_once('@').map(|(userinfo, _)| userinfo)
}

fn unique(taken: &mut HashSet<String>, base: &str) -> String {
    let base = if base.is_empty() { "server" } else { base };
    let name = (1..)
        .map(|n| {
            if n == 1 {
                base.to_string()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| base.to_string());
    taken.insert(name.clone());
    name
}

/// Имя vmess и разобранный JSON целиком: писать имя обратно надо в тот же документ.
fn vmess_field(encoded: &str) -> Option<(String, serde_json::Value)> {
    let value: serde_json::Value = serde_json::from_str(&Base64::decode_text(encoded)?).ok()?;
    let name = value.get("ps")?.as_str()?.to_string();
    Some((name, value))
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%' && index + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[index + 1..index + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Параметры ссылки как есть. `vmess://` их не имеет — там JSON.
fn query(line: &str) -> Option<Vec<(String, String)>> {
    let head = line.split('#').next()?;
    let (_, query) = head.split_once('?')?;
    Some(
        query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                (name.to_string(), percent_decode(value))
            })
            .collect(),
    )
}

fn vmess_json(encoded: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&Base64::decode_text(encoded)?).ok()
}

/// Слово начинается со схемы: буква, затем буквы, цифры или `+.-`, затем `://`.
fn starts_with_scheme(word: &str) -> bool {
    word.split_once("://").is_some_and(|(scheme, _)| {
        scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_pasted_into_one_line_come_apart() {
        assert_eq!(
            LinkParser::lines_of("vless://a@h:443#One vless://b@h:443#Two"),
            ["vless://a@h:443#One", "vless://b@h:443#Two"]
        );
        assert_eq!(
            LinkParser::lines_of(" vless://a@h:443#My Node\nhy2://b@h:443#Two\n"),
            ["vless://a@h:443#My Node", "hy2://b@h:443#Two"],
            "пробел в имени ссылку не режет, перевод строки режет"
        );
        assert_eq!(
            LinkParser::lines_of("vless://a@h:443#see http://x"),
            ["vless://a@h:443#see", "http://x"],
            "слово со схемой — уже другая ссылка"
        );
    }

    #[test]
    fn names_lose_decoration_but_keep_letters() {
        assert_eq!(LinkParser::normalize("📡 CapyHub · LTE 1"), "CapyHub LTE 1");
        assert_eq!(
            LinkParser::normalize("🇷🇺 Без VPN"),
            "Без VPN",
            "кириллица остаётся"
        );
        assert_eq!(
            LinkParser::normalize("🇩🇪DE-1"),
            "DE-1",
            "флаг без пробела не склеивает"
        );
        assert_eq!(
            LinkParser::normalize("香港 01"),
            "香港 01",
            "иероглифы остаются"
        );
        assert_eq!(
            LinkParser::normalize("  a\u{200d}\u{fe0f}  b  "),
            "a b",
            "ZWJ и вариации — прочь"
        );
        assert_eq!(
            LinkParser::normalize("✨🎉"),
            "",
            "имя только из эмодзи схлопывается в пустое"
        );
    }

    /// Схема не разбирается вовсе: правится один фрагмент, остальное байт в байт.
    #[test]
    fn only_the_name_changes_even_for_an_unknown_scheme() {
        let line =
            "juicity://uuid:pass@juicity.example:443?alpn=h3#%F0%9F%87%B5%F0%9F%87%B1%20Poland";
        let out = &LinkParser::clean(&[line.to_string()], &mut HashSet::new())[0];
        assert!(
            out.starts_with("juicity://uuid:pass@juicity.example:443?alpn=h3#"),
            "тело ссылки не тронуто: {out}"
        );
        assert_eq!(LinkParser::name_of(out).as_deref(), Some("Poland"));
    }

    /// Решётка внутри имени законна, и панели её присылают. По последней имя резалось бы
    /// на «1», а ядро при этом видит «Node #1» — таблица и выбор разъехались бы.
    #[test]
    fn a_name_may_contain_a_hash_of_its_own() {
        let line = "vless://u@a.example:443?type=tcp#Node #1";
        assert_eq!(LinkParser::name_of(line).as_deref(), Some("Node #1"));

        let renamed = LinkParser::set_name(line, "Other");
        assert_eq!(LinkParser::name_of(&renamed).as_deref(), Some("Other"));
        assert!(
            renamed.starts_with("vless://u@a.example:443?type=tcp#"),
            "от старого имени не осталось огрызка: {renamed}"
        );
    }

    /// Логина нет — адрес сразу после `://`; `@` в имени адресом не становится, а `/`
    /// в base64-логине не обрезает его.
    #[test]
    fn the_address_is_found_with_or_without_a_login() {
        let cases = [
            (
                "hysteria://a.example:443?auth=x#Node @home",
                "a.example:443",
            ),
            ("ss://YWVzOnB3Lw==@b.example:8388#S", "b.example:8388"),
            ("trojan://p@c.example:443/?sni=x#N", "c.example:443"),
            ("anytls://p@[::1]:443#A", "[::1]:443"),
        ];
        for (line, address) in cases {
            assert_eq!(
                LinkParser::endpoint_of(line).as_deref(),
                Some(address),
                "{line}"
            );
        }
    }

    /// У `ssr://` всё внутри base64, и имя, и адрес — как у vmess, только не JSON.
    #[test]
    fn ssr_keeps_its_body_and_gets_a_clean_name() {
        let body = format!(
            "2001:db8::1:8388:auth_aes128_sha1:aes-256-cfb:tls1.2_ticket_auth:{}/?obfsparam={}&remarks={}",
            Base64::encode_url(b"pw"),
            Base64::encode_url(b"cdn.example"),
            Base64::encode_url("🇯🇵 Токио".as_bytes())
        );
        let line = format!("ssr://{}", Base64::encode_url(body.as_bytes()));
        assert_eq!(
            LinkParser::endpoint_of(&line).as_deref(),
            Some("[2001:db8::1]:8388")
        );

        let out = &LinkParser::clean(&[line], &mut HashSet::new())[0];
        assert_eq!(LinkParser::name_of(out).as_deref(), Some("Токио"));
        let params = LinkParser::params(out);
        assert_eq!(params["obfsparam"], "cdn.example", "остальное на месте");
        assert_eq!(params["password"], "pw");
        assert!(!params.contains_key("remarks"), "имя показывается отдельно");
    }

    /// Пара `port`+`protocol` — узел; одна ссылка mieru их несёт несколько.
    #[test]
    fn a_mieru_link_splits_into_one_node_per_port() {
        let line = "mierus://u:p@1.2.3.4?handshake-mode=HANDSHAKE_NO_WAIT&port=6666&port=9998-9999&profile=default&protocol=TCP&protocol=UDP";
        let out = LinkParser::split(&[line.to_string()]);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(
            LinkParser::name_of(&out[0]).as_deref(),
            Some("default 6666/TCP")
        );
        assert_eq!(
            LinkParser::endpoint_of(&out[0]).as_deref(),
            Some("1.2.3.4:6666")
        );
        assert_eq!(
            LinkParser::endpoint_of(&out[1]).as_deref(),
            Some("1.2.3.4:9998")
        );
        let params = LinkParser::params(&out[1]);
        assert_eq!(params["port"], "9998-9999");
        assert_eq!(params["protocol"], "UDP");
        assert_eq!(
            params["handshake-mode"], "HANDSHAKE_NO_WAIT",
            "остальное едет в каждую"
        );

        let single = LinkParser::split(&["mierus://u:p@h.example?port=1&protocol=TCP#Мой".into()]);
        assert_eq!(LinkParser::name_of(&single[0]).as_deref(), Some("Мой"));

        let broken = "mierus://u:p@h.example?port=1&port=2&protocol=TCP";
        assert_eq!(
            LinkParser::split(&[broken.into()]),
            [broken],
            "не сходится — как есть"
        );
    }

    /// Вектор собран по `DEEP_LINK.md` руками: пример в README их библиотеки устарел —
    /// длина адреса там `0x03` при одиннадцати байтах, их же декодер его не прочтёт.
    #[test]
    fn a_trusttunnel_deep_link_is_tlv_inside_base64() {
        let mut payload = vec![1, 15];
        payload.extend_from_slice(b"vpn.example.com");
        payload.extend_from_slice(&[2, 11]);
        payload.extend_from_slice(b"1.2.3.4:443");
        payload.extend_from_slice(&[5, 5]);
        payload.extend_from_slice(b"alice");
        // Длина двухбайтовым varint (`0x40 0x09`) — так её вправе написать любой кодировщик.
        payload.extend_from_slice(&[6, 0x40, 9]);
        payload.extend_from_slice(b"secret123");
        let line = &format!("tt://?{}", Base64::encode_url(&payload));
        assert_eq!(
            LinkParser::endpoint_of(line).as_deref(),
            Some("1.2.3.4:443")
        );
        let params = LinkParser::params(line);
        assert_eq!(params["hostname"], "vpn.example.com");
        assert_eq!(params["username"], "alice");
        assert_eq!(params["password"], "secret123");
        assert_eq!(LinkParser::name_of(line), None);

        let named = LinkParser::set_name(line, "Мой");
        assert_eq!(LinkParser::name_of(&named).as_deref(), Some("Мой"));
        assert_eq!(
            LinkParser::endpoint_of(&named).as_deref(),
            Some("1.2.3.4:443")
        );

        // Версия 2 — формат, которого мы не знаем: спецификация велит отказать.
        let future = format!("tt://?{}", Base64::encode_url(&[0, 1, 2]));
        assert_eq!(LinkParser::endpoint_of(&future), None);
    }

    #[test]
    fn a_link_without_a_name_is_left_alone() {
        let bare = "ss://YWVzOnB3@host.example:8388";
        assert_eq!(
            LinkParser::clean(&[bare.to_string()], &mut HashSet::new())[0],
            bare
        );
    }

    #[test]
    fn vmess_keeps_its_json_and_gets_a_clean_name() {
        let json =
            r#"{"v":"2","ps":"🇩🇪 Berlin 1","add":"de.example.org","port":"443","id":"uuid"}"#;
        let line = format!("vmess://{}", Base64::encode(json.as_bytes()));
        let out = &LinkParser::clean(&[line], &mut HashSet::new())[0];
        assert_eq!(LinkParser::name_of(out).as_deref(), Some("Berlin 1"));

        let restored = vmess_field(out.strip_prefix("vmess://").unwrap())
            .unwrap()
            .1;
        assert_eq!(restored["add"], "de.example.org", "остальные поля на месте");
        assert_eq!(restored["port"], "443", "порт остался строкой, как был");
    }

    /// Ядро не разводит одинаковые имена само — в группе появляются два одинаковых узла,
    /// и выбор по имени становится неоднозначным (S-012).
    #[test]
    fn duplicate_names_are_separated() {
        let lines: Vec<String> = ["#Sweden", "#Sweden", "#Sweden", "#%E2%9C%A8"]
            .iter()
            .map(|tail| format!("trojan://pw@a.example:443{tail}"))
            .collect();
        let names: Vec<_> = LinkParser::clean(&lines, &mut HashSet::new())
            .iter()
            .filter_map(|l| LinkParser::name_of(l))
            .collect();
        assert_eq!(names, ["Sweden", "Sweden 2", "Sweden 3", "server"]);
    }
}
