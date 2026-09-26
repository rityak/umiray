//! Имена узлов: нормализация и разведение дубликатов — до того, как ссылку увидит ядро.
//!
//! Правим **только имя**, не трогая остальную ссылку. Имя живёт во фрагменте после `#`,
//! и это верно для любой схемы, включая те, которых мы не знаем (D-031). Исключение одно —
//! `vmess://`, где имя лежит в поле `ps` внутри base64-JSON.
//!
//! Почему не отдать это ядру: его `override.proxy-name` чистит грубее — `·` остаётся,
//! ведущий пробел убирается только вторым паттерном (S-012).

use std::collections::HashSet;

use crate::nodes::codec;

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
        .map(|line| match name_of(line) {
            Some(name) => set_name(line, &unique(taken, &normalize(&name))),
            // Имени нет вовсе — ядро назовёт узел само, вмешиваться не во что.
            None => line.clone(),
        })
        .collect()
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

/// Адрес и порт узла — то, чего API ядра не отдаёт вовсе (S-012).
///
/// Разбирать протокол для этого не нужно: во всех схемах адрес стоит после `@` и до первого
/// `/`, `?` или `#`. Работает и для схем, которых мы не знаем.
pub fn endpoint_of(line: &str) -> Option<String> {
    if let Some(encoded) = line.strip_prefix("vmess://") {
        let value: serde_json::Value = serde_json::from_str(&codec::decode_text(encoded)?).ok()?;
        let host = value.get("add")?.as_str()?;
        let port = value.get("port")?;
        let port = port
            .as_str()
            .map(str::to_string)
            .or_else(|| Some(port.to_string()))?;
        return Some(format!("{host}:{port}"));
    }
    let (_, after) = line.split_once('@')?;
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    Some(after[..end].to_string()).filter(|address| !address.is_empty())
}

/// Что стоит между `://` и `@`. У большинства схем это идентификатор пользователя,
/// у `wireguard://` — приватный ключ клиента. Раскодировано: ключ в base64 приезжает
/// с процентами вместо `+` и `=`.
pub fn userinfo_of(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("://")?;
    let (userinfo, _) = rest.split_once('@')?;
    Some(percent_decode(userinfo))
}

/// Имя узла в ссылке. Нужно и снаружи: чтобы знать, какие имена уже заняты.
pub fn name_of(line: &str) -> Option<String> {
    if let Some(encoded) = line.strip_prefix("vmess://") {
        return vmess_field(encoded).map(|(name, _)| name);
    }
    // По первой решётке, а не по последней: фрагмент — это всё, что после неё, и решётка
    // внутри имени («Node #1») законна. `rsplit` отрезал бы от имени хвост, и таблица
    // показывала бы «1» там, где ядро видит «Node #1».
    let (_, fragment) = line.split_once('#')?;
    Some(percent_decode(fragment))
}

pub fn set_name(line: &str, name: &str) -> String {
    if let Some(encoded) = line.strip_prefix("vmess://") {
        return match vmess_field(encoded) {
            Some((_, mut value)) => {
                value["ps"] = serde_json::Value::String(name.to_string());
                format!("vmess://{}", codec::encode(value.to_string().as_bytes()))
            }
            None => line.to_string(),
        };
    }
    // Та же первая решётка, что и в `name_of`: иначе от старого имени остался бы огрызок
    // перед новым.
    let head = line.split_once('#').map_or(line, |(head, _)| head);
    format!("{head}#{}", percent_encode(name))
}

/// Имя vmess и разобранный JSON целиком: писать имя обратно надо в тот же документ.
fn vmess_field(encoded: &str) -> Option<(String, serde_json::Value)> {
    let value: serde_json::Value = serde_json::from_str(&codec::decode_text(encoded)?).ok()?;
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
    query(line)
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, value)| !name.is_empty() && !value.is_empty())
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
    serde_json::from_str(&codec::decode_text(encoded)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_decoration_but_keep_letters() {
        assert_eq!(normalize("📡 CapyHub · LTE 1"), "CapyHub LTE 1");
        assert_eq!(normalize("🇷🇺 Без VPN"), "Без VPN", "кириллица остаётся");
        assert_eq!(normalize("🇩🇪DE-1"), "DE-1", "флаг без пробела не склеивает");
        assert_eq!(normalize("香港 01"), "香港 01", "иероглифы остаются");
        assert_eq!(
            normalize("  a\u{200d}\u{fe0f}  b  "),
            "a b",
            "ZWJ и вариации — прочь"
        );
        assert_eq!(
            normalize("✨🎉"),
            "",
            "имя только из эмодзи схлопывается в пустое"
        );
    }

    /// Схема не разбирается вовсе: правится один фрагмент, остальное байт в байт.
    #[test]
    fn only_the_name_changes_even_for_an_unknown_scheme() {
        let line = "tuic://uuid:pass@tuic.example:443?alpn=h3#%F0%9F%87%B5%F0%9F%87%B1%20Poland";
        let out = &clean(&[line.to_string()], &mut HashSet::new())[0];
        assert!(
            out.starts_with("tuic://uuid:pass@tuic.example:443?alpn=h3#"),
            "тело ссылки не тронуто: {out}"
        );
        assert_eq!(name_of(out).as_deref(), Some("Poland"));
    }

    /// Решётка внутри имени законна, и панели её присылают. По последней имя резалось бы
    /// на «1», а ядро при этом видит «Node #1» — таблица и выбор разъехались бы.
    #[test]
    fn a_name_may_contain_a_hash_of_its_own() {
        let line = "vless://u@a.example:443?type=tcp#Node #1";
        assert_eq!(name_of(line).as_deref(), Some("Node #1"));

        let renamed = set_name(line, "Other");
        assert_eq!(name_of(&renamed).as_deref(), Some("Other"));
        assert!(
            renamed.starts_with("vless://u@a.example:443?type=tcp#"),
            "от старого имени не осталось огрызка: {renamed}"
        );
    }

    #[test]
    fn a_link_without_a_name_is_left_alone() {
        let bare = "ss://YWVzOnB3@host.example:8388";
        assert_eq!(clean(&[bare.to_string()], &mut HashSet::new())[0], bare);
    }

    #[test]
    fn vmess_keeps_its_json_and_gets_a_clean_name() {
        let json =
            r#"{"v":"2","ps":"🇩🇪 Berlin 1","add":"de.example.org","port":"443","id":"uuid"}"#;
        let line = format!("vmess://{}", codec::encode(json.as_bytes()));
        let out = &clean(&[line], &mut HashSet::new())[0];
        assert_eq!(name_of(out).as_deref(), Some("Berlin 1"));

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
        let names: Vec<_> = clean(&lines, &mut HashSet::new())
            .iter()
            .filter_map(|l| name_of(l))
            .collect();
        assert_eq!(names, ["Sweden", "Sweden 2", "Sweden 3", "server"]);
    }
}
