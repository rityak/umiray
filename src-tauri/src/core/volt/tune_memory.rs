//! Память подбора стратегии (D-185): что выбрала проверка — для этой стратегии, этих адресов
//! проверки, этого бинарника и **этой сети**. Переживает перезапуск клиента, чтобы
//! подключение не ждало проверок каждый раз; в другой сети — свой результат.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::volt::Options;
use crate::core::volt::Uplink;
use crate::core::volt_tune::TuneReport;
use crate::db::{Db, Table};

/// Сколько результат считается свежим: дольше — проверка повторится при следующем
/// запуске Relay. Кнопка «Проверить» не ждёт.
const FRESH: u64 = 24 * 60 * 60;
const ROW: &str = "volt-tune";

#[derive(Clone, Serialize, Deserialize)]
pub struct Remembered {
    pub key: String,
    /// Секунды эпохи.
    pub checked: u64,
    pub report: TuneReport,
    /// Стратегия-победитель для Relay; `None` — Relay идёт со своей.
    pub yaml: Option<String>,
}

#[derive(Default)]
pub struct TuneMemory {
    last: Mutex<Option<Remembered>>,
}

impl TuneMemory {
    /// Что определяет ответ проверки. Порты, режим и включение — нет: от них стратегия
    /// не работает лучше или хуже.
    pub fn key(options: &Options, uplink: &Uplink) -> String {
        let binary = super::Volt::directory().join("volt-relay.exe");
        let metadata = std::fs::metadata(&binary).ok();
        // Адреса — те, что проверяются на деле (D-189): выбор сервисов меняет ответ.
        let targets = crate::collections::Collections::volt()
            .map(|catalog| options.probe_targets(&catalog))
            .unwrap_or_else(|_| options.probe_urls.clone());
        format!(
            "{}\n{:?}\n{:?}\n{}",
            serde_json::to_string(&(&options.relay_yaml, &targets)).unwrap_or_default(),
            metadata.as_ref().map(std::fs::Metadata::len),
            metadata.and_then(|metadata| metadata.modified().ok()),
            uplink.network
        )
    }

    /// Результат для ключа, моложе суток; из памяти, а нет — из базы.
    pub fn fresh(&self, key: &str) -> Option<Remembered> {
        let now = crate::stamp::Stamp::now()?;
        self.find(key)
            .filter(|remembered| now.saturating_sub(remembered.checked) < FRESH)
    }

    /// Результат для ключа любого возраста: его стратегией Relay и идёт, пока проверка
    /// не повторилась.
    pub fn find(&self, key: &str) -> Option<Remembered> {
        let mut last = self.last.lock().unwrap();
        if let Some(remembered) = last.as_ref().filter(|remembered| remembered.key == key) {
            return Some(remembered.clone());
        }
        let stored: Remembered = Db::get(Table::State, ROW, &part(key))
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str(&text).ok())?;
        (stored.key == key).then(|| {
            *last = Some(stored.clone());
            stored
        })
    }

    pub fn remember(&self, remembered: Remembered) {
        // Не записалось в базу — помним в памяти: следующий запуск просто проверит заново.
        if let Ok(text) = serde_json::to_string(&remembered) {
            let _ = Db::put(Table::State, ROW, &part(&remembered.key), &text);
        }
        *self.last.lock().unwrap() = Some(remembered);
    }
}

/// Ключ длинный — в часть строки базы идёт его отпечаток.
fn part(key: &str) -> String {
    crate::core::release::sha256(key.as_bytes())[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> TuneReport {
        TuneReport {
            checked_at: String::new(),
            scope: "direct-https".into(),
            urls: Vec::new(),
            selected: Some("tls-auto".into()),
            unblocked: false,
            candidates: Vec::new(),
        }
    }

    /// Другая сеть — другой ответ; перезапуск клиента ответ не теряет.
    #[test]
    fn the_answer_belongs_to_the_network_and_outlives_the_client() {
        let _sandbox = crate::paths::Sandbox::new("volt-tune-memory");
        let options = Options::default();
        let home = Uplink {
            network: "Ethernet|192.168.1.1".into(),
            ..Uplink::default()
        };
        let cafe = Uplink {
            network: "Wi-Fi|10.0.0.1".into(),
            ..Uplink::default()
        };
        let key = TuneMemory::key(&options, &home);
        assert_ne!(key, TuneMemory::key(&options, &cafe));
        let tuned = Options {
            auto_select: true,
            relay_port: 9999,
            ..options.clone()
        };
        assert_eq!(
            key,
            TuneMemory::key(&tuned, &home),
            "порты и подбор ответ не меняют"
        );
        TuneMemory::default().remember(Remembered {
            key: key.clone(),
            checked: crate::stamp::Stamp::now().unwrap(),
            report: report(),
            yaml: Some("winner".into()),
        });
        let restarted = TuneMemory::default();
        assert_eq!(
            restarted.fresh(&key).unwrap().yaml.as_deref(),
            Some("winner")
        );
        assert!(restarted.fresh(&TuneMemory::key(&options, &cafe)).is_none());
        TuneMemory::default().remember(Remembered {
            key: key.clone(),
            checked: 0,
            report: report(),
            yaml: Some("old".into()),
        });
        let later = TuneMemory::default();
        assert!(
            later.fresh(&key).is_none(),
            "сутки прошли — проверить заново"
        );
        assert_eq!(later.find(&key).unwrap().yaml.as_deref(), Some("old"));
    }
}
