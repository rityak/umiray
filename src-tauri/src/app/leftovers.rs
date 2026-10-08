//! Что клиент оставил в системе — прокси и запрет — и как это вернуть, если клиента
//! больше нет (D-175).
//!
//! Обычно возвращает сам клиент: при выходе и при следующем запуске (`boot`). Но выход
//! из сеанса на Linux убивает его вместе с дисплеем, без `RunEvent::Exit` (B-053), и без
//! автозапуска следующий запуск может не наступить вовсе: браузер без интернета, а с kill
//! switch — машина без сети. Поэтому пока снимок есть, в автозапуске лежит запись
//! `--restore`: вход в систему возвращает сеть и запись убирает.

use crate::app::state::AppState;
use crate::system::autostart::{Autostart, RESTORE};

pub struct Leftovers;

impl Leftovers {
    /// Запись при входе есть ровно тогда, когда есть что возвращать. Зовётся после каждой
    /// записи снимка прокси или запрета.
    pub fn mark(state: &AppState) {
        let settings = state.settings.get();
        let held = settings.proxy_backup.is_some() || settings.kill_switch_backup.is_some();
        if let Err(why) = Autostart::set_restore(held) {
            eprintln!("запись возврата сети при входе: {why}");
        }
    }

    pub fn asked() -> bool {
        std::env::args().any(|arg| arg == RESTORE)
    }

    /// Вход в систему после смерти клиента: вернуть прокси и запрет и выйти без окна.
    /// С автозапуском клиент поднимется сам и вернёт то же шагами запуска — второй
    /// процесс над теми же снимками только спорил бы с ним.
    pub fn restore() {
        if Autostart::enabled() {
            return;
        }
        let state = AppState::new();
        if let Err(why) = state.proxy.release(&state) {
            eprintln!("возврат системного прокси: {why}");
        }
        if let Err(why) = state.kill_switch.release(&state) {
            eprintln!("снятие запрета: {why}");
        }
        // Снимков могло не быть вовсе (клиент успел вернуть сам) — запись тогда лишняя.
        Self::mark(&state);
    }
}
