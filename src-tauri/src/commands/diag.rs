//! Подбор настроек мастером (D-030, D-105, D-162).

use tauri::State;

use crate::app::state::AppState;
use crate::diag::dns::DnsFilter;
use crate::diag::Report;
use crate::error::Result;

/// Подобрать и записать одно значение в «Настройки mihomo»: `dns-race`, `pmtu`.
/// `dns_filter` — из каких резолверов выбирать DNS, без него — чистые. Запись доезжает
/// до живого ядра.
#[tauri::command]
pub async fn diag_apply(
    id: String,
    dns_filter: Option<DnsFilter>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Report> {
    state
        .diagnostics
        .apply(&app, &state, &id, dns_filter.unwrap_or_default())
        .await
}
