//! Группы узлов формой: разбор `proxy-groups` в то, чем распоряжается окно, и сборка
//! обратно (D-074, образец — `config/mode.rs`).
//!
//! Работает с **текстом**, а не с диском: форма правит тот же черновик, что открыт
//! в коде, и записывает его та же кнопка «Сохранить». Поэтому здесь нет ни `files::read`,
//! ни `files::write` — только две чистые функции, которые и проверяются `cargo test`.
//!
//! Правило одно: **чего форма не знает, того она не трогает.** Поля вне `KNOWN` остаются
//! в группе как лежали, а их имена уезжают в окно — там такая группа помечена.
//!
//! **Комментарии при сборке теряются**: документ пересобирается через `serde_yaml`.
//! Та же цена, что и у переключателя режима (D-052).

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::yaml::{set, top_mapping};

/// Ключ, за который отвечает раздел «Группы».
const KEY: &str = "proxy-groups";

/// Поля, которыми распоряжается форма. Всё остальное — «в коде».
const KNOWN: [&str; 9] = [
    "name",
    "type",
    "use",
    "proxies",
    "filter",
    "url",
    "interval",
    "tolerance",
    "strategy",
];

/// Группа так, как её видит окно. Имена полей нейтральные (`kind`, `sources`), потому что
/// за спеллинг ядра отвечает не окно: здесь он превращается в `type` и `use`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    /// Как группа выбирает узел: `select`, `url-test`, `fallback`, `load-balance`.
    /// Незнакомое значение доезжает до окна как есть — форма его показывает, но не правит.
    pub kind: String,
    /// Источники целиком — `use:` ядра. Живой список: новые узлы подписки приезжают сами.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Узлы и группы по имени — `proxies:` ядра.
    #[serde(default)]
    pub proxies: Vec<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub interval: Option<u64>,
    #[serde(default)]
    pub tolerance: Option<u64>,
    #[serde(default)]
    pub strategy: Option<String>,
    /// Имена полей, которых форма не знает. Она их не трогает — но и не молчит про них.
    #[serde(default)]
    pub extra: Vec<String>,
    /// Место группы в исходном документе. Пусто — группу завела форма.
    ///
    /// По нему при сборке находится та самая запись, поэтому незнакомое поле переживает
    /// и переименование, и перестановку: за строкой в окне ездит её origin.
    #[serde(default)]
    pub origin: Option<usize>,
}

/// Группы документа. Пустой документ и документ без `proxy-groups` — это ноль групп,
/// а не ошибка.
///
/// Запись, которую форма не собрала бы обратно **без потерь**, — ошибка: показать половину
/// документа и записать поверх второй половины хуже, чем честно отправить в код.
pub fn parse(text: &str) -> Result<Vec<Group>> {
    let map = top_mapping(text)?;
    let Some(value) = map.get(Value::from(KEY)) else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let list = value
        .as_sequence()
        .ok_or_else(|| AppError::invalid("proxy-groups должен быть списком групп"))?;
    list.iter()
        .enumerate()
        .map(|(at, item)| one(item, at))
        .collect()
}

fn one(value: &Value, at: usize) -> Result<Group> {
    let map = value
        .as_mapping()
        .ok_or_else(|| AppError::invalid(format!("Группа №{} записана не полями", at + 1)))?;
    let name = text_at(map, "name")
        .ok_or_else(|| AppError::invalid(format!("У группы №{} нет имени", at + 1)))?;
    Ok(Group {
        name,
        kind: text_at(map, "type").unwrap_or_default(),
        sources: list_at(map, "use"),
        proxies: list_at(map, "proxies"),
        filter: text_at(map, "filter"),
        url: text_at(map, "url"),
        interval: number_at(map, "interval"),
        tolerance: number_at(map, "tolerance"),
        strategy: text_at(map, "strategy"),
        extra: map
            .keys()
            .filter_map(Value::as_str)
            .filter(|key| !KNOWN.contains(key))
            .map(str::to_string)
            .collect(),
        origin: Some(at),
    })
}

/// Собрать документ заново с этими группами. Всё, что лежит в документе помимо
/// `proxy-groups`, остаётся нетронутым.
pub fn render(text: &str, groups: &[Group]) -> Result<String> {
    let mut map = top_mapping(text)?;
    let was: Vec<Value> = map
        .get(Value::from(KEY))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let list: Vec<Value> = groups
        .iter()
        .map(|group| Value::Mapping(entry(group, &was)))
        .collect();
    // Пустой список ядро отвергает, поэтому ноль групп — это отсутствие ключа, а не `[]`.
    if list.is_empty() {
        without(&mut map, KEY);
    } else {
        set(&mut map, KEY, Value::Sequence(list));
    }
    serde_yaml::to_string(&Value::Mapping(map)).map_err(|e| AppError::invalid(e.to_string()))
}

/// Запись группы: то, что лежало, плюс поля формы поверх. Пустое поле формы означает
/// «этого поля быть не должно», а не «оставить как было», — иначе снятый фильтр
/// продолжал бы резать группу.
fn entry(group: &Group, was: &[Value]) -> Mapping {
    let mut map = group
        .origin
        .and_then(|at| was.get(at))
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    set(&mut map, "name", Value::from(group.name.clone()));
    put(&mut map, "type", some_text(&group.kind));
    put(&mut map, "use", some_list(&group.sources));
    put(&mut map, "proxies", some_list(&group.proxies));
    put(
        &mut map,
        "filter",
        group.filter.as_deref().and_then(some_text),
    );
    put(&mut map, "url", group.url.as_deref().and_then(some_text));
    put(&mut map, "interval", group.interval.map(Value::from));
    put(&mut map, "tolerance", group.tolerance.map(Value::from));
    put(
        &mut map,
        "strategy",
        group.strategy.as_deref().and_then(some_text),
    );
    map
}

fn some_text(text: &str) -> Option<Value> {
    let text = text.trim();
    (!text.is_empty()).then(|| Value::from(text.to_string()))
}

fn some_list(items: &[String]) -> Option<Value> {
    (!items.is_empty())
        .then(|| Value::Sequence(items.iter().map(|item| Value::from(item.clone())).collect()))
}

/// Поставить или убрать поле.
fn put(map: &mut Mapping, key: &str, value: Option<Value>) {
    match value {
        Some(value) => set(map, key, value),
        None => without(map, key),
    }
}

/// Убрать ключ, не переставляя соседей: `Mapping::remove` меняет порядок, а порядок полей
/// в документе принадлежит тому, кто его писал.
fn without(map: &mut Mapping, key: &str) {
    let key = Value::from(key);
    if !map.contains_key(&key) {
        return;
    }
    *map = std::mem::take(map)
        .into_iter()
        .filter(|(name, _)| name != &key)
        .collect();
}

fn text_at(map: &Mapping, key: &str) -> Option<String> {
    map.get(Value::from(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn number_at(map: &Mapping, key: &str) -> Option<u64> {
    map.get(Value::from(key)).and_then(Value::as_u64)
}

fn list_at(map: &Mapping, key: &str) -> Vec<String> {
    map.get(Value::from(key))
        .and_then(Value::as_sequence)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: &str = "proxy-groups:
  - name: Европа
    type: url-test
    use: [capyhub]
    filter: '^(Poland 1|Poland 2)$'
    url: http://www.google.com/generate_204
    interval: 300
    tolerance: 150
    lazy: true
  - name: Дом
    type: select
    proxies: [DIRECT]
";

    #[test]
    fn a_group_reads_back_in_the_words_of_the_window() {
        let groups = parse(MINE).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Европа");
        assert_eq!(groups[0].kind, "url-test");
        assert_eq!(groups[0].sources, vec!["capyhub"]);
        assert_eq!(groups[0].filter.as_deref(), Some("^(Poland 1|Poland 2)$"));
        assert_eq!(groups[0].interval, Some(300));
        assert_eq!(groups[0].tolerance, Some(150));
        assert_eq!(
            groups[0].extra,
            vec!["lazy"],
            "чего форма не знает — называет"
        );
        assert_eq!(groups[1].proxies, vec!["DIRECT"]);
        assert!(groups[1].extra.is_empty());
    }

    /// Главное обещание формы: незнакомое поле не теряется — и не отстаёт от своей группы,
    /// когда её переименовали и переставили.
    #[test]
    fn what_the_form_does_not_know_survives_a_rename_and_a_move() {
        let mut groups = parse(MINE).unwrap();
        groups[0].name = "Европа-2".into();
        groups.swap(0, 1);
        let out = render(MINE, &groups).unwrap();
        let after = parse(&out).unwrap();
        assert_eq!(after[0].name, "Дом");
        assert_eq!(after[1].name, "Европа-2");
        assert_eq!(after[1].extra, vec!["lazy"]);
        let map = top_mapping(&out).unwrap();
        assert_eq!(map["proxy-groups"][1]["lazy"], Value::from(true));
    }

    /// Снятое поле обязано исчезнуть из документа: «пусто» в форме означает «нет поля»,
    /// иначе снятый фильтр продолжал бы резать группу.
    #[test]
    fn an_emptied_field_leaves_the_document() {
        let mut groups = parse(MINE).unwrap();
        groups[0].filter = None;
        groups[0].tolerance = None;
        let map = top_mapping(&render(MINE, &groups).unwrap()).unwrap();
        let group = map["proxy-groups"][0].as_mapping().unwrap();
        assert!(!group.contains_key(Value::from("filter")));
        assert!(!group.contains_key(Value::from("tolerance")));
        assert!(
            group.contains_key(Value::from("lazy")),
            "чужое поле при этом на месте"
        );
        assert_eq!(
            group.keys().filter_map(Value::as_str).next(),
            Some("name"),
            "порядок полей не тасуется"
        );
    }

    #[test]
    fn the_rest_of_the_document_is_none_of_our_business() {
        let text = "mixed-port: 7777\nproxy-groups:\n  - name: Дом\n    type: select\n";
        let out = render(text, &parse(text).unwrap()).unwrap();
        assert_eq!(top_mapping(&out).unwrap()["mixed-port"], Value::from(7777));
    }

    #[test]
    fn the_last_group_removed_takes_the_key_with_it() {
        let out = render(MINE, &[]).unwrap();
        assert!(
            !top_mapping(&out).unwrap().contains_key(Value::from(KEY)),
            "пустой список ядро отвергает — ключа быть не должно"
        );
    }

    /// Новая группа приходит без origin: копировать ей нечего, и чужие поля к ней прилипнуть
    /// не должны.
    #[test]
    fn a_new_group_starts_clean() {
        let fresh = Group {
            name: "Своя".into(),
            kind: "select".into(),
            sources: vec!["capyhub".into()],
            proxies: Vec::new(),
            filter: None,
            url: None,
            interval: None,
            tolerance: None,
            strategy: None,
            extra: Vec::new(),
            origin: None,
        };
        let map = top_mapping(&render(MINE, &[fresh]).unwrap()).unwrap();
        let group = map["proxy-groups"][0].as_mapping().unwrap();
        assert_eq!(group["name"], Value::from("Своя"));
        assert!(!group.contains_key(Value::from("lazy")));
        assert!(!group.contains_key(Value::from("interval")));
    }

    /// Документ, который форма не соберёт обратно без потерь, она не открывает вовсе.
    #[test]
    fn a_document_the_form_cannot_rebuild_is_refused() {
        assert!(parse("proxy-groups:\n  - просто строка\n").is_err());
        assert!(
            parse("proxy-groups:\n  - type: select\n").is_err(),
            "без имени"
        );
        assert!(parse("proxy-groups: 5\n").is_err());
        assert!(parse("").unwrap().is_empty());
        assert!(parse("proxy-groups:\n").unwrap().is_empty());
    }
}
