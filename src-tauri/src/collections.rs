//! Коллекции: то, что клиент поставляет **данными**, а владеет ими пользователь (D-100).
//!
//! Списков такого рода в клиенте уже три — публичные резолверы, эталонные ресурсы
//! и встроенные наборы правил, — и все они устроены одинаково: образец вшит в бинарь,
//! при первом запуске ложится на диск, дальше это обычный файл. Перекомпилировать клиент
//! ради строки с адресом или доменом — цена, которой не должно быть.
//!
//! **Две формы.** Документ — один файл с известной схемой (`dns.yaml`, `sites.yaml`).
//! Папка — много файлов одной схемы, которые перечисляются на лету (`rules/`): набор
//! правил заводят и удаляют по одному, а список резолверов правят целиком.
//!
//! Типы здесь нарочно «широкие»: `proto` и `filter` — строки, а не перечисления. Файл
//! правит человек, и незнакомое слово не должно ронять всю коллекцию: с ним разбирается
//! тот, кто читает (`diag::dns` не умеет DNSCrypt и говорит об этом строкой в консоли),
//! а не разбор файла.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::paths;

/// Имена коллекций-документов — они же имена файлов без расширения.
pub const DNS: &str = "dns";
pub const SITES: &str = "sites";

/// Имя коллекции-папки со встроенными наборами правил (D-083).
pub const RULES: &str = "rules";

/// Образцы, вшитые в бинарь. Единственное место, где коллекции живут внутри кода,
/// и только затем, чтобы было чем засеять пустую папку.
const DNS_SHIPPED: &str = include_str!("../../collections/dns.yaml");
const SITES_SHIPPED: &str = include_str!("../../collections/sites.yaml");

/// Наборы правил — та же раздача, только папкой. Пара «имя файла, содержимое».
const RULES_SHIPPED: [(&str, &str); 2] = [
    (
        "direct-ru",
        include_str!("../../collections/rules/direct-ru.yaml"),
    ),
    (
        "block-ads",
        include_str!("../../collections/rules/block-ads.yaml"),
    ),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolvers {
    /// Версия формата. Пока одна; появится вторая — по ней и будем переезжать.
    pub version: u32,
    pub providers: Vec<Provider>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub site: String,
    pub variants: Vec<Variant>,
}

/// Набор адресов одного провайдера: с фильтрацией, без неё, семейный. Это разные
/// резолверы — и мерить их надо порознь, поэтому вариант, а не поле у провайдера.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: String,
    pub name: String,
    /// Что режет: `none`, `ads`, `family`, `security`, `bypass`. Слово свободное.
    #[serde(default)]
    pub filter: String,
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    /// `udp`, `tcp`, `doh`, `dot`, `doq`, `doh3` — и что угодно ещё: незнакомое
    /// не ломает файл, его просто некому померить.
    pub proto: String,
    /// Ровно то, что уйдёт в `nameserver:` ядра. Никаких сборок из частей: адрес,
    /// собранный из трёх полей, рано или поздно соберётся не так.
    pub addr: String,
    #[serde(default)]
    pub ipv6: bool,
}

/// Эталонный ресурс: по нему видно, что именно недоступно.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: String,
    pub name: String,
    pub url: String,
    /// `base` · `blocked` · `cdn` — и что угодно ещё: слово свободное, как и `filter`
    /// у резолверов.
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sites {
    pub version: u32,
    pub sites: Vec<Site>,
}

/// Положить образцы, если коллекций ещё нет.
///
/// Папку целиком, а не файл по отдельности: удалённая коллекция не должна возвращаться
/// сама при следующем запуске — иначе «удалить» означало бы «удалить до перезапуска».
/// Переезд со старой раскладки идёт **до** этого вызова и оставляет папку заполненной,
/// поэтому здесь она уже существует и раздача не трогает ничего (D-100).
pub fn seed() -> Result<()> {
    if paths::collections_dir().exists() {
        return Ok(());
    }
    fill_missing()
}

/// Дописать то, чего в коллекциях нет. Нужен переезду: старая установка приносит
/// `catalog/` и `rulesets/` порознь, и любой из них мог отсутствовать.
pub fn fill_missing() -> Result<()> {
    std::fs::create_dir_all(paths::collections_dir())?;
    for (name, shipped) in [(DNS, DNS_SHIPPED), (SITES, SITES_SHIPPED)] {
        let path = paths::collection_file(name);
        if !path.exists() {
            crate::atomic::write(path, shipped)?;
        }
    }
    let rules = paths::collection_folder(RULES);
    std::fs::create_dir_all(&rules)?;
    for (id, shipped) in RULES_SHIPPED {
        let path = rules.join(format!("{id}.yaml"));
        if !path.exists() {
            crate::atomic::write(path, shipped)?;
        }
    }
    Ok(())
}

/// Прочитать коллекцию резолверов. Файла нет — читаем вшитый образец: она нужна окну
/// и без диска, а первый запуск не должен показывать пустой список.
pub fn dns() -> Result<Resolvers> {
    parse_named(DNS, DNS_SHIPPED)
}

/// Прочитать список эталонных ресурсов.
pub fn sites() -> Result<Sites> {
    parse_named(SITES, SITES_SHIPPED)
}

/// Общее чтение документа: файл с диска, а нет его — вшитый образец.
fn parse_named<T: serde::de::DeserializeOwned>(name: &str, shipped: &str) -> Result<T> {
    let text = std::fs::read_to_string(paths::collection_file(name))
        .unwrap_or_else(|_| shipped.to_string());
    read(&text, name)
}

fn read<T: serde::de::DeserializeOwned>(text: &str, name: &str) -> Result<T> {
    serde_yaml::from_str(text).map_err(|e| {
        AppError::invalid(format!(
            "Коллекция «{name}» не читается: {e}. Файл — {}",
            paths::collection_file(name).display()
        ))
    })
}

/// Файлы коллекции-папки: `(идентификатор, содержимое)`, по алфавиту.
///
/// Порядок задаётся здесь, а не тем, как файлы легли на диск: у наборов правил от него
/// зависит порядок строк в собранном конфиге.
pub fn folder(name: &str) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(paths::collection_folder(name)) else {
        return Vec::new();
    };
    let mut items: Vec<(String, String)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yaml"))
        .filter_map(|entry| {
            let id = entry.path().file_stem()?.to_str()?.to_string();
            Some((id, std::fs::read_to_string(entry.path()).ok()?))
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Образцы обязаны читаться своей же схемой: вшитый в бинарь битый файл означал бы,
    /// что на чистой машине раздел не открывается вовсе.
    #[test]
    fn the_shipped_samples_parse() {
        read::<Resolvers>(DNS_SHIPPED, DNS).expect("резолверы");
        read::<Sites>(SITES_SHIPPED, SITES).expect("ресурсы");
        for (id, text) in RULES_SHIPPED {
            let value: serde_yaml::Value = serde_yaml::from_str(text).expect(id);
            assert!(value.get("title").is_some(), "{id}: нет заголовка");
            assert!(value.get("rules").is_some(), "{id}: нет правил");
        }
    }
}
