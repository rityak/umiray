//! Общий HTTP-клиент: один User-Agent и один таймаут на все исходящие запросы.
//!
//! Отдельный модуль, потому что наружу ходят двое — подписка и загрузчик ядра, — и
//! представляться они должны одинаково.

use std::time::Duration;

use crate::error::{AppError, Result};

/// Свой User-Agent (D-015). Панели подписок по нему выбирают формат ответа.
pub const USER_AGENT: &str = concat!("umiray/", env!("CARGO_PKG_VERSION"));

const TIMEOUT: Duration = Duration::from_secs(60);

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
