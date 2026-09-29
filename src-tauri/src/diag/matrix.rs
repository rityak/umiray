//! Перебор настроек ядра на стенде: что из них быстрее и что течёт.
//!
//! Вопрос, ради которого раздел затевался наполовину: `fake-ip` или `redir-host`,
//! нужен ли сниффер, какой резолвер брать. На него нельзя ответить чтением документации —
//! только замером, и замерять надо **на этой машине и в этой сети**.
//!
//! Перебор идёт на стенде (D-098): одноразовое ядро со своим портом и без TUN. Рабочий
//! VPN при этом не трогается ни разу — иначе проба, которая ищет лучшее, по дороге рвала
//! бы соединение тому, кто её запустил.
//!
//! **Стек TUN здесь не перебирается.** Адаптер в системе один, и поднять его стендом
//! значит оборвать связь. Это отдельная задача, и говорить о ней надо прямо, а не молча
//! показывать таблицу без половины строк.

use std::time::{Duration, Instant};

use crate::diag::bench::Bench;
use crate::diag::report::Report;
use crate::diag::report::Row;
use crate::diag::report::Tone;
use crate::diag::report::Verdict;
use crate::error::Result;

/// Что перебираем. Пара «имя — как это выглядит в конфиге»: набор нарочно маленький,
/// потому что каждая строка стоит запуска ядра, а отвечают эти четыре на девять десятых
/// вопросов.
struct Combo {
    name: &'static str,
    enhanced: &'static str,
    sniffer: bool,
    nameserver: &'static str,
}

const COMBOS: [Combo; 4] = [
    Combo {
        name: "fake-ip + сниффер",
        enhanced: "fake-ip",
        sniffer: true,
        nameserver: "https://dns.google/dns-query",
    },
    Combo {
        name: "redir-host + сниффер",
        enhanced: "redir-host",
        sniffer: true,
        nameserver: "https://dns.google/dns-query",
    },
    Combo {
        name: "fake-ip без сниффера",
        enhanced: "fake-ip",
        sniffer: false,
        nameserver: "https://dns.google/dns-query",
    },
    Combo {
        name: "fake-ip + системный DNS",
        enhanced: "fake-ip",
        sniffer: true,
        // Пустая строка — подставим адрес системного резолвера на месте: он у каждой
        // машины свой, и вписывать его сюда значило бы соврать на чужой.
        nameserver: "",
    },
];

/// Что спрашиваем у каждого сочетания. Имя с длинной цепочкой CNAME: на нём разница
/// между режимами разрешения видна, а на голом адресе — нет.
const PROBE: &str = "www.google.com";

/// Сколько ждём ответа от стенда.
const TIMEOUT: Duration = Duration::from_millis(2500);

pub struct Matrix;

impl Matrix {
    /// `matrix`: прогнать сочетания и сказать, какое брать.
    pub async fn measure() -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("matrix");
        report.say(
            Tone::Info,
            format!("matrix: {} сочетаний на стенде, имя {PROBE}", COMBOS.len()),
        );
        report.say(
            Tone::Dim,
            "стек TUN так не померить: адаптер один на систему, и стенд оборвал бы связь",
        );
        report.columns = ["Сочетание", "Ядро встало", "Имя за", "Ответ", "Вывод"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let system = crate::system::net::NetInfo::first_resolver();
        let mut best: Option<(u64, usize)> = None;
        let mut results: Vec<(String, Option<u64>, String)> = Vec::new();

        for combo in COMBOS.iter() {
            let nameserver = if combo.nameserver.is_empty() {
                match &system {
                    Some(addr) => addr.clone(),
                    None => {
                        report.say(
                            Tone::Dim,
                            format!("{}: системный резолвер не найден — пропускаем", combo.name),
                        );
                        results.push((combo.name.to_string(), None, "нет системного DNS".into()));
                        continue;
                    }
                }
            } else {
                combo.nameserver.to_string()
            };

            let at = Instant::now();
            let answered = tokio::time::timeout(TIMEOUT, async {
                let bench = Bench::with(&document(combo, &nameserver)).await?;
                bench.resolve(PROBE).await
            })
            .await;
            let ms = at.elapsed().as_millis() as u64;

            let (ok, what) = match answered {
                Err(_) => (false, format!("таймаут {} мс", TIMEOUT.as_millis())),
                Ok(Err(error)) => (false, error.to_string()),
                Ok(Ok(addresses)) => (true, addresses.join(", ")),
            };
            report.say(
                if ok { Tone::Ok } else { Tone::Bad },
                format!("{:<26} {:>8}  {what}", combo.name, Report::millis(ms)),
            );
            if ok {
                match best {
                    Some((was, _)) if was <= ms => {}
                    _ => best = Some((ms, results.len())),
                }
            }
            results.push((combo.name.to_string(), ok.then_some(ms), what));
        }

        for (place, (name, ms, what)) in results.iter().enumerate() {
            let winner = matches!(best, Some((_, at)) if at == place);
            report.rows.push(Row {
                cells: vec![
                    name.clone(),
                    if ms.is_some() { "да" } else { "нет" }.to_string(),
                    ms.map(Report::millis).unwrap_or_else(|| "—".to_string()),
                    what.clone(),
                    match (winner, ms.is_some()) {
                        (true, _) => "берём",
                        (false, true) => "мимо",
                        (false, false) => "не встало",
                    }
                    .to_string(),
                ],
                verdict: match (winner, ms.is_some()) {
                    (true, _) => Verdict::Ok,
                    (false, true) => Verdict::Idle,
                    (false, false) => Verdict::Bad,
                },
                mark: winner,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match best {
            Some((took, at)) => (
                Verdict::Ok,
                format!(
                    "быстрее всех «{}» — {}",
                    results[at].0,
                    Report::millis(took)
                ),
            ),
            None => (Verdict::Bad, "ни одно сочетание не заработало".to_string()),
        };
        Ok(report.finish(verdict, headline, ms))
    }
}

/// Конфиг одного сочетания. Отличается от стендового только тремя полями — и это
/// нарочно: если менять больше одного за раз, замер перестаёт отвечать на вопрос.
fn document(combo: &Combo, nameserver: &str) -> String {
    format!(
        "# Стенд диагностики: перебор сочетаний. Файл временный.\n\
         mixed-port: 0\n\
         mode: rule\n\
         log-level: silent\n\
         sniffer:\n  \
           enable: {}\n  \
           sniff:\n    \
             TLS:\n      \
               ports: [443]\n\
         dns:\n  \
           enable: true\n  \
           listen: \"\"\n  \
           ipv6: false\n  \
           enhanced-mode: {}\n  \
           nameserver:\n    \
             - \"{nameserver}\"\n\
         rules:\n  \
           - MATCH,DIRECT\n",
        combo.sniffer, combo.enhanced
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Сочетания отличаются ровно одним полем от базового — иначе замер не отвечает
    /// на вопрос «что дало разницу».
    #[test]
    fn each_combo_changes_one_thing() {
        let base = &COMBOS[0];
        for combo in COMBOS.iter().skip(1) {
            let changed = (combo.enhanced != base.enhanced) as u8
                + (combo.sniffer != base.sniffer) as u8
                + (combo.nameserver != base.nameserver) as u8;
            assert_eq!(changed, 1, "{}: изменено {changed} полей", combo.name);
        }
    }

    /// Стенд не слушает портов и не поднимает адаптер — в том числе и здесь.
    #[test]
    fn the_matrix_bench_listens_to_nothing() {
        let yaml = document(&COMBOS[0], "8.8.8.8");
        assert!(yaml.contains("mixed-port: 0"));
        assert!(yaml.contains("listen: \"\""));
        assert!(!yaml.contains("tun:"));
        assert!(yaml.contains("enhanced-mode: fake-ip"));
        assert!(yaml.contains("enable: true"));
    }

    #[test]
    fn turning_the_sniffer_off_shows_in_the_config() {
        let yaml = document(&COMBOS[2], "8.8.8.8");
        assert!(yaml.contains("enable: false"));
    }
}
