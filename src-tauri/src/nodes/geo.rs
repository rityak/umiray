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
use crate::error::Result;
use crate::paths::Paths;
use crate::yaml::Yaml;

/// Поле `client.yaml`: через сколько часов спрашивать заново. Ноль — не спрашивать вовсе.
const HOURS: &str = "geo-hours";

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
    pub fn load() -> Cache {
        std::fs::read_to_string(Paths::geo())
            .ok()
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

    /// Спросить про всё, чему подошёл срок, и запомнить.
    ///
    /// Последовательно и без спешки: узлов десятки, а не тысячи, и торопиться некуда —
    /// зато чужой сервис не получает залп.
    pub async fn refresh() -> Result<()> {
        let hours = GeoCache::hours();
        if hours == 0 {
            return Ok(());
        }
        let addresses: Vec<String> = crate::nodes::source_catalog::SourceCatalog::nodes()
            .into_iter()
            .filter_map(|node| node.address)
            .collect();
        let mut cache = GeoCache::load();
        let want = GeoCache::due(
            &addresses,
            &cache,
            hours,
            crate::stamp::Stamp::now().unwrap_or_default(),
        );
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
}

fn save(cache: &Cache) -> Result<()> {
    Paths::ensure_root()?;
    let text = serde_json::to_string_pretty(cache)
        .map_err(|e| crate::error::AppError::io(format!("Кэш стран не записался: {e}")))?;
    Ok(crate::atomic::AtomicFile::write(Paths::geo(), text)?)
}

/// Страна одного адреса. Снаружи `None` — ответа не было (сеть, отказ сервиса): это
/// не знание, и в кэш оно не ложится. `Some(None)` — сервис ответил, но страны не знает.
/// Список узлов показывается и без флагов, так что отказом это не бывает.
async fn ask(address: &str) -> Option<Option<String>> {
    let Some(ip) = resolve(address).await else {
        return Some(None);
    };
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

/// `host:port` в адрес. Сервис имена хостов не принимает (S-017), поэтому резолвим сами.
async fn resolve(address: &str) -> Option<String> {
    // Уже адрес — не тревожим DNS.
    if let Some((host, _)) = address.rsplit_once(':') {
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Some(host.to_string());
        }
    }
    tokio::net::lookup_host(address)
        .await
        .ok()?
        .next()
        .map(|socket| socket.ip().to_string())
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
