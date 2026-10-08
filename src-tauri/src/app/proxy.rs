//! Системный прокси как обвязка клиента (D-047, D-155): прописать адрес работающего ядра
//! в Windows, запомнив, что стояло там до нас, и вернуть как было.
//!
//! Реестр трогает `system/sysproxy.rs`; здесь — правило «наше или чужое» и память о том,
//! что было до нас. Память лежит в настройках: она обязана пережить падение клиента.

use crate::app::engine::{Capture, Engine};
use crate::app::state::AppState;
use crate::error::Result;
use crate::system::sysproxy::Backup;
use crate::system::sysproxy::ProxySetting;

pub struct SystemProxy;

impl SystemProxy {
    /// Адрес, который прописываем в систему. Только у локального прокси: в TUN перехватывается
    /// весь трафик машины, у qd — приложения, и прописывать там нечего (D-154).
    pub fn address(capture: Option<&Capture>) -> Option<String> {
        match capture {
            Some(Capture::LocalProxy { port }) => Some(format!("127.0.0.1:{port}")),
            _ => None,
        }
    }

    /// Прописать адрес ядра в систему, запомнив прежнюю настройку.
    ///
    /// Неудача не роняет запуск: ядро уже работает, адрес виден в окне, и прописать его руками
    /// пользователь может сам. Правду про реестр всё равно скажет `Status::system_proxy`.
    pub fn engage(&self, state: &AppState, engine: &dyn Engine) -> Result<()> {
        let Some(address) = Self::address(engine.state().capture.as_ref()) else {
            return Ok(());
        };
        if let Some(mut backup) = state.settings.get().proxy_backup {
            // Кто-то сменил прокси после нас — не отбираем его обратно при reconnect.
            if !backup.ours.is_empty() && !ProxySetting::is_ours(&backup.ours) {
                return Ok(());
            }
            if backup.ours == address && ProxySetting::is_ours(&address) {
                return Ok(());
            }
            let old = backup.ours.clone();
            ProxySetting::apply(&address)?;
            backup.ours = address;
            if let Err(why) = remember(state, Some(backup)) {
                if !old.is_empty() {
                    let _ = ProxySetting::apply(&old);
                }
                return Err(why);
            }
            return Ok(());
        }
        let mut previous = ProxySetting::read()?;
        previous.ours = address.clone();
        // Снимок должен пережить падение между записью реестра и следующей строкой.
        remember(state, Some(previous))?;
        if let Err(why) = ProxySetting::apply(&address) {
            let _ = remember(state, None);
            return Err(why);
        }
        Ok(())
    }

    /// Снять наш прокси и вернуть то, что стояло раньше. Зовётся и при остановке ядра,
    /// и при выходе, и при старте — последнее и есть лекарство от залипания после падения.
    pub fn release(&self, state: &AppState) -> Result<()> {
        // Снимка нет — значит включали не мы, и в реестре чужая настройка: другой VPN,
        // корпоративный прокси. Выключить её «на всякий случай» значит сломать человеку сеть
        // тем, что он всего лишь закрыл наше окно.
        let Some(backup) = state.settings.get().proxy_backup else {
            return Ok(());
        };
        ProxySetting::restore(&backup)?;
        remember(state, None)
    }
}

/// Запомнить, что стояло в настройках Windows до нас (D-047). `None` означает «мы уже
/// вернули как было» — и именно по наличию снимка при старте видно, что прошлый запуск
/// не успел прибраться.
fn remember(state: &AppState, backup: Option<Backup>) -> Result<()> {
    state
        .settings
        .update(|settings| settings.proxy_backup = backup)?;
    crate::app::leftovers::Leftovers::mark(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_local_proxy_is_written_into_windows() {
        assert_eq!(
            SystemProxy::address(Some(&Capture::LocalProxy { port: 2080 })).as_deref(),
            Some("127.0.0.1:2080")
        );
        assert_eq!(SystemProxy::address(Some(&Capture::Divert)), None);
        assert_eq!(SystemProxy::address(None), None);
    }
}
