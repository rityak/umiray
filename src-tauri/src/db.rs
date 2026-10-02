//! Хранилище клиента — один файл SQLite (D-170). Доменов не знает: таблица, ключ, часть,
//! тело.
//!
//! Строка — `(id, part, body)`: у источника части — метаданные, сырьё, собранное и правки,
//! у документа часть одна, пустая. Тело — текст того же формата, что лежал в файле (YAML,
//! JSON, список строк), поэтому разбор над хранилищем не изменился.
//!
//! Соединение открывается на каждый вызов: путь зависит от `LOCALAPPDATA`, который подменяют
//! проверки, а открыть файл стоит доли миллисекунды. ponytail: открытие на вызов; общее
//! соединение под замком — когда замер покажет, что открытия заметны.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, Result};
use crate::paths::Paths;

/// Таблица — домен данных. Кто вправе писать в какую, держит тест `tables`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// Документы конфига: Mihomo Settings, Umiray Settings, группы.
    Documents,
    /// Наборы маршрута: метаданные и документ.
    Presets,
    /// Источники узлов: метаданные, сырьё, собранные записи, правки записей.
    Sources,
    /// Rule sets: метаданные и нейтральные списки.
    Lists,
    /// Коллекции: документы (`dns`, `lists`) и папки (`rules`).
    Collections,
    /// Мелкое состояние клиента: настройки окна, HWID, кэш стран, ссылка qd.
    State,
    /// Файлы прежней раскладки, которые были единственной копией пользователя.
    Archive,
}

impl Table {
    pub const ALL: [Table; 7] = [
        Table::Documents,
        Table::Presets,
        Table::Sources,
        Table::Lists,
        Table::Collections,
        Table::State,
        Table::Archive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Table::Documents => "documents",
            Table::Presets => "presets",
            Table::Sources => "sources",
            Table::Lists => "lists",
            Table::Collections => "collections",
            Table::State => "state",
            Table::Archive => "archive",
        }
    }
}

/// Номер схемы в `user_version`. Новая таблица — следующий номер и шаг в `migrate`.
const SCHEMA: i64 = 1;

pub struct Db;

impl Db {
    /// Есть ли база вообще. Нужно переезду: её отсутствие и значит «каталог ещё файловый».
    pub fn exists() -> bool {
        Paths::db().is_file()
    }

    pub fn get(table: Table, id: &str, part: &str) -> Result<Option<String>> {
        let sql = format!(
            "SELECT body FROM {} WHERE id = ?1 AND part = ?2",
            table.name()
        );
        open()?
            .query_row(&sql, params![id, part], |row| row.get(0))
            .optional()
            .map_err(failed)
    }

    /// Все строки одной части: `(id, тело)` по возрастанию `id`.
    pub fn with_part(table: Table, part: &str) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT id, body FROM {} WHERE part = ?1 ORDER BY id",
            table.name()
        );
        rows(&open()?, &sql, part)
    }

    /// Все части одного `id`: `(часть, тело)` по возрастанию части.
    pub fn parts(table: Table, id: &str) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT part, body FROM {} WHERE id = ?1 ORDER BY part",
            table.name()
        );
        rows(&open()?, &sql, id)
    }

    pub fn put(table: Table, id: &str, part: &str, body: &str) -> Result<()> {
        Db::batch(|batch| batch.put(table, id, part, body))
    }

    pub fn remove(table: Table, id: &str, part: &str) -> Result<()> {
        Db::batch(|batch| batch.remove(table, id, part))
    }

    /// Убрать `id` со всеми частями.
    pub fn remove_all(table: Table, id: &str) -> Result<()> {
        Db::batch(|batch| batch.remove_all(table, id))
    }

    pub fn clear(table: Table) -> Result<()> {
        Db::batch(|batch| batch.clear(table))
    }

    /// Несколько записей одной транзакцией: либо все, либо ни одной.
    pub fn batch(change: impl FnOnce(&Batch) -> Result<()>) -> Result<()> {
        let mut connection = open()?;
        let transaction = connection.transaction().map_err(failed)?;
        change(&Batch(&transaction))?;
        transaction.commit().map_err(failed)
    }

    /// Копия базы в `to`, где осталось только `keep` (D-163): таблица целиком (`None`)
    /// или одна её строка. Остальное в копии пусто.
    pub fn copy(to: &Path, keep: &[(Table, Option<&str>)]) -> Result<()> {
        // Поверх самой базы копия подменила бы её урезанной — без HWID, rule sets, архива
        // (B-046). Файла нет — это не база.
        let base = std::fs::canonicalize(Paths::db()).ok();
        if base.is_some() && std::fs::canonicalize(to).ok() == base {
            return Err(AppError::invalid(
                "Это сама база клиента — сохраните копию в другое место",
            ));
        }
        let staged = to.with_extension("db.part");
        let _ = std::fs::remove_file(&staged);
        open()?
            .execute("VACUUM INTO ?1", params![staged.to_string_lossy()])
            .map_err(failed)?;
        let result = (|| {
            let copy = Connection::open(&staged).map_err(failed)?;
            for table in Table::ALL {
                let sql = format!("DELETE FROM {} WHERE id != ?1", table.name());
                match keep.iter().find(|(kept, _)| *kept == table) {
                    Some((_, None)) => continue,
                    Some((_, Some(id))) => copy.execute(&sql, params![id]),
                    None => copy.execute(&format!("DELETE FROM {}", table.name()), []),
                }
                .map_err(failed)?;
            }
            copy.execute("VACUUM", []).map_err(failed)?;
            drop(copy);
            Ok(std::fs::rename(&staged, to)?)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&staged);
        }
        result
    }
}

/// Записи внутри транзакции `Db::batch`.
pub struct Batch<'a>(&'a rusqlite::Transaction<'a>);

impl Batch<'_> {
    pub fn put(&self, table: Table, id: &str, part: &str, body: &str) -> Result<()> {
        let sql = format!(
            "INSERT INTO {} (id, part, body) VALUES (?1, ?2, ?3)
             ON CONFLICT (id, part) DO UPDATE SET body = excluded.body",
            table.name()
        );
        self.0
            .execute(&sql, params![id, part, body])
            .map(drop)
            .map_err(failed)
    }

    pub fn remove(&self, table: Table, id: &str, part: &str) -> Result<()> {
        let sql = format!("DELETE FROM {} WHERE id = ?1 AND part = ?2", table.name());
        self.0
            .execute(&sql, params![id, part])
            .map(drop)
            .map_err(failed)
    }

    pub fn remove_all(&self, table: Table, id: &str) -> Result<()> {
        let sql = format!("DELETE FROM {} WHERE id = ?1", table.name());
        self.0.execute(&sql, params![id]).map(drop).map_err(failed)
    }

    pub fn clear(&self, table: Table) -> Result<()> {
        let sql = format!("DELETE FROM {}", table.name());
        self.0.execute(&sql, []).map(drop).map_err(failed)
    }
}

fn open() -> Result<Connection> {
    Paths::ensure_root()?;
    let connection = Connection::open(Paths::db()).map_err(failed)?;
    // Окно, такт и команда пишут из разных потоков: занятой базе — подождать, а не упасть.
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(failed)?;
    migrate(&connection)?;
    Ok(connection)
}

fn migrate(connection: &Connection) -> Result<()> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(failed)?;
    if version >= SCHEMA {
        return Ok(());
    }
    let mut sql = String::from("BEGIN;");
    for table in Table::ALL {
        sql.push_str(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT NOT NULL,
                part TEXT NOT NULL,
                body TEXT NOT NULL,
                PRIMARY KEY (id, part)
            ) WITHOUT ROWID;",
            table.name()
        ));
    }
    sql.push_str(&format!("PRAGMA user_version = {SCHEMA}; COMMIT;"));
    connection.execute_batch(&sql).map_err(failed)
}

fn rows(connection: &Connection, sql: &str, key: &str) -> Result<Vec<(String, String)>> {
    let mut statement = connection.prepare(sql).map_err(failed)?;
    let found = statement
        .query_map(params![key], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(failed)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(failed);
    found
}

fn failed(why: rusqlite::Error) -> AppError {
    AppError::io(format!("База данных: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кто вправе трогать какую таблицу (D-155, D-170): данные с владельцем, как и пути.
    /// Новая таблица без строки здесь — провал теста.
    const OWNERS: &[(&str, &[&str])] = &[
        ("Documents", &["config/files.rs"]),
        ("Presets", &["config/presets.rs"]),
        ("Sources", &["nodes/sources.rs", "nodes/entries.rs"]),
        ("Lists", &["lists/store.rs"]),
        ("Collections", &["collections.rs"]),
        (
            "State",
            &[
                "app/settings.rs",
                "nodes/device.rs",
                "nodes/geo.rs",
                "core/qd.rs",
            ],
        ),
        ("Archive", &["app/import.rs"]),
    ];
    /// Переезд и экспорт по определению видят все таблицы.
    const WHOLE: &[&str] = &["db.rs", "app/import.rs", "app/data.rs"];

    #[test]
    fn only_the_owner_touches_its_table() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src.clone()];
        let mut wrong = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if !name.ends_with(".rs") || WHOLE.contains(&name.as_str()) || name == "live.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
                for (at, _) in code.match_indices("Table::") {
                    // `ProcessTable::` и прочие чужие типы — не наша таблица.
                    if code[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let item: String = code[at + "Table::".len()..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric())
                        .collect();
                    // `ping::Table::new()` — тоже чужой тип: у нашей таблицы только варианты.
                    if !item.starts_with(|c: char| c.is_ascii_uppercase()) {
                        continue;
                    }
                    match OWNERS.iter().find(|(table, _)| *table == item) {
                        Some((_, owners)) if owners.contains(&name.as_str()) => {}
                        Some(_) => wrong.push(format!("{name}: Table::{item} — чужая таблица")),
                        None => {
                            wrong.push(format!("{name}: Table::{item} — у таблицы нет владельца"))
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// Каждая таблица из `ALL` расписана в `OWNERS` — иначе тест выше её не видит.
    #[test]
    fn every_table_has_an_owner() {
        for table in Table::ALL {
            let name = format!("{table:?}");
            assert!(
                OWNERS.iter().any(|(owner, _)| *owner == name),
                "{name} без владельца"
            );
        }
    }

    /// B-046: экспорт, сохранённый поверх самой базы, подменял её урезанной копией —
    /// без HWID (новый займёт слот в подписке), rule sets и архива.
    #[test]
    fn a_copy_never_replaces_the_base_itself() {
        let _sandbox = crate::paths::Sandbox::new("copy-onto-base");
        Db::put(Table::State, "hwid", "", "0123456789abcdef").unwrap();
        assert!(Db::copy(&Paths::db(), &[(Table::State, Some("settings"))]).is_err());
        assert_eq!(
            Db::get(Table::State, "hwid", "").unwrap().as_deref(),
            Some("0123456789abcdef"),
            "база цела"
        );
    }
}
