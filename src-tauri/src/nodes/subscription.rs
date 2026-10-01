//! Подписка: как представиться провайдеру и что сделать с ответом.
//!
//! Ссылки не разбираем — их прочитает ядро (D-031). Скачиваем сами, потому что панель
//! опознаёт устройство по нашим заголовкам (S-006, S-007), а служебные записи провайдера
//! надо показать отдельно, а не молча превратить в узлы.

use crate::error::{AppError, Result};
use crate::nodes::device::Device;
use crate::nodes::link::LinkParser;

/// Скачать тело подписки.
///
/// `x-hwid` обязателен: без него панель отвечает 404 и подписка не открывается вовсе.
/// Остальные заголовки необязательны, но по ним человек различает свои устройства в списке
/// панели — иначе непонятно, какой слот чей (стандарт XTLS, документация Remnawave).
pub struct Fetched {
    pub body: String,
    /// Как панель сама себя называет. Замерено: приходит в `profile-title`, иногда
    /// в виде `base64:...`. Лучше хоста из адреса — его человек не выбирал.
    pub title: Option<String>,
}

pub struct Subscription;

impl Subscription {
    pub async fn fetch(url: &str) -> Result<Fetched> {
        let response = crate::http::Http::client()?
            .get(url)
            .header("X-Hwid", Device::hwid()?)
            .header("X-Device-Os", "Windows")
            .header("X-Ver-Os", Device::os_version())
            .header("X-Device-Model", "PC")
            .send()
            .await
            .map_err(|e| AppError::network(format!("Не удалось получить подписку: {e}")))?;

        // Панель говорит о переполнении отдельным заголовком — это точный ответ на вопрос
        // «почему список пуст», в отличие от догадки по заглушкам в теле.
        if response
            .headers()
            .get("x-hwid-max-devices-reached")
            .is_some_and(|value| value.as_bytes() == b"true")
        {
            return Err(AppError::Subscription {
                message: "Достигнут лимит устройств в подписке".into(),
                notices: vec![
                    "Освободите слот в личном кабинете провайдера или увеличьте лимит.".into(),
                    format!("Это устройство: {}", Device::hwid()?),
                ],
            });
        }

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            // 404 у панели с привязкой означает именно «не понял, что за устройство».
            return Err(AppError::Subscription {
                message: "Панель не приняла запрос: 404".into(),
                notices: vec![
                    "Обычно это значит, что адрес подписки неверен либо панель не приняла \
идентификатор устройства."
                        .into(),
                ],
            });
        }
        if !status.is_success() {
            return Err(AppError::network(format!("Подписка ответила {status}")));
        }

        let title = response
            .headers()
            .get("profile-title")
            .and_then(|value| value.to_str().ok())
            .and_then(decode_title);
        let body = response
            .text()
            .await
            .map_err(|e| AppError::network(format!("Не удалось прочитать ответ: {e}")))?;
        Ok(Fetched { body, title })
    }

    /// Тело подписки в список ссылок и отдельно — служебные записи провайдера.
    pub fn links(body: &str) -> (Vec<String>, Vec<String>) {
        let text = decode_base64(body).unwrap_or_else(|| body.to_string());
        let mut nodes = Vec::new();
        let mut notices = Vec::new();

        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if is_placeholder(line) {
                notices.push(LinkParser::name_of(line).unwrap_or_else(|| line.to_string()));
            } else {
                nodes.push(line.to_string());
            }
        }
        (nodes, notices)
    }
}

/// `profile-title` приходит либо открытым текстом, либо как `base64:<...>`.
fn decode_title(raw: &str) -> Option<String> {
    let title = match raw.strip_prefix("base64:") {
        Some(encoded) => crate::nodes::codec::Base64::decode_text(encoded)?,
        None => raw.to_string(),
    };
    let title = title.trim();
    (!title.is_empty()).then(|| crate::nodes::link::LinkParser::normalize(title))
}

/// Заглушка провайдера — ссылка в никуда: адрес `0.0.0.0` он ставит именно для этого.
fn is_placeholder(line: &str) -> bool {
    line.contains("@0.0.0.0:")
}

/// Тело подписки в base64. Отличаем его от открытого текста по тому, что внутри оказались
/// ссылки: сам по себе декодер об этом не судит, он нужен и разбору отдельных ссылок.
fn decode_base64(body: &str) -> Option<String> {
    let text = crate::nodes::codec::Base64::decode_text(body)?;
    text.contains("://").then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn the_panel_name_arrives_either_plain_or_in_base64() {
        assert_eq!(
            decode_title("base64:Q2FweUh1Yg==").as_deref(),
            Some("CapyHub")
        );
        assert_eq!(decode_title("CapyHub").as_deref(), Some("CapyHub"));
        assert_eq!(decode_title("  ").as_deref(), None, "пустое имя — не имя");
    }

    const LIMIT: &str = "vless://0000@0.0.0.0:1?encryption=none#%D0%9B%D0%B8%D0%BC%D0%B8%D1%82";
    const LIVE: &str = "vless://uuid@real.example:443?encryption=none#Sweden";

    #[test]
    fn links_in_plain_text_and_base64_read_the_same() {
        let plain = format!("{LIMIT}\n{LIVE}\n");
        let encoded = base64::engine::general_purpose::STANDARD.encode(&plain);

        for body in [plain.clone(), encoded] {
            let (nodes, notices) = Subscription::links(&body);
            assert_eq!(nodes, [LIVE], "живой узел один");
            assert_eq!(notices, ["Лимит"], "заглушку показываем, а не прячем");
        }
    }

    /// Схемы, которых мы не знаем, проходят насквозь: разбирать их будет ядро.
    #[test]
    fn unknown_schemes_pass_through_untouched() {
        let exotic = "juicity://uuid:pass@juicity.example:443?congestion_control=bbr#J";
        let (nodes, notices) = Subscription::links(exotic);
        assert_eq!(nodes, [exotic]);
        assert!(notices.is_empty());
    }
}
