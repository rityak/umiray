//! Каталог узлов, прочитанный из опубликованных источников.

use crate::nodes::entries::EntryPatch;
use crate::nodes::link::LinkParser;
use crate::nodes::source_build::SourceBuilder;
use crate::nodes::sources::built_proxies;
use crate::nodes::sources::SourceStore;

/// Узел так, как он виден **из файла**. Это и есть состав списка (D-061): полный,
/// в порядке файла и доступный до подключения.
struct Parsed {
    name: String,
    kind: String,
    address: Option<String>,
    /// Дойдёт ли узел до ядра — сам или через наш шов (D-063).
    supported: bool,
}

/// Разбор файла источника в узлы. Форматов два (D-019), и различаются они наличием ссылок:
/// список `vless://…` или clash-YAML с разделом `proxies`.
///
/// Это **не** разбор протоколов, который по D-031 отдан ядру: отсюда читаются только имя,
/// вид и адрес — то, что нужно, чтобы показать таблицу. Всё остальное про узел по-прежнему
/// знает ядро.
fn parse(text: &str) -> Vec<Parsed> {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(text) else {
        return Vec::new();
    };
    let Some(proxies) = value
        .get("proxies")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return Vec::new();
    };
    proxies
        .iter()
        .filter_map(|proxy| {
            let name = proxy.get("name")?.as_str()?.to_string();
            let address = match (proxy.get("server"), proxy.get("port")) {
                (Some(host), Some(port)) => Some(format!(
                    "{}:{}",
                    host.as_str().unwrap_or_default(),
                    port.as_u64().unwrap_or_default()
                )),
                _ => None,
            };
            Some(Parsed {
                name,
                kind: kind_of(
                    proxy
                        .get("type")
                        .and_then(serde_yaml::Value::as_str)
                        .unwrap_or(""),
                ),
                address,
                // Запись ядро читает как есть. А вот **по имени её не взять**: она лежит
                // в провайдере, а группа видит провайдер только через `use:` (S-012,
                // перепроверено на D-122: `'узел' not found`). Поимённых узлов у нас
                // больше нет вовсе — с тех пор, как всякий узел лежит в провайдере.
                supported: true,
            })
        })
        .collect()
}

/// Имя протокола так, как его пишет ядро: `vless` → `Vless`. Нужно, чтобы таблица
/// не менялась при подключении — там эти имена приходят от него.
fn kind_of(raw: &str) -> String {
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub struct SourceCatalog;

impl SourceCatalog {
    /// Узлы всех источников, прочитанные прямо с диска. **Это и есть список** (D-061).
    ///
    /// Порядок — источники, как их отдаёт `list()`, и внутри каждого порядок строк файла:
    /// он не меняется от опроса к опросу и не зависит от того, работает ли ядро. Задержки
    /// здесь нет: её приносит отдельный замер (D-062).
    pub fn nodes() -> Vec<crate::nodes::Node> {
        SourceStore::list()
            .into_iter()
            .flat_map(|source| {
                let id = source.id;
                // Правки читаем **раз на источник**, а не на узел: список опрашивается
                // каждую секунду, а узлов в подписке бывает две сотни.
                let written = EntryPatch::load(&id);
                let keys: Vec<Option<String>> = built_proxies(&id)
                    .iter()
                    .map(SourceBuilder::identity_of)
                    .collect();
                let mine = id.clone();
                let known = parse(&SourceStore::content(&id))
                    .into_iter()
                    .enumerate()
                    .map(move |(at, parsed)| {
                        let identity = keys.get(at).cloned().flatten();
                        let recoded = identity.as_ref().is_some_and(|key| {
                            written.get(key).is_some_and(|patch| !patch.is_empty())
                        });
                        crate::nodes::Node {
                            name: parsed.name,
                            kind: parsed.kind,
                            source: mine.clone(),
                            supported: parsed.supported,
                            delay: None,
                            method: None,
                            fallback: false,
                            address: parsed.address,
                            // Страну дописывает `enrich`: её знает кэш, а не файл источника.
                            country: None,
                            edited: recoded,
                        }
                    });
                // Чего разбор не осилил — видно, но не работает (D-122). Не показать вовсе
                // значило бы, что узел пропал молча.
                let left = id.clone();
                let missed = source
                    .skipped
                    .into_iter()
                    .map(move |line| crate::nodes::Node {
                        name: LinkParser::name_of(&line).unwrap_or_else(|| "без имени".into()),
                        kind: kind_of(line.split("://").next().unwrap_or_default()),
                        source: left.clone(),
                        supported: false,
                        delay: None,
                        method: None,
                        fallback: false,
                        address: LinkParser::endpoint_of(&line),
                        country: None,
                        edited: false,
                    });
                known.chain(missed)
            })
            .collect()
    }
}
