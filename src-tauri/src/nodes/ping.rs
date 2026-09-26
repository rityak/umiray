//! Сколько до сервера — своей проверкой, не ядром (D-062).
//!
//! Вопрос, который задают глазами таблице узлов, — «какой сервер ближе», и ответ на него
//! не зависит от того, работает ли ядро. Проверка ядра отвечала на другой вопрос: «дойдёт
//! ли через этот узел запрос до gstatic», и валили её вполне живые серверы.
//!
//! Способ выбирает человек (D-069), потому что ни один не отвечает на все вопросы сразу:
//!
//! - **ICMP** годится любому протоколу, но его режут на половине хостингов;
//! - **TCP-коннект** доказывает, что порт открыт, но `hysteria2`, `tuic` и `wireguard`
//!   живут на UDP, и их TCP-порт молчит у совершенно рабочего сервера;
//! - **через прокси** — единственное, что говорит про сам узел, а не про хост, и меряет
//!   это ядро: своего транспорта до чужого сервера у клиента нет (`app/measure.rs`).
//!
//! Здесь живут те два, которые клиент делает сам, **и общий фолбэк**: выбранный способ
//! промолчал — пробуем ICMP. Такое число помечено `fallback`, и таблица показывает его
//! иначе: «хост жив, а выбранная проверка результата не дала» — это не то же самое,
//! что «сервер не отозвался вовсе».

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::nodes::health;

/// Сколько ждём ответа. Секунда с лишним: дальние серверы отвечают за 300–400 мс,
/// а таблица из тридцати узлов не должна собираться полминуты.
const TIMEOUT: Duration = Duration::from_millis(1200);

/// Сколько ждём в туннеле. Больше обычного: в первый запрос входит рукопожатие с сервером,
/// и у дальнего узла оно доходило до 635 мс (замерено, S-016). Мёртвый узел столько
/// и стоит — своего таймаута ядра мы не ждём.
const TUNNEL: Duration = Duration::from_millis(2500);

/// Куда ходим через туннель — тот же адрес, по которому ядро проверяет живость (D-108).
/// Раньше здесь стояла своя константа, и один узел получал от клиента и от ядра два
/// разных вердикта. 204 без тела — самый дешёвый ответ, какой бывает; замерен в S-016.
fn target() -> (String, String) {
    let url = health::url().unwrap_or_else(|_| health::DEFAULT.to_string());
    let (host, path) = health::split(&url)
        .unwrap_or_else(|| health::split(health::DEFAULT).expect("умолчание обязано разбираться"));
    (host.to_string(), path.to_string())
}

/// Протоколы, у которых нет TCP-порта вовсе: они живут на UDP, и «порт закрыт» про них
/// не значит ничего. TCP-замер у такого узла молчит **всегда** — даже у совершенно
/// живого, — и колонка задержки годами показывала бы прочерк или ICMP до хоста.
///
/// Список по подстроке, а не точным совпадением: имя схемы приходит от парсера ссылки
/// и пишется по-разному («Hysteria2», «hysteria2», «wireguard»).
pub fn udp_only(kind: &str) -> bool {
    const MARKS: [&str; 4] = ["hysteria", "tuic", "wireguard", "juicity"];
    let lower = kind.to_lowercase();
    MARKS.iter().any(|mark| lower.contains(mark))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    Icmp,
    /// Умолчание: TCP не режут, прав он не требует, и заодно доказывает, что порт открыт.
    /// Молчащий порт добирается фолбэком на ICMP, поэтому пустой колонки это не даёт.
    #[default]
    Tcp,
    Proxy,
    ProxyKeepalive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub ms: u32,
    /// Чем **на самом деле** получено число. При фолбэке это всегда `Icmp`, а не то,
    /// что просили: колонка обязана показывать измеренное, а не заказанное.
    pub method: Method,
    /// Сработал запасной способ. Показывается отдельно — цветом и значком (D-069).
    pub fallback: bool,
}

/// Замеры по адресам. Ключ — `host:port` из самой ссылки: имя узла меняется при обновлении
/// подписки, а адрес — то, что мы на самом деле пинговали.
pub type Table = HashMap<String, Reply>;

/// Померить все адреса разом.
///
/// Каждый замер уходит в отдельный поток пула: ICMP и `connect` блокирующие, а тридцать
/// узлов по секунде подряд — это полминуты ожидания вместо одной.
pub async fn sweep(addresses: Vec<String>, method: Method) -> Table {
    let mut tasks = Vec::with_capacity(addresses.len());
    for address in addresses {
        tasks.push(tokio::task::spawn_blocking(move || {
            let reply = measure(&address, method);
            (address, reply)
        }));
    }
    let mut table = Table::new();
    for task in tasks {
        if let Ok((address, Some(reply))) = task.await {
            table.insert(address, reply);
        }
    }
    table
}

/// Один замер выбранным способом; промолчал — фолбэк на ICMP (D-069).
///
/// «Через прокси» прямого замера здесь не имеет: его делает ядро, а сюда такой запрос
/// приходит уже за одним фолбэком — для узлов, которым ядро ничего не намеряло.
fn measure(address: &str, method: Method) -> Option<Reply> {
    let target = resolve(address)?;
    let direct = match method {
        Method::Icmp => icmp(target.ip()),
        Method::Tcp => tcp(target),
        Method::Proxy | Method::ProxyKeepalive => None,
    };
    if let Some(ms) = direct {
        return Some(Reply {
            ms,
            method,
            fallback: false,
        });
    }
    // У самого ICMP фолбэка нет: запасным способом для него был бы он же.
    if method == Method::Icmp {
        return None;
    }
    icmp(target.ip()).map(|ms| Reply {
        ms,
        method: Method::Icmp,
        fallback: true,
    })
}

/// Сколько отвечает узел по **уже поднятому** туннелю (D-072, замерено в S-016).
///
/// Меряем не первый запрос, а второй: в первом лежит рукопожатие с сервером, и оно
/// в разы больше самого ответа — 635 мс против 122 у одного и того же узла. Туннель идёт
/// через служебный вход ядра, а какой узел на том конце — решает вызывающий, наведя
/// служебную группу.
///
/// Здесь нет ни слова про ядро: снаружи это просто локальный прокси на порту.
pub fn keepalive(port: u16) -> Option<u32> {
    let (host, path) = target();
    let entrance = SocketAddr::from(([127, 0, 0, 1], port));
    let mut socket = TcpStream::connect_timeout(&entrance, TUNNEL).ok()?;
    socket.set_read_timeout(Some(TUNNEL)).ok()?;
    socket.set_write_timeout(Some(TUNNEL)).ok()?;
    socket
        .write_all(format!("CONNECT {host}:80 HTTP/1.1\r\nHost: {host}:80\r\n\r\n").as_bytes())
        .ok()?;
    let mut head = [0u8; 128];
    let read = socket.read(&mut head).ok()?;
    if !String::from_utf8_lossy(&head[..read]).contains(" 200 ") {
        return None;
    }
    // Первый запрос поднимает соединение до сервера, второй меряет ответ по нему.
    in_tunnel(&mut socket, &host, &path)?;
    in_tunnel(&mut socket, &host, &path)
}

/// Один запрос в открытом туннеле.
///
/// Ответ дочитываем **до конца заголовков**, а не «сколько дали»: у 204 тела нет, и пустая
/// строка — это ровно один ответ. Одиночный `read` оставлял хвост первого ответа в буфере,
/// и второй замер показывал ноль — не скорость, а остатки (S-016, поймано измерением).
fn in_tunnel(socket: &mut TcpStream, host: &str, path: &str) -> Option<u32> {
    let started = Instant::now();
    socket
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: keep-alive\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut answer: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 256];
    while !answer.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = socket.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        answer.extend_from_slice(&buffer[..read]);
    }
    Some(started.elapsed().as_millis() as u32)
}

/// Адрес ссылки в адрес сокета. Имя хоста разрешается здесь же — блокирующе, мы и так
/// в отдельном потоке.
fn resolve(address: &str) -> Option<SocketAddr> {
    address.to_socket_addrs().ok()?.next()
}

/// Время установления TCP-соединения. Не «работает ли узел», а «сколько до него»:
/// разорванное сразу же соединение — это и есть замер.
fn tcp(target: SocketAddr) -> Option<u32> {
    let started = Instant::now();
    TcpStream::connect_timeout(&target, TIMEOUT).ok()?;
    Some(started.elapsed().as_millis() as u32)
}

/// Обычный ping. Прав администратора не требует: `IcmpSendEcho` работает от пользователя,
/// в отличие от сырого сокета.
#[cfg(windows)]
fn icmp(host: IpAddr) -> Option<u32> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
    };

    // Только IPv4: у шестой версии своя функция и свой формат ответа, а серверы подписок
    // приезжают четвёркой. Шестёрку померит TCP.
    let IpAddr::V4(host) = host else {
        return None;
    };

    // Тело запроса произвольное; 32 байта — то же, что шлёт системный `ping`.
    let payload = [0u8; 32];
    // Ответ кладётся в буфер вместе с телом и служебным хвостом — так требует функция.
    let mut buffer = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 8];

    // SAFETY: буфер живёт до конца функции и по размеру не меньше требуемого, дескриптор
    // закрывается на всех путях выхода.
    unsafe {
        let handle = IcmpCreateFile();
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let replies = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(host.octets()),
            payload.as_ptr() as *const c_void,
            payload.len() as u16,
            std::ptr::null(),
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
            TIMEOUT.as_millis() as u32,
        );
        IcmpCloseHandle(handle);
        if replies == 0 {
            return None;
        }
        let reply = &*(buffer.as_ptr() as *const ICMP_ECHO_REPLY);
        // Статус 0 — дошло и вернулось. Всё прочее (недоступен, время истекло) — молчание:
        // «сервер ответил за 0 мс» было бы враньём.
        (reply.Status == 0).then_some(reply.RoundTripTime)
    }
}

#[cfg(not(windows))]
fn icmp(_host: IpAddr) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кто живёт на UDP: у них TCP-порт молчит и у живого узла тоже.
    #[test]
    fn udp_protocols_are_recognised_whatever_the_spelling() {
        for kind in ["Hysteria2", "hysteria2", "TUIC", "Wireguard", "juicity"] {
            assert!(udp_only(kind), "{kind}");
        }
        for kind in ["Vless", "Trojan", "Shadowsocks", "vmess", ""] {
            assert!(!udp_only(kind), "{kind}");
        }
    }

    /// Мусор вместо адреса — не паника и не ноль. Проверяется `resolve`, а не `measure`:
    /// петля отвечает по ICMP на любой порт, и через неё ветки не различить.
    #[test]
    fn nonsense_is_not_an_address() {
        for address in ["", "не адрес", "1.2.3.4", "host:порт", "1.2.3.4:70000"] {
            assert_eq!(resolve(address), None, "{address}");
        }
    }

    /// Выбранный способ промолчал — число всё равно есть, но помечено запасным (D-069).
    /// Петля отвечает по ICMP на любой порт, поэтому ветка проверяется без сети.
    #[cfg(windows)]
    #[test]
    fn a_silent_port_falls_back_to_icmp_and_says_so() {
        let closed = "127.0.0.1:1";
        let reply = measure(closed, Method::Tcp).expect("петля обязана ответить по ICMP");
        assert_eq!(reply.method, Method::Icmp);
        assert!(reply.fallback, "это запасной способ, а не заказанный");

        let asked = measure("127.0.0.1:1", Method::Icmp).expect("ICMP до петли");
        assert_eq!(asked.method, Method::Icmp);
        assert!(!asked.fallback, "заказанный способ фолбэком не считается");
    }

    /// Свой же слушающий сокет отвечает, а соседний порт — нет. Это и есть вся ветка `tcp`:
    /// замер там — время установления соединения, а не «жив ли узел».
    #[test]
    fn tcp_answers_only_where_someone_listens() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let live = listener.local_addr().unwrap();
        assert!(tcp(live).is_some(), "открытый порт обязан ответить");

        let dead = resolve("127.0.0.1:1").unwrap();
        assert_eq!(tcp(dead), None, "закрытый порт — не замер");
    }
}
