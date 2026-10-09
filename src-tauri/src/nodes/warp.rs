//! Cloudflare WARP выпускает сам клиент (D-165).
//!
//! Протокол — мобильного клиента WARP, тот же, что у wgcf, usque и Throne: `POST /reg`
//! с ключом X25519 заводит устройство, для MASQUE `PATCH /reg/<id>` досылает ключ ECDSA
//! P-256. Ответ Cloudflare становится записью узла; токен устройства узлу не нужен и не
//! хранится. Здесь — разговор с API; запись из ответа собирает `warp_entry`, ключ P-256 —
//! `warp_key`, и оба проверяются без сети.

use serde::Deserialize;
use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::nodes::warp_entry::{masque_entry, wireguard_entry, Device};

const API: &str = "https://api.cloudflareclient.com/v0a4471";
/// Заголовки мобильного клиента: API отвечает на них, а не на наш User-Agent (D-015).
const CLIENT_VERSION: &str = "a-6.35-4471";
const CLIENT_AGENT: &str = "WARP for Android";
/// Какой туннель выпустить. Приходит из окна строкой, незнакомое отвергается (D-144).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tunnel {
    Masque,
    Wireguard,
}

/// Пара X25519 устройства в base64. Её даёт ядро (`KeyGen`), а не клиент.
pub struct WgKeys {
    pub private: String,
    pub public: String,
}

pub struct Warp;

impl Warp {
    /// Зарегистрировать устройство и собрать узел. `mask` — маскировка рукопожатия WireGuard
    /// (D-118): кладётся там, где узел рождается, как у ссылки.
    ///
    /// `through` — порт `mixed` работающего ядра. В российских сетях API режут по имени
    /// в рукопожатии (S-029): напрямую запрос не доходит, и идёт он через уже поднятый VPN,
    /// как у Throne. Без ядра — напрямую, и отказ говорит, что сделать.
    pub async fn issue(
        tunnel: Tunnel,
        keys: &WgKeys,
        mask: Option<Value>,
        through: Option<u16>,
        bootstrap: Option<reqwest::Client>,
    ) -> Result<Mapping> {
        let volt = bootstrap.is_some();
        let client = match bootstrap {
            Some(client) => client,
            None => client(through)?,
        };
        let serial = crate::stamp::Stamp::id()?;
        let body = register_body(&keys.public, &serial, &crate::stamp::Stamp::utc());
        let device = call(&client, reqwest::Method::POST, "/reg", None, &body)
            .await
            .map_err(|error| {
                if volt {
                    error
                } else {
                    unreachable(error, through)
                }
            })?;
        if device.id.is_empty() || device.token.is_empty() {
            return Err(AppError::network(
                "Cloudflare зарегистрировал устройство, но не вернул его номер",
            ));
        }
        match tunnel {
            Tunnel::Wireguard => wireguard_entry(&device, keys, mask),
            Tunnel::Masque => {
                let key = crate::nodes::warp_key::MasqueKey::generate()?;
                let body = serde_json::json!({
                    "key": key.public,
                    "key_type": "secp256r1",
                    "tunnel_type": "masque",
                });
                let path = format!("/reg/{}", device.id);
                let enrolled = call(
                    &client,
                    reqwest::Method::PATCH,
                    &path,
                    Some(&device.token),
                    &body,
                )
                .await?;
                masque_entry(&enrolled, &key.private)
            }
        }
    }
}

fn client(through: Option<u16>) -> Result<reqwest::Client> {
    let Some(port) = through else {
        return crate::http::Http::client();
    };
    let broken =
        |e: reqwest::Error| AppError::network(format!("Не удалось создать HTTP-клиент: {e}"));
    let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).map_err(broken)?;
    reqwest::Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(broken)
}

/// Недоступность — не отказ: Cloudflare не ответил вовсе. Напрямую это значит, что путь
/// закрыт, и сказать надо, что поможет, а не только что сломалось.
fn unreachable(error: AppError, through: Option<u16>) -> AppError {
    let AppError::Network(message) = &error else {
        return error;
    };
    if !message.starts_with("Cloudflare не ответил") {
        return error;
    }
    match through {
        None => AppError::network(
            "API Cloudflare недоступен напрямую. Подключитесь к любому \
             узлу и выпустите снова: запрос пойдёт через прокси",
        ),
        Some(_) => AppError::network(format!(
            "API Cloudflare не ответил и через прокси — проверьте, что узел работает ({message})"
        )),
    }
}

fn register_body(public: &str, serial: &str, tos: &str) -> serde_json::Value {
    serde_json::json!({
        "key": public,
        "install_id": "",
        "fcm_token": "",
        "tos": tos,
        "model": "PC",
        "serial_number": serial,
        "key_type": "curve25519",
        "tunnel_type": "wireguard",
        "locale": "en_US",
    })
}

async fn call(
    client: &reqwest::Client,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: &serde_json::Value,
) -> Result<Device> {
    let mut request = client
        .request(method, format!("{API}{path}"))
        .header(reqwest::header::USER_AGENT, CLIENT_AGENT)
        .header("CF-Client-Version", CLIENT_VERSION)
        .json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| AppError::network(format!("Cloudflare не ответил: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AppError::network(format!("Ответ Cloudflare оборвался: {e}")))?;
    if !status.is_success() {
        return Err(AppError::network(refusal(status.as_u16(), &text)));
    }
    serde_json::from_str(&text)
        .map_err(|e| AppError::network(format!("Cloudflare ответил не так, как ждали: {e}")))
}

/// Отказ Cloudflare словами: их сообщение, если оно есть, и что делать при частых запросах.
fn refusal(status: u16, body: &str) -> String {
    if status == 429 {
        return "Cloudflare просит подождать: слишком частые регистрации".into();
    }
    let said: Vec<String> = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("errors")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|error| error.get("message")?.as_str().map(str::to_string))
        .collect();
    match said.is_empty() {
        true => format!("Cloudflare отказал: {status}"),
        false => format!("Cloudflare отказал ({status}): {}", said.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_register_body_names_the_terms_and_a_curve25519_key() {
        let body = register_body("PUB", "0011223344556677", "2026-10-01T10:00:00.000Z");
        assert_eq!(body["key"], "PUB");
        assert_eq!(body["key_type"], "curve25519");
        assert_eq!(body["tos"], "2026-10-01T10:00:00.000Z");
        assert_eq!(body["serial_number"], "0011223344556677");
    }

    /// Молчание API — не отказ Cloudflare: напрямую подсказываем VPN, через VPN — узел.
    #[test]
    fn silence_says_what_helps() {
        let silent = || AppError::network("Cloudflare не ответил: timed out");
        let direct = unreachable(silent(), None).to_string();
        assert!(direct.contains("Подключитесь"), "{direct}");
        let proxied = unreachable(silent(), Some(2080)).to_string();
        assert!(proxied.contains("через прокси"), "{proxied}");
        let refused = unreachable(AppError::network("Cloudflare отказал: 400"), None);
        assert!(
            refused.to_string().contains("отказал"),
            "отказ остаётся отказом"
        );
    }

    #[test]
    fn a_refusal_carries_cloudflare_words() {
        assert!(refusal(429, "").contains("подождать"));
        let said = refusal(400, r#"{"errors":[{"code":1,"message":"Invalid key"}]}"#);
        assert!(
            said.contains("Invalid key") && said.contains("400"),
            "{said}"
        );
        assert_eq!(refusal(500, "<html>"), "Cloudflare отказал: 500");
    }
}
