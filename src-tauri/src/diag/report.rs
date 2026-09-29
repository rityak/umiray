//! Один отчёт на все утилиты (D-097).
//!
//! У раздела две стороны, и обе читают **одно и то же**: «Проверка» берёт вердикт
//! и заголовок, «Инструменты» — строки консоли и таблицу. Если бы у сторон были разные
//! типы, утилиту пришлось бы дописывать дважды, а расходиться они начали бы на второй.
//!
//! Строка консоли несёт не цвет, а **тон**: раскраска — дело окна, а модулю знать
//! о темах незачем.

use serde::{Deserialize, Serialize};

/// Чем кончилась утилита. Те же четыре состояния, что у всего окна (STYLEGUIDE),
/// плюс «не гонялось» — у пробы, которой нечего было мерить.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Прошло: ответ есть и он тот, которого ждали.
    Ok,
    /// Прошло с оговоркой: ответ есть, но он говорит о проблеме.
    Warn,
    /// Не прошло.
    Bad,
    /// Нечего было мерить: нет ядра, нет туннеля, нет файла.
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// Обычная строка: что запустили, чем кончилось.
    Info,
    Ok,
    Warn,
    Bad,
    /// Подробность, которую читают только когда что-то не сошлось.
    Dim,
}

/// Строка консоли.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub tone: Tone,
    pub text: String,
}

impl Line {
    pub fn new(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
        }
    }
}

/// Строка таблицы. Ячейки — уже готовый текст: считать и форматировать умеет утилита,
/// окно только показывает.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub cells: Vec<String>,
    pub verdict: Verdict,
    /// Строка, которую утилита предлагает взять: отмеченные резолверы, лучшее сочетание.
    /// На ней стоит кнопка «применить» — поэтому это поле отчёта, а не догадка окна.
    #[serde(default)]
    pub mark: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Идентификатор утилиты — тот же, что в списке.
    pub tool: String,
    pub verdict: Verdict,
    /// Одна строка для «Проверки»: что нашлось. Не «ок» — а что именно.
    pub headline: String,
    pub ms: u64,
    /// Заголовки таблицы; пусто — таблицы у этой утилиты нет.
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<Row>,
    /// Сырой вывод для консоли — в том порядке, в каком он появлялся.
    #[serde(default)]
    pub lines: Vec<Line>,
}

impl Report {
    pub fn new(tool: &str) -> Self {
        Self {
            tool: tool.to_string(),
            verdict: Verdict::Idle,
            headline: String::new(),
            ms: 0,
            columns: Vec::new(),
            rows: Vec::new(),
            lines: Vec::new(),
        }
    }

    pub fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.lines.push(Line::new(tone, text));
    }

    /// Итог: вердикт, заголовок и последняя строка консоли — одним движением, чтобы
    /// они не разъехались.
    pub fn finish(mut self, verdict: Verdict, headline: impl Into<String>, ms: u64) -> Self {
        let headline = headline.into();
        self.say(
            match verdict {
                Verdict::Ok => Tone::Ok,
                Verdict::Warn => Tone::Warn,
                Verdict::Bad => Tone::Bad,
                Verdict::Idle => Tone::Dim,
            },
            format!("{headline} · {}", Report::millis(ms)),
        );
        self.verdict = verdict;
        self.headline = headline;
        self.ms = ms;
        self
    }
}

impl Report {
    /// Время по-человечески: миллисекунды до секунды, дальше секунды с десятой.
    pub fn millis(ms: u64) -> String {
        if ms < 1000 {
            format!("{ms} мс")
        } else {
            format!("{},{} с", ms / 1000, (ms % 1000) / 100)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_reads_as_people_write_it() {
        assert_eq!(Report::millis(0), "0 мс");
        assert_eq!(Report::millis(999), "999 мс");
        assert_eq!(Report::millis(1000), "1,0 с");
        assert_eq!(Report::millis(2140), "2,1 с");
    }

    /// Вердикт, заголовок и последняя строка консоли обязаны говорить одно и то же:
    /// расходятся они ровно тогда, когда их ставят порознь.
    #[test]
    fn the_summary_and_the_last_line_agree() {
        let report = Report::new("t").finish(Verdict::Warn, "5 из 9", 2100);
        assert_eq!(report.verdict, Verdict::Warn);
        assert_eq!(report.headline, "5 из 9");
        let last = report.lines.last().unwrap();
        assert_eq!(last.tone, Tone::Warn);
        assert_eq!(last.text, "5 из 9 · 2,1 с");
    }
}
