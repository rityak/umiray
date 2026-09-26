//! Команды про настройки самого клиента (D-068).
//!
//! Домен `client`, а не `config`: тот про конфиг ядра, а здесь то, чего ядро не видит.

use tauri::State;

use crate::app::client;
use crate::app::state::AppState;
use crate::error::Result;
use crate::nodes::ping::Method;

/// Чем мерить, сколько до сервера (D-069).
#[tauri::command]
pub fn client_ping_get() -> Result<Method> {
    client::ping()
}

/// Куда бьёт проверка живости — одна цель на ядро и на замер клиента (D-108).
#[tauri::command]
pub fn client_health_get() -> Result<String> {
    crate::nodes::health::url()
}

/// Записать цель. Доезжает до ядра пересборкой конфига, как и всё остальное из формы.
#[tauri::command]
pub fn client_health_set(url: String) -> Result<()> {
    crate::nodes::health::set_url(&url)
}

/// Через сколько часов перепрашивать страну узла; 0 — не спрашивать вовсе (D-084).
#[tauri::command]
pub fn client_geo_get() -> u64 {
    crate::nodes::geo::hours()
}

/// Записать срок и сразу же спросить о том, чего ещё не знаем: включили — флаги должны
/// появиться, а не ждать фонового такта.
#[tauri::command]
pub async fn client_geo_set(hours: u64) -> Result<()> {
    crate::nodes::geo::set_hours(hours)?;
    tauri::async_runtime::spawn(async {
        let _ = crate::nodes::geo::refresh().await;
    });
    Ok(())
}

/// Форма пишет в тот же документ, что и редактор (D-052): своего значения у неё нет.
/// Незнакомый способ отвергается на границе — `Method` разбирается serde, а не строкой.
///
/// Смена способа заодно обнуляет прошлые замеры — почему, сказано в `state::set_ping`.
#[tauri::command]
pub fn client_ping_set(method: Method, state: State<AppState>) -> Result<()> {
    state.set_ping(method)
}

/// Чем маскировать рукопожатие WireGuard (D-118). Одна настройка на все узлы: она про
/// путь, а путь у них общий.
#[tauri::command]
pub fn client_mask_get() -> crate::config::awg::Mask {
    crate::config::awg::get()
}

/// Записать маску и отдать записанное. Сочетания, при которых туннель не встанет,
/// отвергаются здесь же — с текстом, а не молча (D-118).
#[tauri::command]
pub fn client_mask_set(mask: crate::config::awg::Mask) -> Result<crate::config::awg::Mask> {
    crate::config::awg::set_mask(mask)
}
