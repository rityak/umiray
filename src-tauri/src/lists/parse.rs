//! Список доменов и подсетей из того формата, в котором его публикуют (D-157).
//!
//! Смысл выводится из формата, а не настраивается на каждый источник:
//!
//! - JSON sing-box (`{"version":…,"rules":[…]}`) — поля говорят сами за себя;
//! - `payload:` Clash/mihomo — голый домен **точный**, `+.x` — с поддоменами;
//! - простой список по строке — голый домен **с поддоменами**: так пишут antifilter,
//!   itdog и re:filter, и блокировка домена в реестре накрывает его поддомены. Там же
//!   понимаются `domain:`/`full:` v2ray и строки правил вида `DOMAIN-SUFFIX,x`.
//!
//! Чего нейтральный список выразить не может — `keyword`, `regexp`, логика — не
//! переносится и считается пропущенным: половина правила, молча ставшая целым, повела бы
//! трафик не туда.

use std::collections::BTreeSet;
use std::net::IpAddr;

use serde::Deserialize;

use crate::error::{AppError, Result};

/// Что осталось от списка: две нейтральные части и счёт потерь.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Payload {
    /// `+.x` — домен с поддоменами, `x` — ровно он. По алфавиту, без повторов.
    pub domains: Vec<String>,
    /// Подсети `адрес/длина`. Одиночный адрес становится `/32` или `/128`.
    pub cidrs: Vec<String>,
    /// Записи, которых нейтральный список не выражает, и мусор.
    pub skipped: usize,
}

/// Одна запись, как её понял разбор.
enum Entry {
    Suffix(String),
    Exact(String),
    Cidr(String),
    Skip,
    /// Комментарий или пустая строка — не потеря.
    Nothing,
}

/// Копит записи из нескольких тел: у списка бывает несколько адресов (домены отдельно,
/// подсети отдельно), а на диск он ложится одним.
#[derive(Default)]
pub struct ListParser {
    suffix: BTreeSet<String>,
    exact: BTreeSet<String>,
    cidrs: BTreeSet<String>,
    skipped: usize,
}

impl ListParser {
    /// Добавить тело одного адреса. Отказ — только если формат узнан, а не читается:
    /// обрывок JSON означает сломанную загрузку, а не пустой список.
    pub fn feed(&mut self, body: &str) -> Result<()> {
        let body = body.trim_start_matches('\u{feff}');
        let head = body.trim_start();
        if head.starts_with('{') {
            return self.feed_sing_box(head);
        }
        if body
            .lines()
            .any(|line| line.trim_start().starts_with("payload:"))
        {
            return self.feed_payload(body);
        }
        for line in body.lines() {
            let entry = plain(line);
            self.take(entry);
        }
        Ok(())
    }

    pub fn finish(self) -> Payload {
        // Точный домен, который и так накрыт своим же суффиксом, — повтор.
        let exact = self
            .exact
            .into_iter()
            .filter(|name| !self.suffix.contains(name));
        let mut domains: Vec<String> = self
            .suffix
            .iter()
            .map(|name| format!("+.{name}"))
            .chain(exact)
            .collect();
        domains.sort();
        Payload {
            domains,
            cidrs: self.cidrs.into_iter().collect(),
            skipped: self.skipped,
        }
    }

    fn take(&mut self, entry: Entry) {
        match entry {
            Entry::Suffix(name) => {
                self.suffix.insert(name);
            }
            Entry::Exact(name) => {
                self.exact.insert(name);
            }
            Entry::Cidr(net) => {
                self.cidrs.insert(net);
            }
            Entry::Skip => self.skipped += 1,
            Entry::Nothing => {}
        }
    }

    fn feed_sing_box(&mut self, body: &str) -> Result<()> {
        #[derive(Deserialize)]
        struct Source {
            rules: Vec<Headless>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Headless {
            #[serde(rename = "type")]
            kind: Option<String>,
            domain: Vec<String>,
            domain_suffix: Vec<String>,
            domain_keyword: Vec<String>,
            domain_regex: Vec<String>,
            ip_cidr: Vec<String>,
        }
        let source: Source = serde_json::from_str(body)
            .map_err(|e| AppError::invalid(format!("Список JSON не читается: {e}")))?;
        for rule in source.rules {
            if rule.kind.as_deref() == Some("logical") {
                self.skipped += 1;
                continue;
            }
            self.skipped += rule.domain_keyword.len() + rule.domain_regex.len();
            for name in rule.domain {
                self.take(domain(&name).map_or(Entry::Skip, Entry::Exact));
            }
            // `.x` у sing-box — только поддомены, `x` — домен с ними. Нейтральный список
            // различает лишь «с поддоменами» и «ровно он», и `.x` честнее всего расширить
            // до первого: блокировка домена накрывает и его самого (S-028).
            for name in rule.domain_suffix {
                self.take(domain(wildcardless(&name)).map_or(Entry::Skip, Entry::Suffix));
            }
            for net in rule.ip_cidr {
                self.take(cidr(&net).map_or(Entry::Skip, Entry::Cidr));
            }
        }
        Ok(())
    }

    fn feed_payload(&mut self, body: &str) -> Result<()> {
        #[derive(Deserialize)]
        struct Document {
            #[serde(default)]
            payload: Vec<serde_yaml::Value>,
        }
        let document: Document = serde_yaml::from_str(body)
            .map_err(|e| AppError::invalid(format!("Список payload не читается: {e}")))?;
        for item in document.payload {
            let entry = match item.as_str() {
                Some(text) => clash(text),
                None => Entry::Skip,
            };
            self.take(entry);
        }
        Ok(())
    }
}

/// Строка простого списка: голый домен — с поддоменами.
fn plain(line: &str) -> Entry {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with("//") || line.starts_with('!') {
        return Entry::Nothing;
    }
    if let Some(entry) = classical(line) {
        return entry;
    }
    // v2ray: `domain:x`, `full:x`; атрибут `@ads` после пробела смысла не меняет. Любое
    // другое слово после пробела — чужой формат (hosts: `0.0.0.0 x.com`), и первое слово
    // в нём вовсе не запись.
    let mut words = line.split_whitespace();
    let line = words.next().unwrap_or_default();
    if words.any(|word| !word.starts_with('@')) {
        return Entry::Skip;
    }
    if let Some((kind, value)) = line.split_once(':') {
        match kind {
            "domain" => return domain(value).map_or(Entry::Skip, Entry::Suffix),
            "full" => return domain(value).map_or(Entry::Skip, Entry::Exact),
            "keyword" | "regexp" | "include" => return Entry::Skip,
            // Двоеточие ещё бывает в адресе IPv6 — дальше разберёт `cidr`.
            _ => {}
        }
    }
    if let Some(net) = cidr(line) {
        return Entry::Cidr(net);
    }
    domain(wildcardless(line)).map_or(Entry::Skip, Entry::Suffix)
}

/// Запись `payload:`: голый домен — точный, как читает Clash.
fn clash(text: &str) -> Entry {
    let text = text.trim();
    if let Some(entry) = classical(text) {
        return entry;
    }
    if let Some(net) = cidr(text) {
        return Entry::Cidr(net);
    }
    if text.starts_with("+.") || text.starts_with('.') || text.starts_with("*.") {
        return domain(wildcardless(text)).map_or(Entry::Skip, Entry::Suffix);
    }
    domain(text).map_or(Entry::Skip, Entry::Exact)
}

/// Строка правила: `DOMAIN-SUFFIX,x`, `IP-CIDR,x,no-resolve`. `None` — это не она.
fn classical(line: &str) -> Option<Entry> {
    let mut parts = line.split(',').map(str::trim);
    let kind = parts.next()?;
    let value = parts.next()?;
    Some(match kind.to_ascii_uppercase().as_str() {
        "DOMAIN-SUFFIX" => domain(wildcardless(value)).map_or(Entry::Skip, Entry::Suffix),
        "DOMAIN" => domain(value).map_or(Entry::Skip, Entry::Exact),
        "IP-CIDR" | "IP-CIDR6" => cidr(value).map_or(Entry::Skip, Entry::Cidr),
        _ => Entry::Skip,
    })
}

/// `+.x`, `.x`, `*.x` — всё это «x с поддоменами».
fn wildcardless(text: &str) -> &str {
    text.trim_start_matches("+.")
        .trim_start_matches("*.")
        .trim_start_matches('.')
}

/// Домен в том виде, в каком его сравнивает ядро: строчные, без точки на конце (её
/// конвертер mihomo отвергает, S-028), кириллица — в punycode. `None` — не домен.
fn domain(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('.');
    if text.is_empty() {
        return None;
    }
    let name = if text.is_ascii() {
        text.to_ascii_lowercase()
    } else {
        idna::domain_to_ascii(text).ok()?
    };
    let valid = name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        });
    valid.then_some(name)
}

/// Подсеть `адрес/длина` или одиночный адрес. `None` — не адрес.
fn cidr(text: &str) -> Option<String> {
    let text = text.trim();
    let (address, length) = match text.split_once('/') {
        Some((address, length)) => (address, Some(length.parse::<u8>().ok()?)),
        None => (text, None),
    };
    let address: IpAddr = address.parse().ok()?;
    let most = if address.is_ipv4() { 32 } else { 128 };
    let length = length.unwrap_or(most);
    (length <= most).then(|| format!("{address}/{length}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(bodies: &[&str]) -> Payload {
        let mut parser = ListParser::default();
        for body in bodies {
            parser.feed(body).unwrap();
        }
        parser.finish()
    }

    /// antifilter, itdog, re:filter: блокировка домена накрывает поддомены.
    #[test]
    fn a_plain_list_means_each_domain_with_its_subdomains() {
        let out = parsed(&["# список\n.ua\n4PDA.to\nmsplata.ru.\n\n1.1.1.0/24\n2.2.2.2\n"]);
        assert_eq!(out.domains, ["+.4pda.to", "+.msplata.ru", "+.ua"]);
        assert_eq!(out.cidrs, ["1.1.1.0/24", "2.2.2.2/32"]);
        assert_eq!(out.skipped, 0, "комментарий и пустая строка — не потеря");
    }

    /// Clash/mihomo: голый домен точный, `+.` — с поддоменами; подсети в том же списке.
    #[test]
    fn a_payload_keeps_exact_and_suffix_apart() {
        let out = parsed(&[
            "payload:\n  - '+.youtube.com'\n  - 'bilet.nspk.ru'\n  - '1.32.194.67/32'\n  - IP-CIDR6,2001:db8::/32\n",
        ]);
        assert_eq!(out.domains, ["+.youtube.com", "bilet.nspk.ru"]);
        assert_eq!(out.cidrs, ["1.32.194.67/32", "2001:db8::/32"]);
    }

    /// v2ray: `domain:` — с поддоменами, `full:` — ровно он, `keyword:` не переносится.
    #[test]
    fn v2ray_prefixes_are_understood_and_keywords_are_counted_as_lost() {
        let out = parsed(&["domain:zona.media\nfull:ab.chatgpt.com @ads\nkeyword:casino\n"]);
        assert_eq!(out.domains, ["+.zona.media", "ab.chatgpt.com"]);
        assert_eq!(out.skipped, 1);
    }

    /// Строки правил ClashX: `DOMAIN-SUFFIX,.ua` — тот же суффикс, что и `.ua`.
    #[test]
    fn classical_lines_become_the_same_entries() {
        let out = parsed(&[
            "DOMAIN-SUFFIX,.ua\nDOMAIN,x.com\nIP-CIDR,10.0.0.0/8,no-resolve\nDOMAIN-KEYWORD,ads\n",
        ]);
        assert_eq!(out.domains, ["+.ua", "x.com"]);
        assert_eq!(out.cidrs, ["10.0.0.0/8"]);
        assert_eq!(out.skipped, 1);
    }

    /// antizapret: JSON sing-box с кириллическим доменом, точкой на конце и logical-правилом.
    #[test]
    fn a_sing_box_source_is_read_by_its_fields() {
        let out = parsed(&[r#"{"version":1,"rules":[
            {"domain":["кафеэмиль.рф","a.ru."],"domain_suffix":["b.ru",".c.ru"],"domain_regex":["^x"],"ip_cidr":["1.2.3.0/24"]},
            {"type":"logical","mode":"and","rules":[]}
        ]}"#]);
        assert_eq!(
            out.domains[..3],
            ["+.b.ru", "+.c.ru", "a.ru"],
            "`.c.ru` — суффикс sing-box, а не мусор"
        );
        assert!(
            out.domains[3].starts_with("xn--") && out.domains[3].ends_with(".xn--p1ai"),
            "кириллица — в punycode: так имя приходит в DNS и SNI"
        );
        assert_eq!(out.cidrs, ["1.2.3.0/24"]);
        assert_eq!(out.skipped, 2, "regex и logical не переносятся");
    }

    /// Несколько адресов — один список; точный домен под своим же суффиксом — повтор.
    #[test]
    fn two_bodies_become_one_list_without_repeats() {
        let out = parsed(&["telegram.org\n", "payload:\n  - telegram.org\n  - t.me\n"]);
        assert_eq!(out.domains, ["+.telegram.org", "t.me"]);
    }

    /// Мусор не становится доменом: в `.mrs` он уехал бы строкой, которую ядро отвергнет.
    #[test]
    fn nonsense_is_counted_and_not_kept() {
        let out = parsed(&["0.0.0.0 ads.com\nhttp://x.ru/path\n1.2.3.4/40\n-\n"]);
        assert!(out.domains.is_empty());
        assert!(out.cidrs.is_empty(), "hosts-строка — не подсеть 0.0.0.0");
        assert_eq!(out.skipped, 4);
        assert!(ListParser::default().feed("{ broken").is_err());
    }
}
