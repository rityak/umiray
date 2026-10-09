//! Что система думает про сеть: маршруты по умолчанию, свои резолверы, хозяин порта.
//!
//! Linux отвечает файлами `/proc/net` и утилитой `ss` — их вывод от языка не зависит.
//!
//! Читаем командлетами `NetTCPIP`/`DnsClient`, а не `route print` и `ipconfig`: их вывод
//! переведён на язык системы и разбит по ширине консоли, а мы уже обжигались на
//! локализованном выводе (`schtasks`, GOTCHAS). У командлетов значения — данные,
//! одинаковые везде.
//!
//! Модуль **только читает**. Ничего не меняет и не решает: что из прочитанного считать
//! бедой — дело того, кто спросил.

#[cfg(test)]
use std::collections::HashSet;

#[cfg(windows)]
use crate::error::AppError;
use crate::error::Result;

/// Маршрут по умолчанию: чей адаптер и с какой метрикой. Побеждает наименьшая сумма
/// метрик — по ней и видно, идёт ли трафик в туннель или мимо него.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub adapter: String,
    pub gateway: String,
    pub metric: u32,
    /// Номер адаптера в системе: им привязывают сокет к адаптеру (VOLT, D-176). На Linux 0.
    pub index: u32,
}

/// Резолверы, прописанные системой на адаптере.
#[cfg(test)]
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

pub struct NetInfo;

impl NetInfo {
    /// Маршруты по умолчанию — все, а не один: их бывает несколько, и вопрос как раз в том,
    /// чей выиграл.
    #[cfg(windows)]
    pub fn default_routes() -> Result<Vec<Route>> {
        let raw = powershell(
        "Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | \
         ForEach-Object { \"$($_.InterfaceAlias)|$($_.NextHop)|$($_.RouteMetric + $_.InterfaceMetric)|$($_.ifIndex)\" }",
    )?;
        let mut routes = parse_routes(&raw);
        // Побеждает меньшая метрика — сортируем сразу, чтобы читающий не считал сам.
        routes.sort_by_key(|route| route.metric);
        Ok(routes)
    }

    /// DNS поднятых физических адаптеров до появления TUN. Берём оба семейства: IPv6 DNS
    /// может быть единственным путём утечки, даже когда основной маршрут IPv4.
    #[cfg(all(test, windows))]
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

    /// Кто слушает TCP-порт: имя процесса с номером, а если имени не прочитать — один номер
    /// (D-133). Спрашивается только на отказе: PowerShell стоит полсекунды, а удачному
    /// запуску ответ не нужен вовсе.
    #[cfg(windows)]
    pub fn port_owner(port: u16) -> Option<String> {
        let raw = powershell(&format!(
        "Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue | \
         Select-Object -First 1 | ForEach-Object {{ \
         \"$($_.OwningProcess)|$((Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue).ProcessName)\" }}"
    ))
    .ok()?;
        parse_owner(&raw)
    }

    /// Linux: один маршрут — тот, что ОС выберет для адреса в интернете (`ip route get`).
    /// Таблица `main` здесь не отвечает: ядро в TUN уводит трафик правилами `ip rule`
    /// в свою таблицу, а маршрут по умолчанию в `main` остаётся физическим — по нему
    /// исправный туннель выглядел бы обворованным.
    #[cfg(not(windows))]
    pub fn default_routes() -> Result<Vec<Route>> {
        let out = std::process::Command::new("ip")
            .args(["-o", "route", "get", "1.1.1.1"])
            .output()?;
        Ok(chosen_route(&String::from_utf8_lossy(&out.stdout))
            .into_iter()
            .collect())
    }

    #[cfg(not(windows))]
    pub fn port_owner(port: u16) -> Option<String> {
        let out = std::process::Command::new("ss")
            .args(["-Htlnp", &format!("sport = :{port}")])
            .output()
            .ok()?;
        ss_owner(&String::from_utf8_lossy(&out.stdout))
    }
}

/// `1.1.1.1 via 198.18.0.2 dev Meta table 2022 src 198.18.0.1 uid 1000` → адаптер и шлюз.
#[cfg(not(windows))]
fn chosen_route(line: &str) -> Option<Route> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let after = |key: &str| {
        words
            .iter()
            .position(|word| *word == key)
            .and_then(|at| words.get(at + 1))
            .map(|word| (*word).to_string())
    };
    Some(Route {
        adapter: after("dev")?,
        gateway: after("via").unwrap_or_default(),
        metric: 0,
        index: 0,
    })
}

/// `users:(("mihomo",pid=4242,fd=7))` → `mihomo (PID 4242)`. Чужой процесс `ss` без прав
/// не называет — тогда ответа нет: номера без имени тоже нет.
#[cfg(not(windows))]
fn ss_owner(raw: &str) -> Option<String> {
    let users = raw.lines().next()?.split("users:((\"").nth(1)?;
    let (name, rest) = users.split_once('"')?;
    let pid = rest.split("pid=").nth(1)?.split(',').next()?;
    Some(format!("{name} (PID {pid})"))
}

#[cfg(any(windows, test))]
fn parse_owner(raw: &str) -> Option<String> {
    let parts = fields(raw, 2).next()?;
    Some(if parts[1].is_empty() {
        format!("процесс {}", parts[0])
    } else {
        format!("{} (PID {})", parts[1], parts[0])
    })
}

#[cfg(any(windows, test))]
fn parse_routes(raw: &str) -> Vec<Route> {
    fields(raw, 4)
        .filter_map(|parts| {
            Some(Route {
                adapter: parts[0].to_string(),
                gateway: parts[1].to_string(),
                metric: parts[2].parse().ok()?,
                index: parts[3].parse().ok()?,
            })
        })
        .collect()
}

#[cfg(test)]
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
#[cfg(any(windows, test))]
fn fields(raw: &str, count: usize) -> impl Iterator<Item = Vec<&str>> {
    raw.lines().filter_map(move |line| {
        let parts: Vec<&str> = line.trim().split('|').map(str::trim).collect();
        (parts.len() == count && !parts[0].is_empty()).then_some(parts)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Маршрут по умолчанию бывает не один — и выигрывает тот, у кого метрика меньше.
    #[test]
    fn routes_carry_the_metric_that_decides() {
        let list = parse_routes("Ethernet|192.168.1.1|35|12\nMeta|0.0.0.0|4|58\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].adapter, "Meta");
        assert_eq!(list[1].metric, 4);
        assert_eq!((list[0].index, list[1].index), (12, 58));
    }

    /// Строка без числа — не маршрут: пропускаем, а не падаем.
    #[test]
    fn a_broken_line_is_skipped() {
        assert!(parse_routes("Ethernet|192.168.1.1|как-то так|12\n").is_empty());
        assert!(parse_routes("Ethernet|192.168.1.1|35\n").is_empty());
        assert!(parse_routes("одно поле\n").is_empty());
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
    #[cfg(not(windows))]
    fn the_route_the_kernel_would_take_is_named() {
        let tunnel =
            "1.1.1.1 via 198.18.0.2 dev Meta table 2022 src 198.18.0.1 uid 1000 \\    cache \n";
        let route = chosen_route(tunnel).unwrap();
        assert_eq!(
            (route.adapter.as_str(), route.gateway.as_str()),
            ("Meta", "198.18.0.2")
        );
        let direct = "1.1.1.1 via 192.168.232.2 dev ens33 src 192.168.232.129 uid 0 \n";
        assert_eq!(chosen_route(direct).unwrap().adapter, "ens33");
        assert!(chosen_route("").is_none(), "нет ответа — нет и маршрута");
    }

    #[test]
    #[cfg(not(windows))]
    fn ss_names_the_listener() {
        let line = "LISTEN 0 4096 127.0.0.1:7890 0.0.0.0:* users:((\"mihomo\",pid=4242,fd=7))\n";
        assert_eq!(ss_owner(line).as_deref(), Some("mihomo (PID 4242)"));
        assert_eq!(ss_owner(""), None);
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
