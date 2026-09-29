//! Что отвечает на имена: кто из резолверов доходит и тот ли адрес возвращает.
//!
//! Две утилиты и один набор примитивов под ними (D-097):
//!
//! - **`dns-race`** — спросить одно имя у всех кандидатов сразу и посмотреть, кто ответил
//!   и за сколько. Это ответ на «какой DNS вообще работает у моего провайдера».
//! - **`dns-spoof`** — сверить обычный DNS с шифрованным. Разошлись адреса — значит
//!   на пути кто-то отвечает вместо резолвера.
//!
//! Кандидаты берутся из коллекции (`collections/dns.yaml`), а не отсюда: список адресов —
//! данные, и меняться он должен файлом.
//!
//! **Сами умеем `udp` и `doh`.** Оба — один и тот же пакет (`wire`), только по разной
//! трубе. `dot`, `doq` и `doh3` требуют TLS и QUIC своими руками — их мы спрашиваем
//! **через ядро** (D-098): стенд поднимает одноразовое mihomo ровно с этим резолвером
//! и отвечает через `/dns/query`. Дороже (запуск процесса на кандидата), зато без двух
//! чужих стеков в клиенте. Нет ядра — такие строки честно помечаются пропущенными,
//! а не исчезают из таблицы.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::collections::Collections;
use crate::collections::Resolvers;
use crate::diag::bench::Bench;
use crate::diag::report::{Report, Row, Tone, Verdict};
use crate::diag::wire;
use crate::diag::wire::DnsWire;
use crate::error::{AppError, Result};
use crate::http::Http;

/// Порт обычного DNS. В коллекции он не пишется — там адрес, а не сокет.
const PLAIN_PORT: u16 = 53;

/// Что спрашиваем, если не сказано иное. Имя нарочно скучное: у него стабильный ответ,
/// его не блокируют, и подмена на нём видна сразу.
pub const DEFAULT_DOMAIN: &str = "example.com";

/// На чём ловим подмену. Первое — контрольное, остальные два в России подменяют чаще
/// прочих, и именно на них видно, что отвечает не резолвер.
pub const SPOOF_DOMAINS: [&str; 3] = ["example.com", "rutracker.org", "www.google.com"];

/// Кто из провайдеров годится в контрольные для сверки: у них есть DoH и они отвечают
/// из России. Порядок — порядок попыток.
const CONTROLS: [&str; 4] = ["google", "cloudflare", "quad9", "adguard"];

/// Чужой публичный резолвер: его в конфиге нет, и спрашиваем мы его нарочно. Ответ
/// подменным адресом означает, что запрос перехватил туннель, а не долетел до Quad9.
///
/// Раньше здесь стоял заведомо мёртвый адрес из TEST-NET-1 — приём красивый, но живой
/// прогон под TUN его опроверг: до перехвата такой пакет не доезжает вовсе, и проба
/// объявляла утечку там, где её не было.
const FOREIGN: &str = "9.9.9.9";

/// Диапазон подменных адресов ядра по умолчанию (`fake-ip-range: 198.18.0.1/16`).
/// Адрес отсюда — это ответ туннеля, а не настоящего резолвера.
const FAKE_IP: [u8; 2] = [198, 18];

/// Один адрес, у которого можно спросить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub provider: String,
    pub variant: String,
    pub filter: String,
    pub proto: String,
    pub addr: String,
}

impl Candidate {
    /// Строка для списка: провайдер и вариант. Адрес рядом отдельной колонкой — он же
    /// уходит в `nameserver:` ядра как есть, без сборки из частей.
    pub fn title(&self) -> String {
        format!("{} · {}", self.provider, self.variant)
    }
}

/// Чем кончился один вопрос одному адресу.
#[derive(Debug, Clone)]
pub struct Shot {
    pub candidate: Candidate,
    /// Сколько шёл ответ. Пусто — ответа не было.
    pub ms: Option<u64>,
    pub ips: Vec<IpAddr>,
    /// Почему не вышло. Пусто — вышло.
    pub error: Option<String>,
    /// Мы этот протокол не умеем: это не отказ сети, а наш предел.
    pub skipped: bool,
}

impl Shot {
    fn failed(candidate: Candidate, error: impl Into<String>) -> Self {
        Self {
            candidate,
            ms: None,
            ips: Vec::new(),
            error: Some(error.into()),
            skipped: false,
        }
    }

    pub fn ok(&self) -> bool {
        self.error.is_none() && !self.ips.is_empty()
    }
}

pub struct DnsProbe;

impl DnsProbe {
    /// Кандидаты из коллекции резолверов.
    ///
    /// По умолчанию — по одному адресу на протокол у **первого** варианта каждого
    /// провайдера: полный перебор это под шестьдесят запросов, а вопрос «какой DNS у меня
    /// работает» решается двумя десятками. `all` включает всё, включая семейные варианты.
    pub fn candidates(resolvers: &Resolvers, all: bool) -> Vec<Candidate> {
        let mut out = Vec::new();
        for provider in &resolvers.providers {
            for (index, variant) in provider.variants.iter().enumerate() {
                if !all && index > 0 {
                    break;
                }
                let mut taken: HashSet<&str> = HashSet::new();
                for server in &variant.servers {
                    // Шестая версия молчит там, где её нет, и таблица наполняется мусором.
                    if server.ipv6 {
                        continue;
                    }
                    if !all && !taken.insert(server.proto.as_str()) {
                        continue;
                    }
                    out.push(Candidate {
                        provider: provider.name.clone(),
                        variant: variant.name.clone(),
                        filter: variant.filter.clone(),
                        proto: server.proto.clone(),
                        addr: server.addr.clone(),
                    });
                }
            }
        }
        out
    }

    /// Спросить один адрес — тем, что умеет сам клиент.
    pub async fn shoot(candidate: &Candidate, domain: &str, timeout: Duration) -> Shot {
        let id = request_id();
        let packet = match DnsWire::query(domain, wire::TYPE_A, id) {
            Ok(packet) => packet,
            Err(error) => return Shot::failed(candidate.clone(), error.to_string()),
        };

        let started = Instant::now();
        let raw = match candidate.proto.as_str() {
            "udp" => tokio::time::timeout(timeout, over_udp(&candidate.addr, &packet)).await,
            "doh" => tokio::time::timeout(timeout, over_https(&candidate.addr, &packet)).await,
            // Сюда не приходят: шифрованные точки идут через стенд (`shoot_via_core`).
            other => {
                return Shot {
                    candidate: candidate.clone(),
                    ms: None,
                    ips: Vec::new(),
                    error: Some(format!("{other} клиент сам не умеет")),
                    skipped: true,
                };
            }
        };
        let ms = started.elapsed().as_millis() as u64;

        let bytes = match raw {
            Err(_) => {
                return Shot::failed(
                    candidate.clone(),
                    format!("таймаут {} мс", timeout.as_millis()),
                )
            }
            Ok(Err(error)) => return Shot::failed(candidate.clone(), error.to_string()),
            Ok(Ok(bytes)) => bytes,
        };

        match DnsWire::answer(&bytes, id) {
            Err(error) => Shot::failed(candidate.clone(), error.to_string()),
            Ok(answer) if answer.rcode != 0 => Shot::failed(
                candidate.clone(),
                format!("резолвер отказал, код {}", answer.rcode),
            ),
            Ok(answer) if answer.ips.is_empty() => {
                Shot::failed(candidate.clone(), "ответ без адресов".to_string())
            }
            Ok(answer) => Shot {
                candidate: candidate.clone(),
                ms: Some(ms),
                ips: answer.ips,
                error: None,
                skipped: false,
            },
        }
    }

    /// Спросить адрес **резолвером ядра** (D-098): поднять стенд с этим `nameserver`
    /// и задать вопрос через `/dns/query`.
    ///
    /// Время считается вместе с запуском ядра — и это честно: столько и стоит спросить
    /// у точки, которую сам клиент не умеет. Сравнивать его с временем обычного запроса
    /// нельзя, и таблица поэтому называет способ отдельной колонкой.
    pub async fn shoot_via_core(candidate: &Candidate, domain: &str, timeout: Duration) -> Shot {
        let started = Instant::now();
        // Ядру нужно встать, а потом ещё ответить: свой таймаут только на запрос был бы
        // вдвое короче, чем ждёт человек.
        let whole = timeout + Duration::from_millis(1200);
        let asked = tokio::time::timeout(whole, async {
            let bench = Bench::start(&candidate.addr).await?;
            bench.resolve(domain).await
        })
        .await;
        let ms = started.elapsed().as_millis() as u64;

        match asked {
            Err(_) => Shot::failed(
                candidate.clone(),
                format!("таймаут {} мс", whole.as_millis()),
            ),
            Ok(Err(error)) => Shot::failed(candidate.clone(), error.to_string()),
            Ok(Ok(addresses)) => {
                let ips: Vec<IpAddr> = addresses.iter().filter_map(|a| a.parse().ok()).collect();
                if ips.is_empty() {
                    return Shot::failed(candidate.clone(), "ответ без адресов".to_string());
                }
                Shot {
                    candidate: candidate.clone(),
                    ms: Some(ms),
                    ips,
                    error: None,
                    skipped: false,
                }
            }
        }
    }

    /// Спросить всех.
    ///
    /// Свои протоколы идут разом — это просто пакеты, и ждать их по очереди значит сложить
    /// таймауты. Шифрованные — тройками: каждый поднимает своё ядро.
    pub async fn race(domain: &str, timeout: Duration, all: bool, core: bool) -> Result<Vec<Shot>> {
        let resolvers = Collections::dns()?;
        let list = DnsProbe::candidates(&resolvers, all);
        let mut set = tokio::task::JoinSet::new();
        let mut queue: Vec<(usize, Candidate)> = Vec::new();
        for (index, candidate) in list.into_iter().enumerate() {
            if ours(&candidate.proto) {
                let domain = domain.to_string();
                set.spawn(
                    async move { (index, DnsProbe::shoot(&candidate, &domain, timeout).await) },
                );
            } else {
                queue.push((index, candidate));
            }
        }

        let mut shots: Vec<(usize, Shot)> = Vec::new();
        while let Some(done) = set.join_next().await {
            match done {
                Ok(pair) => shots.push(pair),
                Err(e) => return Err(AppError::network(format!("Замер сорвался: {e}"))),
            }
        }

        if core {
            for chunk in queue.chunks(BENCHES) {
                let mut benches = tokio::task::JoinSet::new();
                for (index, candidate) in chunk.iter().cloned() {
                    let domain = domain.to_string();
                    benches.spawn(async move {
                        (
                            index,
                            DnsProbe::shoot_via_core(&candidate, &domain, timeout).await,
                        )
                    });
                }
                while let Some(done) = benches.join_next().await {
                    match done {
                        Ok(pair) => shots.push(pair),
                        Err(e) => return Err(AppError::network(format!("Стенд сорвался: {e}"))),
                    }
                }
            }
        } else {
            for (index, candidate) in queue {
                shots.push((
                    index,
                    Shot {
                        candidate,
                        ms: None,
                        ips: Vec::new(),
                        error: Some("шифрованные не мерили — выключено".to_string()),
                        skipped: true,
                    },
                ));
            }
        }

        // Порядок коллекции, а не порядок ответов: таблицу читают глазами, и прыгающие
        // строки в ней хуже, чем ожидание.
        shots.sort_by_key(|(index, _)| *index);
        Ok(shots.into_iter().map(|(_, shot)| shot).collect())
    }

    /// Кого предлагаем прописать: их отмечает звёздочкой отчёт и их же берёт «Умный DNS»
    /// (D-105). Одна функция на оба — отмеченное в таблице и записанное в конфиг обязаны
    /// совпадать, а два одинаковых `sort` однажды разъедутся.
    ///
    /// **Шифрованные вперёд, и только потом быстрые.** Выбор по одной скорости выбрал бы
    /// резолвер провайдера — он всегда ближе всех, — а это ровно тот, кто и подменяет ответы;
    /// раздел с проверкой подмены рядом делал бы это особенно неловко. Внутри каждой половины
    /// порядок по времени ответа, и обычные добираются, только если шифрованных не хватило
    /// (их могли не мерить вовсе — галка «через ядро»).
    pub fn fastest(shots: &[Shot], take: usize) -> Vec<usize> {
        let mut best: Vec<(bool, u64, usize)> = shots
            .iter()
            .enumerate()
            .filter_map(|(index, shot)| {
                shot.ms
                    .filter(|_| shot.ok())
                    .map(|ms| (shot.candidate.proto == "udp", ms, index))
            })
            .collect();
        best.sort_unstable();
        best.into_iter()
            .take(take)
            .map(|(_, _, index)| index)
            .collect()
    }

    pub async fn race_report(
        domain: &str,
        timeout: Duration,
        all: bool,
        core: bool,
    ) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("dns-race");
        report.say(
            Tone::Info,
            format!(
                "dns-race domain={domain} timeout={} мс шифрованные={}",
                timeout.as_millis(),
                if core {
                    "через ядро"
                } else {
                    "мимо"
                }
            ),
        );

        let shots = DnsProbe::race(domain, timeout, all, core).await?;
        report.columns = [
            "Резолвер",
            "Что режет",
            "Транспорт",
            "Адрес",
            "Ответ за",
            "Что вернул",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let marked: HashSet<usize> = DnsProbe::fastest(&shots, BEST).into_iter().collect();

        let mut answered = 0;
        let mut measurable = 0;
        for (index, shot) in shots.iter().enumerate() {
            if !shot.skipped {
                measurable += 1;
            }
            if shot.ok() {
                answered += 1;
            }
            let (verdict, tone, what) = match (&shot.error, shot.skipped) {
                (None, _) => (Verdict::Ok, Tone::Ok, addresses(&shot.ips)),
                (Some(error), false) => (Verdict::Bad, Tone::Bad, error.clone()),
                (Some(error), true) => (Verdict::Idle, Tone::Dim, error.clone()),
            };
            let took = shot
                .ms
                .map(crate::diag::report::Report::millis)
                .unwrap_or_else(|| "—".to_string());
            report.say(
                tone,
                format!(
                    "{:<28} {:<4} {:>8}  {}",
                    clip(&shot.candidate.addr, 28),
                    shot.candidate.proto,
                    took,
                    what
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    shot.candidate.title(),
                    shot.candidate.filter.clone(),
                    shot.candidate.proto.clone(),
                    shot.candidate.addr.clone(),
                    took,
                    what,
                ],
                verdict,
                mark: marked.contains(&index),
            });
        }

        let verdict = if answered == 0 {
            Verdict::Bad
        } else if answered < measurable {
            Verdict::Warn
        } else {
            Verdict::Ok
        };
        Ok(report.finish(
            verdict,
            format!("{answered} из {measurable}"),
            started.elapsed().as_millis() as u64,
        ))
    }

    /// `dns-leak`: кто отвечает на имена — туннель или сеть за ним.
    ///
    /// Признак один и однозначный: **подменный адрес**. Диапазон `198.18.0.0/16` ядро выдаёт
    /// само (`fake-ip`), и получить его от настоящего резолвера невозможно. Спрашиваем двоих:
    ///
    /// - **системного** — того, кого спрашивает вся машина. В TUN ядро прописывает на своём
    ///   адаптере себя, и ответ обязан быть подменным. Это главный вопрос: настоящий адрес
    ///   здесь означает, что имена уходят провайдеру.
    /// - **чужого публичного**, которого в конфиге нет. Его ответ говорит про приложения
    ///   со своим DNS: перехватывает ли туннель и их тоже.
    ///
    /// Заведомо мёртвый адрес на этом месте стоял и снят: живой прогон под TUN показал, что
    /// до перехвата такой пакет не доезжает, и проба врала про утечку при работающем туннеле.
    ///
    /// В режиме Proxy перехвата нет по устройству режима, и «утечкой» это называть нельзя:
    /// там имена и должны разрешаться системой. Проба это говорит, а не молчит.
    pub async fn leak_report(mode: Option<&str>, timeout: Duration) -> Result<Report> {
        DnsProbe::leak_report_with(mode, timeout, &[]).await
    }

    /// Живая проверка может передать resolver физического адаптера, снятый до старта TUN.
    /// После старта системный список уже изменён ядром, и восстановить тот адрес из него нельзя.
    pub async fn leak_report_with(
        mode: Option<&str>,
        timeout: Duration,
        physical: &[String],
    ) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("dns-leak");
        report.columns = ["Кого спросили", "Ответ", "Что это значит"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let Some(mode) = mode else {
            report.say(Tone::Dim, "ядро не запущено — перехватывать нечему");
            return Ok(report.finish(Verdict::Idle, "ядро не запущено", 0));
        };
        let tun = mode.eq_ignore_ascii_case("tun");
        report.say(
            Tone::Info,
            format!("dns-leak mode={mode} чужой резолвер={FOREIGN}"),
        );

        // Системный — первым: он и есть ответ на вопрос.
        let system = crate::system::net::NetInfo::first_resolver();
        let (system_faked, _, system_answer) = match &system {
            Some(addr) => asked(addr, "Системный", DEFAULT_DOMAIN, timeout).await,
            None => (false, false, "система не назвала резолвера".to_string()),
        };
        let system_name = system.clone().unwrap_or_else(|| "—".to_string());
        let system_meaning = match (&system, system_faked) {
            (None, _) => "спросить некого",
            (Some(_), true) => "отвечает туннель",
            (Some(_), false) => "отвечает не туннель",
        };
        report.say(
            if system_faked { Tone::Ok } else { Tone::Bad },
            format!("{system_name:<16} {system_answer:<24} {system_meaning}"),
        );
        report.rows.push(Row {
            cells: vec![
                format!("{system_name} (системный)"),
                system_answer,
                system_meaning.to_string(),
            ],
            verdict: if system_faked {
                Verdict::Ok
            } else {
                Verdict::Bad
            },
            mark: true,
        });

        let mut physical_ok = true;
        for address in physical
            .iter()
            .filter(|address| Some(address.as_str()) != system.as_deref() && *address != FOREIGN)
        {
            let (faked, answered, answer) =
                asked(address, "Физический", DEFAULT_DOMAIN, timeout).await;
            let ok = intercepted_or_blocked(faked, answered);
            physical_ok &= ok;
            let meaning = match (faked, answered) {
                (true, _) => "перехвачен туннелем",
                (false, false) => "заблокирован",
                (false, true) => "ушёл через физический адаптер",
            };
            report.say(
                if ok { Tone::Ok } else { Tone::Bad },
                format!("{address:<16} {answer:<24} {meaning}"),
            );
            report.rows.push(Row {
                cells: vec![format!("{address} (физический)"), answer, meaning.into()],
                verdict: if ok { Verdict::Ok } else { Verdict::Bad },
                mark: true,
            });
        }

        let (foreign_faked, _, foreign_answer) =
            asked(FOREIGN, "Чужой", DEFAULT_DOMAIN, timeout).await;
        let foreign_meaning = if foreign_faked {
            "перехвачен туннелем"
        } else {
            "ушёл к самому резолверу"
        };
        report.say(
            if foreign_faked { Tone::Ok } else { Tone::Warn },
            format!("{FOREIGN:<16} {foreign_answer:<24} {foreign_meaning}"),
        );
        report.rows.push(Row {
            cells: vec![
                format!("{FOREIGN} (чужой)"),
                foreign_answer,
                foreign_meaning.to_string(),
            ],
            verdict: if foreign_faked {
                Verdict::Ok
            } else {
                Verdict::Warn
            },
            mark: false,
        });

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match (tun, system_faked, foreign_faked, physical_ok) {
            (true, true, true, true) => (Verdict::Ok, "имена идут через туннель".to_string()),
            (true, _, _, false) => (
                Verdict::Bad,
                "утечка: DNS физического адаптера обошёл туннель".to_string(),
            ),
            (true, true, false, _) => (
                Verdict::Warn,
                "система через туннель, но приложение со своим DNS уйдёт мимо".to_string(),
            ),
            (true, false, _, _) => (
                Verdict::Bad,
                "утечка: система спрашивает не туннель".to_string(),
            ),
            (false, _, true, _) => (
                Verdict::Warn,
                "в режиме Proxy запросы кто-то перехватывает".to_string(),
            ),
            (false, _, false, _) => (
                Verdict::Idle,
                "режим Proxy: имена разрешает система, перехвата и не должно быть".to_string(),
            ),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// `dns-spoof`: обычный DNS против шифрованного.
    ///
    /// Логика простая и от этого надёжная: контрольный ответ берём по DoH — его на пути
    /// не подменить, не сломав TLS. Всё, что разошлось с ним по обычному порту, — подмена.
    pub async fn spoof_report(domains: &[String], timeout: Duration) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("dns-spoof");
        let resolvers = Collections::dns()?;

        // Контролей **два**, и это не запас прочности. Замерено: `example.com` отвечает
        // одному резолверу адресами Akamai, другому — Cloudflare, и один контроль объявлял
        // вторую половину подменой. Честный ответ — это то, что видят шифрованные точки
        // **вместе**, а не то, что увидела одна.
        let controls: Vec<Candidate> = CONTROLS
            .iter()
            .filter_map(|id| by_id(&resolvers, id, "doh"))
            .take(2)
            .collect();
        if controls.is_empty() {
            return Err(AppError::invalid(
                "В коллекции нет ни одной точки DoH — сверять не с чем".to_string(),
            ));
        }
        // Системный резолвер — первым: его подменяют чаще всех прочих, потому что именно
        // его выдал провайдер и именно его спрашивает вся остальная машина.
        let mut plain: Vec<Candidate> = crate::system::net::NetInfo::first_resolver()
            .map(|addr| Candidate {
                provider: "Системный".into(),
                variant: "от провайдера".into(),
                filter: String::new(),
                proto: "udp".into(),
                addr,
            })
            .into_iter()
            .collect();
        plain.extend(
            CONTROLS
                .iter()
                .filter_map(|id| by_id(&resolvers, id, "udp"))
                .take(2),
        );
        if plain.is_empty() {
            return Err(AppError::invalid(
                "В коллекции нет обычных резолверов — сверять нечего".to_string(),
            ));
        }
        report.say(
            Tone::Info,
            format!(
                "dns-spoof control={} plain={}",
                controls
                    .iter()
                    .map(|c| c.addr.as_str())
                    .collect::<Vec<_>>()
                    .join(" + "),
                plain.len()
            ),
        );

        report.columns = ["Имя", "По DoH", "По обычному DNS", "Резолвер"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut hijacked = 0;
        let mut checked = 0;
        let mut blind = 0;
        for domain in domains {
            let mut truth: Vec<IpAddr> = Vec::new();
            for control in &controls {
                let shot = DnsProbe::shoot(control, domain, timeout).await;
                for ip in shot.ips {
                    if !truth.contains(&ip) {
                        truth.push(ip);
                    }
                }
            }
            if truth.is_empty() {
                blind += 1;
                report.say(
                    Tone::Warn,
                    format!("{domain}: шифрованные точки не ответили — судить не о чем"),
                );
                report.rows.push(Row {
                    cells: vec![domain.clone(), "—".into(), "—".into(), "контроль".into()],
                    verdict: Verdict::Idle,
                    mark: false,
                });
                continue;
            }
            let honest: HashSet<IpAddr> = truth.iter().copied().collect();
            for candidate in &plain {
                let shot = DnsProbe::shoot(candidate, domain, timeout).await;
                checked += 1;
                let verdict = match (&shot.error, same_place(&truth, &shot.ips)) {
                    (Some(_), _) | (_, None) => Verdict::Idle,
                    (None, Some(true)) => Verdict::Ok,
                    (None, Some(false)) => Verdict::Bad,
                };
                if verdict == Verdict::Bad {
                    hijacked += 1;
                }
                let exact = shot.ips.iter().any(|ip| honest.contains(ip));
                let got = match (&shot.error, verdict) {
                    (Some(error), _) => error.clone(),
                    // Разошлись, но в одной сети — это соседний узел площадки, а не подмена.
                    // Молчать об этом нельзя: человек видит разные числа и вправе спросить.
                    (None, Verdict::Ok) if !exact => {
                        format!("{} · тот же сегмент", addresses(&shot.ips))
                    }
                    (None, _) => addresses(&shot.ips),
                };
                report.say(
                    match verdict {
                        Verdict::Bad => Tone::Bad,
                        Verdict::Ok => Tone::Ok,
                        _ => Tone::Dim,
                    },
                    format!(
                        "{:<20} doh={:<16} {}={}",
                        domain,
                        addresses(&truth),
                        candidate.addr,
                        got
                    ),
                );
                report.rows.push(Row {
                    cells: vec![domain.clone(), addresses(&truth), got, candidate.title()],
                    verdict,
                    mark: false,
                });
            }
        }

        let verdict = if checked == 0 {
            Verdict::Idle
        } else if hijacked > 0 {
            Verdict::Bad
        } else if blind > 0 {
            Verdict::Warn
        } else {
            Verdict::Ok
        };
        let headline = match (checked, hijacked) {
            (0, _) => "сверить не удалось".to_string(),
            (_, 0) => "совпадает".to_string(),
            (_, n) => format!("подменяют, {n} из {checked}"),
        };
        Ok(report.finish(verdict, headline, started.elapsed().as_millis() as u64))
    }
}

/// Этот протокол клиент умеет сам — одним пакетом по своей трубе.
fn ours(proto: &str) -> bool {
    matches!(proto, "udp" | "doh")
}

async fn over_udp(addr: &str, packet: &[u8]) -> Result<Vec<u8>> {
    let target = socket_addr(addr)?;
    let bind = if target.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind)
        .await
        .map_err(|e| AppError::network(format!("Не открыть UDP-сокет: {e}")))?;
    socket
        .send_to(packet, target)
        .await
        .map_err(|e| AppError::network(format!("Запрос не ушёл: {e}")))?;
    // 512 байт хватает ответу без EDNS; длиннее нам и не нужно — мы читаем адреса.
    let mut buffer = vec![0u8; 512];
    let read = socket
        .recv(&mut buffer)
        .await
        .map_err(|e| AppError::network(format!("Ответ не пришёл: {e}")))?;
    buffer.truncate(read);
    Ok(buffer)
}

async fn over_https(url: &str, packet: &[u8]) -> Result<Vec<u8>> {
    // Мимо системного прокси: в System им стоит наше ядро, и гонка резолверов
    // мерила бы туннель, а не резолвер.
    let response = Http::direct()?
        .post(url)
        .header("content-type", "application/dns-message")
        .header("accept", "application/dns-message")
        .body(packet.to_vec())
        .send()
        .await
        .map_err(|e| AppError::network(short(&e.to_string())))?;
    if !response.status().is_success() {
        return Err(AppError::network(format!(
            "HTTP {}",
            response.status().as_u16()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::network(format!("Ответ оборвался: {e}")))?;
    Ok(bytes.to_vec())
}

/// Адрес коллекции в сокет: порт там не пишется, потому что это адрес, а не сокет.
fn socket_addr(addr: &str) -> Result<SocketAddr> {
    if let Ok(socket) = addr.parse::<SocketAddr>() {
        return Ok(socket);
    }
    let ip: IpAddr = addr
        .parse()
        .map_err(|_| AppError::invalid(format!("«{addr}» не адрес: обычному DNS нужен IP")))?;
    Ok(SocketAddr::new(ip, PLAIN_PORT))
}

/// Строка ошибки reqwest тянет за собой всю цепочку источников; в консоли нужна суть.
fn short(text: &str) -> String {
    text.split(':')
        .next_back()
        .unwrap_or(text)
        .trim()
        .to_string()
}

fn request_id() -> u16 {
    let mut bytes = [0u8; 2];
    // Случайность здесь не про безопасность, а про то, чтобы не принять чужой пакет;
    // не вышло — берём хоть что-то, лишь бы не постоянную единицу.
    if getrandom::fill(&mut bytes).is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(1);
        bytes = (now as u16).to_be_bytes();
    }
    u16::from_be_bytes(bytes)
}

/// Сколько стендов поднимаем разом. По одному — двенадцать секунд на прогон (замерено);
/// все сразу — десяток процессов по полсотни мегабайт, и меряем уже не сеть, а планировщик
/// Windows. Тройка держит прогон в пяти секундах и не превращается в нагрузку.
const BENCHES: usize = 3;

/// `dns-race` целиком: замер, таблица, вердикт.
/// Сколько резолверов предлагается взять. Три: один — единственная точка отказа,
/// а длинный список ядро опрашивает параллельно и берёт первый ответ, то есть смысла
/// в нём немного.
pub const BEST: usize = 3;

/// Адрес в колонку консоли: у DoH это URL, и полный он растаскивает всю ленту.
fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit - 1).collect::<String>() + "…"
}

fn addresses(ips: &[IpAddr]) -> String {
    ips.iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Адрес из подменного диапазона ядра — то есть ответ туннеля.
fn fake(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.octets()[..2] == FAKE_IP,
        IpAddr::V6(_) => false,
    }
}

/// Спросить один адрес и сказать, туннель ли ответил.
async fn asked(addr: &str, who: &str, domain: &str, timeout: Duration) -> (bool, bool, String) {
    let candidate = Candidate {
        provider: who.into(),
        variant: addr.into(),
        filter: String::new(),
        proto: "udp".into(),
        addr: addr.into(),
    };
    let shot = DnsProbe::shoot(&candidate, domain, timeout).await;
    let faked = shot.ips.iter().any(fake);
    let answered = shot.ok();
    let answer = match (&shot.error, shot.ips.is_empty()) {
        (Some(error), _) => error.clone(),
        (None, true) => "нет ответа".to_string(),
        (None, false) => addresses(&shot.ips),
    };
    (faked, answered, answer)
}

fn intercepted_or_blocked(faked: bool, answered: bool) -> bool {
    faked || !answered
}

/// Тот же ли это адрес — с поправкой на то, что большие сайты живут на CDN.
///
/// Замерено на живом прогоне: `example.com` отдаёт `8.6.112.6` по DoH и `8.6.112.0`
/// по обычному DNS — это **один и тот же кластер**, разные его узлы, и объявлять такое
/// подменой значит кричать на каждый второй сайт. А `rutracker.org` отдаёт `104.21.32.39`
/// по DoH и `188.186.146.207` по 53 — вот это подмена: адрес из чужой сети.
///
/// Граница — сегмент /24 у четвёртой версии и /64 у шестой: внутри него сидят соседние
/// узлы одной площадки, за ним — уже другая.
fn same_place(left: &[IpAddr], right: &[IpAddr]) -> Option<bool> {
    if left.is_empty() || right.is_empty() {
        return None;
    }
    if left.iter().any(|ip| right.contains(ip)) {
        return Some(true);
    }
    Some(
        left.iter()
            .any(|a| right.iter().any(|b| same_segment(*a, *b))),
    )
}

fn same_segment(a: IpAddr, b: IpAddr) -> bool {
    match (a, b) {
        (IpAddr::V4(a), IpAddr::V4(b)) => a.octets()[..3] == b.octets()[..3],
        (IpAddr::V6(a), IpAddr::V6(b)) => a.octets()[..8] == b.octets()[..8],
        _ => false,
    }
}

fn by_id(resolvers: &Resolvers, id: &str, proto: &str) -> Option<Candidate> {
    let provider = resolvers
        .providers
        .iter()
        .find(|provider| provider.id == id)?;
    let variant = provider.variants.first()?;
    let server = variant
        .servers
        .iter()
        .find(|server| server.proto == proto && !server.ipv6)?;
    Some(Candidate {
        provider: provider.name.clone(),
        variant: variant.name.clone(),
        filter: variant.filter.clone(),
        proto: server.proto.clone(),
        addr: server.addr.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolvers() -> Resolvers {
        Collections::dns().expect("коллекция обязана читаться")
    }

    fn shot(proto: &str, ms: Option<u64>) -> Shot {
        Shot {
            candidate: Candidate {
                provider: proto.into(),
                variant: "тест".into(),
                filter: String::new(),
                proto: proto.into(),
                addr: format!("{proto}-адрес"),
            },
            ms,
            ips: vec![IpAddr::from([1, 1, 1, 1])],
            error: ms.is_none().then(|| "нет ответа".to_string()),
            skipped: false,
        }
    }

    /// Главное правило выбора: шифрованный вперёд обычного, даже если обычный быстрее.
    /// Резолвер провайдера всегда ближе всех — и он же тот, кто подменяет ответы.
    #[test]
    fn the_encrypted_ones_are_offered_first() {
        let shots = vec![
            shot("udp", Some(5)),
            shot("doh", Some(80)),
            shot("dot", Some(40)),
        ];
        let picked: Vec<&str> = DnsProbe::fastest(&shots, 2)
            .into_iter()
            .map(|index| shots[index].candidate.proto.as_str())
            .collect();
        assert_eq!(picked, ["dot", "doh"], "быстрый `udp` обошёл шифрованных");
    }

    /// Но не любой ценой: шифрованных могли не мерить вовсе (галка «через ядро»),
    /// и тогда обычные — единственное, что есть.
    #[test]
    fn plain_resolvers_fill_the_rest() {
        let shots = vec![
            shot("udp", Some(9)),
            shot("dot", Some(40)),
            shot("udp", Some(3)),
        ];
        let picked: Vec<u64> = DnsProbe::fastest(&shots, 3)
            .into_iter()
            .map(|index| shots[index].ms.unwrap())
            .collect();
        assert_eq!(picked, [40, 3, 9], "внутри половин порядок по времени");
    }

    /// Не ответивший не предлагается: прописать молчащий резолвер — это выключить имена.
    #[test]
    fn a_silent_resolver_is_never_offered() {
        let shots = vec![shot("dot", None), shot("udp", Some(7))];
        assert_eq!(DnsProbe::fastest(&shots, 3).len(), 1);
    }

    /// Умолчание — не вся коллекция: полный перебор это десятки запросов, а вопрос
    /// решается двумя десятками.
    #[test]
    fn the_default_set_is_narrower_than_the_whole_collection() {
        let resolvers = resolvers();
        let few = DnsProbe::candidates(&resolvers, false);
        let every = DnsProbe::candidates(&resolvers, true);
        assert!(!few.is_empty());
        assert!(
            few.len() < every.len(),
            "{} против {}",
            few.len(),
            every.len()
        );
        assert!(few.len() <= 40, "кандидатов слишком много: {}", few.len());
    }

    /// Адреса шестой версии в замер не идут: на машине без IPv6 они дают ложные отказы.
    #[test]
    fn ipv6_stays_out() {
        let resolvers = resolvers();
        for candidate in DnsProbe::candidates(&resolvers, true) {
            assert!(!candidate.addr.contains("::"), "{}", candidate.addr);
        }
    }

    /// Строка кандидата уходит в конфиг ядра как есть — это и есть смысл коллекции.
    #[test]
    fn a_candidate_is_ready_for_the_core() {
        let resolvers = resolvers();
        let doh = DnsProbe::candidates(&resolvers, true)
            .into_iter()
            .find(|candidate| candidate.proto == "doh")
            .expect("в коллекции нет DoH");
        assert!(doh.addr.starts_with("https://"));
    }

    /// Подменный адрес ядра узнаётся по первым двум байтам: диапазон `198.18.0.0/16`
    /// заведён под тесты производительности (RFC 2544) и в настоящей сети не встречается.
    #[test]
    fn a_fake_address_is_recognised() {
        assert!(fake(&"198.18.0.5".parse().unwrap()));
        assert!(fake(&"198.18.255.255".parse().unwrap()));
        assert!(!fake(&"198.19.0.1".parse().unwrap()));
        assert!(!fake(&"8.8.8.8".parse().unwrap()));
        assert!(!fake(&"2001:4860:4860::8888".parse().unwrap()));
    }

    #[test]
    fn a_physical_resolver_may_be_intercepted_or_blocked_but_not_answer_directly() {
        assert!(intercepted_or_blocked(true, true));
        assert!(intercepted_or_blocked(false, false));
        assert!(!intercepted_or_blocked(false, true));
    }

    /// Без ядра судить не о чем, и проба обязана сказать это, а не выдумать вердикт.
    #[tokio::test]
    async fn without_a_core_the_leak_probe_says_so() {
        let report = DnsProbe::leak_report(None, Duration::from_millis(50))
            .await
            .unwrap();
        assert_eq!(report.verdict, Verdict::Idle);
        assert!(report.headline.contains("не запущено"));
    }

    #[test]
    fn a_bare_ip_gets_the_dns_port() {
        assert_eq!(socket_addr("8.8.8.8").unwrap().port(), 53);
        assert_eq!(socket_addr("8.8.8.8:5353").unwrap().port(), 5353);
        assert!(socket_addr("dns.google").is_err());
    }

    /// Контрольная точка для сверки обязана находиться: без неё `dns-spoof` не запустится.
    #[test]
    fn there_is_a_control_point_for_the_comparison() {
        let resolvers = resolvers();
        let control = by_id(&resolvers, "google", "doh").expect("нет контрольного DoH");
        assert_eq!(control.proto, "doh");
        let plain = by_id(&resolvers, "google", "udp").expect("нет обычного");
        assert_eq!(plain.addr, "8.8.8.8");
    }

    /// Живой прогон 09.09.2026: `example.com` отдал `8.6.112.6` по DoH и `8.6.112.0`
    /// по обычному DNS — это соседние узлы одной площадки, а не подмена. Правило /24
    /// заведено ровно из-за этого случая.
    #[test]
    fn neighbours_in_one_segment_are_not_a_substitution() {
        let doh = ["8.6.112.6".parse().unwrap(), "8.47.69.6".parse().unwrap()];
        let plain = ["8.47.69.0".parse().unwrap(), "8.6.112.0".parse().unwrap()];
        assert_eq!(same_place(&doh, &plain), Some(true));
    }

    /// Тот же прогон: `rutracker.org` по DoH живёт на Cloudflare, а по 53 приходит адрес
    /// российского провайдера. Вот это подмена.
    #[test]
    fn an_address_from_another_network_is_a_substitution() {
        let doh = [
            "172.67.182.196".parse().unwrap(),
            "104.21.32.39".parse().unwrap(),
        ];
        let plain = ["188.186.146.207".parse().unwrap()];
        assert_eq!(same_place(&doh, &plain), Some(false));
    }

    #[test]
    fn without_an_answer_there_is_nothing_to_compare() {
        let doh: [IpAddr; 1] = ["1.2.3.4".parse().unwrap()];
        assert_eq!(same_place(&doh, &[]), None);
        assert_eq!(same_place(&[], &doh), None);
    }

    #[test]
    fn versions_do_not_mix() {
        let four: IpAddr = "8.8.8.8".parse().unwrap();
        let six: IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(!same_segment(four, six));
    }

    /// Кто идёт своей трубой, а кто через стенд. Ошибиться здесь значит либо не померить
    /// половину коллекции, либо поднять ядро ради обычного UDP-пакета.
    #[test]
    fn only_plain_and_https_go_our_own_way() {
        assert!(ours("udp"));
        assert!(ours("doh"));
        for theirs in ["dot", "doq", "doh3", "tcp", "dnscrypt"] {
            assert!(!ours(theirs), "{theirs}");
        }
    }

    /// Незнакомый протокол, дошедший до нашей трубы, обязан попасть в таблицу
    /// пропущенным — а не исчезнуть из неё.
    #[tokio::test]
    async fn an_unknown_protocol_is_skipped_not_hidden() {
        let candidate = Candidate {
            provider: "X".into(),
            variant: "d".into(),
            filter: "none".into(),
            proto: "dnscrypt".into(),
            addr: "sdns://example".into(),
        };
        let shot = DnsProbe::shoot(&candidate, "example.com", Duration::from_millis(50)).await;
        assert!(shot.skipped);
        assert!(!shot.ok());
        assert!(shot.error.unwrap().contains("не умеет"));
    }

    /// Выключенный стенд не прячет строки: шифрованные точки остаются в таблице
    /// с честной пометкой, а не исчезают.
    #[tokio::test]
    async fn without_the_bench_encrypted_points_stay_in_the_table() {
        let shots = DnsProbe::race("example.com", Duration::from_millis(50), false, false)
            .await
            .expect("коллекция обязана читаться");
        let encrypted: Vec<&Shot> = shots
            .iter()
            .filter(|shot| !ours(&shot.candidate.proto))
            .collect();
        assert!(!encrypted.is_empty(), "в коллекции нет шифрованных точек");
        assert!(encrypted.iter().all(|shot| shot.skipped));
    }
}
