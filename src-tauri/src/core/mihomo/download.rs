//! Доставка ядра: последний релиз выбранной сборки с GitHub в каталог приложения
//! (D-006, D-137, D-194).
//!
//! Само собой это не запускается: поход в сеть — по кнопке (D-022). Единственное
//! исключение — выбранной сборки нет на диске, и оно объявлено отдельно (D-094).
//!
//! mihomo не публикует ни контрольных сумм, ни подписей (S-004): доверие держится на TLS
//! и точном адресе официального репозитория, а проверкой целостности служит сама
//! распаковка. miray публикует `sha256sums.txt` — его архив сверяется. И то и другое
//! считается вставшим, только когда ответило на `-v`.

#[cfg(windows)]
use std::io::Cursor;
use std::process::Command;

use super::build::Build;
use crate::core::release;
use crate::error::{AppError, Result};
use crate::http::Http;
use crate::paths::Paths;

const MIHOMO: &str = "https://github.com/MetaCubeX/mihomo";
const MIRAY: &str = "https://github.com/rityak/miray";
const MIRAY_SUMS: &str = "sha256sums.txt";
/// Из шести сборок mihomo под amd64 эта самая широкая по совместимости с CPU (S-004).
/// Под Windows — `.zip`, под Linux — один сжатый бинарь `.gz`.
#[cfg(windows)]
const VARIANT: &str = "windows-amd64-compatible";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const VARIANT: &str = "linux-amd64-compatible";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const VARIANT: &str = "linux-arm64";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const VARIANT: &str = "darwin-arm64";
/// У miray то же самое — `amd64-v1`, любой x86-64; `v3` требует AVX2.
#[cfg(windows)]
const MIRAY_VARIANT: &str = "windows-amd64-v1";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MIRAY_VARIANT: &str = "linux-amd64-v1";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const MIRAY_VARIANT: &str = "linux-arm64";
#[cfg(target_os = "macos")]
const MIRAY_VARIANT: &str = "darwin-arm64";
/// Журнал аномалий `-diag` — разработке, не людям (D-194). Под arm64 его не выпускают.
const MIRAY_DIAG: bool = cfg!(debug_assertions) && !cfg!(target_arch = "aarch64");
const PACKED: &str = if cfg!(windows) { "zip" } else { "gz" };
const MAX_ARCHIVE: usize = 128 * 1024 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;

pub struct CoreDownload;

impl CoreDownload {
    /// Скачивает сборку и кладёт её на место. Возвращает установленную версию.
    pub async fn install(build: Build) -> Result<String> {
        let client = Http::client()?;
        let repo = repo(build);
        let tag = latest_tag(&client, repo).await?;
        let asset = asset(build, &tag);
        let link = |name: &str| format!("{repo}/releases/download/{tag}/{name}");
        let archive = Http::fetch(&client, &link(&asset), MAX_ARCHIVE).await?;
        if build == Build::Miray {
            let sums = Http::fetch(&client, &link(MIRAY_SUMS), 64 * 1024).await?;
            verify(&archive, &String::from_utf8_lossy(&sums), &asset)?;
        }
        let binary = extract_executable(&archive)?;

        Paths::ensure_root()?;
        let path = build.binary();
        replace_binary(&path, &binary)?;
        if let Err(why) = answers(&path) {
            let _ = std::fs::remove_file(&path);
            return Err(why);
        }
        Ok(tag)
    }
}

fn repo(build: Build) -> &'static str {
    match build {
        Build::Mihomo => MIHOMO,
        Build::Miray => MIRAY,
    }
}

/// Имя файла в релизе: `mihomo-windows-amd64-compatible-v1.19.33.zip`,
/// `miray-windows-amd64-v1-diag-v1.19.33-miray.1.zip`.
fn asset(build: Build, tag: &str) -> String {
    match build {
        Build::Mihomo => format!("mihomo-{VARIANT}-{tag}.{PACKED}"),
        Build::Miray => {
            let diag = if MIRAY_DIAG { "-diag" } else { "" };
            format!("miray-{MIRAY_VARIANT}{diag}-{tag}.{PACKED}")
        }
    }
}

/// Не сошлось — отказ до распаковки: на диск ничего не попадает.
fn verify(archive: &[u8], sums: &str, asset: &str) -> Result<()> {
    let want = release::checksum(sums, asset)
        .ok_or_else(|| AppError::invalid(format!("В {MIRAY_SUMS} нет строки для {asset}")))?;
    if release::sha256(archive) != want {
        return Err(AppError::invalid(
            "Контрольная сумма ядра не совпала — файл не записан",
        ));
    }
    Ok(())
}

/// Встало — значит запускается: файл, который не печатает версию, ядром не считается (D-194).
fn answers(path: &std::path::Path) -> Result<()> {
    let mut command = Command::new(path);
    command.arg("-v");
    crate::system::console::Console::hide(&mut command, false);
    let output = command
        .output()
        .map_err(|e| AppError::io(format!("Скачанное ядро не запускается: {e}")))?;
    if !output.status.success() {
        return Err(AppError::io(format!(
            "Скачанное ядро не ответило на -v ({})",
            output.status
        )));
    }
    Ok(())
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
async fn latest_tag(client: &reqwest::Client, repo: &str) -> Result<String> {
    let response = client
        .get(format!("{repo}/releases/latest"))
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

/// Под Linux и macOS релиз — один бинарь, сжатый gzip. Проверяем формат и,
/// на Apple Silicon, тип процессора в заголовке Mach-O.
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
    #[cfg(target_os = "linux")]
    if !binary.starts_with(b"\x7fELF") {
        return Err(AppError::invalid("В релизе файл без заголовка ELF"));
    }
    #[cfg(target_os = "macos")]
    if !macos_arm64(&binary) {
        return Err(AppError::invalid(
            "В релизе файл не является исполняемым Mach-O для Apple Silicon",
        ));
    }
    Ok(binary)
}

#[cfg(target_os = "macos")]
fn macos_arm64(binary: &[u8]) -> bool {
    // MH_MAGIC_64 и CPU_TYPE_ARM64 в little-endian, как в тонком darwin/arm64 бинаре.
    binary.len() >= 8 && binary[..8] == [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01]
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

    #[cfg(not(windows))]
    fn gzip(body: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        out.write_all(body).unwrap();
        out.finish().unwrap()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn only_a_gzipped_elf_is_a_core() {
        assert!(extract_executable(b"not gzip").is_err());
        assert!(extract_executable(&gzip(b"MZ windows")).is_err());
        assert_eq!(
            extract_executable(&gzip(b"\x7fELF core")).unwrap(),
            b"\x7fELF core"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn only_a_gzipped_arm64_macho_is_a_core() {
        let arm64 = [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01, 0, 0];
        let x64 = [0xcf, 0xfa, 0xed, 0xfe, 0x07, 0x00, 0x00, 0x01, 0, 0];
        assert!(extract_executable(b"not gzip").is_err());
        assert!(extract_executable(&gzip(b"\x7fELF linux")).is_err());
        assert!(extract_executable(&gzip(&x64)).is_err());
        assert_eq!(extract_executable(&gzip(&arm64)).unwrap(), arm64);
        assert_eq!(
            asset(Build::Mihomo, "v1.19.34"),
            "mihomo-darwin-arm64-v1.19.34.gz"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore]
    fn live_downloads_and_runs_the_native_core() {
        let _sandbox = crate::paths::Sandbox::new("macos-core-download");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let version = runtime
            .block_on(CoreDownload::install(Build::Mihomo))
            .unwrap();
        assert!(version.starts_with('v'));
        assert!(Build::Mihomo.present());
    }

    /// Имена — как в настоящих релизах: промах здесь — 404 вместо ядра.
    #[test]
    #[cfg(windows)]
    fn asset_names_match_the_releases() {
        assert_eq!(
            asset(Build::Mihomo, "v1.19.33"),
            "mihomo-windows-amd64-compatible-v1.19.33.zip"
        );
        let diag = if cfg!(debug_assertions) { "-diag" } else { "" };
        assert_eq!(
            asset(Build::Miray, "v1.19.33-miray.1"),
            format!("miray-windows-amd64-v1{diag}-v1.19.33-miray.1.zip")
        );
    }

    #[test]
    fn a_miray_archive_must_match_its_sum() {
        let sums = format!("{}  core.zip\n", release::sha256(b"archive"));
        assert!(verify(b"archive", &sums, "core.zip").is_ok());
        assert!(verify(b"tampered", &sums, "core.zip").is_err());
        assert!(verify(b"archive", &sums, "other.zip").is_err());
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
