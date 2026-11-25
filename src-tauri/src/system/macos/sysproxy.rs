//! Системный прокси macOS через `networksetup` для основного сетевого сервиса.
//! Снимок хранит HTTP, HTTPS, SOCKS и исключения — возвращаем их все и не трогаем
//! чужую настройку, появившуюся после нашей.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const BYPASS: &[&str] = &[
    "localhost",
    "127.0.0.1",
    "::1",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub enabled: bool,
    pub server: String,
    #[serde(default)]
    pub ours: String,
    service: String,
    web: ProxyState,
    secure: ProxyState,
    socks: ProxyState,
    bypass: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyState {
    enabled: bool,
    server: String,
    port: String,
}

pub struct ProxySetting;

impl ProxySetting {
    pub fn supported() -> bool {
        service().is_some()
    }

    pub fn is_ours(address: &str) -> bool {
        matches!(ProxySetting::read(), Ok(current) if current.enabled && current.server == address)
    }

    pub fn read() -> Result<Backup> {
        let service = need_service()?;
        let web = read_proxy(&service, "-getwebproxy")?;
        let secure = read_proxy(&service, "-getsecurewebproxy")?;
        let socks = read_proxy(&service, "-getsocksfirewallproxy")?;
        let bypass = run(&["-getproxybypassdomains", &service])?
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty()
                    && *line != "Empty"
                    && !line.starts_with("There aren't any bypass domains set")
            })
            .map(str::to_string)
            .collect();
        Ok(Backup {
            enabled: web.enabled,
            server: format!("{}:{}", web.server, web.port),
            ours: String::new(),
            service,
            web,
            secure,
            socks,
            bypass,
        })
    }

    pub fn apply(address: &str) -> Result<()> {
        let current = ProxySetting::read()?;
        let (host, port) = address
            .rsplit_once(':')
            .ok_or_else(|| AppError::invalid(format!("Адрес прокси без порта: {address}")))?;
        let written = (|| {
            run(&["-setwebproxy", &current.service, host, port])?;
            run(&["-setsecurewebproxy", &current.service, host, port])?;
            run(&["-setsocksfirewallproxy", &current.service, host, port])?;
            let mut bypass = vec!["-setproxybypassdomains", current.service.as_str()];
            bypass.extend(BYPASS);
            run(&bypass)?;
            run(&["-setwebproxystate", &current.service, "on"])?;
            run(&["-setsecurewebproxystate", &current.service, "on"])?;
            run(&["-setsocksfirewallproxystate", &current.service, "on"])?;
            Ok(())
        })();
        if let Err(why) = written {
            let _ = put(&current);
            return Err(why);
        }
        Ok(())
    }

    pub fn restore(previous: &Backup) -> Result<()> {
        if !previous.ours.is_empty() && !ProxySetting::is_ours(&previous.ours) {
            return Ok(());
        }
        put(previous)
    }
}

fn put(previous: &Backup) -> Result<()> {
    run(&["-setwebproxystate", &previous.service, "off"])?;
    run(&["-setsecurewebproxystate", &previous.service, "off"])?;
    run(&["-setsocksfirewallproxystate", &previous.service, "off"])?;
    restore_proxy(
        &previous.service,
        "-setwebproxy",
        "-setwebproxystate",
        &previous.web,
    )?;
    restore_proxy(
        &previous.service,
        "-setsecurewebproxy",
        "-setsecurewebproxystate",
        &previous.secure,
    )?;
    restore_proxy(
        &previous.service,
        "-setsocksfirewallproxy",
        "-setsocksfirewallproxystate",
        &previous.socks,
    )?;
    let mut bypass = vec!["-setproxybypassdomains", previous.service.as_str()];
    if previous.bypass.is_empty() {
        bypass.push("Empty");
    } else {
        bypass.extend(previous.bypass.iter().map(String::as_str));
    }
    run(&bypass)?;
    Ok(())
}

fn restore_proxy(service: &str, set: &str, state: &str, saved: &ProxyState) -> Result<()> {
    if !saved.server.is_empty() && !saved.port.is_empty() {
        run(&[set, service, &saved.server, &saved.port])?;
    }
    run(&[state, service, if saved.enabled { "on" } else { "off" }])?;
    Ok(())
}

fn read_proxy(service: &str, flag: &str) -> Result<ProxyState> {
    let raw = run(&[flag, service])?;
    let value = |name: &str| {
        raw.lines()
            .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix(": "))
            .unwrap_or_default()
            .to_string()
    };
    Ok(ProxyState {
        enabled: value("Enabled") == "Yes",
        server: value("Server"),
        port: value("Port"),
    })
}

fn need_service() -> Result<String> {
    service().ok_or_else(|| AppError::invalid("Не найден основной сетевой сервис macOS"))
}

fn service() -> Option<String> {
    let order = run(&["-listnetworkserviceorder"]).ok()?;
    if let Some(device) = default_device() {
        if let Some(service) = service_for_device(&order, &device) {
            return Some(service);
        }
    }
    // VPN может забрать маршрут в свой utun. Тогда настройки прокси всё равно
    // принадлежат активному физическому сервису, а не вымышленному utun-сервису.
    network_services(&order)
        .into_iter()
        .filter(|(_, device)| !device.is_empty())
        .map(|(name, _)| name)
        .find(|name| {
            run(&["-getinfo", name]).is_ok_and(|info| {
                info.lines().any(|line| {
                    line.trim()
                        .strip_prefix("IP address:")
                        .map(str::trim)
                        .is_some_and(|address| !address.is_empty() && address != "0.0.0.0")
                })
            })
        })
}

fn default_device() -> Option<String> {
    let route = Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    let raw = String::from_utf8_lossy(&route.stdout);
    raw.lines().find_map(|line| {
        line.trim()
            .strip_prefix("interface:")
            .map(str::trim)
            .filter(|device| !device.is_empty())
            .map(str::to_string)
    })
}

fn service_for_device(raw: &str, device: &str) -> Option<String> {
    network_services(raw)
        .into_iter()
        .find(|(_, candidate)| candidate == device)
        .map(|(name, _)| name)
}

fn network_services(raw: &str) -> Vec<(String, String)> {
    let mut services = Vec::new();
    let mut current: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('(') && !line.starts_with("(Hardware Port:") {
            current = line.split_once(") ").map(|(_, name)| name.to_string());
        }
        if line.contains("(Hardware Port:") {
            if let Some(name) = current.take() {
                let device = line
                    .rsplit_once("Device: ")
                    .and_then(|(_, rest)| rest.strip_suffix(')'))
                    .unwrap_or_default()
                    .to_string();
                services.push((name, device));
            }
        }
    }
    services
}

fn run(args: &[&str]) -> Result<String> {
    let out = Command::new(NETWORKSETUP)
        .args(args)
        .output()
        .map_err(|e| AppError::io(format!("networksetup не запустился: {e}")))?;
    if !out.status.success() {
        return Err(AppError::io(format!(
            "networksetup: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_service_is_matched_by_route_device() {
        let raw = "(1) Wi-Fi\n    (Hardware Port: Wi-Fi, Device: en0)\n\n\
                   (2) Thunderbolt Bridge\n    (Hardware Port: Thunderbolt Bridge, Device: bridge0)\n";
        assert_eq!(service_for_device(raw, "en0").as_deref(), Some("Wi-Fi"));
        assert_eq!(service_for_device(raw, "en9"), None);
    }

    #[test]
    fn bypass_keeps_loopback_and_private_networks_direct() {
        assert!(BYPASS.contains(&"127.0.0.1"));
        assert!(BYPASS.contains(&"localhost"));
        assert!(BYPASS.contains(&"192.168.0.0/16"));
    }

    #[test]
    #[ignore]
    fn live_network_proxy_round_trip() {
        let before = ProxySetting::read().unwrap();
        let ours = "127.0.0.1:3090";
        ProxySetting::apply(ours).unwrap();
        assert!(ProxySetting::is_ours(ours));
        ProxySetting::restore(&Backup {
            ours: ours.into(),
            ..before.clone()
        })
        .unwrap();
        let after = ProxySetting::read().unwrap();
        assert_eq!(after.web, before.web);
        assert_eq!(after.secure, before.secure);
        assert_eq!(after.socks, before.socks);
        assert_eq!(after.bypass, before.bypass);
    }
}
