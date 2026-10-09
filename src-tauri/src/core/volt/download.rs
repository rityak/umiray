//! Бинарники VOLT с выпуска `umiray-core` (D-181): один архив и `checksums.txt`. Из архива
//! берутся только известные файлы — что ещё в нём окажется, на диск не попадает.

use std::io::{Cursor, Read};

use crate::core::release::{sha256, Release};
use crate::error::{AppError, Result};
use crate::paths::Paths;

/// Выпуски `umiray-core`.
const REPO: &str = "https://github.com/rityak/umiray-core";
const ASSET: &str = "volt-windows-amd64.zip";
const MAX_ARCHIVE: usize = 64 * 1024 * 1024;
/// Без чего VOLT не работает. Лицензии берутся рядом, если они есть в архиве.
pub const REQUIRED: [&str; 4] = [
    "volt-relay.exe",
    "volt.exe",
    "WinDivert.dll",
    "WinDivert64.sys",
];

pub struct VoltDownload;

impl VoltDownload {
    pub fn present() -> bool {
        REQUIRED
            .iter()
            .all(|name| Paths::volt_dir().join(name).is_file())
    }

    /// Скачать последний выпуск и положить в каталог VOLT. Помощники должны быть погашены:
    /// работающий exe не заменить. Совпадающий файл не переписывается — загруженный
    /// драйвер заперт, а заменять его тем же незачем.
    pub async fn install() -> Result<String> {
        if !cfg!(windows) {
            return Err(AppError::invalid("VOLT работает только в Windows"));
        }
        let (tag, archive) = Release::fetch(REPO, ASSET, MAX_ARCHIVE, "VOLT").await?;
        let files = unpack(&archive)?;
        let directory = Paths::volt_dir();
        std::fs::create_dir_all(&directory)?;
        for (name, body) in files {
            let target = directory.join(&name);
            let same = std::fs::read(&target).is_ok_and(|old| sha256(&old) == sha256(&body));
            if !same {
                crate::atomic::AtomicFile::write(&target, &body)
                    .map_err(|e| AppError::io(format!("Не удалось записать {name}: {e}")))?;
            }
        }
        Ok(tag)
    }
}

/// Нужные файлы архива по имени. Пути внутри архива не доверяем: берётся только имя, и только
/// из списка; без любого обязательного — отказ целиком.
fn unpack(archive: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|e| AppError::invalid(format!("Архив VOLT не читается: {e}")))?;
    let mut files = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| AppError::invalid(format!("Архив VOLT не читается: {e}")))?;
        let Some(name) = entry.enclosed_name().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        }) else {
            continue;
        };
        let wanted = REQUIRED.contains(&name.as_str())
            || (name.contains("LICENSE") && name.ends_with(".txt"));
        if !entry.is_file() || !wanted {
            continue;
        }
        let mut body = Vec::new();
        entry
            .by_ref()
            .take(MAX_ARCHIVE as u64)
            .read_to_end(&mut body)
            .map_err(|e| AppError::invalid(format!("Архив VOLT не читается: {e}")))?;
        files.push((name, body));
    }
    if let Some(missing) = REQUIRED
        .iter()
        .find(|name| !files.iter().any(|(file, _)| file == *name))
    {
        return Err(AppError::invalid(format!("В архиве VOLT нет {missing}")));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Живая: последний выпуск `umiray-core` скачивается, сходится с `checksums.txt`
    /// и содержит всё, что клиент запускает. На диск ничего не пишет.
    /// `cargo test live_the_release -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_the_release_has_what_the_client_runs() {
        let (tag, archive) = Release::fetch(REPO, ASSET, MAX_ARCHIVE, "VOLT")
            .await
            .unwrap();
        let files = unpack(&archive).unwrap();
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        println!("{tag}: {names:?}");
        assert!(REQUIRED.iter().all(|name| names.contains(name)));
        assert!(names.iter().any(|name| name.contains("LICENSE")));
    }

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn only_known_files_leave_the_archive() {
        let mut files: Vec<(&str, &[u8])> = REQUIRED
            .iter()
            .map(|name| (*name, b"MZ".as_slice()))
            .collect();
        files.push(("WinDivert-LICENSE.txt", b"LGPL"));
        files.push(("README.md", b"not ours"));
        files.push(("../evil.exe", b"MZ"));
        let unpacked = unpack(&archive(&files)).unwrap();
        let names: Vec<&str> = unpacked.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names.len(), 5, "{names:?}");
        assert!(names.contains(&"WinDivert-LICENSE.txt"));
        assert!(!names.contains(&"README.md") && !names.contains(&"evil.exe"));
    }

    #[test]
    fn an_archive_without_the_driver_is_refused() {
        let files: Vec<(&str, &[u8])> = REQUIRED
            .iter()
            .filter(|name| **name != "WinDivert64.sys")
            .map(|name| (*name, b"MZ".as_slice()))
            .collect();
        let error = unpack(&archive(&files)).unwrap_err().to_string();
        assert!(error.contains("WinDivert64.sys"), "{error}");
        assert!(unpack(b"not a zip").is_err());
    }
}
