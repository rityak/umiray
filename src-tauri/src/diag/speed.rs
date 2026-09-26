//! Сколько дают скачать — и не душат ли после первых секунд.
//!
//! Две пробы из плана здесь одна, потому что вопрос один: качаем несколько кусков подряд
//! и смотрим и на среднюю скорость, и на то, как она меняется от куска к куску. Ровная
//! полоса — канал такой и есть; провал к концу — душилка, которая включается не сразу.
//!
//! Кусками, а не одним потоком, нарочно: поток пришлось бы читать по частям, а это
//! отдельная возможность у `reqwest`, которой в сборке нет. Шесть последовательных
//! запросов дают ту же кривую и ничего не стоят сверх того, что уже есть.
//!
//! **Проба качает настоящие мегабайты.** Она помечена тяжёлой и в «быструю» проверку
//! не попадает: диагностика не должна съедать трафик у того, кто просто открыл раздел.

use std::time::{Duration, Instant};

use crate::diag::report::{Report, Row, Tone, Verdict};
use crate::error::{AppError, Result};
use crate::http;

/// Откуда качаем. У Cloudflare это служебная точка их же спидтеста: отдаёт ровно столько
/// байт, сколько попросили, и стоит близко к любому провайдеру.
const SOURCE: &str = "https://speed.cloudflare.com/__down?bytes=";

/// Размер куска. Два мегабайта — компромисс: меньше меряет разгон TCP, а не канал,
/// больше делает пробу дорогой по трафику.
const CHUNK: usize = 2 * 1024 * 1024;

/// Сколько кусков. Шесть: по трём первым видно, как канал разгоняется, по трём последним —
/// душат его или нет.
const CHUNKS: usize = 6;

/// Потолок на один кусок. Медленный канал — это не отказ, но ждать по минуте нельзя.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Скорость куска в мегабитах в секунду.
fn mbits(bytes: usize, ms: u64) -> f64 {
    if ms == 0 {
        return 0.0;
    }
    (bytes as f64 * 8.0) / (ms as f64 * 1000.0)
}

/// Разогревочный кусок. Замерено: первые два куска через только что поднятый туннель
/// не скачались вовсе, а первый прямой шёл вчетверо медленнее следующих — это цена
/// поднятия соединения, а не полоса канала. В зачёт он не идёт.
const WARMUP: usize = 256 * 1024;

/// Один прогон: скорости кусков по порядку.
async fn run(through: Option<u16>, report: &mut Report, label: &str) -> Result<Vec<f64>> {
    let mut builder = reqwest::Client::builder()
        .user_agent(http::USER_AGENT)
        .timeout(TIMEOUT)
        // «Напрямую» значит напрямую: иначе в System проба шла бы через наш же прокси.
        .no_proxy();
    if let Some(port) = through {
        let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
            .map_err(|e| AppError::network(format!("Прокси не задан: {e}")))?;
        builder = builder.proxy(proxy);
    }
    let client = builder
        .build()
        .map_err(|e| AppError::network(format!("Клиент не собрался: {e}")))?;

    // Прогрев: поднять соединение и не считать это скоростью. Ответ выбрасываем.
    let warm = client.get(format!("{SOURCE}{WARMUP}")).send().await;
    if let Ok(response) = warm {
        let _ = response.bytes().await;
    }

    let mut rates: Vec<f64> = Vec::new();
    for number in 1..=CHUNKS {
        let at = Instant::now();
        let got = client.get(format!("{SOURCE}{CHUNK}")).send().await;
        let bytes = match got {
            Ok(response) => response.bytes().await.map(|body| body.len()).unwrap_or(0),
            Err(_) => 0,
        };
        let ms = at.elapsed().as_millis() as u64;
        if bytes == 0 {
            report.say(Tone::Bad, format!("{label}, кусок {number}: не скачался"));
            report.rows.push(Row {
                cells: vec![label.into(), number.to_string(), "—".into(), "—".into()],
                verdict: Verdict::Bad,
                mark: false,
            });
            continue;
        }
        let rate = mbits(bytes, ms);
        rates.push(rate);
        report.say(
            Tone::Ok,
            format!(
                "{label:<12} кусок {number}: {:>5} мс — {rate:.1} Мбит/с",
                ms
            ),
        );
        report.rows.push(Row {
            cells: vec![
                label.to_string(),
                number.to_string(),
                format!("{ms} мс"),
                format!("{rate:.1} Мбит/с"),
            ],
            verdict: Verdict::Ok,
            mark: false,
        });
    }
    Ok(rates)
}

/// Средняя по прогону.
fn average(rates: &[f64]) -> f64 {
    if rates.is_empty() {
        return 0.0;
    }
    rates.iter().sum::<f64>() / rates.len() as f64
}

/// `speed`: полоса и кривая по кускам — напрямую и через туннель.
///
/// Оба прогона в одной пробе нарочно: «через VPN медленнее» — это **сравнение**, и цифра
/// без второй половины на него не отвечает. Порядок тоже не случаен: сперва напрямую,
/// потом через узел, потому что канал разгоняется, и обратный порядок польстил бы туннелю.
pub async fn measure(through: Option<u16>) -> Result<Report> {
    let started = Instant::now();
    let mut report = Report::new("speed");
    report.say(
        Tone::Info,
        format!(
            "speed: {CHUNKS} кусков по {} МБ{}",
            CHUNK / 1024 / 1024,
            match through {
                Some(port) => format!(", напрямую и через прокси 127.0.0.1:{port}"),
                None => ", только напрямую — ядро не запущено".to_string(),
            }
        ),
    );
    report.columns = ["Как", "Кусок", "За", "Скорость"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let direct = run(None, &mut report, "напрямую").await?;
    let tunneled = match through {
        Some(port) => run(Some(port), &mut report, "через узел").await?,
        None => Vec::new(),
    };

    let ms = started.elapsed().as_millis() as u64;
    if direct.is_empty() && tunneled.is_empty() {
        report.say(Tone::Bad, "не скачался ни один кусок");
        return Ok(report.finish(Verdict::Bad, "качать не вышло".to_string(), ms));
    }

    let mine = average(&direct);
    let shape = throttling(&direct);
    if let Some(drop) = shape {
        report.say(
            Tone::Warn,
            format!(
                "напрямую: первые куски {:.1} Мбит/с, последние {:.1} — минус {drop:.0}%: похоже на душилку",
                head(&direct),
                tail(&direct)
            ),
        );
    }

    // Через туннель всегда медленнее — вопрос, насколько. Треть потерь это цена
    // шифрования и лишнего плеча, а вот десятая доля от прямой скорости — уже беда.
    let (verdict, headline) = if tunneled.is_empty() {
        match shape {
            Some(drop) => (
                Verdict::Warn,
                format!("{mine:.0} Мбит/с, к концу падает на {drop:.0}%"),
            ),
            None if through.is_none() => (
                Verdict::Idle,
                format!("{mine:.0} Мбит/с напрямую; ядро не запущено, сравнить не с чем"),
            ),
            None => (Verdict::Ok, format!("{mine:.0} Мбит/с, ровно")),
        }
    } else {
        let out = average(&tunneled);
        let share = if mine > 0.0 { out / mine * 100.0 } else { 0.0 };
        report.say(
            if share >= 50.0 { Tone::Ok } else { Tone::Warn },
            format!("через узел {out:.1} из {mine:.1} Мбит/с — {share:.0}% прямой скорости"),
        );
        report.rows.push(Row {
            cells: vec![
                "итог".into(),
                "—".into(),
                format!("{share:.0}%"),
                format!("{out:.0} из {mine:.0} Мбит/с"),
            ],
            verdict: if share >= 50.0 {
                Verdict::Ok
            } else {
                Verdict::Warn
            },
            mark: true,
        });
        if share >= 50.0 {
            (
                Verdict::Ok,
                format!("через узел {out:.0} Мбит/с — {share:.0}% прямой"),
            )
        } else {
            (
                Verdict::Warn,
                format!("через узел {out:.0} из {mine:.0} Мбит/с — теряется больше половины"),
            )
        }
    };
    Ok(report.finish(verdict, headline, ms))
}

/// На сколько процентов просела скорость к концу. `None` — не просела настолько, чтобы
/// об этом говорить.
///
/// Порог в четверть выбран так: канал сам по себе гуляет процентов на десять-пятнадцать,
/// а душилка режет вдвое и больше. Между ними и проходит граница.
fn throttling(rates: &[f64]) -> Option<f64> {
    if rates.len() < 4 {
        return None;
    }
    let (first, last) = (head(rates), tail(rates));
    if first <= 0.0 {
        return None;
    }
    let drop = (first - last) / first * 100.0;
    (drop >= 25.0).then_some(drop)
}

fn head(rates: &[f64]) -> f64 {
    let take = rates.len() / 3;
    let take = take.max(1);
    rates[..take].iter().sum::<f64>() / take as f64
}

fn tail(rates: &[f64]) -> f64 {
    let take = (rates.len() / 3).max(1);
    rates[rates.len() - take..].iter().sum::<f64>() / take as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megabits_are_counted_the_way_providers_count_them() {
        // 2 МБ за секунду — это ~16,8 Мбит/с.
        let rate = mbits(2 * 1024 * 1024, 1000);
        assert!((rate - 16.7).abs() < 0.2, "{rate}");
    }

    /// Ровный канал душилкой не объявляем: он и сам гуляет на десяток процентов.
    #[test]
    fn an_even_channel_is_not_throttling() {
        assert_eq!(throttling(&[50.0, 48.0, 52.0, 47.0, 51.0, 49.0]), None);
    }

    /// А вот падение вдвое к концу — она самая.
    #[test]
    fn a_drop_by_half_is_throttling() {
        let drop = throttling(&[90.0, 88.0, 60.0, 30.0, 28.0, 27.0]).expect("не заметили");
        assert!(drop > 60.0, "{drop}");
    }

    /// На трёх кусках судить не о чем: сравнивать нечего.
    #[test]
    fn too_few_chunks_to_judge() {
        assert_eq!(throttling(&[90.0, 40.0, 20.0]), None);
    }

    #[test]
    fn an_empty_run_averages_to_zero_and_not_to_a_panic() {
        assert_eq!(average(&[]), 0.0);
        assert!((average(&[10.0, 20.0]) - 15.0).abs() < f64::EPSILON);
    }
}
