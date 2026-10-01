//! Подбор настроек как действие человека (D-105, D-155): замер, запись в документ
//! и доставка правки до живого ядра.

use tauri::AppHandle;

use crate::app::state::AppState;
use crate::diag::{self, Report};
use crate::error::Result;

pub struct Diagnostics;

impl Diagnostics {
    pub async fn apply(
        &self,
        app: &AppHandle,
        state: &AppState,
        id: &str,
        filter: diag::dns::DnsFilter,
    ) -> Result<Report> {
        let report = diag::smart::Smart::apply(id, filter).await?;
        state.connection.apply(app, state).await?;
        Ok(report)
    }
}
