//! Разделы маршрута над своими правилами (D-158): `rule-sets` и `ready`.
//!
//! Чистый разбор и сборка текста — ни диска, ни каталога. Существует ли названный список
//! или набор, проверяет `route_doc` при записи: здесь только форма записи.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::error::{AppError, Result};
use crate::slug::Slug;
use crate::yaml::Yaml;

pub const RULE_SETS: &str = "rule-sets";
pub const READY: &str = "ready";

/// Где запись встаёт в маршруте (D-158): свои правила → `high` → `medium` → `low` →
/// build-in. Внутри уровня rule sets выше готовых наборов.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    #[default]
    Medium,
    Low,
}

impl Priority {
    /// Сверху вниз — в этом порядке уровни и собираются.
    pub const ORDER: [Priority; 3] = [Priority::High, Priority::Medium, Priority::Low];

    /// Умолчание в тексте не пишется: документ без приоритетов читается как раньше.
    fn is_default(&self) -> bool {
        *self == Priority::Medium
    }
}

/// Скачанный список в маршруте (D-157).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleSetUse {
    pub id: String,
    /// Свой список: откуда качать. Пусто — из каталога.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub target: String,
    #[serde(default, skip_serializing_if = "Priority::is_default")]
    pub priority: Priority,
}

/// Готовый набор из коллекции (D-083) в маршруте.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyUse {
    pub id: String,
    /// Пусто — выход самого набора.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Priority::is_default")]
    pub priority: Priority,
}

/// Оба раздела документа.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sections {
    pub rule_sets: Vec<RuleSetUse>,
    pub ready: Vec<ReadyUse>,
}

impl Sections {
    /// Разделы документа. Нет ключа — пусто. Запись не по форме — отказ с названием
    /// раздела и записи: молча выкинутый список повёл бы трафик не туда.
    pub fn read(text: &str) -> Result<Sections> {
        let map = Yaml::top_mapping(text)?;
        let sections = Sections {
            rule_sets: list(map.get(Value::from(RULE_SETS)), RULE_SETS)?,
            ready: list(map.get(Value::from(READY)), READY)?,
        };
        sections.check()?;
        Ok(sections)
    }

    /// Записать разделы в документ. Пустой раздел убирается: `rule-sets: []` в тексте
    /// ничего не говорит, кроме того, что форма его трогала.
    pub fn write(map: &mut serde_yaml::Mapping, sections: &Sections) -> Result<()> {
        sections.check()?;
        for (key, value) in [
            (RULE_SETS, serde_yaml::to_value(&sections.rule_sets)),
            (READY, serde_yaml::to_value(&sections.ready)),
        ] {
            let value = value.map_err(|e| AppError::invalid(e.to_string()))?;
            if value.as_sequence().is_some_and(Vec::is_empty) {
                map.remove(Value::from(key));
            } else {
                Yaml::set(map, key, value);
            }
        }
        Ok(())
    }

    /// Имя — то, что станет путём и именем провайдера; выход — не пустой; адрес — http(s);
    /// повтор — ошибка: второй раз тот же список лишь запутал бы порядок.
    fn check(&self) -> Result<()> {
        let mut seen: Vec<&str> = Vec::new();
        for set in &self.rule_sets {
            name(&set.id, RULE_SETS)?;
            if set.target.trim().is_empty() {
                return Err(AppError::invalid(format!(
                    "{RULE_SETS}: у списка «{}» не указано, куда слать (target)",
                    set.id
                )));
            }
            if let Some(url) = &set.url {
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(AppError::invalid(format!(
                        "{RULE_SETS}: у списка «{}» адрес не http(s): {url}",
                        set.id
                    )));
                }
            }
            twice(&mut seen, &set.id, RULE_SETS)?;
        }
        let mut seen: Vec<&str> = Vec::new();
        for set in &self.ready {
            name(&set.id, READY)?;
            if set
                .target
                .as_deref()
                .is_some_and(|target| target.trim().is_empty())
            {
                return Err(AppError::invalid(format!(
                    "{READY}: у набора «{}» пустой target — уберите его, чтобы взять выход набора",
                    set.id
                )));
            }
            twice(&mut seen, &set.id, READY)?;
        }
        Ok(())
    }
}

fn list<T: serde::de::DeserializeOwned>(value: Option<&Value>, key: &str) -> Result<Vec<T>> {
    match value {
        None => Ok(Vec::new()),
        Some(value) if value.is_null() => Ok(Vec::new()),
        Some(value) => serde_yaml::from_value(value.clone()).map_err(|e| {
            AppError::invalid(format!(
                "{key}: запись не по форме — {e}. Образец: `- id: antizapret` и `target: umiray`"
            ))
        }),
    }
}

fn name(id: &str, key: &str) -> Result<()> {
    if !id.is_empty() && Slug::of(id, "") == id {
        return Ok(());
    }
    Err(AppError::invalid(format!(
        "{key}: имя «{id}» — только строчные буквы, цифры и дефис"
    )))
}

fn twice<'a>(seen: &mut Vec<&'a str>, id: &'a str, key: &str) -> Result<()> {
    if seen.contains(&id) {
        return Err(AppError::invalid(format!("{key}: «{id}» указан дважды")));
    }
    seen.push(id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "rule-sets:
  - id: antizapret
    target: umiray
  - id: my-list
    url: https://example.org/list.txt
    target: DIRECT
ready:
  - id: direct-ru
  - id: block-ads
    target: REJECT
rules:
  - MATCH,umiray
";

    #[test]
    fn both_sections_read_and_write_back_the_same() {
        let sections = Sections::read(DOC).unwrap();
        assert_eq!(sections.rule_sets.len(), 2);
        assert_eq!(
            sections.rule_sets[1].url.as_deref(),
            Some("https://example.org/list.txt")
        );
        assert_eq!(sections.ready[0].target, None, "без target — выход набора");
        let mut map = Yaml::top_mapping(DOC).unwrap();
        Sections::write(&mut map, &sections).unwrap();
        let again = serde_yaml::to_string(&Value::Mapping(map)).unwrap();
        assert_eq!(Sections::read(&again).unwrap(), sections);
        assert!(
            !again.contains("url: null"),
            "пустое поле в тексте не пишется"
        );
    }

    /// Приоритет по умолчанию в тексте не пишется, иной — пишется словом; незнакомое слово —
    /// отказ, а не молча средний уровень.
    #[test]
    fn a_priority_is_written_only_when_it_differs() {
        let sections = Sections::read(
            "rule-sets:\n  - {id: a, target: AUTO, priority: high}\n  - {id: b, target: AUTO}\n",
        )
        .unwrap();
        assert_eq!(sections.rule_sets[0].priority, Priority::High);
        assert_eq!(sections.rule_sets[1].priority, Priority::Medium);
        let mut map = serde_yaml::Mapping::new();
        Sections::write(&mut map, &sections).unwrap();
        let text = serde_yaml::to_string(&Value::Mapping(map)).unwrap();
        assert_eq!(text.matches("priority").count(), 1, "{text}");
        assert!(Sections::read("ready:\n  - {id: x, priority: urgent}\n").is_err());
    }

    #[test]
    fn an_empty_section_leaves_the_document() {
        let mut map = Yaml::top_mapping(DOC).unwrap();
        Sections::write(&mut map, &Sections::default()).unwrap();
        assert!(!map.contains_key(Value::from(RULE_SETS)));
        assert!(!map.contains_key(Value::from(READY)));
        assert!(map.contains_key(Value::from("rules")), "чужое не трогаем");
    }

    /// Код правят руками — каждая ошибка называет раздел и запись.
    #[test]
    fn a_wrong_entry_is_refused_by_name() {
        for (text, says) in [
            ("rule-sets:\n  - id: x\n", "target"),
            (
                "rule-sets:\n  - id: Bad Name\n    target: DIRECT\n",
                "Bad Name",
            ),
            ("rule-sets:\n  - id: x\n    target: ''\n", "куда слать"),
            (
                "rule-sets:\n  - id: x\n    url: ftp://x\n    target: DIRECT\n",
                "http",
            ),
            (
                "rule-sets:\n  - {id: x, target: A}\n  - {id: x, target: B}\n",
                "дважды",
            ),
            ("rule-sets:\n  - {id: x, target: A, tagret: B}\n", "tagret"),
            (
                "ready:\n  - id: direct-ru\n    target: ' '\n",
                "пустой target",
            ),
            ("ready: 12\n", "ready"),
        ] {
            let error = Sections::read(text).unwrap_err().to_string();
            assert!(error.contains(says), "{text}\n→ {error}");
        }
    }
}
