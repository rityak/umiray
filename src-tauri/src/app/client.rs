//! Клиентский конфиг: то, чего нет в конфиге ядра (D-068).
//!
//! Устроен ровно как режим перехвата (D-052, образец — `config/mode.rs`): документ
//! на диске главный, форма пишет в него точечно, а не хранит своё значение отдельно.
//! Разница одна — ядру этот файл не уходит.
//!
//! **Комментарии при записи из формы теряются**: файл пересобирается через `serde_yaml`,
//! а он их не хранит. Та же цена, что и в конфиге ядра.

use serde_yaml::Value;

use crate::config::files::{self, CLIENT};
use crate::error::{AppError, Result};
use crate::nodes::ping::Method;
use crate::yaml::{set, top_mapping};

/// Ключ поля. Нужен обеим сторонам — и чтению, и записи.
const PING: &str = "ping";

/// Чем мерить, сколько до сервера (D-069).
///
/// Файл правится руками, поэтому это **граница с недоверенными данными**: непонятное
/// значение — ошибка с внятным текстом, а не молчаливое умолчание. Отсутствие поля —
/// другое дело: его просто ещё не написали.
pub fn ping() -> Result<Method> {
    let map = top_mapping(&files::read(CLIENT)?)?;
    let Some(value) = map.get(Value::from(PING)) else {
        return Ok(Method::default());
    };
    serde_yaml::from_value(value.clone()).map_err(|_| {
        AppError::invalid(format!(
            "В client.yaml непонятное значение ping: {}. Ожидается icmp, tcp, proxy или proxy-keepalive.",
            serde_yaml::to_string(value).unwrap_or_default().trim()
        ))
    })
}

/// Записать способ, не тронув ничего лишнего.
pub fn set_ping(method: Method) -> Result<()> {
    let mut map = top_mapping(&files::read(CLIENT)?)?;
    let value = serde_yaml::to_value(method).map_err(|e| AppError::invalid(e.to_string()))?;
    set(&mut map, PING, value);
    let text = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|e| AppError::invalid(e.to_string()))?;
    files::write(CLIENT, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Разбор отделён от диска: правило проверяется обычным `cargo test`, не трогая
    /// настоящий `%LOCALAPPDATA%` пользователя.
    fn of(text: &str) -> Result<Method> {
        let map = top_mapping(text).unwrap();
        match map.get(Value::from(PING)) {
            None => Ok(Method::default()),
            Some(value) => serde_yaml::from_value(value.clone())
                .map_err(|_| AppError::invalid("непонятное значение")),
        }
    }

    #[test]
    fn every_documented_value_reads_back() {
        assert_eq!(of("ping: icmp").unwrap(), Method::Icmp);
        assert_eq!(of("ping: tcp").unwrap(), Method::Tcp);
        assert_eq!(of("ping: proxy").unwrap(), Method::Proxy);
        assert_eq!(of("ping: proxy-keepalive").unwrap(), Method::ProxyKeepalive);
    }

    /// Пустой файл — это умолчание, а мусор — ошибка. Разница принципиальная: первое
    /// значит «ещё не выбирали», второе — «написано непонятное», и молча превращать
    /// второе в первое нельзя.
    #[test]
    fn a_missing_field_is_the_default_and_nonsense_is_an_error() {
        assert_eq!(of("").unwrap(), Method::default());
        assert_eq!(of("что-то: другое").unwrap(), Method::default());
        assert!(of("ping: пинг").is_err());
        assert!(of("ping: 12").is_err());
    }

    /// Шаблон — это документация: значение в нём обязано читаться тем же кодом.
    #[test]
    fn the_template_says_a_value_the_client_understands() {
        let template = files::template(CLIENT).unwrap();
        assert_eq!(of(template).unwrap(), Method::Tcp);
    }
}
