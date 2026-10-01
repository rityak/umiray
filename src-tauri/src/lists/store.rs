//! Где лежат скачанные списки и что о них известно (D-157, D-170).
//!
//! В таблице `lists` базы: часть `meta` — метаданные, `domains` и `cidrs` — нейтральные
//! данные. `run/lists/<ядро>/` — то, что из них собрало ядро в своём формате. Части нет —
//! нет и строки: из пустого ядро собрало бы пустой провайдер.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::db::{Db, Table};
use crate::error::{AppError, Result};
use crate::lists::parse::Payload;
use crate::paths::Paths;
use crate::slug::Slug;

/// Список так, как его видит окно.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleList {
    /// Имя файла и имя в правиле `RULE-SET,<id>,…`.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_en: Option<String>,
    /// Откуда. Несколько адресов складываются в один список.
    pub urls: Vec<String>,
    /// Когда скачали в последний раз, в секундах эпохи.
    #[serde(default)]
    pub updated: Option<u64>,
    /// Когда список опубликован — самый свежий `Last-Modified` среди адресов.
    #[serde(default)]
    pub published: Option<u64>,
    #[serde(default)]
    pub domains: usize,
    #[serde(default)]
    pub cidrs: usize,
    /// Сколько записей не перенесли (`keyword`, `regexp`, мусор).
    #[serde(default)]
    pub skipped: usize,
    /// Почему последнее обновление не удалось. Прежние данные при этом на месте.
    #[serde(default)]
    pub error: Option<String>,
}

/// Половина нейтрального списка. Поведение у провайдера ядра одно, поэтому и файлов два.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Domains,
    Cidrs,
}

impl Part {
    pub const ALL: [Part; 2] = [Part::Domains, Part::Cidrs];

    pub fn name(self) -> &'static str {
        match self {
            Part::Domains => "domains",
            Part::Cidrs => "cidrs",
        }
    }
}

pub struct ListStore;

/// Часть с метаданными. Списки перечисляются по ней.
const META: &str = "meta";

impl ListStore {
    /// Все списки, по имени.
    pub fn list() -> Vec<RuleList> {
        let mut lists: Vec<RuleList> = Db::with_part(Table::Lists, META)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, text)| serde_json::from_str::<RuleList>(&text).ok())
            .filter(|list| ListStore::valid(&list.id).is_ok())
            .collect();
        lists.sort_by_key(|list| list.title.to_lowercase());
        lists
    }

    pub fn get(id: &str) -> Result<RuleList> {
        let id = ListStore::valid(id)?;
        let text = Db::get(Table::Lists, id, META)?
            .ok_or_else(|| AppError::invalid(format!("Список не найден: {id}")))?;
        serde_json::from_str(&text)
            .map_err(|e| AppError::invalid(format!("Метаданные списка испорчены: {e}")))
    }

    /// Данные и метаданные — одной транзакцией: числа в метаданных без данных за ними
    /// соврали бы окну.
    pub fn save(list: &RuleList, payload: &Payload) -> Result<()> {
        let id = ListStore::valid(&list.id)?;
        let meta = encode(list)?;
        Db::batch(|batch| {
            for (part, lines) in [
                (Part::Domains, &payload.domains),
                (Part::Cidrs, &payload.cidrs),
            ] {
                if lines.is_empty() {
                    batch.remove(Table::Lists, id, part.name())?;
                } else {
                    batch.put(Table::Lists, id, part.name(), &(lines.join("\n") + "\n"))?;
                }
            }
            batch.put(Table::Lists, id, META, &meta)
        })
    }

    /// Только метаданные: например, отметка о неудаче — данные при этом не трогаются.
    pub fn note(list: &RuleList) -> Result<()> {
        let id = ListStore::valid(&list.id)?;
        Db::put(Table::Lists, id, META, &encode(list)?)
    }

    /// Текст части, если она есть.
    pub fn part(id: &str, part: Part) -> Option<String> {
        let id = ListStore::valid(id).ok()?;
        Db::get(Table::Lists, id, part.name()).ok().flatten()
    }

    /// Куда ядро кладёт собранное из списка: `run/lists/<ядро>/<имя>`. Имя файла решает
    /// ядро — формат его, а не наш.
    pub fn artifact(engine: &str, name: &str) -> PathBuf {
        Paths::lists_dir().join(engine).join(name)
    }

    /// Удалить список вместе со всем, что из него собрали ядра.
    pub fn delete(id: &str) -> Result<()> {
        let id = ListStore::valid(id)?;
        Db::remove_all(Table::Lists, id)?;
        // Идентификатор без точек (`Slug`), поэтому `<id>.` не зацепит чужой список.
        let prefix = format!("{id}.");
        let Ok(engines) = std::fs::read_dir(Paths::lists_dir()) else {
            return Ok(());
        };
        for engine in engines.flatten() {
            if !engine.path().is_dir() {
                continue;
            }
            for file in std::fs::read_dir(engine.path())?.flatten() {
                if file.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
        Ok(())
    }

    /// Идентификатор приходит из вебвью и становится путём и именем в конфиге ядра —
    /// поэтому только то, что `Slug` оставил бы как есть.
    pub fn valid(id: &str) -> Result<&str> {
        if !id.is_empty() && Slug::of(id, "") == id {
            Ok(id)
        } else {
            Err(AppError::invalid(format!("Неверное имя списка: {id}")))
        }
    }
}

fn encode(list: &RuleList) -> Result<String> {
    serde_json::to_string_pretty(list).map_err(|e| AppError::invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Имя из вебвью не выводит из папки и не ломает строку правила.
    #[test]
    fn only_a_slug_is_a_list_name() {
        assert!(ListStore::valid("antizapret").is_ok());
        assert!(ListStore::valid("свой-список-2").is_ok());
        for bad in ["", "../x", "a.b", "a,b", "A", "a@ip", "a b"] {
            assert!(ListStore::valid(bad).is_err(), "{bad}");
        }
    }
}
