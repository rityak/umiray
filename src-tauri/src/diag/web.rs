//! Каким адресом нас видят снаружи — проба сторожа соединения (D-107).
//!
//! Ходит обычным HTTPS тем же клиентом, что и подписки: вопрос «через кого я вышел»
//! решается самим ответом, отдельный транспорт ему не нужен.

use std::time::Duration;

use crate::http;

/// Сколько ждём ответа. Три секунды: зарезанный адрес не ответит и за тридцать,
/// а живой отвечает за сотни миллисекунд.
const TIMEOUT: Duration = Duration::from_secs(3);

/// Куда ходим за внешним адресом. Две точки: одна может лежать, а вопрос «через кого
/// я вышел» слишком важен, чтобы зависеть от одного сервера.
const WHOAMI: [&str; 2] = ["https://api.ipify.org", "https://ifconfig.me/ip"];

pub struct WebProbe;

impl WebProbe {
    /// Внешний адрес: первая точка, которая ответила. Пусто — не ответил никто.
    /// Через прокси, если задан порт; иначе напрямую — даже когда в Windows стоит наш же
    /// прокси (System).
    pub async fn whoami(through: Option<u16>) -> Option<String> {
        for url in WHOAMI {
            let mut builder = reqwest::Client::builder()
                .user_agent(http::USER_AGENT)
                .timeout(TIMEOUT)
                .no_proxy();
            if let Some(port) = through {
                let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).ok()?;
                builder = builder.proxy(proxy);
            }
            let Ok(client) = builder.build() else {
                continue;
            };
            let Ok(response) = client.get(url).send().await else {
                continue;
            };
            let Ok(text) = response.text().await else {
                continue;
            };
            let address = text.trim().to_string();
            if address.parse::<std::net::IpAddr>().is_ok() {
                return Some(address);
            }
        }
        None
    }
}
