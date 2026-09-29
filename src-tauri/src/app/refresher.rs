//! Обновление подписок по расписанию: что считать устаревшим и когда.
//!
//! Когда именно спрашивать — не здесь: часы завела фаза `tick` (D-101), а этот модуль
//! отвечает на её вопрос «не пора ли» и обновляет то, чему пора.

use crate::app::settings::Refresh;
use crate::app::state::AppState;
use crate::error::{AppError, Result};
use crate::nodes::source_import::SourceImporter;
use crate::nodes::sources::SourceStore;

pub struct Refresher;

impl Refresher {
    /// Заход фазы `tick`: обновить то, чему подошёл срок.
    ///
    /// Отказ отдаём одной строкой, а не окном: провайдер мог просто не ответить, узлы остались
    /// прежними, и всплывающее окно посреди работы раздражало бы сильнее, чем помогало. Что
    /// источник не обновился, видно по дате в его карточке — и по строке в логе.
    pub async fn due(state: &AppState, first: bool) -> Result<()> {
        let Some(period) = period(state.settings.get().refresh, first) else {
            return Ok(());
        };
        let failures = Refresher::refresh_all(state, Some(period)).await;
        if failures.is_empty() {
            return Ok(());
        }
        Err(AppError::network(failures.join("; ")))
    }

    /// Обновить подписки и дать живому ядру перечитать их на месте (S-012).
    ///
    /// `period` — сколько минут источник считается свежим; `None` значит «не глядя на срок»,
    /// и это ровно то, чего ждут от нажатой кнопки: человек нажал именно потому, что
    /// расписание его не устраивает.
    ///
    /// Возвращает строки о том, что **не** получилось. Отказ одного источника не отменяет
    /// остальных: подписок бывает несколько, и упавшая одна — не повод бросить прочие.
    pub async fn refresh_all(state: &AppState, period: Option<u64>) -> Vec<String> {
        let mut failures = Vec::new();
        for source in SourceStore::list() {
            if source.url.is_none() || period.is_some_and(|period| !is_due(source.updated, period))
            {
                continue;
            }
            match SourceImporter::refresh(&source.id).await {
                Ok((updated, _)) => {
                    let _ = state.mihomo.reload(&updated.id).await;
                }
                Err(why) => {
                    failures.push(format!("подписка «{}» не обновилась: {why}", source.name))
                }
            }
        }
        failures
    }
}

/// Сколько минут источник считается свежим на этом заходе — и стоит ли вообще заходить.
///
/// Первый заход отличается от прочих (D-024): «обновлять при запуске» — отдельная галка,
/// а «только при запуске» (периода нет) при старте всё равно освежает — иначе `updated`
/// первого запуска может быть сколь угодно старым.
fn period(refresh: Refresh, first: bool) -> Option<u64> {
    match (first, refresh.every_minutes) {
        (true, _) if !refresh.on_start => None,
        (true, 0) => Some(u64::from(Refresh::default().every_minutes)),
        (false, 0) => None,
        (_, minutes) => Some(u64::from(minutes)),
    }
}

/// Пора ли. Источник, который не обновлялся ни разу, пора всегда.
fn is_due(updated: Option<u64>, period_minutes: u64) -> bool {
    let Some(updated) = updated else {
        return true;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    now.saturating_sub(updated) >= period_minutes * 60
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(seconds: u64) -> Option<u64> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Some(now - seconds)
    }

    /// Три правила в одной таблице: галка запуска, «только при запуске» и обычный период.
    #[test]
    fn the_first_pass_is_not_the_same_as_the_rest() {
        let off = Refresh {
            on_start: false,
            every_minutes: 60,
        };
        let only_start = Refresh {
            on_start: true,
            every_minutes: 0,
        };
        let usual = Refresh {
            on_start: true,
            every_minutes: 60,
        };
        assert_eq!(period(off, true), None, "запуск без галки — не наше дело");
        assert_eq!(
            period(off, false),
            Some(60),
            "по расписанию — по-прежнему да"
        );
        assert_eq!(
            period(only_start, true),
            Some(u64::from(Refresh::default().every_minutes)),
            "«только при запуске»: срок берём умолчанием, иначе освежать нечего"
        );
        assert_eq!(
            period(only_start, false),
            None,
            "периода нет — и заходов нет"
        );
        assert_eq!(period(usual, true), Some(60));
        assert_eq!(period(usual, false), Some(60));
    }

    #[test]
    fn a_source_that_never_updated_is_always_due() {
        assert!(is_due(None, 24 * 60));
    }

    #[test]
    fn the_period_is_counted_from_the_last_update() {
        assert!(!is_due(ago(60), 60), "минуту назад — рано для часа");
        assert!(is_due(ago(3600), 60), "час прошёл — пора");
        assert!(is_due(ago(90_000), 24 * 60), "сутки прошли — пора");
        assert!(!is_due(ago(3600), 24 * 60), "час — не сутки");
    }

    /// Часы у машины могут уехать назад; отрицательная разница не должна становиться огромной.
    #[test]
    fn a_clock_from_the_future_does_not_force_an_update() {
        let future = ago(0).map(|now| now + 10_000);
        assert!(!is_due(future, 60));
    }
}
