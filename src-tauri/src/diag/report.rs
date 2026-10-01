//! Итог «умной» функции (D-105) — то, что мастер показывает после «Готово» (D-162).

use serde::Serialize;

/// Чем кончилось. Те же состояния, что у всего окна (STYLEGUIDE), плюс «не гонялось» —
/// мерить было нечего или нельзя.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Подобрали и записали.
    Ok,
    /// Записали, но с оговоркой.
    Warn,
    /// Не вышло.
    Bad,
    /// Не гонялось: нет прав, занят адаптер, нечего мерить.
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Что подбирали: `dns-race`, `pmtu`, `tun-stack`.
    pub tool: String,
    pub verdict: Verdict,
    /// Одна строка для человека: что записано, а не «ок».
    pub headline: String,
}

impl Report {
    pub fn new(tool: &str, verdict: Verdict, headline: impl Into<String>) -> Self {
        Self {
            tool: tool.to_string(),
            verdict,
            headline: headline.into(),
        }
    }
}
