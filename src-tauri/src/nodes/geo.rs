//! Страна узла по его адресу (D-084).
//!
//! Имя узла страну не знает: «Poland 1» может стоять где угодно, а «vless-tls» не говорит
//! ничего. Спрашиваем по адресу — и запоминаем ответ **по `host:port`**: имя панель меняет,
//! адрес нет.
//!
//! Запрос стоит сети, поэтому здесь же и его цена: наружу уходят адреса серверов
//! пользователя. Выключается одним полем `geo-hours: 0` в `client.yaml`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::config::files::Documents;
use crate::config::files::CLIENT;
use crate::db::{Db, Table};
use crate::error::Result;
use crate::yaml::Yaml;

/// Поле `client.yaml`: через сколько часов спрашивать заново. Ноль — не спрашивать вовсе.
const HOURS: &str = "geo-hours";

/// Строка кэша в таблице состояния (D-170).
const ROW: &str = "geo";

/// Неделя. Сервер не переезжает из страны в страну по вторникам, а каждый запрос — это
/// адрес подписки, ушедший наружу.
const DEFAULT_HOURS: u64 = 168;

/// Кого спрашиваем. HTTPS и без ключа; имена хостов не принимает, поэтому адрес
/// разрешаем сами (S-017). ipinfo, а не `api.country.is`: базы спорят о хостингах,
/// и country.is отвечает страной регистрации провайдера — «Poland 1» в Варшаве выходил
/// шведским. ipinfo учитывает геофиды провайдеров, то есть где сервер стоит.
const SERVICE: &str = "https://ipinfo.io/";

/// Через сколько переспрашивать адрес, про который сервис страны не назвал. Не неделя:
/// пустой ответ бывает и от ограничения частоты, а флаг без причины пропадал на неделю.
const UNKNOWN_RETRY: u64 = 3600;

/// Что мы знаем про адрес.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Known {
    /// Код страны из двух букв. Пусто — спросили и не узнали: помним и это, иначе
    /// безымянный адрес спрашивался бы каждый такт.
    #[serde(default)]
    pub country: Option<String>,
    /// Когда спросили, секунды эпохи.
    pub at: u64,
}

pub type Cache = BTreeMap<String, Known>;

pub struct GeoCache;

impl GeoCache {
    /// Страна узла: ответ сервиса по адресу, а если его нет — флаг, который панель
    /// поставила в имя. Флаг — явная метка страны, а не текст вроде «Poland», который
    /// разбирать мы не берёмся (D-084); окно его из имени вырезает и рисует рядом.
    pub fn country(cache: &Cache, address: Option<&str>, name: &str) -> Option<String> {
        address
            .and_then(|address| cache.get(address))
            .and_then(|known| known.country.clone())
            .or_else(|| flag_in(name))
    }

    pub fn load() -> Cache {
        Db::get(Table::State, ROW, "")
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Через сколько часов спрашивать заново; 0 — геозапросы выключены.
    ///
    /// Файл правится руками, и мусор в нём — это умолчание, а не отказ: страна не та вещь,
    /// ради которой стоит не показать список узлов.
    pub fn hours() -> u64 {
        let Ok(text) = Documents::read(CLIENT) else {
            return DEFAULT_HOURS;
        };
        let Ok(map) = Yaml::top_mapping(&text) else {
            return DEFAULT_HOURS;
        };
        match map.get(Value::from(HOURS)) {
            None => DEFAULT_HOURS,
            Some(value) => value.as_u64().unwrap_or(DEFAULT_HOURS),
        }
    }

    /// Записать срок. Точечно, как режим перехвата (D-052): остальное в `client.yaml` не наше.
    pub fn set_hours(hours: u64) -> Result<()> {
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        crate::yaml::Yaml::set(&mut map, HOURS, Value::from(hours));
        let text = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| crate::error::AppError::invalid(e.to_string()))?;
        Documents::write(CLIENT, &text)
    }

    /// Адреса, которые пора спросить: незнакомые и те, чей ответ старше срока.
    pub fn due(addresses: &[String], cache: &Cache, hours: u64, now: u64) -> Vec<String> {
        if hours == 0 {
            return Vec::new();
        }
        let age = hours * 3600;
        let mut want: Vec<String> = addresses
            .iter()
            .filter(|address| match cache.get(*address) {
                None => true,
                Some(known) if known.country.is_none() => {
                    now.saturating_sub(known.at) >= age.min(UNKNOWN_RETRY)
                }
                Some(known) => now.saturating_sub(known.at) >= age,
            })
            .cloned()
            .collect();
        want.sort();
        want.dedup();
        want
    }

    /// Спросить про всё, чему подошёл срок, и запомнить. Это фоновый такт: срок бережёт
    /// чужой сервис от повторов.
    pub async fn refresh() -> Result<()> {
        let hours = GeoCache::hours();
        let cache = GeoCache::load();
        let want = GeoCache::due(
            &addresses(),
            &cache,
            hours,
            crate::stamp::Stamp::now().unwrap_or_default(),
        );
        ask_about(cache, want).await
    }

    /// Спросить заново про все узлы, невзирая на срок: подписку добавили или обновили
    /// по кнопке. Адрес за тем же именем мог сменить страну, а прежний ответ мог быть
    /// неверным — человек нажал «обновить» и ждёт свежих флагов, а не недельных.
    /// `geo-hours: 0` по-прежнему значит «не спрашивать вовсе».
    pub async fn renew() -> Result<()> {
        if GeoCache::hours() == 0 {
            return Ok(());
        }
        let mut want = addresses();
        want.sort();
        want.dedup();
        ask_about(GeoCache::load(), want).await
    }
}

/// Флаг-эмодзи — пара «региональных букв» U+1F1E6…U+1F1FF; берём первую пару в имени.
fn flag_in(name: &str) -> Option<String> {
    let letter = |c: char| {
        let code = c as u32;
        (0x1F1E6..=0x1F1FF)
            .contains(&code)
            .then(|| char::from(b'A' + (code - 0x1F1E6) as u8))
    };
    let chars: Vec<char> = name.chars().collect();
    chars
        .windows(2)
        .find_map(|pair| Some(format!("{}{}", letter(pair[0])?, letter(pair[1])?)))
}

fn addresses() -> Vec<String> {
    crate::nodes::source_catalog::SourceCatalog::nodes()
        .into_iter()
        .filter_map(|node| node.address)
        .collect()
}

/// Последовательно и без спешки: узлов десятки, а не тысячи, и торопиться некуда —
/// зато чужой сервис не получает залп.
async fn ask_about(mut cache: Cache, want: Vec<String>) -> Result<()> {
    if want.is_empty() {
        return Ok(());
    }
    for address in want {
        // Сервис не ответил — не запоминаем ничего: спросим на следующем такте, а не
        // через неделю.
        let Some(country) = ask(&address).await else {
            continue;
        };
        cache.insert(
            address,
            // Часы недоступны — ноль: такая запись просто устареет и спросится заново.
            Known {
                country,
                at: crate::stamp::Stamp::now().unwrap_or_default(),
            },
        );
    }
    save(&cache)
}

fn save(cache: &Cache) -> Result<()> {
    let text = serde_json::to_string_pretty(cache)
        .map_err(|e| crate::error::AppError::io(format!("Кэш стран не записался: {e}")))?;
    Db::put(Table::State, ROW, "", &text)
}

/// Страна одного адреса. Снаружи `None` — ответа не было (сеть, отказ сервиса): это
/// не знание, и в кэш оно не ложится. `Some(None)` — сервис ответил, но страны не знает.
/// Список узлов показывается и без флагов, так что отказом это не бывает.
async fn ask(address: &str) -> Option<Option<String>> {
    // Адрес не разрешился — это сбой сети, а не знание: в кэш он не ложится.
    let ip = resolve(address).await?;
    // Мимо системного прокси: страна нужна серверу, а не нашему туннелю (GOTCHAS).
    let response = crate::http::Http::direct()
        .ok()?
        .get(format!("{SERVICE}{ip}/country"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    Some(country(&response.text().await.ok()?))
}

/// Ответ ipinfo — голый код страны строкой. Проверяем форму: любой другой текст
/// (страница ошибки, пустота) — не страна.
fn country(text: &str) -> Option<String> {
    let code = text.trim().to_uppercase();
    (code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic())).then_some(code)
}

/// Кого спрашиваем, когда система вместо адреса отдала подменный. Адреса, а не имена:
/// имя резолвера разрешилось бы в тот же подменный адрес. JSON-ответ у обоих одинаков.
const RESOLVERS: &[&str] = &[
    "https://1.1.1.1/dns-query?type=A&name=",
    "https://8.8.8.8/resolve?type=A&name=",
];

/// `host:port` в адрес. Сервис имена хостов не принимает (S-017), поэтому резолвим сами.
///
/// Системе верим, пока она не врёт: под TUN с `fake-ip` она отвечает адресом из
/// `198.18.0.0/15`, где сервера быть не может, — ipinfo страны такого не знает, и флаги
/// пропадали ровно у узлов, записанных именем. Тогда спрашиваем DoH: его запрос
/// идёт через туннель, но ответ — настоящий.
async fn resolve(address: &str) -> Option<String> {
    let (host, _) = address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    // Уже адрес — не тревожим DNS.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Some(host.to_string());
    }
    if let Ok(found) = tokio::net::lookup_host(address).await {
        if let Some(ip) = found.map(|socket| socket.ip()).find(|ip| !fake(ip)) {
            return Some(ip.to_string());
        }
    }
    let client = crate::http::Http::direct().ok()?;
    for resolver in RESOLVERS {
        let Ok(response) = client
            .get(format!("{resolver}{host}"))
            .header("accept", "application/dns-json")
            .send()
            .await
        else {
            continue;
        };
        if let Some(ip) = response.text().await.ok().and_then(|text| answer(&text)) {
            return Some(ip);
        }
    }
    None
}

/// Подменный адрес ядра: `198.18.0.0/15` зарезервирован под испытания и в интернете
/// не встречается.
fn fake(ip: &std::net::IpAddr) -> bool {
    matches!(ip, std::net::IpAddr::V4(v4) if v4.octets()[0] == 198 && v4.octets()[1] & 0xFE == 18)
}

/// Первый адрес из ответа DoH в JSON. Ответ чужой — проверяем, что это правда адрес.
fn answer(text: &str) -> Option<String> {
    let reply: serde_json::Value = serde_json::from_str(text).ok()?;
    reply["Answer"]
        .as_array()?
        .iter()
        .filter_map(|record| record["data"].as_str())
        .find_map(|data| data.parse::<std::net::Ipv4Addr>().ok())
        .map(|ip| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_country_is_asked_again_within_the_hour() {
        let mut cache = Cache::new();
        cache.insert(
            "pl1:443".into(),
            Known {
                country: None,
                at: 0,
            },
        );
        let address = vec!["pl1:443".to_string()];
        assert!(GeoCache::due(&address, &cache, 168, 3599).is_empty());
        assert_eq!(GeoCache::due(&address, &cache, 168, 3600), address);
    }

    #[test]
    fn a_flag_in_the_name_stands_in_for_an_unknown_country() {
        let mut cache = Cache::new();
        cache.insert("de:443".into(), known(1));
        assert_eq!(
            GeoCache::country(&cache, Some("de:443"), "\u{1F1F7}\u{1F1FA} LTE").as_deref(),
            Some("NL"),
            "ответ сервиса главнее флага в имени"
        );
        assert_eq!(
            GeoCache::country(&cache, Some("ru:443"), "\u{1F1F7}\u{1F1FA} CapyHub LTE 1")
                .as_deref(),
            Some("RU")
        );
        assert_eq!(
            GeoCache::country(&cache, None, "Poland 1"),
            None,
            "текст не разбираем"
        );
        assert_eq!(
            GeoCache::country(&cache, None, "\u{1F680} fast"),
            None,
            "не флаг"
        );
    }

    #[test]
    fn a_fake_ip_is_not_an_address() {
        assert!(fake(&"198.18.0.4".parse().unwrap()));
        assert!(fake(&"198.19.255.1".parse().unwrap()));
        assert!(!fake(&"198.20.0.1".parse().unwrap()));
        assert!(!fake(&"140.82.121.4".parse().unwrap()));
    }

    #[test]
    fn a_doh_answer_gives_its_first_address() {
        let reply = r#"{"Status":0,"Answer":[{"type":5,"data":"cdn.example."},{"type":1,"data":"140.82.121.4"}]}"#;
        assert_eq!(answer(reply).as_deref(), Some("140.82.121.4"));
        assert_eq!(answer(r#"{"Status":3}"#), None);
        assert_eq!(answer("<html>"), None);
    }

    #[test]
    fn only_a_country_code_is_a_country() {
        assert_eq!(country("pl\n").as_deref(), Some("PL"));
        assert_eq!(country("{\"error\": 429}"), None);
        assert_eq!(country(""), None);
    }

    fn known(at: u64) -> Known {
        Known {
            country: Some("NL".into()),
            at,
        }
    }

    #[test]
    fn only_the_unknown_and_the_stale_are_asked_about() {
        let mut cache = Cache::new();
        cache.insert("fresh:443".into(), known(1000));
        cache.insert("stale:443".into(), known(1));
        let addresses = vec![
            "fresh:443".to_string(),
            "stale:443".to_string(),
            "new:443".to_string(),
            "new:443".to_string(),
        ];
        // Срок — час; «сейчас» — 4000-я секунда.
        assert_eq!(
            GeoCache::due(&addresses, &cache, 1, 4000),
            ["new:443", "stale:443"],
            "свежий не спрашивается, повтор не задваивается"
        );
    }

    /// Ноль часов — это «не спрашивать вовсе», а не «спрашивать всегда».
    #[test]
    fn zero_hours_turns_the_service_off() {
        let addresses = vec!["new:443".to_string()];
        assert!(GeoCache::due(&addresses, &Cache::new(), 0, 4000).is_empty());
    }

    /// Ответ «не узнали» тоже помнится: иначе безымянный адрес спрашивался бы каждый такт.
    #[test]
    fn a_negative_answer_is_remembered_too() {
        let mut cache = Cache::new();
        cache.insert(
            "nowhere:443".into(),
            Known {
                country: None,
                at: 3900,
            },
        );
        let addresses = vec!["nowhere:443".to_string()];
        assert!(GeoCache::due(&addresses, &cache, 1, 4000).is_empty());
    }
}
