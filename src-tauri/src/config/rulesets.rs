//! Встроенные наборы правил: коллекция-папка, тумблер в `client.yaml`, строки в сборку
//! (D-083, D-100).
//!
//! Не набор маршрутизации (D-071) и не документ пользователя: набор задаёт маршрут целиком,
//! а эти включаются вместе и **поверх любого направления**. В документ пользователя отсюда
//! не пишется ничего — включённое подмешивается при сборке, между его правилами и `MATCH`.
//!
//! Сами файлы — коллекция `rules` (D-100): их раздаёт и перечисляет `collections`, здесь
//! остаётся то, что знает только про наборы: разбор файла, тумблер и строки в сборку.
//!
//! Что включено, живёт одним полем `rulesets:` в `client.yaml`: файл клиентский, ядру
//! не уходит (D-068), и у поля один хозяин (D-052). Поле не переименовано вместе
//! с коллекцией намеренно: оно называет **что включено**, а не где оно лежит.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::collections;
use crate::collections::Collections;
use crate::config::files::Documents;
use crate::config::files::CLIENT;
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Поле `client.yaml`, в котором лежит список включённых.
const KEY: &str = "rulesets";

/// Набор так, как его видит окно.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ruleset {
    /// Имя файла без расширения. Оно же то, что записано в `client.yaml`.
    pub id: String,
    /// Человеческие имена набора: перевод выбирает окно (D-151).
    pub title: String,
    #[serde(default)]
    pub title_en: Option<String>,
    pub on: bool,
    /// Строки правил как есть — окно показывает их, когда набор раскрывают.
    pub rules: Vec<String>,
}

pub struct RulesetStore;

impl RulesetStore {
    /// Все наборы коллекции, по алфавиту: порядок в окне не должен зависеть от того,
    /// как файлы легли на диск.
    pub fn list() -> Vec<Ruleset> {
        let on = enabled();
        Collections::folder(collections::RULES)
            .into_iter()
            .filter_map(|(id, text)| {
                let mut set = parse(&id, &text)?;
                set.on = on.contains(&set.id);
                Some(set)
            })
            .collect()
    }

    /// Строки включённых наборов — то, что уходит в сборку.
    ///
    /// Порядок между наборами — тот же алфавитный: два набора редко спорят за один домен,
    /// а стабильный порядок важнее, чем возможность их переставлять.
    pub fn enabled_rules() -> Vec<String> {
        RulesetStore::list()
            .into_iter()
            .filter(|set| set.on)
            .flat_map(|set| set.rules)
            .collect()
    }

    /// Текст набора как есть — тот же файл, что правят руками (D-104).
    ///
    /// Идентификатор приходит из вебвью, поэтому путём он не становится: файл ищется среди
    /// тех, что коллекция и так перечисляет.
    pub fn read(id: &str) -> Result<String> {
        Collections::folder(collections::RULES)
            .into_iter()
            .find(|(name, _)| name == id)
            .map(|(_, text)| text)
            .ok_or_else(|| AppError::invalid(format!("Неизвестный набор правил: {id}")))
    }

    /// Завести свой набор (D-110). Возвращает идентификатор — окно раскроет его редактором.
    ///
    /// Файл сразу с правилом-примером: набор без правил не показывается вовсе (`parse`),
    /// и заведённый из окна пропал бы у пользователя на глазах. Правило выбрано заведомо
    /// безобидное — новый набор ещё и выключен, так что в сборку оно не попадёт.
    pub fn create(title: &str) -> Result<String> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::invalid("У набора должно быть имя"));
        }
        let id = free(&slug(title));
        let text = format!(
            "title: {title}
rules:
  - DOMAIN-SUFFIX,example.com,DIRECT
"
        );
        crate::atomic::AtomicFile::write(path(&id), text)?;
        Ok(id)
    }

    /// Удалить набор вместе с файлом. Включённый сначала выключается: иначе он остался бы
    /// в `client.yaml` именем, которому больше ничего не соответствует.
    ///
    /// Удаляем и поставляемый тоже — коллекция принадлежит пользователю (D-100), и
    /// «этот трогать нельзя» было бы враньём: файл всё равно правится руками.
    pub fn delete(id: &str) -> Result<()> {
        RulesetStore::read(id)?;
        RulesetStore::toggle(id, false)?;
        std::fs::remove_file(path(id))?;
        Ok(())
    }

    /// Записать правку. Перед записью — разбор: файл, который не читается, означал бы набор,
    /// молча выпавший из сборки, а окно показывало бы тумблер включённым.
    ///
    /// Заводить новые файлы этим нельзя: правится то, что уже лежит в папке — для нового
    /// есть `create`.
    pub fn write(id: &str, text: &str) -> Result<()> {
        RulesetStore::read(id)?;
        if parse(id, text).is_none() {
            return Err(AppError::invalid(
                "Набор должен быть YAML со списком `rules:` — и хотя бы одним правилом",
            ));
        }
        crate::atomic::AtomicFile::write(path(id), text)?;
        Ok(())
    }

    /// Включить или выключить. Пишем точечно, как режим перехвата (D-052): остальное
    /// в `client.yaml` не наше.
    pub fn toggle(id: &str, on: bool) -> Result<()> {
        // Идентификатор приходит из вебвью. Путь из него не строится, но в `client.yaml`
        // он попадает — а туда пишем только то, что правда лежит в папке.
        if on && !RulesetStore::list().iter().any(|set| set.id == id) {
            return Err(AppError::invalid(format!("Неизвестный набор правил: {id}")));
        }
        let mut names = enabled();
        names.retain(|name| name != id);
        if on {
            names.push(id.to_string());
        }
        names.sort();
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        Yaml::set(
            &mut map,
            KEY,
            Value::Sequence(names.into_iter().map(Value::from).collect()),
        );
        let text = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(CLIENT, &text)
    }
}

/// Путь к файлу набора. Одно место на весь модуль: идентификатор приходит из вебвью,
/// и склеивать его с путём где попало — способ однажды склеить непроверенный.
fn path(id: &str) -> std::path::PathBuf {
    Collections::file(collections::RULES, id)
}

/// Имя файла из имени набора. Всё, что не буква, не цифра и не дефис, становится дефисом:
/// идентификатор уходит и в путь, и в `client.yaml`, и оставлять там точки и косые черты
/// нельзя. Буквы любые — кириллица в имени файла законна, а переводить её в латиницу
/// значило бы тащить таблицу транслитерации ради имени, которое видит один человек.
fn slug(title: &str) -> String {
    let cut: String = title
        .to_lowercase()
        .chars()
        .map(|letter| {
            if letter.is_alphanumeric() {
                letter
            } else {
                '-'
            }
        })
        .collect();
    // Подряд идущие дефисы схлопываем: «C:\Windows» иначе даёт `c--windows`.
    let mut trimmed = String::with_capacity(cut.len());
    for letter in cut.chars() {
        if letter != '-' || !trimmed.ends_with('-') {
            trimmed.push(letter);
        }
    }
    let trimmed = trimmed.trim_matches('-').to_string();
    // Длинное имя файла — это длинный путь, а он на Windows кончается отказом записи.
    let short: String = trimmed.chars().take(40).collect();
    let short = short.trim_matches('-').to_string();
    if short.is_empty() {
        "nabor".to_string()
    } else {
        short
    }
}

/// Свободное имя рядом с занятым: `свой`, `свой-2`, `свой-3`. Второй набор с тем же
/// именем — обычное дело, а молча перезаписать чужой файл нельзя.
fn free(wanted: &str) -> String {
    let taken: Vec<String> = Collections::folder(collections::RULES)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    if !taken.iter().any(|id| id == wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|number| format!("{wanted}-{number}"))
        .find(|candidate| !taken.iter().any(|id| id == candidate))
        .unwrap_or_else(|| wanted.to_string())
}

/// Что включено по мнению `client.yaml`. Файл правится руками, поэтому мусор здесь —
/// это «ничего не включено», а не отказ собрать конфиг: правила не та вещь, ради которой
/// стоит не поднять VPN.
fn enabled() -> Vec<String> {
    let Ok(text) = Documents::read(CLIENT) else {
        return Vec::new();
    };
    let Ok(map) = Yaml::top_mapping(&text) else {
        return Vec::new();
    };
    names(map.get(Value::from(KEY)))
}

fn names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_sequence)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Файл набора: наш `title` и правила ядра. Без правил набор бессмыслен — такой файл
/// в списке не показываем вовсе.
fn parse(id: &str, text: &str) -> Option<Ruleset> {
    let map = Yaml::top_mapping(text).ok()?;
    let rules: Vec<String> = map
        .get(Value::from("rules"))?
        .as_sequence()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if rules.is_empty() {
        return None;
    }
    Some(Ruleset {
        title_en: map
            .get(Value::from("title_en"))
            .and_then(Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string),
        title: map
            .get(Value::from("title"))
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        id: id.to_string(),
        on: false,
        rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Поставляемые наборы читаются прямо из репозитория — с D-100 они файлы, а не
    /// строковые константы, и разбирать надо ровно то, что уедет пользователю.
    const DIRECT_RU: &str = include_str!("../../../collections/rules/direct-ru.yaml");
    const BLOCK_ADS: &str = include_str!("../../../collections/rules/block-ads.yaml");

    #[test]
    fn a_file_gives_its_title_and_its_rules() {
        let set = parse("direct-ru", DIRECT_RU).unwrap();
        assert_eq!(set.title, "Россия — напрямую");
        assert_eq!(set.title_en.as_deref(), Some("Russia — direct"));
        assert_eq!(set.id, "direct-ru");
        assert!(set.rules.contains(&"DOMAIN-SUFFIX,ru,DIRECT".to_string()));
        assert!(
            !set.rules.iter().any(|rule| rule.contains("push")),
            "пуши — не про страну, в этом наборе им места нет"
        );
        assert_eq!(parse("block-ads", BLOCK_ADS).unwrap().rules.len(), 1);
    }

    /// Без имени набор всё равно показывается — под именем файла. Без правил не показывается
    /// вовсе: включать в нём нечего.
    #[test]
    fn a_set_without_a_title_is_shown_and_one_without_rules_is_not() {
        let named = parse("свой", "rules: [MATCH,DIRECT]").unwrap();
        assert_eq!(named.title, "свой");
        assert_eq!(named.title_en, None);
        assert_eq!(
            parse("blank", "title_en: '  '\nrules: [MATCH,DIRECT]")
                .unwrap()
                .title_en,
            None
        );
        assert!(parse("пустой", "title: Пусто\nrules: []").is_none());
        assert!(parse("никакой", "title: Пусто\n").is_none());
        assert!(parse("сломанный", "%%%").is_none());
    }

    /// Идентификатор уходит и в путь, и в `client.yaml`: всё, что могло бы вывести
    /// из папки, обязано превратиться в дефис ещё здесь.
    #[test]
    fn a_name_becomes_a_file_name_that_cannot_leave_the_folder() {
        assert_eq!(slug("Мой набор"), "мой-набор");
        assert_eq!(slug("Block Ads!"), "block-ads");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug(r"C:\Windows"), "c-windows");
        assert_eq!(slug("..."), "nabor", "пустое имя — тоже имя файла");
        assert_eq!(slug("---"), "nabor");
        assert!(slug(&"я".repeat(80)).chars().count() <= 40);
    }

    /// Поле правится руками, и мусор в нём — это «ничего не включено», а не отказ
    /// собрать конфиг.
    #[test]
    fn nonsense_in_the_field_turns_off_everything_instead_of_breaking_the_vpn() {
        let map = Yaml::top_mapping("rulesets: [direct-ru, block-ads]").unwrap();
        assert_eq!(names(map.get(Value::from(KEY))), ["direct-ru", "block-ads"]);
        assert!(names(
            Yaml::top_mapping("rulesets: 12")
                .unwrap()
                .get(Value::from(KEY))
        )
        .is_empty());
        assert!(names(
            Yaml::top_mapping("ping: tcp")
                .unwrap()
                .get(Value::from(KEY))
        )
        .is_empty());
    }
}
