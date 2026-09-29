//! Формат DNS-сообщения: собрать запрос, разобрать ответ.
//!
//! Своими руками, без библиотеки, и это дешевле, чем кажется: запрос — это заголовок
//! в двенадцать байт плюс имя по меткам, ответ — те же двенадцать байт плюс записи.
//! Взамен мы получаем **один и тот же пакет** для обычного DNS и для DoH (RFC 8484 —
//! это буквально этот же формат по HTTPS), и ни одной новой зависимости.
//!
//! Разбираем ровно то, что нужно вопросу «а тот ли адрес нам вернули»: код ответа
//! и адреса из записей A и AAAA. Всё прочее (CNAME, SOA, дополнительные секции)
//! пропускается по длине.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::{AppError, Result};

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
const CLASS_IN: u16 = 1;

/// Ответ ядру не отдаётся — это внутренний разбор, поэтому и типы простые.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// 0 — всё хорошо, 3 — имени нет, остальное — отказ резолвера.
    pub rcode: u8,
    pub ips: Vec<IpAddr>,
}

pub struct DnsWire;

impl DnsWire {
    /// Собрать запрос. `id` приходит снаружи: он же проверяется в ответе, и подделать
    /// его посреди сети — единственное, что отличает ответ от чужого пакета.
    pub fn query(name: &str, qtype: u16, id: u16) -> Result<Vec<u8>> {
        let mut packet = Vec::with_capacity(32 + name.len());
        packet.extend_from_slice(&id.to_be_bytes());
        // Рекурсия разрешена: спрашиваем резолвер, а не корень.
        packet.extend_from_slice(&0x0100u16.to_be_bytes());
        packet.extend_from_slice(&1u16.to_be_bytes()); // вопросов
        packet.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // ответов, авторитетных, дополнительных
        for label in name.trim_end_matches('.').split('.') {
            let bytes = label.as_bytes();
            if bytes.is_empty() || bytes.len() > 63 {
                return Err(AppError::invalid(format!(
                    "Имя «{name}» не годится для запроса: метка пустая или длиннее 63 байт"
                )));
            }
            packet.push(bytes.len() as u8);
            packet.extend_from_slice(bytes);
        }
        packet.push(0);
        packet.extend_from_slice(&qtype.to_be_bytes());
        packet.extend_from_slice(&CLASS_IN.to_be_bytes());
        Ok(packet)
    }

    /// Разобрать ответ. `id` — тот, что уходил в запросе.
    pub fn answer(packet: &[u8], id: u16) -> Result<Answer> {
        if packet.len() < 12 {
            return Err(AppError::invalid("Ответ короче заголовка DNS".to_string()));
        }
        if u16::from_be_bytes([packet[0], packet[1]]) != id {
            return Err(AppError::invalid(
                "Ответ пришёл на чужой запрос".to_string(),
            ));
        }
        let rcode = packet[3] & 0x0F;
        let questions = u16::from_be_bytes([packet[4], packet[5]]);
        let answers = u16::from_be_bytes([packet[6], packet[7]]);

        let mut at = 12;
        for _ in 0..questions {
            at = skip_name(packet, at)?;
            at = at
                .checked_add(4)
                .filter(|end| *end <= packet.len())
                .ok_or_else(|| AppError::invalid("Ответ обрывается на вопросе".to_string()))?;
        }

        let mut ips = Vec::new();
        for _ in 0..answers {
            at = skip_name(packet, at)?;
            if at + 10 > packet.len() {
                return Err(AppError::invalid("Ответ обрывается на записи".to_string()));
            }
            let rtype = u16::from_be_bytes([packet[at], packet[at + 1]]);
            let len = u16::from_be_bytes([packet[at + 8], packet[at + 9]]) as usize;
            at += 10;
            if at + len > packet.len() {
                return Err(AppError::invalid(
                    "Запись длиннее, чем сам ответ".to_string(),
                ));
            }
            let data = &packet[at..at + len];
            match (rtype, len) {
                (TYPE_A, 4) => ips.push(IpAddr::V4(Ipv4Addr::new(
                    data[0], data[1], data[2], data[3],
                ))),
                (TYPE_AAAA, 16) => {
                    let mut octets = [0u8; 16];
                    octets.copy_from_slice(data);
                    ips.push(IpAddr::V6(Ipv6Addr::from(octets)));
                }
                // CNAME, SOA и прочее пропускаем по длине: на вопрос «тот ли адрес»
                // они не отвечают.
                _ => {}
            }
            at += len;
        }
        Ok(Answer { rcode, ips })
    }
}

/// Пройти имя и вернуть смещение сразу за ним. Имя бывает сжатым — тогда это указатель
/// в два байта, и дальше читать нечего.
fn skip_name(packet: &[u8], mut at: usize) -> Result<usize> {
    loop {
        let len = *packet
            .get(at)
            .ok_or_else(|| AppError::invalid("Ответ обрывается на имени".to_string()))?;
        if len & 0xC0 == 0xC0 {
            return Ok(at + 2);
        }
        at += 1;
        if len == 0 {
            return Ok(at);
        }
        at += len as usize;
        if at > packet.len() {
            return Err(AppError::invalid("Метка имени длиннее ответа".to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_a_header_and_labels() {
        let packet = DnsWire::query("example.com", TYPE_A, 0xBEEF).unwrap();
        assert_eq!(&packet[0..2], &[0xBE, 0xEF]);
        assert_eq!(&packet[2..4], &[0x01, 0x00], "должна стоять рекурсия");
        assert_eq!(&packet[4..6], &[0x00, 0x01], "ровно один вопрос");
        assert_eq!(&packet[12..20], b"\x07example");
        assert_eq!(&packet[20..24], b"\x03com");
        assert_eq!(packet[24], 0, "имя кончается нулём");
        assert_eq!(&packet[25..], &[0, 1, 0, 1]);
    }

    #[test]
    fn a_long_label_is_refused() {
        let long = "a".repeat(64);
        assert!(DnsWire::query(&long, TYPE_A, 1).is_err());
    }

    /// Ответ с сжатым именем и записью A — ровно то, что приходит от живого резолвера.
    #[test]
    fn an_answer_gives_up_its_addresses() {
        let mut packet = DnsWire::query("example.com", TYPE_A, 0x1234).unwrap();
        packet[3] = 0; // rcode 0
        packet[6] = 0;
        packet[7] = 1; // один ответ
        packet.extend_from_slice(&[0xC0, 0x0C]); // указатель на имя вопроса
        packet.extend_from_slice(&TYPE_A.to_be_bytes());
        packet.extend_from_slice(&CLASS_IN.to_be_bytes());
        packet.extend_from_slice(&300u32.to_be_bytes());
        packet.extend_from_slice(&4u16.to_be_bytes());
        packet.extend_from_slice(&[93, 184, 216, 34]);

        let answer = DnsWire::answer(&packet, 0x1234).unwrap();
        assert_eq!(answer.rcode, 0);
        assert_eq!(
            answer.ips,
            vec![IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))]
        );
    }

    /// Подделанный ответ на чужой запрос — не ответ. Это единственная защита от того,
    /// чтобы принять пакет, который мы не спрашивали.
    #[test]
    fn a_foreign_id_is_refused() {
        let packet = DnsWire::query("example.com", TYPE_A, 1).unwrap();
        assert!(DnsWire::answer(&packet, 2).is_err());
    }

    /// Обрезанный пакет не должен паниковать: сеть присылает и такое.
    #[test]
    fn a_truncated_answer_is_an_error_not_a_panic() {
        let mut packet = DnsWire::query("example.com", TYPE_A, 7).unwrap();
        packet[7] = 1; // обещан ответ, которого нет
        assert!(DnsWire::answer(&packet, 7).is_err());
    }

    #[test]
    fn nxdomain_comes_back_as_a_code() {
        let mut packet = DnsWire::query("nope.example", TYPE_A, 9).unwrap();
        packet[3] = 3;
        let answer = DnsWire::answer(&packet, 9).unwrap();
        assert_eq!(answer.rcode, 3);
        assert!(answer.ips.is_empty());
    }
}
