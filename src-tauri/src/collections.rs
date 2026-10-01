//! Коллекции: то, что клиент поставляет **данными**, а владеет ими пользователь (D-100).
//!
//! Списков такого рода в клиенте три — публичные резолверы, каталог rule sets
//! и встроенные наборы правил, — и все они устроены одинаково: образец вшит в бинарь,
//! при первом запуске ложится в таблицу `collections` базы (D-170), дальше это обычный
//! документ. Перекомпилировать клиент ради строки с адресом или доменом — цена, которой
//! не должно быть.
//!
//! **Две формы.** Документ — одна строка с известной схемой (`dns`, `lists`; часть пустая).
//! Папка — много строк одной схемы под одним именем (`rules`, часть — идентификатор): набор
//! правил заводят и удаляют по одному, а список резолверов правят целиком.
//!
//! Типы здесь нарочно «широкие»: `proto` и `filter` — строки, а не перечисления. Документ
//! правит человек, и незнакомое слово не должно ронять всю коллекцию: с ним разбирается
//! тот, кто читает (`diag::dns` не умеет DNSCrypt и говорит об этом строкой в консоли),
//! а не разбор документа.

use serde::{Deserialize, Serialize};

use crate::db::{Db, Table};
use crate::error::{AppError, Result};

/// Имена коллекций-документов.
pub const DNS: &str = "dns";
/// Каталог rule sets, которые клиент умеет скачать (D-157).
pub const LISTS: &str = "lists";

/// Имя коллекции-папки со встроенными наборами правил (D-083).
pub const RULES: &str = "rules";

/// Образцы, вшитые в бинарь. Единственное место, где коллекции живут внутри кода,
/// и только затем, чтобы было чем засеять пустую таблицу.
const DNS_SHIPPED: &str = include_str!("../../collections/dns.yaml");
const LISTS_SHIPPED: &str = include_str!("../../collections/lists.yaml");

/// Наборы правил — та же раздача, только папкой. Пара «идентификатор, содержимое».
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

/// Каталог rule sets (D-157).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCatalog {
    pub version: u32,
    pub lists: Vec<CatalogList>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogList {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_en: Option<String>,
    /// Часть каталога: `blocked`, `services`, `russia`. Слово свободное.
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_en: Option<String>,
    pub urls: Vec<String>,
}

pub struct Collections;

impl Collections {
    /// Положить образцы, если коллекций ещё нет вовсе.
    ///
    /// Целиком и только в пустую таблицу: удалённая коллекция не должна возвращаться сама
    /// при следующем запуске — иначе «удалить» означало бы «удалить до перезапуска». Переезд
    /// с файлов идёт **до** этого вызова и оставляет таблицу заполненной (D-100, D-170).
    pub fn seed() -> Result<()> {
        let empty = [DNS, LISTS, RULES]
            .iter()
            .all(|name| Db::parts(Table::Collections, name).is_ok_and(|rows| rows.is_empty()));
        if !empty {
            return Ok(());
        }
        Db::batch(|batch| {
            for (name, shipped) in [(DNS, DNS_SHIPPED), (LISTS, LISTS_SHIPPED)] {
                batch.put(Table::Collections, name, "", shipped)?;
            }
            for (id, shipped) in RULES_SHIPPED {
                batch.put(Table::Collections, RULES, id, shipped)?;
            }
            Ok(())
        })
    }

    /// Доучить коллекции, перенесённые с файлов (D-170), тому, что новые получают образцом:
    /// перевод названий наборов, каталог rule sets, провайдеры DNS под категории (S-033).
    /// Каждое — один раз: `done` отвечает, стояла ли у старой папки метка этого шага.
    /// Правки владельца сохраняются, удалённое им не возвращается (D-100).
    pub fn upgrade(done: impl Fn(&str) -> bool) -> Result<()> {
        if !done(".rule-titles-v1") {
            for (id, shipped) in RULES_SHIPPED {
                let Some(text) = Db::get(Table::Collections, RULES, id)? else {
                    continue;
                };
                if let Some(updated) = translated_title(&text, shipped) {
                    Db::put(Table::Collections, RULES, id, &updated)?;
                }
            }
        }
        if !done(".lists-v1") && Db::get(Table::Collections, LISTS, "")?.is_none() {
            Db::put(Table::Collections, LISTS, "", LISTS_SHIPPED)?;
        }
        if !done(".dns-v2") {
            if let Some(text) = Db::get(Table::Collections, DNS, "")? {
                if let Some(updated) = with_providers(&text, DNS_SHIPPED, &DNS_ADDED_V2) {
                    Db::put(Table::Collections, DNS, "", &updated)?;
                }
            }
        }
        Ok(())
    }

    /// Прочитать коллекцию резолверов. Строки нет — читаем вшитый образец: она нужна окну
    /// всегда, а первый запуск не должен показывать пустой список.
    pub fn dns() -> Result<Resolvers> {
        parse_named(DNS, DNS_SHIPPED)
    }

    /// Прочитать каталог rule sets (D-157).
    pub fn lists() -> Result<ListCatalog> {
        parse_named(LISTS, LISTS_SHIPPED)
    }

    /// Вшитый образец резолверов — тестам, которым нужна поставка, а не копия на машине.
    #[cfg(test)]
    pub fn shipped_dns() -> Resolvers {
        read(DNS_SHIPPED, DNS).expect("образец резолверов читается")
    }

    /// Положить элемент коллекции-папки. Проверять идентификатор — дело того, кто его принёс.
    pub fn put(folder: &str, id: &str, text: &str) -> Result<()> {
        Db::put(Table::Collections, folder, id, text)
    }

    /// Убрать элемент коллекции-папки.
    pub fn remove(folder: &str, id: &str) -> Result<()> {
        Db::remove(Table::Collections, folder, id)
    }

    /// Элементы коллекции-папки: `(идентификатор, содержимое)`, по алфавиту.
    ///
    /// Порядок задаётся здесь: у наборов правил от него зависит порядок строк в собранном
    /// конфиге.
    pub fn folder(name: &str) -> Vec<(String, String)> {
        Db::parts(Table::Collections, name)
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| !id.is_empty())
            .collect()
    }
}

fn translated_title(text: &str, shipped: &str) -> Option<String> {
    let map = crate::yaml::Yaml::top_mapping(text).ok()?;
    let sample = crate::yaml::Yaml::top_mapping(shipped).ok()?;
    let key = serde_yaml::Value::from("title_en");
    if map.contains_key(&key)
        || map.get(serde_yaml::Value::from("title")) != sample.get(serde_yaml::Value::from("title"))
    {
        return None;
    }
    let title = serde_yaml::to_string(sample.get(&key)?).ok()?;
    let updated = format!("{text}\ntitle_en: {title}");
    crate::yaml::Yaml::top_mapping(&updated).ok()?;
    Some(updated)
}

/// Провайдеры, добавленные в образец под выбор категории DNS (S-033).
const DNS_ADDED_V2: [&str; 4] = ["controld", "mullvad", "dnsforge", "libredns"];

/// Документ резолверов с дописанными блоками образца для тех `ids`, которых в нём нет.
/// `None` — дописывать нечего или итог не читается так, как ждём (например, `providers:`
/// у владельца не последний ключ): тогда файл не трогаем.
fn with_providers(text: &str, shipped: &str, ids: &[&str]) -> Option<String> {
    let own: Resolvers = serde_yaml::from_str(text).ok()?;
    let missing: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| !own.providers.iter().any(|provider| provider.id == *id))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let mut out = text.trim_end().to_string();
    out.push('\n');
    for id in &missing {
        out.push('\n');
        out.push_str(&provider_block(shipped, id)?);
    }
    let after: Resolvers = serde_yaml::from_str(&out).ok()?;
    let complete = missing
        .iter()
        .all(|id| after.providers.iter().any(|provider| provider.id == *id));
    (complete && after.providers.len() == own.providers.len() + missing.len()).then_some(out)
}

/// Текст одного провайдера образца: от его `  - id:` до следующего провайдера
/// или комментария над ним.
fn provider_block(shipped: &str, id: &str) -> Option<String> {
    let head = format!("  - id: {id}");
    let mut lines = shipped.lines().skip_while(|line| line.trim_end() != head);
    let first = lines.next()?;
    let mut block = vec![first];
    block
        .extend(lines.take_while(|line| !line.starts_with("  - id: ") && !line.starts_with("  #")));
    while block.last().is_some_and(|line| line.trim().is_empty()) {
        block.pop();
    }
    Some(block.join("\n") + "\n")
}

/// Общее чтение документа: строка из базы, а нет её — вшитый образец.
fn parse_named<T: serde::de::DeserializeOwned>(name: &str, shipped: &str) -> Result<T> {
    let text = Db::get(Table::Collections, name, "")
        .ok()
        .flatten()
        .unwrap_or_else(|| shipped.to_string());
    read(&text, name)
}

fn read<T: serde::de::DeserializeOwned>(text: &str, name: &str) -> Result<T> {
    serde_yaml::from_str(text)
        .map_err(|e| AppError::invalid(format!("Коллекция «{name}» не читается: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Образцы обязаны читаться своей же схемой: вшитый в бинарь битый файл означал бы,
    /// что на чистой машине раздел не открывается вовсе.
    #[test]
    fn the_shipped_samples_parse() {
        read::<Resolvers>(DNS_SHIPPED, DNS).expect("резолверы");
        let lists = read::<ListCatalog>(LISTS_SHIPPED, LISTS).expect("rule sets");
        for list in &lists.lists {
            assert_eq!(
                crate::slug::Slug::of(&list.id, ""),
                list.id,
                "{}: имя уходит в правило и в путь",
                list.id
            );
            assert!(!list.urls.is_empty(), "{}: неоткуда качать", list.id);
        }
        for (id, text) in RULES_SHIPPED {
            let value: serde_yaml::Value = serde_yaml::from_str(text).expect(id);
            assert!(value.get("title").is_some(), "{id}: нет заголовка");
            assert!(
                value.get("title_en").is_some(),
                "{id}: нет английского заголовка"
            );
            assert!(value.get("rules").is_some(), "{id}: нет правил");
        }
    }

    /// Дописываются только недостающие из названных, текст владельца остаётся как был,
    /// а второй проход ничего не меняет.
    #[test]
    fn new_providers_are_appended_without_touching_the_rest() {
        let own = "# мой комментарий\nversion: 1\nproviders:\n  - id: mine\n    name: Мой\n    variants: []\n  - id: dnsforge\n    name: свой\n    variants: []\n";
        let updated = with_providers(own, DNS_SHIPPED, &DNS_ADDED_V2).unwrap();
        assert!(updated.starts_with(own.trim_end()), "своё переписано");
        let after: Resolvers = serde_yaml::from_str(&updated).unwrap();
        let ids: Vec<&str> = after.providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["mine", "dnsforge", "controld", "mullvad", "libredns"]);
        assert_eq!(after.providers[1].name, "свой", "свой dnsforge не заменён");
        assert!(with_providers(&updated, DNS_SHIPPED, &DNS_ADDED_V2).is_none());
    }

    /// Если `providers:` не последний ключ, дописанное ушло бы не туда — тогда не трогаем.
    #[test]
    fn providers_not_last_means_hands_off() {
        let own = "providers:\n  - id: mine\n    name: Мой\n    variants: []\nversion: 1\n";
        assert!(with_providers(own, DNS_SHIPPED, &DNS_ADDED_V2).is_none());
    }

    #[test]
    fn adding_a_translation_preserves_user_rules_and_names() {
        let shipped = RULES_SHIPPED[1].1;
        let old = "# my comment\ntitle: Блокировка рекламы\nrules: [MATCH,DIRECT]\n";
        let updated = translated_title(old, shipped).unwrap();
        assert!(
            updated.starts_with(old),
            "содержимое и комментарии не переписываются"
        );
        assert!(updated.contains("title_en: Ad blocking"));
        assert!(translated_title(&updated, shipped).is_none());
        assert!(translated_title("title: Mine\nrules: [MATCH,DIRECT]", shipped).is_none());
        assert!(translated_title("%%%", shipped).is_none());
    }
}
