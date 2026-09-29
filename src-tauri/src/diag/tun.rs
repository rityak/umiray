//! Какой стек TUN встаёт на этой машине.
//!
//! Вопрос из формы «Ядра» (D-086): `system` быстрее, но встаёт не везде, `gvisor` встаёт
//! везде и медленнее, `mixed` посередине. Выбирать это чтением документации нельзя —
//! только замером, и замер должен быть на **этой** машине с её драйверами.
//!
//! **Связь при этом не рвётся.** Стенд поднимает адаптер с `auto-route: false`: интерфейс
//! создаётся, стек инициализируется, а таблица маршрутов не трогается вовсе. На вопрос
//! «встаёт ли» этого достаточно, а платить за ответ минутой без интернета не нужно.
//!
//! Права администратора всё равно нужны — адаптер без них не создать, — и проба говорит
//! об этом прямо, а не падает с чужой ошибкой.
//!
//! Пока работает рабочее ядро в режиме TUN, проба не запускается: адаптер в системе один,
//! и драка за него кончится не тем, что мы меряем.

use std::time::{Duration, Instant};

use crate::diag::bench::Bench;
use crate::diag::report::Report;
use crate::diag::report::Row;
use crate::diag::report::Tone;
use crate::diag::report::Verdict;
use crate::error::Result;
use crate::system::elevation::Elevation;
use crate::system::net::NetInfo;

/// Что перебираем. Имена — ровно те, что понимает ядро и что стоят в форме.
const STACKS: [&str; 3] = ["mixed", "system", "gvisor"];

/// Как ядро называет свой адаптер, если имя не задано.
const DEVICE: &str = "umiray-probe";

/// Сколько ждём ответа стенда. Адаптер создаётся дольше, чем встаёт голое ядро.
const TIMEOUT: Duration = Duration::from_millis(6000);

/// Имя, на котором проверяем, что стек не просто поднялся, а работает.
const PROBE: &str = "example.com";

/// Что стек показал на стенде. Типизированный слой (D-097): его спрашивает код, которому
/// нужно решение, — шаг «выбрали TUN» (D-115). Отчёт для окна собран поверх него.
pub struct Stood {
    pub stack: &'static str,
    /// За сколько встал. Пусто — не встал вовсе.
    pub ms: Option<u64>,
    /// Видно ли адаптер в системе, пока стенд жив.
    pub adapter: bool,
    /// Что вернул — адреса или причину отказа.
    pub what: String,
}

pub struct TunProbe;

impl TunProbe {
    /// Поднять по стенду на каждый стек. Отказ — это не «не встал», а «мерить нельзя»:
    /// без прав адаптер не создать, а при работающем TUN он в системе уже занят.
    pub async fn probe(mode: Option<&str>) -> std::result::Result<Vec<Stood>, &'static str> {
        if !Elevation::is_elevated() {
            return Err("нужны права администратора");
        }
        if mode.is_some_and(|mode| mode.eq_ignore_ascii_case("tun")) {
            return Err("выключите VPN: адаптер в системе один");
        }
        let mut results = Vec::new();
        for stack in STACKS {
            let at = Instant::now();
            let answered = tokio::time::timeout(TIMEOUT, async {
                let bench = Bench::with(&document(stack)).await?;
                let addresses = bench.resolve(PROBE).await;
                // Адаптер спрашиваем **пока стенд жив**: после `Drop` его уже нет.
                let seen = NetInfo::adapters()
                    .map(|list| list.iter().any(|adapter| adapter.name.contains(DEVICE)))
                    .unwrap_or(false);
                addresses.map(|found| (found, seen))
            })
            .await;
            let ms = at.elapsed().as_millis() as u64;
            let (ok, adapter, what) = match answered {
                Err(_) => (false, false, format!("таймаут {} мс", TIMEOUT.as_millis())),
                Ok(Err(error)) => (false, false, error.to_string()),
                Ok(Ok((addresses, seen))) => (true, seen, addresses.join(", ")),
            };
            results.push(Stood {
                stack,
                ms: ok.then_some(ms),
                adapter,
                what,
            });
        }
        Ok(results)
    }

    /// Какой стек брать на этой машине: самый быстрый из вставших. Пусто — не встал ни один.
    pub fn winner(results: &[Stood]) -> Option<&Stood> {
        results
            .iter()
            .filter(|stood| stood.ms.is_some())
            .min_by_key(|stood| stood.ms)
    }

    /// `tun-stack`: какой стек встаёт и за сколько.
    pub async fn stacks(mode: Option<&str>) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("tun-stack");
        report.columns = ["Стек", "Встал", "Адаптер", "Имя за", "Вывод"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        report.say(
            Tone::Info,
            format!(
                "tun-stack: {} стека, adapter={DEVICE}, auto-route выключен",
                STACKS.len()
            ),
        );
        report.say(Tone::Dim, "маршруты не трогаем — связь не прервётся");

        let results = match TunProbe::probe(mode).await {
            Err(why) => {
                report.say(Tone::Dim, why);
                return Ok(report.finish(
                    Verdict::Idle,
                    why.to_string(),
                    started.elapsed().as_millis() as u64,
                ));
            }
            Ok(results) => results,
        };

        for stood in &results {
            report.say(
                if stood.ms.is_some() {
                    Tone::Ok
                } else {
                    Tone::Bad
                },
                format!(
                    "{:<8} {:>8}  адаптер {}  {}",
                    stood.stack,
                    stood
                        .ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    if stood.adapter {
                        "есть"
                    } else {
                        "не видно"
                    },
                    stood.what
                ),
            );
        }

        let best = TunProbe::winner(&results).map(|stood| stood.stack);
        for stood in &results {
            let winner = best == Some(stood.stack);
            report.rows.push(Row {
                cells: vec![
                    stood.stack.to_string(),
                    if stood.ms.is_some() { "да" } else { "нет" }.to_string(),
                    if stood.adapter {
                        "есть"
                    } else {
                        "не видно"
                    }
                    .to_string(),
                    stood
                        .ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    // У неудачи в этой колонке стоит **причина**, а не слово «не встал»:
                    // ради причины пробу и гоняют — она называет, чего не хватает драйверу.
                    match (winner, stood.ms.is_some()) {
                        (true, _) => "берём".to_string(),
                        (false, true) => "мимо".to_string(),
                        (false, false) => stood.what.clone(),
                    },
                ],
                verdict: match (winner, stood.ms.is_some()) {
                    (true, _) => Verdict::Ok,
                    (false, true) => Verdict::Idle,
                    (false, false) => Verdict::Bad,
                },
                mark: winner,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let up = results.iter().filter(|stood| stood.ms.is_some()).count();
        // Разница между стеками бывает в пределах шума запуска ядра — замерено на живой
        // машине: все три встали за 1,6 с. Короновать в таком случае одного значит
        // советовать наугад.
        let close = spread(&results).is_some_and(|spread| spread < 15.0);
        let (verdict, headline) = match (TunProbe::winner(&results), up) {
            (None, _) => (
                Verdict::Bad,
                "ни один стек не встал — TUN на этой машине не работает".to_string(),
            ),
            (Some(_), n) if n < STACKS.len() => {
                (Verdict::Warn, format!("встали {n} из {}", STACKS.len()))
            }
            (Some(_), _) if close => (
                Verdict::Ok,
                "встали все, разницы нет — берите любой".to_string(),
            ),
            (Some(best), _) => (
                Verdict::Ok,
                format!(
                    "встали все, быстрее «{}» ({})",
                    best.stack,
                    Report::millis(best.ms.unwrap_or_default())
                ),
            ),
        };
        Ok(report.finish(verdict, headline, ms))
    }
}

/// На сколько процентов самый медленный встал дольше самого быстрого. `None` — сравнивать
/// не с чем: встал один стек или ни одного.
fn spread(results: &[Stood]) -> Option<f64> {
    let times: Vec<u64> = results.iter().filter_map(|stood| stood.ms).collect();
    if times.len() < 2 {
        return None;
    }
    let fast = *times.iter().min()?;
    let slow = *times.iter().max()?;
    (fast > 0).then(|| (slow - fast) as f64 / fast as f64 * 100.0)
}

/// Конфиг стенда с адаптером. Отличается от обычного стендового ровно блоком `tun`.
///
/// `auto-route: false` — то, что делает пробу безопасной: адаптер создаётся, стек
/// инициализируется, а маршрут по умолчанию остаётся у настоящего интерфейса.
/// `auto-detect-interface` выключен по той же причине.
fn document(stack: &str) -> String {
    format!(
        "# Стенд диагностики: проверка стека TUN. Файл временный.\n\
         mixed-port: 0\n\
         mode: rule\n\
         log-level: silent\n\
         tun:\n  \
           enable: true\n  \
           stack: {stack}\n  \
           device: {DEVICE}\n  \
           auto-route: false\n  \
           auto-detect-interface: false\n  \
           dns-hijack: []\n\
         dns:\n  \
           enable: true\n  \
           listen: \"\"\n  \
           ipv6: false\n  \
           enhanced-mode: fake-ip\n  \
           nameserver:\n    \
             - \"https://dns.google/dns-query\"\n\
         rules:\n  \
           - MATCH,DIRECT\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Главное в этой пробе — то, чего она **не** делает: не забирает маршрут и
    /// не перехватывает имена. Иначе замер стоил бы обрыва связи.
    #[test]
    fn the_probe_does_not_take_the_route() {
        let yaml = document("system");
        assert!(yaml.contains("auto-route: false"));
        assert!(yaml.contains("auto-detect-interface: false"));
        assert!(yaml.contains("dns-hijack: []"));
        assert!(yaml.contains("mixed-port: 0"));
    }

    #[test]
    fn the_stack_travels_verbatim() {
        for stack in STACKS {
            assert!(
                document(stack).contains(&format!("stack: {stack}")),
                "{stack}"
            );
        }
    }

    /// Замерено на живой машине: `mixed`, `system` и `gvisor` встали за 1,6 с каждый.
    /// Разница там — шум запуска ядра, и советовать по ней нельзя.
    #[test]
    fn a_difference_within_the_noise_is_not_a_difference() {
        let rows = |a, b, c| {
            ["mixed", "system", "gvisor"]
                .into_iter()
                .zip([a, b, c])
                .map(|(stack, ms)| Stood {
                    stack,
                    ms: Some(ms),
                    adapter: true,
                    what: String::new(),
                })
                .collect::<Vec<_>>()
        };
        assert!(spread(&rows(1600, 1640, 1580)).unwrap() < 15.0, "шум");
        assert!(spread(&rows(1600, 4200, 1580)).unwrap() > 15.0, "разница");
        assert_eq!(spread(&[]), None);
    }

    /// Три стека — ровно те, что понимает форма «Ядра» (D-086). Разъехаться им нельзя:
    /// проба советует то, что человек потом выберет списком.
    #[test]
    fn the_stacks_are_the_ones_the_form_offers() {
        assert_eq!(STACKS, ["mixed", "system", "gvisor"]);
    }

    /// Без прав проба не притворяется, что померила.
    #[tokio::test]
    async fn without_rights_it_says_so() {
        if Elevation::is_elevated() {
            return;
        }
        let report = TunProbe::stacks(None).await.unwrap();
        assert_eq!(report.verdict, Verdict::Idle);
        assert!(report.headline.contains("права"));
    }
}
