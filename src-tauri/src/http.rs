//! Общий HTTP-клиент: один User-Agent и один таймаут на все исходящие запросы.
//!
//! Отдельный модуль, потому что наружу ходят двое — подписка и загрузчики ядер, — и
//! представляться они должны одинаково.

use std::time::Duration;

use crate::error::{AppError, Result};

/// Свой User-Agent (D-015). Панели подписок по нему выбирают формат ответа.
pub const USER_AGENT: &str = concat!("umiray/", env!("CARGO_PKG_VERSION"));

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct Http;

impl Http {
    pub fn client() -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| AppError::network(format!("Не удалось создать HTTP-клиент: {e}")))
    }

    /// Клиент **мимо любого прокси**: для API ядра на 127.0.0.1 и для замеров «напрямую».
    ///
    /// `reqwest` на Windows сам читает системный прокси из реестра, а в режиме System там
    /// стоит наше же ядро. Список исключений (`<local>`, `127.*`) он при этом не соблюдает:
    /// запрос к API уходил в ядро как в прокси и возвращался `502 Bad Gateway` — в System
    /// переставали работать трафик, выбор узла и перезагрузка конфига (GOTCHAS). Замеры
    /// «напрямую» (DoH, часы) по той же причине мерили бы наш собственный туннель.
    pub fn direct() -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|e| AppError::network(format!("Не удалось создать HTTP-клиент: {e}")))
    }

    /// Скачать целиком, но не больше `limit` байт (D-137): оборванная или раздутая загрузка
    /// не должна дойти до записи бинаря. Предел проверяется и по заголовку, и по ходу —
    /// заголовку сервер вправе не прислать.
    pub async fn fetch(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
        Http::download(client, url, limit)
            .await
            .map(|download| download.body)
    }

    /// То же, что `fetch`, и ещё когда сервер считает файл изменённым (`Last-Modified`):
    /// по ней видно, что список полугодовалый, даже если скачан сегодня (D-157).
    pub async fn download(client: &reqwest::Client, url: &str, limit: usize) -> Result<Download> {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Не удалось скачать {url}: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::network(format!(
                "Загрузка вернула {status}: {url}"
            )));
        }
        let too_big = || AppError::network(format!("Файл неожиданно велик: {url}"));
        let declared = response.content_length().unwrap_or_default();
        if declared > limit as u64 {
            return Err(too_big());
        }
        let modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(http_date);
        let mut body = Vec::with_capacity(declared as usize);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| AppError::network(format!("Загрузка оборвалась: {e}")))?
        {
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(too_big());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Download { body, modified })
    }
}

pub struct Download {
    pub body: Vec<u8>,
    /// `Last-Modified` в секундах эпохи. Пусто — сервер не сказал или сказал непонятно.
    pub modified: Option<u64>,
}

/// Дата HTTP в формате, который обязан слать сервер (IMF-fixdate):
/// `Mon, 28 Sep 2026 15:05:47 GMT`. Устаревшие формы не разбираем — за ними дата просто
/// не показывается.
fn http_date(text: &str) -> Option<u64> {
    let mut words = text.split_whitespace().skip(1);
    let day: i64 = words.next()?.parse().ok()?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|name| Some(*name) == words.clone().next())? as i64
        + 1;
    let year: i64 = words.nth(1)?.parse().ok()?;
    let mut clock = words
        .next()?
        .split(':')
        .map(|part| part.parse::<i64>().ok());
    let (hours, minutes, seconds) = (clock.next()??, clock.next()??, clock.next()??);
    // Дни от эпохи по гражданскому календарю (алгоритм Говарда Хиннанта).
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let of_era = y - era * 400;
    let of_year = (153 * m + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    u64::try_from(days * 86_400 + hours * 3600 + minutes * 60 + seconds).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_date_becomes_epoch_seconds() {
        assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            http_date("Mon, 28 Sep 2026 15:05:47 GMT"),
            Some(1_790_607_947)
        );
        assert_eq!(
            http_date("Tue, 29 Feb 2028 12:00:00 GMT"),
            Some(1_835_438_400)
        );
        assert_eq!(http_date("вчера"), None);
    }
}
