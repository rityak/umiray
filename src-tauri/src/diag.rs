//! Диагностика: утилиты, у каждой один вопрос и один отчёт (D-097).
//!
//! У раздела две стороны, и обе стоят на этом модуле: «Проверка» гоняет набор утилит
//! и показывает вердикты, «Инструменты» гоняет одну с параметрами и показывает сырой
//! вывод. Разница только в том, что читают из `Report`.
//!
//! **Это база, а не витрина.** Умный выбор DNS, автоподбор MTU, «починить за меня» —
//! всё это берёт готовые результаты отсюда (`dns::race`, `dns::spoof_report`) и
//! добавляет к ним одно действие. Поэтому у утилит два слоя: типизированный замер
//! (`dns::race` возвращает `Vec<Shot>`) и отчёт для окна (`race_report`). Функция,
//! которой нужно решение, а не картинка, берёт первый.
//!
//! Добавить утилиту — это запись в `TOOLS` и ветка в `run`. Реестра с указателями
//! на функции нет намеренно: утилиты асинхронные, и указатель на `async fn` пришлось бы
//! заворачивать в коробку ради единственного места вызова.

mod bench;
pub mod clock;
pub mod config;
pub mod dns;
pub mod matrix;
pub mod pmtu;
/// Публичный, потому что вердикт и тон читают и живые проверки, и пробы в соседних
/// файлах; `Report` при этом вынесен в корень — им пользуется граница с окном.
pub mod report;
pub mod smart;
pub mod speed;
pub mod system;
pub mod tun;
pub mod udp;
pub mod web;
/// Виден всему крейту: формат DNS-пакета нужен и утилитам здесь, и замерам,
/// которые спрашивают слушатель ядра своим запросом (S-021).
pub(crate) mod wire;

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use report::Report;

use crate::error::{AppError, Result};

/// Сколько ждём ответа по умолчанию. Полторы секунды: живой резолвер отвечает
/// за десятки миллисекунд, а зарезанный не ответит и за десять секунд.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Утилита так, как её видит окно.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: &'static str,
    pub title: &'static str,
    /// Куда положить в списке слева в «Инструментах».
    pub group: &'static str,
    /// На какой вопрос отвечает. Одна строка, без «проверяет корректность».
    pub hint: &'static str,
    /// Какие параметры принимает — по ним окно рисует полосу над консолью.
    pub params: &'static [&'static str],
    /// Ходит ли в сеть.
    pub network: bool,
}

pub const TOOLS: [Tool; 17] = [
    Tool {
        id: "dns-race",
        title: "DNS: fastest resolver",
        group: "DNS",
        hint: "Queries public resolvers in parallel to compare responses and latency",
        params: &["domain", "timeout", "all", "core"],
        network: true,
    },
    Tool {
        id: "dns-spoof",
        title: "DNS spoofing",
        group: "DNS",
        hint: "Compares plain and encrypted DNS. Different answers may indicate DNS interception",
        params: &["domains", "timeout"],
        network: true,
    },
    Tool {
        id: "resolvers",
        title: "System DNS",
        group: "DNS",
        hint: "Shows the DNS servers configured in Windows and used by other applications",
        params: &[],
        network: false,
    },
    Tool {
        id: "dns-leak",
        title: "DNS leak",
        group: "DNS",
        hint: "Checks whether DNS requests bypass the tunnel using a nonexistent domain",
        params: &["timeout"],
        network: true,
    },
    Tool {
        id: "external-ip",
        title: "External IP",
        group: "Transport",
        hint: "Compares direct and tunneled public IP addresses. Matching addresses may indicate a VPN bypass",
        params: &[],
        network: true,
    },
    Tool {
        id: "udp-out",
        title: "Outbound UDP",
        group: "Transport",
        hint: "Checks UDP connectivity, required by hysteria2, tuic and wireguard",
        params: &[],
        network: true,
    },
    Tool {
        id: "tls-sni",
        title: "TLS SNI",
        group: "Transport",
        hint: "Checks TLS connections to specific hostnames, which can be blocked independently of their IP addresses",
        params: &["hosts"],
        network: true,
    },
    Tool {
        id: "pmtu",
        title: "Path MTU",
        group: "Transport",
        hint: "Finds the largest packet that reaches its destination without fragmentation",
        params: &["host"],
        network: true,
    },
    Tool {
        id: "speed",
        title: "Speed",
        group: "Shaping",
        hint: "Compares direct and proxied download speeds. Downloads 12 MB per run",
        params: &[],
        network: true,
    },
    Tool {
        id: "sites",
        title: "Site availability",
        group: "Resources",
        hint: "Checks which sites require VPN. Edit the site list in its collection file",
        params: &[],
        network: true,
    },
    Tool {
        id: "firewall",
        title: "Windows Firewall",
        group: "System",
        hint: "Checks whether Windows Firewall is enabled. Kill-switch cannot work without it",
        params: &[],
        network: false,
    },
    Tool {
        id: "sysproxy",
        title: "System proxy",
        group: "System",
        hint: "Shows the Windows proxy settings. Another proxy may divert traffic unexpectedly",
        params: &[],
        network: false,
    },
    Tool {
        id: "clock",
        title: "System clock",
        group: "System",
        hint: "Checks clock drift. VMess and VLESS with AEAD reject connections when the clock differs too much",
        params: &[],
        network: true,
    },
    Tool {
        id: "routes",
        title: "Routes",
        group: "System",
        hint: "Shows the default route and other tunnels. A route with a lower metric can divert traffic",
        params: &[],
        network: false,
    },
    Tool {
        id: "matrix",
        title: "Configuration tuning",
        group: "Config",
        hint: "Compares DNS and sniffer combinations using a separate core instance",
        params: &[],
        network: true,
    },
    Tool {
        id: "tun-stack",
        title: "TUN stack",
        group: "Config",
        hint: "Checks system, gvisor and mixed stacks without capturing routes or interrupting internet access",
        params: &[],
        network: true,
    },
    Tool {
        id: "config-test",
        title: "Config check",
        group: "Config",
        hint: "Checks whether the core accepts the generated config without starting VPN",
        params: &[],
        network: false,
    },
];

/// Параметры запуска. Плоские и все необязательные: утилита без параметров ничего
/// отсюда не читает, а окно шлёт только то, что показало в полосе.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Args {
    pub domain: Option<String>,
    pub domains: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    /// Перебрать всю коллекцию, а не по одному адресу на протокол.
    pub all: bool,
    /// Мерить ли шифрованные точки через стенд (D-098). Умолчание — да: без них
    /// картина неполная, а режут в России как раз их. Каждая стоит запуска ядра,
    /// поэтому выключатель есть.
    pub core: Option<bool>,
    /// Имена для `tls-sni`.
    pub hosts: Option<Vec<String>>,
    /// До кого мерить MTU. Умолчание — адрес выбранного узла, если он есть.
    pub host: Option<String>,
    /// Маршрутизация применённого набора. Приходит **не из окна**, а из состояния
    /// приложения: окно про наборы здесь ничего не знает и знать не должно.
    #[serde(skip)]
    pub rules: Option<String>,
    /// Порт локального прокси работающего ядра — тем же путём, из состояния. Пусто —
    /// ядро не запущено, и пробы «через туннель» честно говорят, что сравнивать не с чем.
    #[serde(skip)]
    pub proxy: Option<u16>,
    /// В каком режиме работает ядро сейчас (`local` или `tun`). Пусто — не работает.
    /// Оттуда же, из состояния: спрашивать окно значило бы верить его копии правды.
    #[serde(skip)]
    pub mode: Option<String>,
}

impl Args {
    fn timeout(&self) -> Duration {
        match self.timeout_ms {
            // Ноль и вечность одинаково бесполезны: держим в разумных границах.
            Some(ms) if (100..=30_000).contains(&ms) => Duration::from_millis(ms),
            _ => TIMEOUT,
        }
    }

    fn domain(&self) -> String {
        match self.domain.as_deref().map(str::trim) {
            Some(domain) if !domain.is_empty() => domain.to_string(),
            _ => dns::DEFAULT_DOMAIN.to_string(),
        }
    }

    /// До кого мерить MTU.
    ///
    /// Умолчание — `1.1.1.1`: он отвечает по ICMP отовсюду и стоит достаточно далеко,
    /// чтобы узкое место пути (обычно это домашний канал, а не последний хоп) в замер
    /// попало. Адрес **выбранного узла** сюда передаёт окно: список узлов с адресами
    /// есть у него, а искать его отсюда значило бы завести вторую копию этого списка.
    fn host(&self) -> String {
        match self.host.as_deref().map(str::trim) {
            Some(host) if !host.is_empty() => host.to_string(),
            _ => "1.1.1.1".to_string(),
        }
    }

    /// Имена для рукопожатия. Умолчание — из коллекции ресурсов: там ровно те адреса,
    /// про которые и спрашивают, и список правится файлом.
    fn hosts(&self) -> Vec<String> {
        if let Some(list) = &self.hosts {
            let named: Vec<String> = list
                .iter()
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty())
                .collect();
            if !named.is_empty() {
                return named;
            }
        }
        crate::collections::sites()
            .map(|sites| {
                sites
                    .sites
                    .iter()
                    .filter_map(|site| host_of(&site.url))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn domains(&self) -> Vec<String> {
        match &self.domains {
            Some(list) if !list.is_empty() => list
                .iter()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect(),
            _ => dns::SPOOF_DOMAINS.iter().map(|d| d.to_string()).collect(),
        }
    }
}

/// Имя хоста из адреса. Своими руками: тащить разбор URL целиком ради одного отрезка
/// между «//» и «/» — та же история, что с DNS-пакетом.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split('/').next()?.split('@').next_back()?;
    (!host.is_empty()).then(|| host.to_string())
}

pub fn tools() -> Vec<Tool> {
    TOOLS.to_vec()
}

/// Что разбирает `run`. Список рядом с самим разбором и сверяется тестом с `TOOLS`:
/// утилита без ветки — это кнопка, которая ничего не делает, и найти её должен тест,
/// а не пользователь. Гонять для этого сами утилиты нельзя — половина из них ходит
/// в сеть и поднимает ядро.
const DISPATCH: [&str; 17] = [
    "dns-race",
    "dns-spoof",
    "dns-leak",
    "external-ip",
    "udp-out",
    "tls-sni",
    "pmtu",
    "speed",
    "sites",
    "resolvers",
    "firewall",
    "sysproxy",
    "routes",
    "clock",
    "matrix",
    "tun-stack",
    "config-test",
];

/// Запустить одну утилиту.
pub async fn run(id: &str, args: Args) -> Result<Report> {
    // Незнакомое имя отбиваем здесь, а не веткой разбора: так список из `DISPATCH`
    // работает на самом деле, а не только в тесте.
    if !DISPATCH.contains(&id) {
        return Err(AppError::invalid(format!("Утилиты «{id}» нет")));
    }
    match id {
        "dns-race" => {
            dns::race_report(
                &args.domain(),
                args.timeout(),
                args.all,
                args.core.unwrap_or(true),
            )
            .await
        }
        "dns-spoof" => dns::spoof_report(&args.domains(), args.timeout()).await,
        "dns-leak" => dns::leak_report(args.mode.as_deref(), args.timeout()).await,
        "external-ip" => web::external(args.proxy).await,
        "udp-out" => udp::measure().await,
        "tls-sni" => web::tls(&args.hosts()).await,
        "pmtu" => pmtu::measure(&args.host()),
        "speed" => speed::measure(args.proxy).await,
        "sites" => web::sites(None).await,
        "resolvers" => system::resolvers(),
        "firewall" => system::firewall(),
        "sysproxy" => system::proxy(),
        "routes" => system::routes(args.mode.as_deref()),
        "clock" => clock::check().await,
        "matrix" => matrix::measure().await,
        "tun-stack" => tun::stacks(args.mode.as_deref()).await,
        "config-test" => config::test(args.rules.as_deref()),
        // Сюда попадает только имя из `DISPATCH` без своей ветки — то есть наша ошибка,
        // а не пользовательская. Тест `the_list_and_the_dispatch_agree` её и ловит.
        other => Err(AppError::invalid(format!(
            "Утилита «{other}» объявлена, но не разбирается"
        ))),
    }
}

/// Стенд наружу — замерам жизненного цикла (S-020): им нужно живое ядро на своих
/// портах. Ниже по файлу намеренно: `#[cfg(test)]` в начале обрезал бы счётчик
/// строк Т4 (`npm run docs`).
#[cfg(test)]
pub use bench::Bench;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_do_not_repeat() {
        let mut seen = std::collections::HashSet::new();
        for tool in TOOLS {
            assert!(seen.insert(tool.id), "дважды {}", tool.id);
            assert!(!tool.title.is_empty());
            assert!(!tool.hint.is_empty());
        }
    }

    /// Список для окна и разбор в `run` обязаны совпадать: утилита, которой нет в разборе,
    /// даёт кнопку, которая ничего не делает, а ветка без записи в списке недостижима.
    #[test]
    fn the_list_and_the_dispatch_agree() {
        let listed: Vec<&str> = TOOLS.iter().map(|tool| tool.id).collect();
        for id in DISPATCH {
            assert!(listed.contains(&id), "{id} разбирается, но не показан");
        }
        for id in listed {
            assert!(DISPATCH.contains(&id), "{id} показан, но не разбирается");
        }
    }

    /// Незнакомое имя — ошибка, а не тишина.
    #[tokio::test]
    async fn an_unknown_tool_is_refused() {
        assert!(run("нет-такой", Args::default()).await.is_err());
    }

    #[test]
    fn defaults_fill_in_what_the_window_did_not_send() {
        let args = Args::default();
        assert_eq!(args.domain(), dns::DEFAULT_DOMAIN);
        assert_eq!(args.domains().len(), dns::SPOOF_DOMAINS.len());
        assert_eq!(args.timeout(), TIMEOUT);
    }

    /// Нулевой и часовой таймаут одинаково бесполезны — оба заменяются умолчанием.
    #[test]
    fn a_silly_timeout_falls_back() {
        let mut args = Args {
            timeout_ms: Some(0),
            ..Args::default()
        };
        assert_eq!(args.timeout(), TIMEOUT);
        args.timeout_ms = Some(3_600_000);
        assert_eq!(args.timeout(), TIMEOUT);
        args.timeout_ms = Some(800);
        assert_eq!(args.timeout(), Duration::from_millis(800));
    }

    /// Имя хоста нужно `tls-sni`, и берётся оно из того же списка ресурсов.
    #[test]
    fn a_host_is_taken_out_of_the_address() {
        assert_eq!(host_of("https://ya.ru").as_deref(), Some("ya.ru"));
        assert_eq!(
            host_of("https://cp.cloudflare.com/generate_204").as_deref(),
            Some("cp.cloudflare.com")
        );
        assert_eq!(host_of("нет-схемы"), None);
    }

    /// Пустой список имён — не список: подставляем коллекцию, а не идём в сеть ни с чем.
    #[test]
    fn hosts_fall_back_to_the_collection() {
        let empty = Args {
            hosts: Some(vec!["  ".into()]),
            ..Args::default()
        };
        assert!(!empty.hosts().is_empty());
        let mine = Args {
            hosts: Some(vec!["example.org".into()]),
            ..Args::default()
        };
        assert_eq!(mine.hosts(), vec!["example.org".to_string()]);
    }

    #[test]
    fn empty_strings_are_not_a_domain() {
        let args = Args {
            domain: Some("   ".into()),
            ..Args::default()
        };
        assert_eq!(args.domain(), dns::DEFAULT_DOMAIN);
    }
}
