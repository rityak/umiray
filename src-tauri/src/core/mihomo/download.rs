//! Доставка ядра: последний релиз mihomo с GitHub в каталог приложения (D-006, D-137).
//!
//! Само собой это не запускается: поход в сеть — по кнопке (D-022). Единственное
//! исключение — первый старт без ядра, и оно объявлено отдельно (D-094).
//!
//! Проверять скачанное не по чему — релиз не публикует ни контрольных сумм, ни подписей
//! (S-004). Поэтому доверие держится на TLS и точном адресе официального репозитория, а
//! проверкой целостности служит сама распаковка: оборванная загрузка не разожмётся.

#[cfg(windows)]
use std::io::Cursor;

use crate::error::{AppError, Result};
use crate::http::Http;
use crate::paths::Paths;

const LATEST_RELEASE: &str = "https://github.com/MetaCubeX/mihomo/releases/latest";
const DOWNLOAD_BASE: &str = "https://github.com/MetaCubeX/mihomo/releases/download";
/// Из шести сборок под amd64 эта самая широкая по совместимости с CPU (S-004).
/// Под Windows — `.zip`, под Linux — один сжатый бинарь `.gz`.
#[cfg(windows)]
const VARIANT: &str = "windows-amd64-compatible";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const VARIANT: &str = "linux-amd64-compatible";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const VARIANT: &str = "linux-arm64";
const MAX_ARCHIVE: usize = 128 * 1024 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;

pub struct MihomoDownload;

impl MihomoDownload {
    /// Скачивает ядро и кладёт его на место. Возвращает установленную версию.
    pub async fn install() -> Result<String> {
        let client = Http::client()?;
        let tag = latest_tag(&client).await?;
        let archive = fetch_asset(&client, &tag).await?;
        let binary = extract_executable(&archive)?;

        Paths::ensure_root()?;
        replace_binary(&super::Mihomo::binary(), &binary)?;

        Ok(tag)
    }
}

fn replace_binary(path: &std::path::Path, binary: &[u8]) -> Result<()> {
    crate::atomic::AtomicFile::write(path, binary)
        .map_err(|e| AppError::io(format!("Не удалось заменить ядро: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| AppError::io(format!("Ядро не стало исполняемым: {e}")))?;
    }
    Ok(())
}

/// Версию берём из редиректа `/releases/latest` — GitHub API отдельно ограничивает частоту
/// запросов и на общем адресе легко упереться в лимит.
async fn latest_tag(client: &reqwest::Client) -> Result<String> {
    let response = client
        .get(LATEST_RELEASE)
        .send()
        .await
        .map_err(|e| AppError::network(format!("Не удалось узнать версию ядра: {e}")))?;

    let final_url = response.url().as_str().trim_end_matches('/').to_string();
    let tag = final_url.rsplit('/').next().unwrap_or_default().to_string();
    if !tag.starts_with('v') || tag.len() < 2 {
        return Err(AppError::network(format!(
            "GitHub вернул неожиданный адрес релиза: {final_url}"
        )));
    }
    Ok(tag)
}

async fn fetch_asset(client: &reqwest::Client, tag: &str) -> Result<Vec<u8>> {
    let packed = if cfg!(windows) { "zip" } else { "gz" };
    let url = format!("{DOWNLOAD_BASE}/{tag}/mihomo-{VARIANT}-{tag}.{packed}");
    Http::fetch(client, &url, MAX_ARCHIVE).await
}

/// Под Linux релиз — один бинарь, сжатый gzip. Оборванная загрузка не разожмётся,
/// чужой файл не начнётся с заголовка ELF.
#[cfg(not(windows))]
fn extract_executable(archive: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut binary = Vec::new();
    flate2::read::GzDecoder::new(archive)
        .take(MAX_BINARY + 1)
        .read_to_end(&mut binary)
        .map_err(|e| AppError::invalid(format!("Сжатое ядро не распаковалось: {e}")))?;
    if binary.is_empty() || binary.len() as u64 > MAX_BINARY {
        return Err(AppError::invalid(
            "Исполняемый файл ядра имеет неожиданный размер",
        ));
    }
    if !binary.starts_with(b"\x7fELF") {
        return Err(AppError::invalid("В релизе файл без заголовка ELF"));
    }
    Ok(binary)
}

/// В архиве ровно один файл — сам бинарь; имя внутри зависит от варианта сборки,
/// поэтому ищем по расширению, а не по точному имени.
#[cfg(windows)]
fn extract_executable(archive: &[u8]) -> Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|e| AppError::invalid(format!("Архив с ядром не читается: {e}")))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| AppError::invalid(format!("Архив с ядром повреждён: {e}")))?;
        if !entry.name().to_ascii_lowercase().ends_with(".exe") {
            continue;
        }
        if entry.size() == 0 || entry.size() > MAX_BINARY {
            return Err(AppError::invalid(
                "Исполняемый файл ядра имеет неожиданный размер",
            ));
        }
        let mut binary = Vec::with_capacity(entry.size() as usize);
        std::io::copy(&mut entry, &mut binary)
            .map_err(|e| AppError::invalid(format!("Не удалось распаковать ядро: {e}")))?;
        if !binary.starts_with(b"MZ") {
            return Err(AppError::invalid(
                "В архиве файл .exe без заголовка Windows",
            ));
        }
        return Ok(binary);
    }

    Err(AppError::invalid("В архиве релиза нет исполняемого файла"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn broken_archive_is_rejected_rather_than_written() {
        assert!(extract_executable(b"not a zip at all").is_err());
        // Пустой, но валидный zip: читается, а ядра в нём нет.
        let empty_zip = [
            0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert!(extract_executable(&empty_zip).is_err());
    }

    #[test]
    #[cfg(not(windows))]
    fn only_a_gzipped_elf_is_a_core() {
        use std::io::Write;
        let gzip = |body: &[u8]| {
            let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            out.write_all(body).unwrap();
            out.finish().unwrap()
        };
        assert!(extract_executable(b"not gzip").is_err());
        assert!(extract_executable(&gzip(b"MZ windows")).is_err());
        assert_eq!(
            extract_executable(&gzip(b"\x7fELF core")).unwrap(),
            b"\x7fELF core"
        );
    }

    #[test]
    fn an_existing_core_is_replaced_on_windows() {
        let path = std::env::temp_dir().join(format!(
            "umiray-core-replace-{}.exe",
            crate::stamp::Stamp::id().unwrap()
        ));
        std::fs::write(&path, b"old").unwrap();
        replace_binary(&path, b"MZ-new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"MZ-new");
        std::fs::remove_file(path).unwrap();
    }
}
