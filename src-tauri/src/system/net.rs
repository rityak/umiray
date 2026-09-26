//! Что Windows думает про сеть: адаптеры, маршруты по умолчанию, свои резолверы.
//!
//! Читаем командлетами `NetTCPIP`/`DnsClient`, а не `route print` и `ipconfig`: их вывод
//! переведён на язык системы и разбит по ширине консоли, а мы уже обжигались на
//! локализованном выводе (`schtasks`, GOTCHAS). У командлетов значения — данные,
//! одинаковые везде.
//!
//! Модуль **только читает**. Ничего не меняет и не решает: что из прочитанного считать
//! бедой — дело диагностики.

#[cfg(test)]
use std::collections::HashSet;

use crate::error::{AppError, Result};

/// Сетевой адаптер так, как его видит система.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub name: String,
    /// Описание драйвера. По нему и узнаются соседи: «WireGuard Tunnel», «TAP-Windows»,
    /// «Hyper-V Virtual Ethernet».
    pub driver: String,
    /// `Up` · `Disconnected` · `Disabled`.
    pub status: String,
}

/// Маршрут по умолчанию: чей адаптер и с какой метрикой. Побеждает наименьшая сумма
/// метрик — по ней и видно, идёт ли трафик в туннель или мимо него.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub adapter: String,
    pub gateway: String,
    pub metric: u32,
}

/// Резолверы, прописанные системой на адаптере.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolvers {
    pub adapter: String,
    pub servers: Vec<String>,
}

/// Запустить PowerShell и вернуть его вывод. Тот же приём, что у брандмауэра
/// (`killswitch::powershell`): свой, потому что модули разные и тащить один через другой
/// значило бы связать защиту с диагностикой.
#[cfg(windows)]
fn powershell(script: &str) -> Result<String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;

    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|why| AppError::io(format!("Не удалось вызвать PowerShell: {why}")))?;
    if !out.status.success() {
        return Err(AppError::io(format!(
            "Система не ответила: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).replace('\r', ""))
}

#[cfg(not(windows))]
fn powershell(_script: &str) -> Result<String> {
    Err(AppError::io("Только Windows".to_string()))
}

pub fn adapters() -> Result<Vec<Adapter>> {
    let raw = powershell(
        "Get-NetAdapter | ForEach-Object { \"$($_.Name)|$($_.InterfaceDescription)|$($_.Status)\" }",
    )?;
    Ok(parse_adapters(&raw))
}

/// Маршруты по умолчанию — все, а не один: их бывает несколько, и вопрос как раз в том,
/// чей выиграл.
pub fn default_routes() -> Result<Vec<Route>> {
    let raw = powershell(
        "Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | \
         ForEach-Object { \"$($_.InterfaceAlias)|$($_.NextHop)|$($_.RouteMetric + $_.InterfaceMetric)\" }",
    )?;
    let mut routes = parse_routes(&raw);
    // Побеждает меньшая метрика — сортируем сразу, чтобы читающий не считал сам.
    routes.sort_by_key(|route| route.metric);
    Ok(routes)
}

/// Резолверы системы — только у поднятых адаптеров с непустым списком: у выключенной
/// сетевой карты он остаётся от прошлой жизни и в диагностике только мешает.
pub fn resolvers() -> Result<Vec<Resolvers>> {
    let raw = powershell(
        "Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | \
         Where-Object { $_.ServerAddresses.Count -gt 0 } | \
         ForEach-Object { \"$($_.InterfaceAlias)|$($_.ServerAddresses -join ',')\" }",
    )?;
    Ok(parse_resolvers(&raw))
}

/// DNS поднятых физических адаптеров до появления TUN. Берём оба семейства: IPv6 DNS
/// может быть единственным путём утечки, даже когда основной маршрут IPv4.
#[cfg(test)]
pub fn physical_resolvers() -> Result<Vec<String>> {
    let raw = powershell(
        "$physical = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | \
         Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty ifIndex); \
         Get-DnsClientServerAddress -ErrorAction SilentlyContinue | \
         Where-Object { $physical -contains $_.InterfaceIndex -and $_.ServerAddresses.Count -gt 0 } | \
         ForEach-Object { \"$($_.InterfaceAlias)|$($_.ServerAddresses -join ',')\" }",
    )?;
    Ok(resolver_servers(&raw))
}

/// Первый резолвер системы: тот, кого спрашивает всё остальное на машине. Именно его
/// чаще всего и подменяют.
pub fn first_resolver() -> Option<String> {
    resolvers()
        .ok()?
        .into_iter()
        .flat_map(|entry| entry.servers)
        // Локальная заглушка (сам ядро в TUN, DNS-прокси роутера на 127.x) отвечает
        // не за провайдера, и сверять с ней подмену бессмысленно.
        .find(|server| !server.starts_with("127."))
}

/// Кто слушает TCP-порт: имя процесса с номером, а если имени не прочитать — один номер
/// (D-133). Спрашивается только на отказе: PowerShell стоит полсекунды, а удачному
/// запуску ответ не нужен вовсе.
pub fn port_owner(port: u16) -> Option<String> {
    let raw = powershell(&format!(
        "Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue | \
         Select-Object -First 1 | ForEach-Object {{ \
         \"$($_.OwningProcess)|$((Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName)\" }}"
    ))
    .ok()?;
    parse_owner(&raw)
}

fn parse_owner(raw: &str) -> Option<String> {
    let parts = fields(raw, 2).next()?;
    Some(if parts[1].is_empty() {
        format!("процесс {}", parts[0])
    } else {
        format!("{} (PID {})", parts[1], parts[0])
    })
}

fn parse_adapters(raw: &str) -> Vec<Adapter> {
    fields(raw, 3)
        .map(|parts| Adapter {
            name: parts[0].to_string(),
            driver: parts[1].to_string(),
            status: parts[2].to_string(),
        })
        .collect()
}

fn parse_routes(raw: &str) -> Vec<Route> {
    fields(raw, 3)
        .filter_map(|parts| {
            Some(Route {
                adapter: parts[0].to_string(),
                gateway: parts[1].to_string(),
                metric: parts[2].parse().ok()?,
            })
        })
        .collect()
}

fn parse_resolvers(raw: &str) -> Vec<Resolvers> {
    fields(raw, 2)
        .map(|parts| Resolvers {
            adapter: parts[0].to_string(),
            servers: parts[1]
                .split(',')
                .map(str::trim)
                .filter(|server| !server.is_empty())
                .map(str::to_string)
                .collect(),
        })
        .collect()
}

#[cfg(test)]
fn resolver_servers(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    parse_resolvers(raw)
        .into_iter()
        .flat_map(|entry| entry.servers)
        .filter(|server| server != "::1" && !server.starts_with("127."))
        .filter(|server| seen.insert(server.clone()))
        .collect()
}

/// Разбор строк вида `a|b|c`. Разделитель — вертикальная черта: в именах адаптеров
/// («Ethernet 2», «Подключение по локальной сети») бывают и пробелы, и запятые,
/// а черты не бывает.
fn fields(raw: &str, count: usize) -> impl Iterator<Item = Vec<&str>> {
    raw.lines().filter_map(move |line| {
        let parts: Vec<&str> = line.trim().split('|').map(str::trim).collect();
        (parts.len() == count && !parts[0].is_empty()).then_some(parts)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Вывод настоящей машины: имена с пробелами, описания с запятыми и скобками.
    #[test]
    fn adapters_survive_spaces_and_commas_in_names() {
        let raw = "Ethernet|Realtek PCIe GbE Family Controller|Up\n\
                   Подключение 2|Hyper-V Virtual Ethernet Adapter, #2|Disconnected\n";
        let list = parse_adapters(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].name, "Подключение 2");
        assert_eq!(list[1].driver, "Hyper-V Virtual Ethernet Adapter, #2");
        assert_eq!(list[1].status, "Disconnected");
    }

    /// Маршрут по умолчанию бывает не один — и выигрывает тот, у кого метрика меньше.
    #[test]
    fn routes_carry_the_metric_that_decides() {
        let list = parse_routes("Ethernet|192.168.1.1|35\nMeta|0.0.0.0|4\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].adapter, "Meta");
        assert_eq!(list[1].metric, 4);
    }

    /// Строка без числа — не маршрут: пропускаем, а не падаем.
    #[test]
    fn a_broken_line_is_skipped() {
        assert!(parse_routes("Ethernet|192.168.1.1|как-то так\n").is_empty());
        assert!(parse_adapters("одно поле\n").is_empty());
    }

    /// Процесс с правами администратора имени не отдаёт — остаётся номер, и это всё ещё
    /// ответ: по нему его находят в диспетчере задач.
    #[test]
    fn a_port_owner_is_named_or_at_least_numbered() {
        assert_eq!(
            parse_owner("12840|v2rayN\n").as_deref(),
            Some("v2rayN (PID 12840)")
        );
        assert_eq!(parse_owner("4|\n").as_deref(), Some("процесс 4"));
        assert_eq!(parse_owner(""), None, "никто не слушает — и сказать нечего");
    }

    #[test]
    fn resolvers_split_by_comma() {
        let list = parse_resolvers(
            "Ethernet|192.168.1.1,8.8.8.8\nEthernet|2001:4860:4860::8888\nMeta|198.18.0.2\n",
        );
        assert_eq!(list[0].servers, vec!["192.168.1.1", "8.8.8.8"]);
        assert_eq!(list[1].servers, vec!["2001:4860:4860::8888"]);
    }

    #[test]
    fn physical_resolvers_keep_both_families_once() {
        let list =
            resolver_servers("Wi-Fi|192.168.1.1,2001:4860:4860::8888\nEthernet|192.168.1.1,::1\n");
        assert_eq!(list, ["192.168.1.1", "2001:4860:4860::8888"]);
    }
}
