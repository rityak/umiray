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
        Ok(body)
    }
}
