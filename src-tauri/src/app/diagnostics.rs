//! Инструменты как действие человека (D-097, D-155): прогнать утилиту на том, что сейчас
//! работает, и довести до ядра то, что она поправила.
//!
//! Сами утилиты — `diag`; они не знают ни настроек, ни работающего ядра. Здесь им
//! подставляют контекст: применённые правила, порт и режим живого mihomo.

use tauri::AppHandle;

use crate::app::state::AppState;
use crate::diag::Toolbox;
use crate::diag::{self, Args, Report};
use crate::error::Result;

pub struct Diagnostics;

impl Diagnostics {
    pub async fn run(&self, state: &AppState, id: &str, args: Option<Args>) -> Result<Report> {
        Toolbox::run(id, filled(state, args)?).await
    }

    /// Замер плюс одно действие (D-105): утилита правит документ, правка доезжает до ядра.
    pub async fn apply(
        &self,
        app: &AppHandle,
        state: &AppState,
        id: &str,
        args: Option<Args>,
    ) -> Result<Report> {
        let report = diag::smart::Smart::apply(id, filled(state, args)?).await?;
        state.connection.apply(app, state).await?;
        Ok(report)
    }
}

/// Маршрутизация подставляется здесь, а не приходит из окна: какой набор применён —
/// дело состояния приложения, и спрашивать об этом вебвью значило бы завести второй
/// источник истины (D-071). Порт работающего ядра — оттуда же: окно знает его только
/// как число в статусе, а пробам он нужен как адрес прокси.
fn filled(state: &AppState, args: Option<Args>) -> Result<Args> {
    let mut args = args.unwrap_or_default();
    args.rules = state.routing.rules(state)?;
    let status = state.mihomo.status();
    args.proxy = status.port;
    args.mode = status.mode.map(|mode| format!("{mode:?}").to_lowercase());
    Ok(args)
}
