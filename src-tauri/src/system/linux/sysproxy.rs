//! Системный прокси на Linux: настройки рабочего стола, которые читают браузеры и GLib/KIO
//! (D-047, D-173). Единого места, как реестр Windows, здесь нет: GNOME (и всё на его
//! настройках — Cinnamon, Budgie) держит прокси в `gsettings`, KDE — в `kioslaverc`.
//! Другие окружения прокси не объявляют — там возможность выключена (D-174).
//!
//! Правило то же, что на Windows: помним, что стояло до нас, возвращаем именно это
//! и чужое не трогаем.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Мимо прокси — локальные адреса: окну нужен `external-controller` на 127.0.0.1.
const BYPASS: &[&str] = &[
    "localhost",
    "127.0.0.0/8",
    "::1",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
];

/// Ключи GNOME, которые мы меняем, — их же и запоминаем.
const GNOME: &[(&str, &str)] = &[
    ("org.gnome.system.proxy", "mode"),
    ("org.gnome.system.proxy", "ignore-hosts"),
    ("org.gnome.system.proxy.http", "host"),
    ("org.gnome.system.proxy.http", "port"),
    ("org.gnome.system.proxy.https", "host"),
    ("org.gnome.system.proxy.https", "port"),
    ("org.gnome.system.proxy.socks", "host"),
    ("org.gnome.system.proxy.socks", "port"),
];

/// Ключи KDE в `kioslaverc`, группа `Proxy Settings`.
const KDE: &[&str] = &[
    "ProxyType",
    "httpProxy",
    "httpsProxy",
    "socksProxy",
    "NoProxyFor",
];

/// Что стояло до нас и что записали мы (как на Windows).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// Прокси прописан вручную.
    pub enabled: bool,
    /// Адрес HTTP-прокси `хост:порт`.
    pub server: String,
    /// Адрес, который записали мы.
    #[serde(default)]
    pub ours: String,
    /// Значения ключей до нас, как их отдаёт окружение, — ими же и возвращаем.
    #[serde(default)]
    pub saved: Vec<(String, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Desktop {
    Gnome,
    Kde,
}

pub struct ProxySetting;

impl ProxySetting {
    /// Умеет ли это окружение системный прокси вообще (D-174).
    pub fn supported() -> bool {
        desktop().is_some()
    }

    pub fn is_ours(address: &str) -> bool {
        matches!(ProxySetting::read(), Ok(current) if current.enabled && current.server == address)
    }

    pub fn read() -> Result<Backup> {
        match need_desktop()? {
            Desktop::Gnome => {
                // Одним вызовом: статус спрашивает это каждые полторы секунды.
                let saved =
                    gnome_values(&gsettings(&["list-recursively", "org.gnome.system.proxy"])?);
                let value = |key: &str| {
                    saved
                        .iter()
                        .find(|(name, _)| name == key)
                        .map(|(_, value)| value.trim_matches('\'').to_string())
                        .unwrap_or_default()
                };
                Ok(Backup {
                    enabled: value("org.gnome.system.proxy mode") == "manual",
                    server: format!(
                        "{}:{}",
                        value("org.gnome.system.proxy.http host"),
                        value("org.gnome.system.proxy.http port")
                    ),
                    ours: String::new(),
                    saved,
                })
            }
            Desktop::Kde => {
                let saved = KDE
                    .iter()
                    .map(|key| Ok((key.to_string(), kde_get(key)?)))
                    .collect::<Result<Vec<_>>>()?;
                let value = |key: &str| {
                    saved
                        .iter()
                        .find(|(name, _)| name == key)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default()
                };
                Ok(Backup {
                    enabled: value("ProxyType") == "1",
                    server: kde_address(&value("httpProxy")),
                    ours: String::new(),
                    saved,
                })
            }
        }
    }

    /// Записать наш адрес. Сорвалось на полпути — возвращаем, что было.
    pub fn apply(address: &str) -> Result<()> {
        let current = ProxySetting::read()?;
        let (host, port) = address
            .rsplit_once(':')
            .ok_or_else(|| AppError::invalid(format!("Адрес прокси без порта: {address}")))?;
        let written = match need_desktop()? {
            Desktop::Gnome => write_gnome(host, port),
            Desktop::Kde => write_kde(host, port),
        };
        if let Err(why) = written {
            let _ = put(&current);
            return Err(why);
        }
        Ok(())
    }

    /// Вернуть как было. Чужое не трогаем: адрес уже не наш — значит прокси сменил
    /// кто-то другой, и его свежую настройку нашей несвежей затирать нельзя.
    pub fn restore(previous: &Backup) -> Result<()> {
        if !previous.ours.is_empty() && !ProxySetting::is_ours(&previous.ours) {
            return Ok(());
        }
        put(previous)
    }
}

/// Окружение не меняется за жизнь процесса — узнаём один раз.
fn desktop() -> Option<Desktop> {
    static FOUND: std::sync::OnceLock<Option<Desktop>> = std::sync::OnceLock::new();
    *FOUND.get_or_init(detect)
}

fn detect() -> Option<Desktop> {
    let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if current
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"))
        && kde_tool("kwriteconfig").is_some()
    {
        return Some(Desktop::Kde);
    }
    gsettings(&["get", "org.gnome.system.proxy", "mode"])
        .is_ok()
        .then_some(Desktop::Gnome)
}

fn need_desktop() -> Result<Desktop> {
    desktop().ok_or_else(|| {
        AppError::invalid("Это окружение рабочего стола не объявляет системный прокси")
    })
}

fn write_gnome(host: &str, port: &str) -> Result<()> {
    let bypass = BYPASS
        .iter()
        .map(|host| format!("'{host}'"))
        .collect::<Vec<_>>()
        .join(", ");
    for schema in [
        "org.gnome.system.proxy.http",
        "org.gnome.system.proxy.https",
        "org.gnome.system.proxy.socks",
    ] {
        gsettings(&["set", schema, "host", host])?;
        gsettings(&["set", schema, "port", port])?;
    }
    gsettings(&[
        "set",
        "org.gnome.system.proxy",
        "ignore-hosts",
        &format!("[{bypass}]"),
    ])?;
    // Режим — последним: включаем, когда адрес уже на месте.
    gsettings(&["set", "org.gnome.system.proxy", "mode", "manual"])?;
    Ok(())
}

fn write_kde(host: &str, port: &str) -> Result<()> {
    kde_set("httpProxy", &format!("http://{host} {port}"))?;
    kde_set("httpsProxy", &format!("http://{host} {port}"))?;
    kde_set("socksProxy", &format!("socks://{host} {port}"))?;
    kde_set("NoProxyFor", &BYPASS.join(","))?;
    kde_set("ProxyType", "1")?;
    kde_notify();
    Ok(())
}

fn put(previous: &Backup) -> Result<()> {
    match need_desktop()? {
        Desktop::Gnome => {
            // Режим — первым: выключаем раньше, чем меняем адрес под ним.
            let mut saved = previous.saved.clone();
            saved.sort_by_key(|(name, _)| !name.ends_with(" mode"));
            for (name, value) in &saved {
                if let Some((schema, key)) = name.split_once(' ') {
                    gsettings(&["set", schema, key, value])?;
                }
            }
        }
        Desktop::Kde => {
            // Только свои ключи: снимок мог остаться от GNOME на той же машине, и его ключи
            // в `kioslaverc` были бы мусором.
            for (key, value) in previous
                .saved
                .iter()
                .filter(|(key, _)| KDE.contains(&key.as_str()))
            {
                kde_set(key, value)?;
            }
            kde_notify();
        }
    }
    Ok(())
}

fn gsettings(args: &[&str]) -> Result<String> {
    run("gsettings", args)
}

/// Строки `схема ключ значение` — только наши ключи.
fn gnome_values(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            let (schema, key, value) = (parts.next()?, parts.next()?, parts.next()?);
            GNOME
                .contains(&(schema, key))
                .then(|| (format!("{schema} {key}"), value.to_string()))
        })
        .collect()
}

/// `kwriteconfig6` в Plasma 6, `kwriteconfig5` — в пятой.
fn kde_tool(base: &str) -> Option<String> {
    ["6", "5"]
        .iter()
        .map(|version| format!("{base}{version}"))
        .find(|tool| Command::new(tool).arg("--help").output().is_ok())
}

fn kde_get(key: &str) -> Result<String> {
    let tool =
        kde_tool("kreadconfig").ok_or_else(|| AppError::io("kreadconfig не найден".to_string()))?;
    run(
        &tool,
        &[
            "--file",
            "kioslaverc",
            "--group",
            "Proxy Settings",
            "--key",
            key,
        ],
    )
}

fn kde_set(key: &str, value: &str) -> Result<()> {
    let tool = kde_tool("kwriteconfig")
        .ok_or_else(|| AppError::io("kwriteconfig не найден".to_string()))?;
    run(
        &tool,
        &[
            "--file",
            "kioslaverc",
            "--group",
            "Proxy Settings",
            "--key",
            key,
            value,
        ],
    )
    .map(|_| ())
}

/// Сказать KIO перечитать настройки, иначе запущенные программы KDE увидят их не сразу.
fn kde_notify() {
    let _ = Command::new("dbus-send")
        .args([
            "--type=signal",
            "/KIO/Scheduler",
            "org.kde.KIO.Scheduler.reparseSlaveConfiguration",
            "string:",
        ])
        .status();
}

/// `http://127.0.0.1 7890` → `127.0.0.1:7890`.
fn kde_address(value: &str) -> String {
    let value = value.split("://").last().unwrap_or_default();
    value.trim().replacen(' ', ":", 1)
}

fn run(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| AppError::io(format!("{program} не запустился: {e}")))?;
    if !out.status.success() {
        return Err(AppError::io(format!(
            "{program}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_gnome_keys_are_remembered() {
        let listing = "org.gnome.system.proxy mode 'manual'\n\
                       org.gnome.system.proxy use-same-proxy true\n\
                       org.gnome.system.proxy.http host '127.0.0.1'\n\
                       org.gnome.system.proxy.http port 7890\n";
        assert_eq!(
            gnome_values(listing),
            [
                (
                    "org.gnome.system.proxy mode".to_string(),
                    "'manual'".to_string()
                ),
                (
                    "org.gnome.system.proxy.http host".into(),
                    "'127.0.0.1'".into()
                ),
                ("org.gnome.system.proxy.http port".into(), "7890".into()),
            ]
        );
    }

    /// Живьём, в сеансе рабочего стола: поставить свой прокси и вернуть прежний байт в байт.
    /// `cargo test live_desktop_proxy -- --ignored` из сеанса (или с его `DBUS_SESSION_BUS_ADDRESS`).
    #[test]
    #[ignore]
    fn live_desktop_proxy_round_trip() {
        let before = ProxySetting::read().expect("окружение объявляет прокси");
        let ours = "127.0.0.1:7890";
        ProxySetting::apply(ours).unwrap();
        assert!(ProxySetting::is_ours(ours), "адрес не встал");
        ProxySetting::restore(&Backup {
            ours: ours.into(),
            ..before.clone()
        })
        .unwrap();
        assert_eq!(
            ProxySetting::read().unwrap().saved,
            before.saved,
            "вернули не то"
        );
    }

    #[test]
    fn a_kde_proxy_reads_as_host_and_port() {
        assert_eq!(kde_address("http://127.0.0.1 7890"), "127.0.0.1:7890");
        assert_eq!(kde_address(""), "");
    }

    #[test]
    fn bypass_keeps_the_loopback_out_of_the_proxy() {
        assert!(BYPASS.contains(&"127.0.0.0/8"));
        assert!(BYPASS.contains(&"localhost"));
    }
}
