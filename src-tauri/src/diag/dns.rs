//! Какой DNS работает у этого провайдера: спросить одно имя у всех кандидатов сразу
//! и посмотреть, кто ответил и за сколько. Зовёт подбор резолверов мастера (D-105).
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
use crate::diag::wire;
use crate::diag::wire::DnsWire;
use crate::error::{AppError, Result};
use crate::http::Http;

/// Порт обычного DNS. В коллекции он не пишется — там адрес, а не сокет.
const PLAIN_PORT: u16 = 53;

/// Что спрашиваем, если не сказано иное. Имя нарочно скучное: у него стабильный ответ,
/// его не блокируют, и подмена на нём видна сразу.
pub const DEFAULT_DOMAIN: &str = "example.com";

/// Один адрес, у которого можно спросить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub provider: String,
    pub variant: String,
    pub filter: String,
    pub proto: String,
    pub addr: String,
}

/// Из каких резолверов выбирает подбор (D-168, S-033). Слова `filter` коллекции — данные;
/// здесь только то, какие из них человек готов получить.
///
/// `family` и `bypass` не входят ни в одну: семейный фильтр режет взрослое, а обход
/// отвечает на заблокированное адресами чужих серверов — такое ставят руками, не подбором.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DnsFilter {
    /// Отвечают как есть.
    #[default]
    Clean,
    /// Режут рекламу.
    Ads,
    /// Любой из чистых, режущих рекламу и режущих опасное.
    Any,
}

impl DnsFilter {
    fn admits(self, filter: &str) -> bool {
        match self {
            DnsFilter::Clean => filter == "none",
            DnsFilter::Ads => filter == "ads",
            DnsFilter::Any => matches!(filter, "none" | "ads" | "security"),
        }
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
}

impl Shot {
    fn failed(candidate: Candidate, error: impl Into<String>) -> Self {
        Self {
            candidate,
            ms: None,
            ips: Vec::new(),
            error: Some(error.into()),
        }
    }

    pub fn ok(&self) -> bool {
        self.error.is_none() && !self.ips.is_empty()
    }
}

pub struct DnsProbe;

impl DnsProbe {
    /// Кандидаты из коллекции резолверов — те, что подходят под выбранную категорию.
    ///
    /// По одному адресу на протокол у **первого** варианта каждой подходящей категории
    /// провайдера: полный перебор это под сотню запросов, а вопрос «какой DNS у меня
    /// работает» решается несколькими десятками.
    pub fn candidates(resolvers: &Resolvers, filter: DnsFilter) -> Vec<Candidate> {
        let mut out = Vec::new();
        for provider in &resolvers.providers {
            let mut seen: HashSet<&str> = HashSet::new();
            for variant in &provider.variants {
                if !filter.admits(&variant.filter) || !seen.insert(variant.filter.as_str()) {
                    continue;
                }
                let mut taken: HashSet<&str> = HashSet::new();
                let has_doh = variant.servers.iter().any(|server| server.proto == "doh");
                for server in &variant.servers {
                    // Шестая версия молчит там, где её нет, и таблица наполняется мусором.
                    if server.ipv6 {
                        continue;
                    }
                    // Через стенд — только у варианта без DoH: время стенда включает запуск
                    // ядра и у DoH того же провайдера не выигрывает никогда, а стоит секунду.
                    // ponytail: DoT, живой там, где DoH режут, так не найдётся — мерить
                    // стендом вторым кругом, если DoH не ответил ни у кого.
                    if has_doh && !ours(&server.proto) {
                        continue;
                    }
                    if !taken.insert(server.proto.as_str()) {
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
                }
            }
        }
    }

    /// Спросить всех.
    ///
    /// Свои протоколы идут разом — это просто пакеты, и ждать их по очереди значит сложить
    /// таймауты. Шифрованные — тройками: каждый поднимает своё ядро.
    pub async fn race(domain: &str, timeout: Duration, filter: DnsFilter) -> Result<Vec<Shot>> {
        let resolvers = Collections::dns()?;
        let list = DnsProbe::candidates(&resolvers, filter);
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

        // Порядок коллекции, а не порядок ответов: при равном времени выигрывает тот,
        // кто стоит в коллекции раньше, а не тот, чей поток проснулся первым.
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
        // Один провайдер — одно место: DoH, DoT и DoQ одного AdGuard — это одна точка
        // отказа под тремя именами, а не три резолвера.
        let mut providers: HashSet<&str> = HashSet::new();
        best.into_iter()
            .filter(|(_, _, index)| providers.insert(shots[*index].candidate.provider.as_str()))
            .take(take)
            .map(|(_, _, index)| index)
            .collect()
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

/// Сколько резолверов предлагается взять. Ядро спрашивает весь список разом и берёт
/// первый ответ (`batchExchange`), так что четвёртый — это ещё один шанс на быстрый ответ,
/// а не очередь; дальше прибавка уже не окупает лишние запросы (D-169).
pub const BEST: usize = 4;

#[cfg(test)]
mod tests {
    use super::*;

    fn resolvers() -> Resolvers {
        Collections::shipped_dns()
    }

    fn shot(proto: &str, ms: Option<u64>) -> Shot {
        Shot {
            candidate: Candidate {
                // У каждого свой провайдер: двух от одного подбор не берёт (`fastest`).
                provider: format!("{proto}-{ms:?}"),
                variant: "тест".into(),
                filter: String::new(),
                proto: proto.into(),
                addr: format!("{proto}-адрес"),
            },
            ms,
            ips: vec![IpAddr::from([1, 1, 1, 1])],
            error: ms.is_none().then(|| "нет ответа".to_string()),
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

    /// Но не любой ценой: шифрованные могли не ответить, и тогда обычные — единственное,
    /// что есть.
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

    /// Не вся коллекция: полный перебор это десятки запросов, а вопрос решается двумя
    /// десятками.
    #[test]
    fn the_candidates_are_a_short_list() {
        let few = DnsProbe::candidates(&resolvers(), DnsFilter::Any);
        assert!(!few.is_empty());
        assert!(few.len() <= 40, "кандидатов слишком много: {}", few.len());
    }

    /// Каждая категория выбирает, а не берёт всех подряд: хотя бы двое остаются за бортом,
    /// иначе «четыре быстрых» значило бы «все, какие есть» (S-033). Рекламных провайдеров
    /// с шифрованием шесть — запас ровно два.
    #[test]
    fn every_category_has_enough_providers() {
        let resolvers = resolvers();
        for filter in [DnsFilter::Clean, DnsFilter::Ads, DnsFilter::Any] {
            let providers: HashSet<String> = DnsProbe::candidates(&resolvers, filter)
                .into_iter()
                .filter(|candidate| candidate.proto != "udp")
                .map(|candidate| candidate.provider)
                .collect();
            assert!(providers.len() >= BEST + 2, "{filter:?}: {providers:?}");
        }
    }

    /// Категория не протекает: в чистые не попадает режущий рекламу, а семейный
    /// и обход не попадают никуда.
    #[test]
    fn a_category_keeps_to_itself() {
        let resolvers = resolvers();
        let filters = |filter| -> HashSet<String> {
            DnsProbe::candidates(&resolvers, filter)
                .into_iter()
                .map(|candidate| candidate.filter)
                .collect()
        };
        assert_eq!(
            filters(DnsFilter::Clean),
            HashSet::from(["none".to_string()])
        );
        assert_eq!(filters(DnsFilter::Ads), HashSet::from(["ads".to_string()]));
        let any = filters(DnsFilter::Any);
        assert!(
            !any.contains("family") && !any.contains("bypass"),
            "{any:?}"
        );
    }

    /// Три точки одного провайдера — одна точка отказа: подбор берёт следующего.
    #[test]
    fn one_provider_takes_one_place() {
        let mut shots = vec![
            shot("doh", Some(10)),
            shot("dot", Some(20)),
            shot("doq", Some(30)),
        ];
        shots[1].candidate.provider = shots[0].candidate.provider.clone();
        let picked: Vec<u64> = DnsProbe::fastest(&shots, 3)
            .into_iter()
            .map(|index| shots[index].ms.unwrap())
            .collect();
        assert_eq!(picked, [10, 30]);
    }

    /// Адреса шестой версии в замер не идут: на машине без IPv6 они дают ложные отказы.
    #[test]
    fn ipv6_stays_out() {
        let resolvers = resolvers();
        for candidate in DnsProbe::candidates(&resolvers, DnsFilter::Any) {
            assert!(!candidate.addr.contains("::"), "{}", candidate.addr);
        }
    }

    /// Строка кандидата уходит в конфиг ядра как есть — это и есть смысл коллекции.
    #[test]
    fn a_candidate_is_ready_for_the_core() {
        let resolvers = resolvers();
        let doh = DnsProbe::candidates(&resolvers, DnsFilter::Any)
            .into_iter()
            .find(|candidate| candidate.proto == "doh")
            .expect("в коллекции нет DoH");
        assert!(doh.addr.starts_with("https://"));
    }

    #[test]
    fn a_bare_ip_gets_the_dns_port() {
        assert_eq!(socket_addr("8.8.8.8").unwrap().port(), 53);
        assert_eq!(socket_addr("8.8.8.8:5353").unwrap().port(), 5353);
        assert!(socket_addr("dns.google").is_err());
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

    /// Незнакомый протокол, дошедший до нашей трубы, — честный отказ с причиной,
    /// а не тишина и не ответ.
    #[tokio::test]
    async fn an_unknown_protocol_is_refused_with_a_reason() {
        let candidate = Candidate {
            provider: "X".into(),
            variant: "d".into(),
            filter: "none".into(),
            proto: "dnscrypt".into(),
            addr: "sdns://example".into(),
        };
        let shot = DnsProbe::shoot(&candidate, "example.com", Duration::from_millis(50)).await;
        assert!(!shot.ok());
        assert!(shot.error.unwrap().contains("не умеет"));
    }
}
