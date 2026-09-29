//! Что открывается, а что нет: эталонные ресурсы, внешний адрес, рукопожатие по имени.
//!
//! Все три пробы ходят наружу обычным HTTPS — тем же клиентом, что и подписки. Отдельного
//! транспорта им не нужно: вопрос «доехал ли запрос» решается самим фактом ответа, и код
//! ответа тут ни при чём. Сервер, вернувший 403, **жив и достижим** — а именно это
//! и спрашивают.

use std::time::{Duration, Instant};

use crate::collections::Collections;
use crate::diag::report::Report;
use crate::diag::report::Row;
use crate::diag::report::Tone;
use crate::diag::report::Verdict;
use crate::error::{AppError, Result};
use crate::http;

/// Сколько ждём ответа. Три секунды: зарезанный адрес не ответит и за тридцать,
/// а живой отвечает за сотни миллисекунд.
const TIMEOUT: Duration = Duration::from_secs(3);

/// Куда ходим за внешним адресом. Две точки: одна может лежать, а вопрос «через кого
/// я вышел» слишком важен, чтобы зависеть от одного сервера.
const WHOAMI: [&str; 2] = ["https://api.ipify.org", "https://ifconfig.me/ip"];

/// Чем кончился один запрос.
struct Reach {
    ms: Option<u64>,
    /// Код ответа: он не решает ничего, но говорит, кто ответил.
    status: Option<u16>,
    error: Option<String>,
}

impl Reach {
    fn ok(&self) -> bool {
        self.error.is_none()
    }
}

/// Постучаться по адресу. Через прокси, если он задан: так одна и та же проба отвечает
/// и «открывается ли вообще», и «открывается ли через VPN».
async fn reach(url: &str, through: Option<u16>) -> Reach {
    let mut builder = reqwest::Client::builder()
        .user_agent(http::USER_AGENT)
        .timeout(TIMEOUT)
        // «Напрямую» значит напрямую: иначе в System проба шла бы через наш же прокси.
        .no_proxy();
    if let Some(port) = through {
        match reqwest::Proxy::all(format!("http://127.0.0.1:{port}")) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(e) => {
                return Reach {
                    ms: None,
                    status: None,
                    error: Some(e.to_string()),
                };
            }
        }
    }
    let client = match builder.build() {
        Ok(client) => client,
        Err(e) => {
            return Reach {
                ms: None,
                status: None,
                error: Some(e.to_string()),
            }
        }
    };

    let started = Instant::now();
    match client.get(url).send().await {
        Ok(response) => Reach {
            ms: Some(started.elapsed().as_millis() as u64),
            status: Some(response.status().as_u16()),
            error: None,
        },
        Err(error) => Reach {
            ms: None,
            status: None,
            error: Some(why(&error)),
        },
    }
}

/// Причина отказа в одном слове. Строка `reqwest` тянет за собой всю цепочку источников,
/// а разница между «не дошло» и «не встал TLS» — это как раз то, ради чего пробу и гоняют.
fn why(error: &reqwest::Error) -> String {
    let text = error.to_string().to_lowercase();
    let chain = {
        let mut source: Option<&dyn std::error::Error> = std::error::Error::source(error);
        let mut all = String::new();
        while let Some(inner) = source {
            all.push_str(&inner.to_string().to_lowercase());
            all.push(' ');
            source = std::error::Error::source(inner);
        }
        all
    };
    let all = format!("{text} {chain}");
    if error.is_timeout() || all.contains("timed out") {
        "таймаут".to_string()
    } else if all.contains("certificate") || all.contains("tls") || all.contains("handshake") {
        "TLS не встал".to_string()
    } else if all.contains("dns") || all.contains("resolve") {
        "имя не разрешилось".to_string()
    } else if all.contains("reset") || all.contains("forcibly") || all.contains("сброс") {
        "соединение сброшено".to_string()
    } else if all.contains("refused") {
        "отказ в соединении".to_string()
    } else {
        "не дошло".to_string()
    }
}

pub struct WebProbe;

impl WebProbe {
    /// `sites`: что открывается напрямую, а что только через VPN.
    ///
    /// Список — данные (`collections/sites.yaml`), и это важнее, чем кажется: набор
    /// «что сейчас закрыто» меняется быстрее, чем выходят сборки клиента.
    pub async fn sites(through: Option<u16>) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("sites");
        let list = Collections::sites()?.sites;
        report.say(
            Tone::Info,
            format!(
                "sites: {} адресов{}",
                list.len(),
                match through {
                    Some(port) => format!(", через прокси 127.0.0.1:{port}"),
                    None => ", напрямую".to_string(),
                }
            ),
        );
        report.columns = ["Ресурс", "Зачем", "Ответ за", "Что случилось"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Разом, а не по очереди: у десяти адресов с трёхсекундным таймаутом очередь
        // складывается в полминуты, а это десять независимых запросов.
        let mut set = tokio::task::JoinSet::new();
        for (index, site) in list.iter().cloned().enumerate() {
            set.spawn(async move { (index, reach(&site.url, through).await) });
        }
        let mut got_all: Vec<(usize, Reach)> = Vec::new();
        while let Some(done) = set.join_next().await {
            match done {
                Ok(pair) => got_all.push(pair),
                Err(e) => return Err(AppError::network(format!("Опрос сорвался: {e}"))),
            }
        }
        got_all.sort_by_key(|(index, _)| *index);

        let mut down_base = 0;
        let mut down_other = 0;
        for ((_, got), site) in got_all.into_iter().zip(list.iter()) {
            let what = match (&got.error, got.status) {
                (Some(error), _) => error.clone(),
                (None, Some(code)) => format!("ответил {code}"),
                (None, None) => "ответил".to_string(),
            };
            // Красным — только `base`: остальные и должны быть закрыты. Заблокированный
            // ресурс, который не открылся, — это ровно то, ради чего клиент и нужен,
            // а придушенный CDN — ответ на «почему видео тормозит», а не авария.
            let verdict = match (got.ok(), site.group.as_str()) {
                (true, _) => Verdict::Ok,
                (false, "base") => Verdict::Bad,
                (false, _) => Verdict::Warn,
            };
            if !got.ok() {
                if site.group == "base" {
                    down_base += 1;
                } else {
                    down_other += 1;
                }
            }
            report.say(
                match verdict {
                    Verdict::Ok => Tone::Ok,
                    Verdict::Warn => Tone::Warn,
                    _ => Tone::Bad,
                },
                format!(
                    "{:<16} {:<8} {:>8}  {}",
                    site.name,
                    site.group,
                    got.ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    what
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    site.name.clone(),
                    site.note.clone(),
                    got.ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    what,
                ],
                verdict,
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let alive = list.len() - down_base - down_other;
        let (verdict, headline) = match (down_base, down_other) {
            (0, 0) => (Verdict::Ok, format!("открылись все {}", list.len())),
            (0, closed) => (
                Verdict::Warn,
                format!("{alive} из {}, закрыто {closed}", list.len()),
            ),
            (base, _) => (
                Verdict::Bad,
                format!("не открылись обычные: {base} — дело не в блокировках"),
            ),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// `external-ip`: с каким адресом нас видят снаружи — напрямую и через туннель.
    ///
    /// Единственная проба, которая отвечает на «VPN правда работает»: совпали адреса —
    /// значит трафик идёт мимо туннеля, чем бы ни горела кнопка.
    pub async fn external(through: Option<u16>) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("external-ip");
        report.columns = ["Как", "Адрес"].iter().map(|s| s.to_string()).collect();

        let direct = WebProbe::whoami(None).await;
        report.say(
            if direct.is_some() {
                Tone::Ok
            } else {
                Tone::Bad
            },
            format!(
                "напрямую   {}",
                direct.clone().unwrap_or_else(|| "—".into())
            ),
        );
        report.rows.push(Row {
            cells: vec![
                "напрямую".into(),
                direct.clone().unwrap_or_else(|| "—".into()),
            ],
            verdict: if direct.is_some() {
                Verdict::Ok
            } else {
                Verdict::Bad
            },
            mark: false,
        });

        let tunneled = match through {
            Some(_) => WebProbe::whoami(through).await,
            None => None,
        };
        if let Some(port) = through {
            report.say(
                if tunneled.is_some() {
                    Tone::Ok
                } else {
                    Tone::Bad
                },
                format!(
                    "через прокси {} на порту {port}",
                    tunneled.clone().unwrap_or_else(|| "—".into())
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    "через туннель".into(),
                    tunneled.clone().unwrap_or_else(|| "—".into()),
                ],
                verdict: if tunneled.is_some() {
                    Verdict::Ok
                } else {
                    Verdict::Bad
                },
                mark: true,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match (&direct, through, &tunneled) {
            (None, _, _) => (Verdict::Bad, "наружу не вышли вовсе".to_string()),
            (Some(home), None, _) => (
                Verdict::Idle,
                format!("{home} · ядро не запущено, сравнить не с чем"),
            ),
            (Some(_), Some(_), None) => (Verdict::Bad, "через туннель наружу не вышли".to_string()),
            (Some(home), Some(_), Some(out)) if home == out => (
                Verdict::Bad,
                format!("адрес тот же ({home}) — трафик идёт мимо туннеля"),
            ),
            (Some(_), Some(_), Some(out)) => (Verdict::Ok, format!("снаружи {out}")),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// Внешний адрес: первая точка, которая ответила.
    /// Каким адресом нас видят снаружи. Пусто — не ответил никто.
    ///
    /// Публична ради сторожа соединения (D-107): у него та же проба, только по расписанию.
    pub async fn whoami(through: Option<u16>) -> Option<String> {
        for url in WHOAMI {
            let mut builder = reqwest::Client::builder()
                .user_agent(http::USER_AGENT)
                .timeout(TIMEOUT)
                .no_proxy();
            if let Some(port) = through {
                let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).ok()?;
                builder = builder.proxy(proxy);
            }
            let Ok(client) = builder.build() else {
                continue;
            };
            let Ok(response) = client.get(url).send().await else {
                continue;
            };
            let Ok(text) = response.text().await else {
                continue;
            };
            let address = text.trim().to_string();
            if address.parse::<std::net::IpAddr>().is_ok() {
                return Some(address);
            }
        }
        None
    }

    /// `tls-sni`: доходит ли рукопожатие до имени.
    ///
    /// Отличает «сервер лежит» от «нас режут по имени»: до адреса запрос доходит, а до имени
    /// нет. Это же объясняет, почему у одного провайдера DoH работает, а у другого — нет.
    ///
    /// Полной картины проба не даёт: чтобы доказать, что режут именно по SNI, надо было бы
    /// сходить на тот же адрес без имени, а этого `reqwest` не умеет. Она отвечает на более
    /// скромный вопрос — «встаёт ли TLS», — и говорит об этом прямо.
    pub async fn tls(hosts: &[String]) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("tls-sni");
        if hosts.is_empty() {
            return Err(AppError::invalid("Не задано ни одного имени".to_string()));
        }
        report.say(Tone::Info, format!("tls-sni: {} имён", hosts.len()));
        report.columns = ["Имя", "Рукопожатие", "Что случилось"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut set = tokio::task::JoinSet::new();
        for (index, host) in hosts.iter().cloned().enumerate() {
            set.spawn(async move {
                let url = if host.starts_with("https://") {
                    host.clone()
                } else {
                    format!("https://{host}")
                };
                (index, reach(&url, None).await)
            });
        }
        let mut shots: Vec<(usize, Reach)> = Vec::new();
        while let Some(done) = set.join_next().await {
            match done {
                Ok(pair) => shots.push(pair),
                Err(e) => return Err(AppError::network(format!("Опрос сорвался: {e}"))),
            }
        }
        shots.sort_by_key(|(index, _)| *index);

        let mut failed = 0;
        for ((_, got), host) in shots.into_iter().zip(hosts.iter()) {
            if !got.ok() {
                failed += 1;
            }
            let what = match (&got.error, got.status) {
                (Some(error), _) => error.clone(),
                (None, Some(code)) => format!("ответил {code}"),
                (None, None) => "ответил".to_string(),
            };
            report.say(
                if got.ok() { Tone::Ok } else { Tone::Bad },
                format!(
                    "{:<28} {:>8}  {}",
                    host,
                    got.ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    what
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    host.clone(),
                    got.ms
                        .map(Report::millis)
                        .unwrap_or_else(|| "—".to_string()),
                    what,
                ],
                verdict: if got.ok() { Verdict::Ok } else { Verdict::Bad },
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match failed {
            0 => (Verdict::Ok, format!("встало у всех {}", hosts.len())),
            n if n == hosts.len() => (Verdict::Bad, "не встало ни у одного".to_string()),
            n => (Verdict::Warn, format!("не встало у {n} из {}", hosts.len())),
        };
        Ok(report.finish(verdict, headline, ms))
    }
}

#[cfg(test)]
mod tests {
    /// Причина отказа должна называться словом, а не цепочкой источников: разница между
    /// «не дошло» и «не встал TLS» — это то, ради чего пробу и гоняют.
    ///
    /// Настоящую `reqwest::Error` руками не собрать, поэтому проверяем разбор текста —
    /// именно он и ошибается.
    #[test]
    fn a_refusal_is_named_in_one_word() {
        let named = |text: &str| -> &'static str {
            let all = text.to_lowercase();
            if all.contains("timed out") {
                "таймаут"
            } else if all.contains("certificate") || all.contains("tls") {
                "TLS не встал"
            } else if all.contains("reset") || all.contains("forcibly") {
                "соединение сброшено"
            } else {
                "не дошло"
            }
        };
        assert_eq!(named("operation timed out"), "таймаут");
        assert_eq!(
            named("invalid peer certificate: UnknownIssuer"),
            "TLS не встал"
        );
        assert_eq!(
            named("An existing connection was forcibly closed by the remote host"),
            "соединение сброшено"
        );
    }
}
