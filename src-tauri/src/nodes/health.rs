//! Куда бьём, проверяя живость узла, — одно место на весь клиент (D-108).
//!
//! Было два адреса: ядро проверяло провайдеров по `www.gstatic.com`, а клиент мерил
//! задержку через `cp.cloudflare.com`. Один и тот же узел получал от них два вердикта,
//! и объяснить расхождение было нечем. Хуже другое: gstatic в России шейпится, и живой
//! узел **выпадал из `AUTO`** — без единой строки в логе, потому что для ядра это
//! обычная неудачная проверка.
//!
//! Отсюда и `expected-status`: без него ответ заглушки провайдера или страницы
//! captive portal — тоже ответ, и мёртвый выход считается живым. Ждём ровно 204.
//!
//! Адрес лежит в `client.yaml` (D-068) и берётся оттуда обоими: ядру он уезжает полем
//! группы при сборке конфига, клиенту — хостом и путём для запроса в туннеле. Ключ
//! читает этот модуль, а не форма: правило «у поля один хозяин» (D-052).

use crate::config::files::Documents;
use crate::config::files::CLIENT;
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

const KEY: &str = "health-url";
const EVERY: &str = "health-interval";

/// Как часто группы перепроверяют узлы, секунд, — умолчание и границы. Чаще минуты — трафик
/// и нагрузка на узлы ради ничего; реже суток — мёртвый узел держится в группе весь день.
pub const INTERVAL: u32 = 300;
const SHORTEST: u32 = 60;
const LONGEST: u32 = 86_400;

/// Проверка живости так, как её получает сборка: куда бить и как часто.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub url: String,
    pub interval: u32,
}

impl Default for Check {
    fn default() -> Self {
        Check {
            url: DEFAULT.to_string(),
            interval: INTERVAL,
        }
    }
}

/// Умолчание. Cloudflare, а не gstatic: последний в России шейпится, а этот адрес
/// к тому же уже замерен как цель замера через туннель (S-016).
pub const DEFAULT: &str = "http://cp.cloudflare.com/generate_204";

/// Что считаем ответом живого. Ровно 204 и ничего больше: любой другой код — это
/// кто-то по дороге, а не наш узел.
pub const EXPECTED: u16 = 204;

pub struct HealthCheck;

impl HealthCheck {
    /// Адрес проверки живости. Файл правится руками, поэтому это **граница с недоверенными
    /// данными**: непонятное значение — ошибка с внятным текстом, а не молчаливое умолчание.
    pub fn url() -> Result<String> {
        let map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        let Some(value) = map.get(serde_yaml::Value::from(KEY)) else {
            return Ok(DEFAULT.to_string());
        };
        let text = value.as_str().unwrap_or_default().trim();
        if HealthCheck::split(text).is_none() {
            return Err(AppError::invalid(format!(
                "В client.yaml непонятное значение {KEY}: {text}. Ожидается адрес вида {DEFAULT}."
            )));
        }
        Ok(text.to_string())
    }

    pub fn set_url(url: &str) -> Result<()> {
        let url = url.trim();
        if HealthCheck::split(url).is_none() {
            return Err(AppError::invalid(format!(
                "Адрес проверки живости должен выглядеть как {DEFAULT}"
            )));
        }
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        Yaml::set(&mut map, KEY, serde_yaml::Value::from(url));
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(CLIENT, &text)
    }

    pub fn check() -> Result<Check> {
        Ok(Check {
            url: HealthCheck::url()?,
            interval: HealthCheck::interval(),
        })
    }

    /// Частота перепроверки групп. Число вне границ или мусор — умолчание: из-за поля,
    /// поправленного руками, VPN не должен перестать подниматься.
    pub fn interval() -> u32 {
        Documents::read(CLIENT)
            .ok()
            .and_then(|text| Yaml::top_mapping(&text).ok())
            .and_then(|map| {
                map.get(serde_yaml::Value::from(EVERY))
                    .and_then(serde_yaml::Value::as_u64)
            })
            .and_then(|seconds| u32::try_from(seconds).ok())
            .filter(|seconds| (SHORTEST..=LONGEST).contains(seconds))
            .unwrap_or(INTERVAL)
    }

    pub fn set_interval(seconds: u32) -> Result<()> {
        if !(SHORTEST..=LONGEST).contains(&seconds) {
            return Err(AppError::invalid(format!(
                "Перепроверка групп — от {SHORTEST} до {LONGEST} секунд"
            )));
        }
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        Yaml::set(&mut map, EVERY, serde_yaml::Value::from(seconds));
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(CLIENT, &text)
    }

    /// Хост и путь — то, из чего клиент собирает свой запрос в туннеле.
    ///
    /// Только `http://`, и это не упущение: замер идёт через `CONNECT` на 80-й порт, а ядро
    /// проверяет провайдеров тем же дешёвым запросом без TLS. Адрес с `https://` молча
    /// не заработал бы ни там, ни там — поэтому он отвергается на границе.
    pub fn split(url: &str) -> Option<(&str, &str)> {
        let rest = url.strip_prefix("http://")?;
        let cut = rest.find('/')?;
        let (host, path) = rest.split_at(cut);
        (!host.is_empty() && !host.contains(':') && path.len() > 1).then_some((host, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_splits_into_a_host_and_a_path() {
        assert_eq!(
            HealthCheck::split(DEFAULT),
            Some(("cp.cloudflare.com", "/generate_204"))
        );
        assert_eq!(
            HealthCheck::split("http://www.gstatic.com/generate_204"),
            Some(("www.gstatic.com", "/generate_204"))
        );
    }

    /// Шаблон — это документация: адрес в нём обязан разбираться тем же кодом,
    /// и совпадать с умолчанием, иначе чистая установка и пустой файл разошлись бы.
    #[test]
    fn the_template_says_the_target_the_client_uses() {
        let map = Yaml::top_mapping(Documents::template(CLIENT).unwrap()).unwrap();
        let said = map.get(serde_yaml::Value::from(KEY)).unwrap();
        assert_eq!(said.as_str(), Some(DEFAULT));
    }

    /// Шаблон называет ту же частоту, что берёт клиент без поля.
    #[test]
    fn the_template_says_the_interval_the_client_uses() {
        let map = Yaml::top_mapping(Documents::template(CLIENT).unwrap()).unwrap();
        let said = map.get(serde_yaml::Value::from(EVERY)).unwrap();
        assert_eq!(said.as_u64(), Some(u64::from(INTERVAL)));
    }

    /// Всё, что не доедет до ядра или до замера, отвергаем здесь, а не молча.
    #[test]
    fn anything_the_probe_cannot_use_is_refused() {
        assert_eq!(
            HealthCheck::split("https://cp.cloudflare.com/generate_204"),
            None
        );
        assert_eq!(HealthCheck::split("http://cp.cloudflare.com"), None);
        assert_eq!(HealthCheck::split("http://cp.cloudflare.com/"), None);
        assert_eq!(HealthCheck::split("http://:80/generate_204"), None);
        assert_eq!(HealthCheck::split("cp.cloudflare.com/generate_204"), None);
        assert_eq!(HealthCheck::split(""), None);
    }
}
