//! Сколько до сервера: каким способом просили, тем и меряем (D-069).
//!
//! Способов четыре, и живут они в трёх разных местах:
//!
//! - **ICMP и TCP** клиент делает сам (`nodes/ping.rs`) — ядро для них не нужно вовсе;
//! - **через прокси** меряет ядро: узлы провайдера поимённо не адресуются, зато один
//!   вызов проверяет весь источник разом и параллельно;
//! - **через прокси с keep-alive** меряем **своим** соединением через служебный вход
//!   ядра (D-072): только так во втором запросе не остаётся рукопожатия.
//!
//! Модуль отдельный, потому что это разводка, а не состояние: `AppState` о ней не знает
//! ничего, кроме результата, и цикла между ними не возникает.

use crate::core::mihomo::Mihomo;
use crate::error::{AppError, Result};
use crate::nodes::ping::Pinger;
use crate::nodes::ping::{self, Method, Table};
use crate::nodes::Node;

/// Куда сообщать готовые замеры по ходу дела. Нужен только последовательному способу:
/// у него на тридцати узлах десятки секунд, и держать всё до конца значило бы показывать
/// пустую колонку всё это время (S-016).
pub type Progress<'a> = &'a (dyn Fn(&str, ping::Reply) + Sync);

pub struct Measure;

impl Measure {
    pub async fn run(
        nodes: &[Node],
        mihomo: &Mihomo,
        method: Method,
        progress: Progress<'_>,
    ) -> Result<Table> {
        match method {
            Method::Icmp | Method::Tcp => own(nodes, mihomo, method).await,
            Method::Proxy => through_proxy(&nodes.iter().collect::<Vec<_>>(), mihomo).await,
            Method::ProxyKeepalive => through_tunnel(nodes, mihomo, progress).await,
        }
    }
}

/// Свои способы — ICMP и TCP, — но **не для всех**.
///
/// У `hysteria2`, `tuic` и `wireguard` TCP-порта нет вовсе: проверка молчит и у живого
/// узла, и в колонке годами стоял бы прочерк или ICMP до хоста, который про сам туннель
/// не говорит ничего. Такие узлы меряет ядро — если оно запущено; если нет, им достаётся
/// прежний фолбэк, и это честнее, чем пустая колонка.
async fn own(nodes: &[Node], mihomo: &Mihomo, method: Method) -> Result<Table> {
    let (udp, rest): (Vec<&Node>, Vec<&Node>) =
        nodes.iter().partition(|node| Pinger::udp_only(&node.kind));
    let mut table = Pinger::sweep(unique(&rest), method).await;

    if udp.is_empty() {
        return Ok(table);
    }
    if !mihomo.status().running {
        table.extend(Pinger::sweep(unique(&udp), method).await);
        return Ok(table);
    }

    // Замер через ядро может не выйти по своим причинам (узел не заведён провайдером,
    // ядро отказало) — тогда остаётся прежний путь, а не пустая колонка.
    match through_proxy(&udp, mihomo).await {
        Ok(measured) => {
            for (address, mut reply) in measured {
                // Число получено не тем способом, который просили: колонка обязана
                // показывать измеренное и помечать это (D-069).
                reply.fallback = true;
                table.insert(address, reply);
            }
        }
        Err(_) => table.extend(Pinger::sweep(unique(&udp), method).await),
    }
    Ok(table)
}

/// Второй запрос в уже поднятом туннеле — «чистое время ответа» (D-072).
///
/// Служебная группа одна на всех, поэтому замер **последовательный**: навели на узел,
/// померили, навели на следующий. Цена замерена в S-016 — 0.2–0.7 с на живой узел
/// и таймаут на мёртвый, — поэтому каждый результат уходит наружу сразу, а не в конце.
async fn through_tunnel(nodes: &[Node], mihomo: &Mihomo, progress: Progress<'_>) -> Result<Table> {
    let Some(port) = mihomo.probe_port() else {
        return Err(AppError::invalid(
            "Замер через прокси идёт через само ядро — сначала подключитесь.",
        ));
    };

    let mut table = Table::new();
    for node in nodes {
        let Some(address) = node.address.clone() else {
            continue;
        };
        // Один сервер в двух подписках — один замер: наводить группу второй раз незачем.
        if table.contains_key(&address) {
            continue;
        }
        // Узел, которого ядро не завело (D-063), навести нельзя — ему достанется фолбэк.
        if mihomo.probe_select(&node.name).await.is_err() {
            continue;
        }
        let Ok(Some(ms)) = tokio::task::spawn_blocking(move || Pinger::keepalive(port)).await
        else {
            continue;
        };
        let reply = ping::Reply {
            ms,
            method: Method::ProxyKeepalive,
            fallback: false,
        };
        progress(&address, reply);
        table.insert(address, reply);
    }

    // Кто через туннель не отозвался, тот получает ICMP — как и в обычном «через прокси».
    let missing: Vec<String> = addresses(nodes)
        .into_iter()
        .filter(|address| !table.contains_key(address))
        .collect();
    table.extend(Pinger::sweep(missing, Method::ProxyKeepalive).await);
    Ok(table)
}

/// Запрос **через сам узел**, два подхода, лучший из двух.
///
/// Меряет ядро, и только у провайдера целиком: узлы провайдера не адресуются поимённо —
/// `GET /proxies/<узел>/delay` отвечает 404 (S-011, S-012). Зато один вызов меряет весь
/// источник разом и параллельно, поэтому два подхода стоят двух вызовов, а не двух
/// замеров на узел.
///
/// Потолок: узлы, которые клиент кладёт в конфиг сам (D-063), в провайдерах не лежат
/// и здесь замера не получат — им достанется фолбэк. Их путь — `GET /proxies/<имя>/delay`,
/// он для них работает; делать это стоит, когда таких схем станет больше одной.
async fn through_proxy(nodes: &[&Node], mihomo: &Mihomo) -> Result<Table> {
    if !mihomo.status().running {
        return Err(AppError::invalid(
            "Замер через прокси идёт через само ядро — сначала подключитесь.",
        ));
    }

    let mut sources: Vec<&str> = nodes.iter().map(|node| node.source.as_str()).collect();
    sources.sort_unstable();
    sources.dedup();
    for _ in 0..2 {
        for source in &sources {
            mihomo.healthcheck(source).await?;
        }
    }

    let history = mihomo.delays().await?;
    let mut table = Table::new();
    for node in nodes {
        let (Some(address), Some(ms)) = (
            node.address.as_deref(),
            history.get(&node.name).and_then(|seen| best_of_two(seen)),
        ) else {
            continue;
        };
        table.insert(
            address.to_string(),
            ping::Reply {
                ms,
                method: Method::Proxy,
                fallback: false,
            },
        );
    }

    // Кто через прокси не отозвался, тот получает ICMP: «сервер жив, а через него
    // не ходит» и «сервера нет вовсе» — разные новости (D-069). Способ передаём тот же,
    // который просили: прямого замера у него нет, и `sweep` сразу уходит в фолбэк.
    let missing: Vec<String> = unique(nodes)
        .into_iter()
        .filter(|address| !table.contains_key(address))
        .collect();
    table.extend(Pinger::sweep(missing, Method::Proxy).await);
    Ok(table)
}

/// Лучшее из двух последних измерений. Ноль у ядра означает «не ответил», а не «мгновенно».
fn best_of_two(history: &[u32]) -> Option<u32> {
    history
        .iter()
        .rev()
        .take(2)
        .copied()
        .filter(|ms| *ms > 0)
        .min()
}

/// То же самое для подмножества по ссылкам: `Node` не копируется, а мерить приходится
/// то одну половину списка, то другую.
fn unique(nodes: &[&Node]) -> Vec<String> {
    let mut addresses: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.address.clone())
        .collect();
    addresses.sort_unstable();
    addresses.dedup();
    addresses
}

/// Адреса всех узлов, по разу каждый: один сервер может стоять в двух подписках,
/// и мерить его дважды незачем.
fn addresses(nodes: &[Node]) -> Vec<String> {
    let mut addresses: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.address.clone())
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    /// «Два подхода, лучший из двух» — это правило, а не пожелание: берём именно последние
    /// два замера, потому что перед чтением истории мы прогнали ровно две проверки.
    #[test]
    fn the_best_of_the_last_two_wins() {
        assert_eq!(best_of_two(&[10, 900, 300, 120]), Some(120));
        assert_eq!(best_of_two(&[120]), Some(120));
        assert_eq!(best_of_two(&[]), None);
    }

    /// Ноль у ядра — это «не ответил». Пропустить его значило бы объявить мёртвый узел
    /// самым быстрым в таблице.
    #[test]
    fn a_zero_is_silence_and_not_the_fastest_node() {
        assert_eq!(best_of_two(&[0, 0]), None);
        assert_eq!(best_of_two(&[300, 0]), Some(300));
    }
}
