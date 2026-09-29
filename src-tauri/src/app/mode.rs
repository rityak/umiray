//! Режим перехвата так, как его выбирает человек (D-060).
//!
//! В конфиге ядра лежит половина этого выбора — `tun.enable` (D-052). Вторая половина,
//! `System`, полем конфига не является вовсе: это запись нашего адреса в настройки Windows,
//! и владелец у неё `settings.json` (D-047). Собрать обе половины в одно понятие можно
//! только здесь, где видно и файл, и настройки.
//!
//! Здесь только запись выбора; до живого ядра его доводит `Connection::set_mode` (D-143).

use serde::{Deserialize, Serialize};

use crate::app::settings;
use crate::app::state::AppState;
use crate::config::mode::Mode;
use crate::error::Result;

/// Что стоит в шапке. `Off` здесь нет: выключение — это кнопка питания, а не режим (D-060).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Работает без прав администратора, поэтому и по умолчанию (D-023).
    #[default]
    Local,
    /// Тот же локальный прокси, просто прописанный в настройки Windows (D-047).
    System,
    Tun,
}

impl Choice {
    /// Каким этот выбор виден ядру. `System` для него неотличим от `Local` — разница
    /// живёт в реестре Windows, а не в конфиге.
    pub fn core(self) -> Mode {
        match self {
            Choice::Tun => Mode::Tun,
            _ => Mode::Local,
        }
    }
}

impl Choice {
    /// Что выбрано сейчас. Читается, а не помнится: конфиг правит и редактор тоже (D-052).
    pub fn get(state: &AppState) -> Choice {
        if Mode::current() == Mode::Tun {
            return Choice::Tun;
        }
        if state.settings.get().system_proxy {
            Choice::System
        } else {
            Choice::Local
        }
    }

    /// Записать выбор. Обе половины сразу и в этом порядке: сорвётся запись конфига —
    /// не изменится ничего, а обратный порядок оставил бы намерение без файла.
    pub fn set(state: &AppState, choice: Choice) -> Result<()> {
        Mode::write(choice.core())?;
        state.settings.patch(settings::Patch {
            system_proxy: Some(choice == Choice::System),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ядру видна только половина выбора, и это ровно то, почему перезапуск нужен
    /// не всегда: `System` от `Local` отличается записью в реестре, а не конфигом.
    /// Считает разницу теперь `core::apply` — по самим конфигам, а не по этому выбору.
    #[test]
    fn the_core_sees_only_the_tun_half_of_the_choice() {
        assert_eq!(Choice::Tun.core(), Mode::Tun);
        assert_eq!(Choice::Local.core(), Mode::Local);
        assert_eq!(Choice::System.core(), Mode::Local);
    }
}
