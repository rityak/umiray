//! Переезд каталога данных stable/dev (D-150), без удаления прежней установки.

use std::path::Path;

use crate::error::Result;
use crate::paths;

const MARKER: &str = ".data-v2";

pub fn migrate() -> Result<()> {
    adopt(&paths::legacy_root(), &paths::root())
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
    crate::atomic::write(marker, "1\n")?;
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
            crate::atomic::write(target, std::fs::read(entry.path())?)?;
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
        let temp =
            std::env::temp_dir().join(format!("umiray-data-{}", crate::stamp::id().unwrap()));
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
}
