//! Бинарник с выпуска на GitHub: последний тег из ленты, файл и его сумма из
//! `checksums.txt`. Так качаются qd (D-161) и VOLT (D-181); mihomo — своим путём (D-137).

use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::http::Http;

const SUMS: &str = "checksums.txt";

pub struct Release;

impl Release {
    /// Последний выпуск `repo` (`https://github.com/owner/name`) и его файл `asset`, сверенный
    /// с `checksums.txt` того же выпуска. Не сошлось — отказ: на диск ничего не попадает.
    /// `what` — имя для человека в отказе.
    pub async fn fetch(
        repo: &str,
        asset: &str,
        limit: usize,
        what: &str,
    ) -> Result<(String, Vec<u8>)> {
        let client = Http::client()?;
        let feed = Http::fetch(&client, &format!("{repo}/releases.atom"), 1024 * 1024)
            .await
            .map_err(|e| AppError::network(format!("Не удалось узнать версию {what}: {e}")))?;
        let tag = newest_tag(&String::from_utf8_lossy(&feed))
            .ok_or_else(|| AppError::network(format!("У {what} нет ни одного выпуска")))?;
        let link = |name: &str| format!("{repo}/releases/download/{tag}/{name}");
        let sums = Http::fetch(&client, &link(SUMS), 64 * 1024).await?;
        let want = checksum(&String::from_utf8_lossy(&sums), asset)
            .ok_or_else(|| AppError::invalid(format!("В {SUMS} нет строки для {asset}")))?;
        let body = Http::fetch(&client, &link(asset), limit).await?;
        if sha256(&body) != want {
            return Err(AppError::invalid(format!(
                "Контрольная сумма {what} не совпала — файл не записан"
            )));
        }
        Ok((tag, body))
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Лента выпусков GitHub идёт от нового к старому: первый тег — последний.
fn newest_tag(feed: &str) -> Option<String> {
    let (_, rest) = feed.split_once("/releases/tag/")?;
    let tag: String = rest
        .chars()
        .take_while(|c| !matches!(c, '"' | '\'' | '<' | '>') && !c.is_whitespace())
        .collect();
    (!tag.is_empty()).then_some(tag)
}

/// Строка `checksums.txt` для файла → его SHA-256 строчными. Формат `sha256sum`:
/// хеш, пробелы, имя; звёздочка перед именем — двоичный режим.
fn checksum(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Лента идёт от нового к старому; тег — до кавычки, как в настоящей ленте GitHub.
    #[test]
    fn the_newest_tag_is_the_first_one_in_the_feed() {
        let feed = r#"<feed><entry><link rel="alternate" type="text/html" href="https://github.com/jaywehosl/qd/releases/tag/v0.1.7-alpha"/></entry>
<entry><link href="https://github.com/jaywehosl/qd/releases/tag/v0.1.6-alpha"/></entry></feed>"#;
        assert_eq!(newest_tag(feed).as_deref(), Some("v0.1.7-alpha"));
        assert_eq!(newest_tag("<feed></feed>"), None);
        assert_eq!(newest_tag(r#"href=".../releases/tag/""#), None);
    }

    #[test]
    fn the_checksum_line_is_found_by_file_name() {
        let sums = "ABC123  qd-core-windows-amd64.exe\n\
                    def456 *qd-windows-amd64.exe\n";
        assert_eq!(
            checksum(sums, "qd-core-windows-amd64.exe").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            checksum(sums, "qd-windows-amd64.exe").as_deref(),
            Some("def456")
        );
        assert_eq!(checksum(sums, "qd-core-linux-amd64"), None);
    }

    #[test]
    fn the_digest_is_lowercase_hex() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
