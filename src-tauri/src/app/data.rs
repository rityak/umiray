//! Каталог данных целиком: переезд stable/dev (D-150), без удаления прежней установки,
//! и экспорт настроек копией базы (D-163, D-170).

use std::path::Path;

use crate::db::{Db, Table};
use crate::error::Result;
use crate::paths;
use crate::paths::Paths;

const MARKER: &str = ".data-v2";

/// Что считается настройками пользователя (D-163): таблица целиком или одна её строка.
/// Не входят rule sets и страны (кэши), архив старых файлов, HWID — на другой машине
/// он занял бы чужой слот устройства, — и бинари с `run/`: это не база вовсе.
const SETTINGS: &[(Table, Option<&str>)] = &[
    (Table::Documents, None),
    (Table::Presets, None),
    (Table::Sources, None),
    (Table::Collections, None),
    (Table::State, Some("settings")),
];

pub struct DataDir;

impl DataDir {
    /// Скопировать недостающее из прежнего каталога. Базы ещё нет — значит, каталог файловый
    /// и переезд не случился; есть — копировать поверх неё старые файлы нельзя: они
    /// перезаписали бы то, что уже в базе (D-170).
    pub fn migrate() -> Result<()> {
        if Db::exists() {
            return Ok(());
        }
        adopt(&Paths::legacy_root(), &Paths::root())
    }

    /// Копия настроек (D-163) — база с одними настройками.
    pub fn export(to: &Path) -> Result<()> {
        Db::copy(to, SETTINGS)
    }
}

fn adopt(from: &Path, to: &Path) -> Result<()> {
    let marker = to.join(MARKER);
    if marker.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    if from.is_dir() {
        copy_missing(from, to, true)?;
    }
    // Только после всех файлов: прерванный переезд продолжится при следующем запуске.
    crate::atomic::AtomicFile::write(marker, "1\n")?;
    Ok(())
}

fn copy_missing(from: &Path, to: &Path, root: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = if root && entry.file_name() == "mihomo.exe" {
            to.join(paths::CORE_NAME)
        } else {
            to.join(entry.file_name())
        };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_missing(&entry.path(), &target, false)?;
        } else if kind.is_file() && !target.exists() {
            // Atomic write не оставляет полфайла, которое следующий прогон принял бы
            // за существующие данные. Исходный каталог остаётся резервной копией.
            crate::atomic::AtomicFile::write(target, std::fs::read(entry.path())?)?;
        } else if kind.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Не удалось перенести ссылку {}", entry.path().display()),
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_old_and_new_data_and_does_not_reimport_deleted_files() {
        let temp = std::env::temp_dir().join(format!(
            "umiray-data-{}",
            crate::stamp::Stamp::id().unwrap()
        ));
        let old = temp.join("old");
        let new = temp.join("new");
        std::fs::create_dir_all(old.join("sources")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("hwid.txt"), "same-device").unwrap();
        std::fs::write(old.join("settings.json"), "old-settings").unwrap();
        std::fs::write(old.join("sources/source.raw"), "subscription").unwrap();
        std::fs::write(old.join("mihomo.exe"), "core").unwrap();
        std::fs::write(new.join("settings.json"), "new-settings").unwrap();
        std::fs::write(new.join("umiray.exe"), "installed-client").unwrap();

        adopt(&old, &new).unwrap();
        assert_eq!(
            std::fs::read_to_string(new.join("hwid.txt")).unwrap(),
            "same-device"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "new-settings"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("umiray.exe")).unwrap(),
            "installed-client"
        );
        assert_eq!(
            std::fs::read_to_string(new.join(paths::CORE_NAME)).unwrap(),
            "core"
        );
        assert!(
            old.join("sources/source.raw").exists(),
            "старые данные не удаляем"
        );
        std::fs::remove_file(new.join("sources/source.raw")).unwrap();
        adopt(&old, &new).unwrap();
        assert!(
            !new.join("sources/source.raw").exists(),
            "удалённое не возвращается"
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    /// В копию едут настройки, а не всё подряд (D-163): HWID с другой машины занял бы
    /// чужой слот устройства, кэши качаются заново.
    #[test]
    fn the_export_carries_settings_and_leaves_the_device_id_and_caches() {
        let sandbox = crate::paths::Sandbox::new("export");
        Db::batch(|batch| {
            batch.put(Table::Documents, "advanced", "", "mode: rule\n")?;
            batch.put(Table::Sources, "00000000000000aa", "raw", "vless://x")?;
            batch.put(Table::Collections, "rules", "ru", "rules: []\n")?;
            batch.put(Table::State, "settings", "", "{}")?;
            batch.put(Table::State, "hwid", "", "0123456789abcdef")?;
            batch.put(Table::State, "geo", "", "{}")?;
            batch.put(Table::Lists, "ads", "meta", "{}")?;
            batch.put(Table::Archive, "config.yaml.migrated", "", "old")
        })
        .unwrap();

        let to = sandbox.dir.join("umiray-settings.db");
        DataDir::export(&to).unwrap();
        let copy = rusqlite::Connection::open(&to).unwrap();
        let rows = |table: Table| -> Vec<String> {
            let mut statement = copy
                .prepare(&format!("SELECT id FROM {} ORDER BY id", table.name()))
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<String>>>()
                .unwrap()
        };
        assert_eq!(rows(Table::Documents), ["advanced"]);
        assert_eq!(rows(Table::Sources), ["00000000000000aa"]);
        assert_eq!(rows(Table::Collections), ["rules"]);
        assert_eq!(rows(Table::State), ["settings"], "HWID и кэш стран не едут");
        assert!(rows(Table::Lists).is_empty(), "rule sets — кэш");
        assert!(rows(Table::Archive).is_empty());
        drop(copy);
    }
}
