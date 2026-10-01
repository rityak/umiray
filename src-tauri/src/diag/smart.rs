//! Замер плюс одна запись (D-105) — «Рекомендованная» мастера (D-162).
//!
//! У каждой функции готовый замер из соседнего модуля и одна запись в «Настройки mihomo»
//! (`advanced.yaml`); у `recommended` замера нет — только рекомендованный конфиг (D-169). Мерим заново при каждом вызове: записать в конфиг вчерашнего
//! победителя — ровно тот случай, когда клиент делает хуже молча.
//!
//! Итог — строка для человека: что записано и чем оно отличается от нейтрального,
//! если отличается (фильтрующий DNS называется фильтрующим).

use std::time::Duration;

use crate::config::advanced::Advanced;
use crate::config::recommended::Recommended;
use crate::diag::dns::{self, Candidate, DnsFilter, DnsProbe};
use crate::diag::pmtu::{self, PmtuProbe};
use crate::diag::report::{Report, Verdict};
use crate::error::{AppError, Result};

/// Сколько ждём резолвер: живой отвечает за десятки миллисекунд, а зарезанный
/// не ответит и за десять секунд.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// До кого меряем MTU: `1.1.1.1` отвечает по ICMP отовсюду и стоит достаточно далеко,
/// чтобы узкое место пути (обычно домашний канал) попало в замер.
const MTU_HOST: &str = "1.1.1.1";

pub struct Smart;

impl Smart {
    /// Подобрать и записать. `filter` — из каких резолверов выбирать DNS.
    pub async fn apply(id: &str, filter: DnsFilter) -> Result<Report> {
        match id {
            "recommended" => recommended(),
            "dns-race" => resolvers(filter).await,
            "pmtu" => mtu(),
            other => Err(AppError::invalid(format!("Подбора «{other}» нет"))),
        }
    }
}

/// Рекомендованный конфиг ядра целиком (D-169) — до замеров: резолверы и MTU, которые
/// они подберут, ложатся уже поверх него.
fn recommended() -> Result<Report> {
    Recommended::apply()?;
    Ok(Report::new(
        "recommended",
        Verdict::Ok,
        "Конфиг ядра: рекомендованный, одинаково подходит для Proxy, System и TUN",
    ))
}

/// Три самых быстрых резолвера — в `dns.nameserver`. Шифрованные вперёд (D-105).
///
/// Пишем через форму «Настройки mihomo» (D-086), а не в файл руками: у поля один хозяин,
/// и он же проверяет остальное.
async fn resolvers(filter: DnsFilter) -> Result<Report> {
    let shots = DnsProbe::race(dns::DEFAULT_DOMAIN, TIMEOUT, filter).await?;
    let picked: Vec<&Candidate> = DnsProbe::fastest(&shots, dns::BEST)
        .into_iter()
        .map(|index| &shots[index].candidate)
        .collect();
    if picked.is_empty() {
        return Ok(Report::new(
            "dns-race",
            Verdict::Bad,
            "DNS не подобран: ни один резолвер не ответил",
        ));
    }

    let mut options = Advanced::read()?;
    options.nameserver = picked
        .iter()
        .map(|candidate| candidate.addr.clone())
        .collect();
    Advanced::write(&options)?;

    let names: Vec<String> = picked.iter().map(|candidate| named(candidate)).collect();
    Ok(Report::new(
        "dns-race",
        Verdict::Ok,
        format!("DNS: {}", names.join(", ")),
    ))
}

/// Резолвер словами: провайдер, протокол и что он режет, если режет.
fn named(candidate: &Candidate) -> String {
    let proto = match candidate.proto.as_str() {
        "dot" => "DoT",
        "doh" => "DoH",
        "doq" => "DoQ",
        "doh3" => "DoH3",
        "udp" | "tcp" => "без шифрования",
        other => other,
    };
    let filter = match candidate.filter.as_str() {
        "ads" => ", блокирует рекламу",
        "security" => ", блокирует опасные сайты",
        "family" => ", семейный фильтр",
        // Comss и похожие отвечают на заблокированное адресами своих серверов: трафик
        // к таким сервисам идёт через чужие прокси, и сказать об этом обязаны.
        "bypass" => ", ведёт заблокированное через свои серверы",
        _ => "",
    };
    format!("{} ({proto}{filter})", candidate.provider)
}

/// MTU пути минус заголовок туннеля — в `tun.mtu`.
///
/// Число — не «сколько держит путь», а сколько остаётся туннелю: записать сюда MTU пути
/// значило бы получить ровно ту фрагментацию, от которой замер и спасает.
fn mtu() -> Result<Report> {
    let Some(path) = PmtuProbe::path(MTU_HOST)? else {
        return Ok(Report::new(
            "pmtu",
            Verdict::Idle,
            format!("MTU не подобран: {MTU_HOST} не отвечает на ping"),
        ));
    };
    let advice = path.saturating_sub(pmtu::TUNNEL);

    let mut options = Advanced::read()?;
    options.mtu = advice;
    Advanced::write(&options)?;

    Ok(Report::new("pmtu", Verdict::Ok, format!("MTU: {advice}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(provider: &str, proto: &str, filter: &str) -> Candidate {
        Candidate {
            provider: provider.into(),
            variant: String::new(),
            filter: filter.into(),
            proto: proto.into(),
            addr: String::new(),
        }
    }

    /// Фильтрующий резолвер называется фильтрующим: мастер не прописывает блокировку
    /// рекламы молча.
    #[test]
    fn a_filtering_resolver_says_what_it_filters() {
        assert_eq!(
            named(&candidate("AdGuard DNS", "dot", "ads")),
            "AdGuard DNS (DoT, блокирует рекламу)"
        );
        assert_eq!(
            named(&candidate("Cloudflare DNS", "doh", "none")),
            "Cloudflare DNS (DoH)"
        );
        assert_eq!(
            named(&candidate("Comss.one DNS", "doh", "bypass")),
            "Comss.one DNS (DoH, ведёт заблокированное через свои серверы)"
        );
    }
}
