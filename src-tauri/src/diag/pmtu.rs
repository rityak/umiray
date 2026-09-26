//! Какой пакет доходит до узла целиком: подбор MTU.
//!
//! Прямое продолжение B-010: туннель WireGuard не встаёт, потому что датаграмма больше,
//! чем принимает та сторона, а MTU в форме «Ядра» до сих пор подбирался наугад. Здесь
//! он подбирается замером.
//!
//! Приём стандартный: ICMP с запретом фрагментации (`DF`) и двоичный поиск по размеру
//! тела. Самый большой прошедший пакет плюс двадцать восемь байт заголовков и есть MTU
//! пути. Прав администратора не нужно — `IcmpSendEcho` работает от пользователя, как
//! и обычный замер задержки.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use crate::diag::report::{Report, Row, Tone, Verdict};
use crate::error::{AppError, Result};

/// Заголовки IPv4 (20) и ICMP (8): их размер не входит в тело, но входит в MTU.
const HEADERS: u32 = 28;

/// Границы поиска. Ниже 576 путь не бывает по стандарту, выше 1500 не бывает Ethernet;
/// туннели живут между 1280 и 1420, и именно туда поиск и приходит.
const LOW: u32 = 576;
const HIGH: u32 = 1500;

const TIMEOUT: Duration = Duration::from_millis(1200);

/// `pmtu`: наибольший пакет, доходящий до адреса целиком.
pub fn measure(host: &str) -> Result<Report> {
    let started = Instant::now();
    let mut report = Report::new("pmtu");
    let address =
        resolve(host).ok_or_else(|| AppError::invalid(format!("«{host}» не адрес и не имя")))?;
    report.say(
        Tone::Info,
        format!("pmtu {host} → {address}, DF, {LOW}…{HIGH}"),
    );
    report.columns = ["Пробовали", "Тело", "Результат"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Сперва убеждаемся, что адрес вообще отвечает: без этого поиск сойдётся в «ничего
    // не проходит» и обвинит канал в том, чего он не делал.
    if !fits(address, LOW - HEADERS) {
        report.say(
            Tone::Bad,
            format!("{LOW} байт не прошли — узел молчит по ICMP"),
        );
        return Ok(report.finish(
            Verdict::Idle,
            "узел не отвечает по ICMP — мерить нечем".to_string(),
            started.elapsed().as_millis() as u64,
        ));
    }

    let mut low = LOW;
    search(address, |size, passed| {
        low = size.max(low);
        report.say(
            if passed { Tone::Ok } else { Tone::Dim },
            format!(
                "{size:>5} байт  тело {:>5}  {}",
                size - HEADERS,
                if passed {
                    "прошло"
                } else {
                    "не прошло"
                }
            ),
        );
        report.rows.push(Row {
            cells: vec![
                size.to_string(),
                (size - HEADERS).to_string(),
                if passed {
                    "прошло"
                } else {
                    "не прошло"
                }
                .to_string(),
            ],
            verdict: if passed { Verdict::Ok } else { Verdict::Idle },
            mark: false,
        });
    });

    let ms = started.elapsed().as_millis() as u64;
    let advice = low.saturating_sub(TUNNEL);
    report.say(
        Tone::Info,
        format!("путь держит {low}; туннелю оставить {advice} — 60 байт уходят под его заголовок"),
    );
    report.rows.push(Row {
        cells: vec![
            "итог".into(),
            advice.to_string(),
            format!("MTU пути {low}, туннелю {advice}"),
        ],
        verdict: Verdict::Ok,
        mark: true,
    });

    let (verdict, headline) = if low >= HIGH {
        (Verdict::Ok, format!("{low} — полный кадр"))
    } else {
        (Verdict::Warn, format!("{low}, туннелю {advice}"))
    };
    Ok(report.finish(verdict, headline, ms))
}

/// Сколько уходит под собственный заголовок туннеля: у WireGuard это 60 байт на IPv4.
pub const TUNNEL: u32 = 60;

/// Наибольший пакет, доходящий до адреса целиком, — типизированный слой под отчётом
/// (D-097). Пусто — узел молчит по ICMP, и мерить нечем.
///
/// Им пользуется и таблица выше, и автоподбор MTU (D-105): второго двоичного поиска
/// в клиенте быть не должно.
pub fn path(host: &str) -> Result<Option<u32>> {
    let address =
        resolve(host).ok_or_else(|| AppError::invalid(format!("«{host}» не адрес и не имя")))?;
    if !fits(address, LOW - HEADERS) {
        return Ok(None);
    }
    let mut low = LOW;
    search(address, |size, passed| {
        if passed {
            low = size.max(low);
        }
    });
    Ok(Some(low))
}

/// Сам поиск: `low` всегда проходит, `high` всегда нет. О каждой пробе рассказывает
/// вызывающему — таблице нужны все шаги, автоподбору только итог.
fn search(address: Ipv4Addr, mut step: impl FnMut(u32, bool)) {
    let (mut low, mut high) = (LOW, HIGH + 1);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let passed = fits(address, middle - HEADERS);
        step(middle, passed);
        if passed {
            low = middle;
        } else {
            high = middle;
        }
    }
}

/// Проходит ли пакет с телом такого размера и запретом фрагментации.
#[cfg(windows)]
fn fits(host: Ipv4Addr, payload: u32) -> bool {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, IP_OPTION_INFORMATION,
    };

    /// Бит «не фрагментировать» в поле флагов. Без него длинный пакет доедет разрезанным,
    /// и проба всегда отвечала бы «проходит».
    const IP_FLAG_DF: u8 = 0x02;

    let body = vec![0u8; payload as usize];
    let mut buffer = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + body.len() + 8];
    let options = IP_OPTION_INFORMATION {
        Ttl: 64,
        Tos: 0,
        Flags: IP_FLAG_DF,
        OptionsSize: 0,
        OptionsData: std::ptr::null_mut(),
    };

    // SAFETY: буферы живут до конца функции, размер ответа не меньше требуемого,
    // дескриптор закрывается на всех путях выхода.
    unsafe {
        let handle = IcmpCreateFile();
        if handle == INVALID_HANDLE_VALUE {
            return false;
        }
        let replies = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(host.octets()),
            body.as_ptr() as *const c_void,
            body.len() as u16,
            &options as *const IP_OPTION_INFORMATION,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
            TIMEOUT.as_millis() as u32,
        );
        IcmpCloseHandle(handle);
        if replies == 0 {
            return false;
        }
        let reply = &*(buffer.as_ptr() as *const ICMP_ECHO_REPLY);
        // 0 — дошло и вернулось. «Нужна фрагментация» приходит отдельным статусом,
        // и он для нас значит ровно «не прошло».
        reply.Status == 0
    }
}

#[cfg(not(windows))]
fn fits(_host: Ipv4Addr, _payload: u32) -> bool {
    false
}

/// Адрес или имя в адрес. Имя разрешаем системой: MTU меряется до конкретного узла,
/// и какой именно из его адресов взят — видно в первой строке отчёта.
fn resolve(host: &str) -> Option<Ipv4Addr> {
    let host = host.trim();
    // Пустое имя система разрешает в петлю (`:80` — это `127.0.0.1:80`), и проба
    // померила бы MTU до самой себя. Замерено — так и вышло.
    if host.is_empty() {
        return None;
    }
    if let Ok(IpAddr::V4(v4)) = host.parse::<IpAddr>() {
        return Some(v4);
    }
    // Порт нужен только для разрешения имени; какой именно — безразлично.
    use std::net::ToSocketAddrs;
    format!("{host}:80")
        .to_socket_addrs()
        .ok()?
        .find_map(|address| match address.ip() {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_stays_an_address() {
        assert_eq!(resolve("1.2.3.4"), Some(Ipv4Addr::new(1, 2, 3, 4)));
        assert_eq!(resolve(" 8.8.8.8 "), Some(Ipv4Addr::new(8, 8, 8, 8)));
    }

    /// Пустая строка адресом не станет.
    ///
    /// Проверяем **только** её: на подменяющем провайдере любое несуществующее имя
    /// разрешается в адрес заглушки, и «мусор не разрешается» там просто неправда.
    /// Замерено на этой же машине — `dns-spoof` ловит ту же подмену.
    #[test]
    fn an_empty_string_is_not_an_address() {
        assert_eq!(resolve(""), None);
        assert_eq!(resolve("   "), None);
    }

    /// Заголовки в MTU входят, а в тело — нет: тело меньше кадра ровно на двадцать
    /// восемь байт. Перепутать их значит промахнуться на столько же — ровно столько,
    /// сколько отделяет рабочий туннель от неработающего.
    #[test]
    fn the_body_is_the_frame_minus_the_headers() {
        assert_eq!(HEADERS, 28);
        assert_eq!(LOW - HEADERS, 548);
        assert_eq!(HIGH - HEADERS, 1472, "столько же шлёт системный ping -f -l");
    }
}
