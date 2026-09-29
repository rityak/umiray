//! Kill switch как обвязка клиента (D-073, D-155): запереть выход мимо туннеля, запомнив,
//! что стояло в брандмауэре до нас, и вернуть как было.
//!
//! Правила брандмауэра пишет `system/killswitch.rs`; здесь — когда запирать и память
//! о прежнем. Память лежит в настройках: её наличие и означает «запрет наш», в том числе
//! после падения клиента.

use tauri::AppHandle;

use crate::app::engine::{Capture, Engine};
use crate::app::settings::Patch;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::core::EngineId;
use crate::error::Result;
use crate::system::killswitch::Backup;
use crate::system::killswitch::Firewall;

pub struct KillSwitch;

impl KillSwitch {
    /// Запереть выход (D-073). Только в TUN: в local и system ядро — обычный прокси, мимо
    /// которого приложение вправе ходить, и запирать машину за него мы не подряжались.
    /// Выпускаем бинарь того ядра, что держит адаптер, и через тот адаптер, что оно подняло.
    ///
    /// Молчаливый отказ намеренный: не встало правило — VPN всё равно работает, просто без
    /// подстраховки, и ронять из-за этого подключение хуже. Что защиты нет, видно в статусе.
    pub fn engage(&self, state: &AppState, engine: &dyn Engine) -> Result<()> {
        let Some(Capture::Tun { device }) = engine.state().capture else {
            return Ok(());
        };
        let settings = state.settings.get();
        if !settings.kill_switch {
            return Ok(());
        }
        // При reconnect правила уже стоят, а снимок содержит состояние до первой установки.
        // Повторный `engage` снял бы снимок с нашего же Block и породил дубликаты правил.
        if settings.kill_switch_backup.is_some() {
            return Ok(());
        }
        let backup = Backup {
            profiles: Firewall::profiles()?,
        };
        // Сначала сохраняем исходное состояние, затем меняем машину: падение между ними
        // оставит данные, по которым следующий запуск всё вернёт.
        remember(state, Some(backup.clone()))?;
        if let Err(why) = Firewall::apply(&engine.binary(), &device) {
            let _ = Firewall::release(&backup);
            let _ = remember(state, None);
            return Err(why);
        }
        Ok(())
    }

    /// Снять запрет и вернуть умолчание брандмауэра. Как и у прокси, зовётся при остановке,
    /// при выходе и **при старте** — последнее возвращает машине сеть после падения клиента.
    pub fn release(&self, state: &AppState) -> Result<()> {
        // Снимка нет — запрет не наш (или его нет вовсе), и трогать чужую настройку
        // брандмауэра мы не вправе.
        let Some(backup) = state.settings.get().kill_switch_backup else {
            return Ok(());
        };
        Firewall::release(&backup)?;
        remember(state, None)
    }

    /// Тумблер (D-073): меняет и настройку, и брандмауэр — сработать обязан сейчас, а не при
    /// следующем подключении. Под замком перехода: иначе параллельный запуск успел бы
    /// поставить свой запрет поверх.
    ///
    /// Выключение снимает запрет **всегда**, не спрашивая, работает ли ядро: если правило
    /// осталось от прошлой жизни клиента, тумблер — самый естественный способ его убрать.
    pub async fn set(&self, app: &AppHandle, state: &AppState, on: bool) -> Result<Status> {
        let _transition = state.connection.lock().await;
        state.settings.patch(Patch {
            kill_switch: Some(on),
            ..Default::default()
        })?;
        if on {
            for id in EngineId::ALL {
                self.engage(state, state.engine(id))?;
            }
        } else {
            self.release(state)?;
        }
        Ok(state.connection.shown(app, state))
    }
}

/// То же для брандмауэра, что и у прокси: наличие снимка и означает «запрет наш» (D-073).
fn remember(state: &AppState, backup: Option<Backup>) -> Result<()> {
    state
        .settings
        .update(|settings| settings.kill_switch_backup = backup)
}
