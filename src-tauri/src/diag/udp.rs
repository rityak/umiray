//! Ходит ли UDP наружу — и на нестандартных портах тоже.
//!
//! Вопрос не праздный: `hysteria2`, `tuic` и `wireguard` живут на UDP, и там, где его
//! режут, они не встанут вовсе — а выглядеть это будет как «сервер не отвечает». TCP-проба
//! про это не скажет ничего: она молчит у совершенно живого UDP-узла (D-069).
//!
//! Меряем запросом STUN — это двадцать байт заголовка и ответ с нашим внешним адресом.
//! Выбран он потому, что отвечает **любой** публичный сервер, работает на нестандартных
//! портах (19302, 3478) и заодно бесплатно даёт внешний адрес: если он не совпал с тем,
//! что вернул HTTPS, значит UDP и TCP выходят разными путями — а это уже объяснение
//! половине странностей.
//!
//! До самого узла подписки так не постучишься: на чужой UDP-порт никто не обязан
//! отвечать, и молчание там не значит ничего. Узел меряется через ядро (D-069), а эта
//! проба отвечает на предыдущий вопрос — «а UDP вообще выпускают».

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::diag::report::Report;

use crate::diag::report::Row;

use crate::diag::report::Tone;

use crate::diag::report::Verdict;
use crate::error::{AppError, Result};

/// Куда стучимся. Три разных хозяина и три разных порта: если ответит только один,
/// дело не в нём, а в том, что режут остальных.
const SERVERS: [&str; 3] = [
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
    "stun.cloudflare.com:3478",
];

const TIMEOUT: Duration = Duration::from_millis(1500);

/// Постоянная из RFC 5389: по ней ответ отличается от чужого пакета, и ею же
/// «зашифрован» адрес в ответе.
const MAGIC: u32 = 0x2112_A442;

/// Запрос привязки — единственный, который нам нужен.
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;
/// Атрибут с нашим внешним адресом, сложенным по модулю два с постоянной.
const XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Собрать запрос: заголовок в двадцать байт и ничего больше.
fn request(id: [u8; 12]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(20);
    packet.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes()); // длина тела
    packet.extend_from_slice(&MAGIC.to_be_bytes());
    packet.extend_from_slice(&id);
    packet
}

/// Разобрать ответ и достать внешний адрес.
fn mapped(packet: &[u8], id: [u8; 12]) -> Option<SocketAddr> {
    if packet.len() < 20 {
        return None;
    }
    if u16::from_be_bytes([packet[0], packet[1]]) != BINDING_RESPONSE {
        return None;
    }
    if u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]) != MAGIC {
        return None;
    }
    // Чужой ответ — не наш: без этой проверки годится любой пакет, прилетевший на порт.
    if packet[8..20] != id {
        return None;
    }

    let mut at = 20;
    while at + 4 <= packet.len() {
        let kind = u16::from_be_bytes([packet[at], packet[at + 1]]);
        let len = u16::from_be_bytes([packet[at + 2], packet[at + 3]]) as usize;
        let body = at + 4;
        if body + len > packet.len() {
            return None;
        }
        if kind == XOR_MAPPED_ADDRESS && len >= 8 && packet[body + 1] == 0x01 {
            // Порт и адрес сложены с постоянной — иначе их правил бы по дороге NAT.
            let port =
                u16::from_be_bytes([packet[body + 2], packet[body + 3]]) ^ (MAGIC >> 16) as u16;
            let raw = u32::from_be_bytes([
                packet[body + 4],
                packet[body + 5],
                packet[body + 6],
                packet[body + 7],
            ]) ^ MAGIC;
            return Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(raw)), port));
        }
        // Атрибуты выровнены по четыре байта — хвост в счёт длины не входит.
        at = body + len.div_ceil(4) * 4;
    }
    None
}

fn transaction() -> [u8; 12] {
    let mut id = [0u8; 12];
    if getrandom::fill(&mut id).is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        id[..8].copy_from_slice(&now.to_be_bytes());
    }
    id
}

async fn ask(server: &str) -> Result<(SocketAddr, u64)> {
    let id = transaction();
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| AppError::network(format!("сокет не открылся: {e}")))?;
    let started = Instant::now();
    socket
        .send_to(&request(id), server)
        .await
        .map_err(|e| AppError::network(format!("запрос не ушёл: {e}")))?;
    let mut buffer = vec![0u8; 512];
    let read = socket
        .recv(&mut buffer)
        .await
        .map_err(|e| AppError::network(format!("ответа нет: {e}")))?;
    let ms = started.elapsed().as_millis() as u64;
    let address = mapped(&buffer[..read], id)
        .ok_or_else(|| AppError::network("ответ не похож на STUN".to_string()))?;
    Ok((address, ms))
}

pub struct UdpProbe;

impl UdpProbe {
    /// `udp-out`: выпускают ли UDP и каким мы выглядим снаружи по нему.
    pub async fn measure() -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("udp-out");
        report.say(
            Tone::Info,
            format!("udp-out: {} серверов STUN", SERVERS.len()),
        );
        report.columns = ["Сервер", "Ответ за", "Каким видят снаружи"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut answered = 0;
        let mut seen: Option<IpAddr> = None;
        for server in SERVERS {
            let got = tokio::time::timeout(TIMEOUT, ask(server)).await;
            let (verdict, tone, what, took) = match got {
                Ok(Ok((address, ms))) => {
                    answered += 1;
                    seen.get_or_insert(address.ip());
                    (
                        Verdict::Ok,
                        Tone::Ok,
                        address.to_string(),
                        Report::millis(ms),
                    )
                }
                Ok(Err(error)) => (Verdict::Bad, Tone::Bad, error.to_string(), "—".to_string()),
                Err(_) => (
                    Verdict::Bad,
                    Tone::Bad,
                    format!("таймаут {} мс", TIMEOUT.as_millis()),
                    "—".to_string(),
                ),
            };
            report.say(tone, format!("{server:<28} {took:>8}  {what}"));
            report.rows.push(Row {
                cells: vec![server.to_string(), took, what],
                verdict,
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match (answered, seen) {
            (0, _) => (
                Verdict::Bad,
                "UDP наружу не выходит — hysteria2 и wireguard не встанут".to_string(),
            ),
            (n, Some(address)) if n < SERVERS.len() => (
                Verdict::Warn,
                format!("{address}, но ответили {n} из {}", SERVERS.len()),
            ),
            (_, Some(address)) => (Verdict::Ok, format!("ходит, снаружи {address}")),
            (_, None) => (Verdict::Warn, "ответ пришёл, но без адреса".to_string()),
        };
        Ok(report.finish(verdict, headline, ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_twenty_bytes_with_the_magic() {
        let id = [7u8; 12];
        let packet = request(id);
        assert_eq!(packet.len(), 20);
        assert_eq!(&packet[0..2], &[0x00, 0x01]);
        assert_eq!(&packet[4..8], &MAGIC.to_be_bytes());
        assert_eq!(&packet[8..20], &id);
    }

    /// Ответ настоящего сервера: заголовок, атрибут `XOR-MAPPED-ADDRESS` и адрес,
    /// сложенный с постоянной. Собираем его руками — иначе разбор проверять нечем.
    #[test]
    fn an_answer_gives_up_the_outside_address() {
        let id = [1u8; 12];
        let mut packet = Vec::new();
        packet.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
        packet.extend_from_slice(&12u16.to_be_bytes());
        packet.extend_from_slice(&MAGIC.to_be_bytes());
        packet.extend_from_slice(&id);
        packet.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
        packet.extend_from_slice(&8u16.to_be_bytes());
        packet.push(0); // зарезервировано
        packet.push(0x01); // IPv4
        let port: u16 = 54321;
        packet.extend_from_slice(&(port ^ (MAGIC >> 16) as u16).to_be_bytes());
        let address = u32::from(Ipv4Addr::new(176, 212, 23, 145));
        packet.extend_from_slice(&(address ^ MAGIC).to_be_bytes());

        let found = mapped(&packet, id).expect("адрес не разобрался");
        assert_eq!(found.ip(), IpAddr::V4(Ipv4Addr::new(176, 212, 23, 145)));
        assert_eq!(found.port(), port);
    }

    /// Ответ на чужой запрос — не ответ. На открытый UDP-порт прилетает что угодно.
    #[test]
    fn a_foreign_transaction_is_refused() {
        let id = [1u8; 12];
        let mut packet = request(id);
        packet[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes());
        assert!(mapped(&packet, [2u8; 12]).is_none());
    }

    #[test]
    fn rubbish_is_not_an_answer() {
        assert!(mapped(&[], [0u8; 12]).is_none());
        assert!(mapped(&[0u8; 24], [0u8; 12]).is_none());
    }
}
