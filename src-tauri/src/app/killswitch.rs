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
use crate::error::Result;
use crate::system::killswitch::{Allowed, Backup, Firewall};

pub struct KillSwitch;

impl KillSwitch {
    /// Привести запрет к ядру, которое сейчас держит трафик (D-073). Зовётся, когда ядро
    /// поднялось, и тумблером.
    ///
    /// Только в TUN: в local и system ядро — обычный прокси, мимо которого приложение вправе
    /// ходить. Поэтому запрет, оставшийся от TUN после смены режима, снимается: перезапуск
    /// его не трогает, и без этого в Proxy всё мимо прокси стояло без сети (B-042). Выпускаем
    /// бинарь того ядра, что держит адаптер, и через тот адаптер, что оно подняло; сменился
    /// адаптер — меняем разрешения, не открывая машину.
    ///
    /// Молчаливый отказ намеренный: не встало правило — VPN всё равно работает, просто без
    /// подстраховки, и ронять из-за этого подключение хуже. Что защиты нет, видно в статусе.
    pub fn follow(&self, state: &AppState, engine: &dyn Engine) -> Result<()> {
        let tun = match engine.state().capture {
            Some(Capture::Tun { device }) => Some(Allowed {
                core: engine.binary().display().to_string(),
                device,
            }),
            _ => None,
        };
        let settings = state.settings.get();
        let held = settings.kill_switch_backup;
        match step(tun.as_ref(), settings.kill_switch, held.as_ref()) {
            Step::Keep => Ok(()),
            Step::Release => self.release(state),
            Step::Engage(allowed) => {
                let backup = Backup {
                    profiles: Firewall::profiles()?,
                    allowed: Some(allowed.clone()),
                };
                // Сначала сохраняем исходное состояние, затем меняем машину: падение между
                // ними оставит данные, по которым следующий запуск всё вернёт.
                remember(state, Some(backup.clone()))?;
                if let Err(why) = Firewall::apply(&allowed) {
                    let _ = Firewall::release(&backup);
                    let _ = remember(state, None);
                    return Err(why);
                }
                Ok(())
            }
            Step::Renew(allowed) => {
                // Снимок прежний: в нём то, что стояло до нас, а не наш же запрет (D-134).
                Firewall::renew(&allowed)?;
                remember(
                    state,
                    held.map(|backup| Backup {
                        allowed: Some(allowed),
                        ..backup
                    }),
                )
            }
        }
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
    /// Выключение снимает запрет, не спрашивая, работает ли ядро: если правило осталось
    /// от прошлой жизни клиента, тумблер — самый естественный способ его убрать.
    pub async fn set(&self, app: &AppHandle, state: &AppState, on: bool) -> Result<Status> {
        let _transition = state.connection.lock().await;
        state.settings.patch(Patch {
            kill_switch: Some(on),
            ..Default::default()
        })?;
        match state.running() {
            Some((id, _)) if on => self.follow(state, state.engine(id))?,
            _ => self.release(state)?,
        }
        Ok(state.connection.shown(app, state))
    }
}

/// Что сделать с запретом.
#[derive(Debug, PartialEq)]
enum Step {
    Keep,
    Engage(Allowed),
    /// Запрет наш, но выпускает не то ядро или не тот адаптер.
    Renew(Allowed),
    Release,
}

/// `tun` — кого выпускать, если трафик держит адаптер; `wanted` — тумблер; `held` — снимок,
/// то есть наш запрет уже стоит.
fn step(tun: Option<&Allowed>, wanted: bool, held: Option<&Backup>) -> Step {
    match (tun, held) {
        (Some(allowed), None) if wanted => Step::Engage(allowed.clone()),
        (Some(allowed), Some(backup)) if wanted => match backup.allowed.as_ref() {
            Some(was) if was == allowed => Step::Keep,
            _ => Step::Renew(allowed.clone()),
        },
        (_, Some(_)) => Step::Release,
        (_, None) => Step::Keep,
    }
}

/// То же для брандмауэра, что и у прокси: наличие снимка и означает «запрет наш» (D-073).
fn remember(state: &AppState, backup: Option<Backup>) -> Result<()> {
    state
        .settings
        .update(|settings| settings.kill_switch_backup = backup)?;
    crate::app::leftovers::Leftovers::mark(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(device: &str) -> Allowed {
        Allowed {
            core: r"C:\umiray\mihomo.exe".into(),
            device: device.into(),
        }
    }

    fn held(device: Option<&str>) -> Backup {
        Backup {
            profiles: Vec::new(),
            allowed: device.map(allowed),
        }
    }

    #[test]
    fn the_lock_follows_the_capture() {
        let tun = allowed("Meta");
        assert_eq!(step(Some(&tun), true, None), Step::Engage(tun.clone()));
        assert_eq!(
            step(Some(&tun), true, Some(&held(Some("Meta")))),
            Step::Keep,
            "переподключение: правила уже те, снимок не переснимаем (D-134)"
        );
        assert_eq!(
            step(Some(&tun), false, None),
            Step::Keep,
            "тумблер выключен"
        );
        assert_eq!(
            step(None, true, None),
            Step::Keep,
            "в Proxy запирать нечего"
        );
    }

    /// B-042: TUN сменили на Proxy при включённом запрете — перезапуск ядра его не трогал,
    /// и всё мимо прокси оставалось без сети.
    #[test]
    fn a_lock_left_from_tun_is_lifted_in_proxy() {
        assert_eq!(step(None, true, Some(&held(Some("Meta")))), Step::Release);
    }

    /// B-042: адаптер переименовали в «Настройках» — разрешение выпускало прежний, и при
    /// живом туннеле машина стояла без сети.
    #[test]
    fn a_new_adapter_renews_the_allowances() {
        let tun = allowed("umiray");
        assert_eq!(
            step(Some(&tun), true, Some(&held(Some("Meta")))),
            Step::Renew(tun.clone())
        );
        assert_eq!(
            step(Some(&tun), true, Some(&held(None))),
            Step::Renew(tun.clone()),
            "снимок прошлой сборки не знает, кого выпускал, — обновить"
        );
    }
}
