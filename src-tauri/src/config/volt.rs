use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::config::files::{Documents, CLIENT, VOLT_RELAY, VOLT_VPN};
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

pub const RELAY_DEFAULT: &str = crate::config::files::VOLT_RELAY_DEFAULT;
pub const VPN_DEFAULT: &str = crate::config::files::VOLT_VPN_DEFAULT;

/// Поля стратегий: в окно они едут вместе с настройками, а живут своими документами (D-183).
const STRATEGIES: [(&str, &str); 2] = [("relayYaml", VOLT_RELAY), ("vpnYaml", VOLT_VPN)];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPool {
    pub id: String,
    pub label: String,
    pub count: usize,
    pub file: String,
    pub source: String,
    pub revision: String,
}

pub fn strategy_parse(yaml: &str) -> Result<serde_json::Value> {
    if yaml.len() > 262_144 {
        return Err(AppError::invalid("VOLT strategy exceeds 256 KiB"));
    }
    let map = Yaml::top_mapping(yaml)?;
    serde_json::to_value(map).map_err(|e| AppError::invalid(format!("VOLT strategy: {e}")))
}

pub fn strategy_render(strategy: serde_json::Value) -> Result<String> {
    if !strategy.is_object() {
        return Err(AppError::invalid("VOLT strategy must be a mapping"));
    }
    let yaml = serde_yaml::to_string(&strategy).map_err(|e| AppError::invalid(e.to_string()))?;
    strategy_parse(&yaml)?;
    Ok(yaml)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Relay,
    #[default]
    Auto,
}

/// Что идёт в обход (D-182, D-186): выбранные сервисы — всегда через VOLT, остальное как
/// решил маршрут; или весь трафик выхода `DIRECT` — тогда сервисов нет вовсе.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Services,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Options {
    pub direct_enabled: bool,
    pub scope: Scope,
    /// Сервисы из коллекции `volt` (D-182), которые идут через VOLT при любом выходе;
    /// только при `scope: services` (D-186).
    pub services: Vec<String>,
    /// Свои домены туда же: суффиксом, как `DOMAIN-SUFFIX`.
    pub domains: Vec<String>,
    /// Как идёт выход `DIRECT` при `scope: direct`: сначала напрямую или сразу VOLT.
    pub mode: Mode,
    pub relay_port: u16,
    pub auto_port: u16,
    pub relay_yaml: String,
    pub vpn_enabled: bool,
    pub vpn_yaml: String,
    pub endpoints: Vec<String>,
    pub bootstrap_ips: Vec<std::net::IpAddr>,
    pub auto_select: bool,
    /// Свои адреса проверки подбора; пусто — по тому, что обходится (D-189).
    pub probe_urls: Vec<String>,
}

/// Прежние умолчания адресов проверки: в документе они значат «по выбору» (D-189).
const PAST_PROBES: [&[&str]; 2] = [
    &[
        "https://discord.com/api/v10/gateway",
        "https://www.youtube.com/robots.txt",
        "https://redirector.googlevideo.com/report_mapping",
    ],
    &[
        "https://discord.com/api/v10/gateway",
        "https://www.youtube.com/robots.txt",
    ],
];

impl Default for Options {
    fn default() -> Self {
        Self {
            direct_enabled: false,
            scope: Scope::Services,
            services: vec!["youtube".into(), "discord".into()],
            domains: Vec::new(),
            mode: Mode::Auto,
            relay_port: if cfg!(debug_assertions) { 3101 } else { 3181 },
            auto_port: if cfg!(debug_assertions) { 3103 } else { 3183 },
            relay_yaml: RELAY_DEFAULT.into(),
            vpn_enabled: false,
            vpn_yaml: VPN_DEFAULT.into(),
            endpoints: Vec::new(),
            bootstrap_ips: Vec::new(),
            auto_select: false,
            probe_urls: Vec::new(),
        }
    }
}

/// Выходы VOLT для сборки конфига ядра.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub mode: Mode,
    pub relay_port: u16,
    pub auto_port: u16,
    pub password: String,
    /// Выход `DIRECT` уходит в VOLT (`scope: direct`).
    pub exit: bool,
    /// Правила без цели — сервисы и свои домены; цель `DIRECT-VOLT` дописывает сборка.
    pub rules: Vec<String>,
}

impl Options {
    pub fn get() -> Result<Self> {
        let map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        let mut value = map
            .get(Value::from("volt"))
            .cloned()
            .unwrap_or_else(|| Value::Mapping(Default::default()));
        // Стратегия из прошлых версий ещё лежит строкой в блоке — она и читается, а запись
        // перенесёт её в документ.
        if let Some(block) = value.as_mapping_mut() {
            for (key, doc) in STRATEGIES {
                if !block.contains_key(Value::from(key)) {
                    block.insert(key.into(), Documents::read(doc)?.into());
                }
            }
        }
        let options =
            Self::from_value(value).map_err(|e| AppError::invalid(format!("VOLT: {e}")))?;
        options.validate()?;
        Ok(options)
    }

    fn from_value(mut value: Value) -> std::result::Result<Self, serde_yaml::Error> {
        if let Some(map) = value.as_mapping_mut() {
            // Свой каталог бинарников ушёл с загрузкой с выпуска (D-181): старое поле
            // не должно ронять разбор настроек.
            map.remove(Value::from("directory"));
            if let Some(enabled) = map.remove(Value::from("enabled")) {
                let enabled = enabled.as_bool().ok_or_else(|| {
                    <serde_yaml::Error as serde::de::Error>::custom("VOLT enabled must be boolean")
                })?;
                map.entry(Value::from("directEnabled"))
                    .or_insert(enabled.into());
                let vpn = map
                    .get(Value::from("vpnEnabled"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                map.insert("vpnEnabled".into(), (enabled && vpn).into());
            }
            let past = map
                .get(Value::from("probeUrls"))
                .and_then(Value::as_sequence)
                .is_some_and(|urls| {
                    PAST_PROBES.iter().any(|past| {
                        urls.len() == past.len()
                            && urls
                                .iter()
                                .zip(past.iter())
                                .all(|(url, past)| url.as_str() == Some(past))
                    })
                });
            if past {
                map.remove(Value::from("probeUrls"));
            }
            // До D-182 включённый VOLT переписывал весь `DIRECT`: ближе всего к этому —
            // выход `DIRECT` через VOLT. Новое включение пишет `scope` само.
            if !map.contains_key(Value::from("scope"))
                && map
                    .get(Value::from("directEnabled"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                map.insert("scope".into(), "direct".into());
            }
        }
        serde_yaml::from_value(value)
    }

    pub fn active(&self) -> bool {
        self.direct_enabled || self.vpn_enabled
    }

    pub fn write(&self) -> Result<()> {
        self.validate()?;
        let mut block = serde_yaml::to_value(self).map_err(|e| AppError::invalid(e.to_string()))?;
        if let Some(block) = block.as_mapping_mut() {
            for (key, doc) in STRATEGIES {
                let text = block.remove(Value::from(key));
                let text = text.as_ref().and_then(Value::as_str).unwrap_or_default();
                if Documents::read(doc)? != text {
                    Documents::write(doc, text)?;
                }
            }
        }
        let mut map = Yaml::top_mapping(&Documents::read(CLIENT)?)?;
        map.insert(Value::from("volt"), block);
        Documents::write(
            CLIENT,
            &serde_yaml::to_string(&map).map_err(|e| AppError::invalid(e.to_string()))?,
        )
    }

    /// Правила без цели: выбранные сервисы коллекции по порядку коллекции, затем свои
    /// домены. Незнакомый id сервиса пропускается — коллекцию правит человек. При «весь
    /// трафик DIRECT» окно их прячет — и в маршрут они не идут (D-186).
    pub fn rules(&self, catalog: &crate::collections::VoltServices) -> Vec<String> {
        if self.scope == Scope::Direct {
            return Vec::new();
        }
        catalog
            .services
            .iter()
            .filter(|service| self.services.contains(&service.id))
            .flat_map(|service| service.rules.iter().cloned())
            .chain(
                self.domains
                    .iter()
                    .map(|domain| format!("DOMAIN-SUFFIX,{domain}")),
            )
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        // Домен уходит строкой правила ядра: запятая или пробел сделали бы из него другое правило.
        if self.domains.len() > 256
            || self.domains.iter().any(|domain| {
                domain.len() > 253
                    || !domain.contains('.')
                    || domain.starts_with('.')
                    || domain.ends_with('.')
                    || !domain.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'
                    })
            })
        {
            return Err(AppError::invalid(
                "Свои домены VOLT — до 256 имён вида example.com: строчные латинские буквы, цифры, точка и дефис",
            ));
        }
        if self.relay_port == 0 || self.auto_port == 0 || self.relay_port == self.auto_port {
            return Err(AppError::invalid(
                "VOLT listener ports must be distinct and nonzero",
            ));
        }
        for yaml in [&self.relay_yaml, &self.vpn_yaml] {
            if yaml.len() > 262_144 {
                return Err(AppError::invalid("VOLT strategy exceeds 256 KiB"));
            }
            Yaml::top_mapping(yaml)?;
        }
        if self.endpoints.len() > 64 || self.bootstrap_ips.len() > 16 {
            return Err(AppError::invalid(
                "VOLT supports at most 64 endpoints and 16 bootstrap IPs",
            ));
        }
        if !self.probe_urls.is_empty() {
            validate_probe_urls(&self.probe_urls)?;
        }
        for endpoint in &self.endpoints {
            let address: std::net::SocketAddr = endpoint
                .parse()
                .map_err(|_| AppError::invalid("VOLT endpoints must be real IP:port addresses"))?;
            if address.port() == 0 || !real_ip(address.ip()) {
                return Err(AppError::invalid("VOLT endpoints cannot use loopback, unspecified, multicast or fake-IP addresses"));
            }
        }
        if self.bootstrap_ips.iter().any(|ip| !real_ip(*ip)) {
            return Err(AppError::invalid(
                "VOLT bootstrap addresses must be real IPs",
            ));
        }
        Ok(())
    }

    /// Что проверяет подбор (D-189): свои адреса, а нет — то, что обходится: выбранные
    /// сервисы и свои домены, при «весь трафик DIRECT» — все сервисы. Не больше четырёх:
    /// подбор гоняет каждый адрес дважды на вариант.
    pub fn probe_targets(&self, catalog: &crate::collections::VoltServices) -> Vec<String> {
        if !self.probe_urls.is_empty() {
            return self.probe_urls.clone();
        }
        let chosen = |service: &&crate::collections::VoltService| {
            self.scope == Scope::Direct || self.services.contains(&service.id)
        };
        let mut targets: Vec<String> = catalog
            .services
            .iter()
            .filter(chosen)
            .flat_map(|service| service.probe_targets())
            .collect();
        if self.scope == Scope::Services {
            targets.extend(
                self.domains
                    .iter()
                    .map(|domain| format!("https://{domain}/")),
            );
        }
        if targets.is_empty() {
            targets = catalog
                .services
                .iter()
                .flat_map(|service| service.probe_targets())
                .collect();
        }
        let mut seen = std::collections::HashSet::new();
        targets.retain(|url| seen.insert(url.clone()));
        targets.truncate(4);
        targets
    }

    /// Куда ведёт выход `DIRECT` в «Соединении».
    pub fn target(&self) -> &'static str {
        if !self.direct_enabled || self.scope == Scope::Services {
            "DIRECT"
        } else if self.mode == Mode::Auto {
            "DIRECT-AUTO"
        } else {
            "DIRECT-VOLT"
        }
    }
}

pub fn validate_probe_urls(urls: &[String]) -> Result<()> {
    if urls.is_empty() || urls.len() > 4 {
        return Err(AppError::invalid(
            "VOLT selection requires 1..4 HTTPS test URLs",
        ));
    }
    for text in urls {
        let url = reqwest::Url::parse(text)
            .map_err(|e| AppError::invalid(format!("VOLT test URL: {e}")))?;
        if text.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.port() == Some(0)
        {
            return Err(AppError::invalid(
                "VOLT test URLs must use HTTPS without credentials or fragments",
            ));
        }
    }
    Ok(())
}

pub fn real_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !(ip.octets()[0] == 198 && matches!(ip.octets()[1], 18 | 19))
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return real_ip(ip.into());
            }
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && ip.segments()[0] != 0xfc00
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_edits_preserve_nested_custom_matching_and_stages() {
        let yaml = "version: 1\nprofiles:\n  - name: custom\n    match:\n      network: tcp\n      all:\n        - payloads: [tls]\n        - not: {hosts: [private.ru]}\n      signatures: [{offset: 0, hex: '1603'}]\n    stages:\n      - {action: fake, fake: {kind: tls, server_names: [ya.ru, vk.com], repeats: 2}}\n      - {action: split, positions: [midsld], packet_limit: 2}\n";
        let mut parsed = strategy_parse(yaml).unwrap();
        let matching = parsed["profiles"][0]["match"].clone();
        let fake = parsed["profiles"][0]["stages"][0].clone();
        parsed["profiles"][0]["stages"][1]["packet_limit"] = 3.into();
        let rendered = strategy_render(parsed).unwrap();
        let result = strategy_parse(&rendered).unwrap();
        assert_eq!(result["profiles"][0]["match"], matching);
        assert_eq!(result["profiles"][0]["stages"][0], fake);
        assert_eq!(result["profiles"][0]["stages"][1]["packet_limit"], 3);
        assert!(strategy_render(serde_json::json!([])).is_err());
        assert!(strategy_parse("profiles: [").is_err());
        assert!(strategy_parse(&" ".repeat(262_145)).is_err());
    }

    #[test]
    fn browser_preview_presets_match_the_native_yaml() {
        let preview: serde_json::Value =
            serde_json::from_str(include_str!("../../../collections/volt/strategy-mock.json"))
                .unwrap();
        for (name, yaml) in [
            ("relay", RELAY_DEFAULT),
            ("vpn", VPN_DEFAULT),
            (
                "vpn-noise",
                include_str!("../../../collections/volt/vpn-noise.yaml"),
            ),
            (
                "vpn-tcp",
                include_str!("../../../collections/volt/vpn-tcp.yaml"),
            ),
        ] {
            assert_eq!(preview[name], strategy_parse(yaml).unwrap());
        }
    }

    #[test]
    fn probe_urls_reject_credentials_and_non_https() {
        for url in [
            "http://ya.ru/",
            "https://user:secret@ya.ru/",
            "https://ya.ru/#secret",
            "not a URL",
        ] {
            assert!(validate_probe_urls(&[url.into()]).is_err());
        }
        assert!(validate_probe_urls(&[]).is_err());
        assert!(validate_probe_urls(&vec!["https://ya.ru/".into(); 5]).is_err());
    }
    #[test]
    fn independent_switches_and_legacy_migration() {
        for direct in [false, true] {
            for vpn in [false, true] {
                let options = Options {
                    direct_enabled: direct,
                    vpn_enabled: vpn,
                    ..Options::default()
                };
                assert_eq!(options.active(), direct || vpn);
                assert_eq!(
                    options.target(),
                    "DIRECT",
                    "сервисы не трогают выход DIRECT"
                );
                let exit = Options {
                    scope: Scope::Direct,
                    ..options
                };
                assert_eq!(exit.target() != "DIRECT", direct);
            }
        }
        for enabled in [false, true] {
            let value =
                serde_yaml::from_str(&format!("enabled: {enabled}\nvpnEnabled: true\n")).unwrap();
            let options = Options::from_value(value).unwrap();
            assert_eq!(options.direct_enabled, enabled);
            assert_eq!(options.vpn_enabled, enabled);
            // Включённый до D-182 VOLT вёл в себя весь DIRECT — так и остаётся.
            let scope = if enabled {
                Scope::Direct
            } else {
                Scope::Services
            };
            assert_eq!(options.scope, scope);
        }
        // Поле своего каталога ушло (D-181); документ, где оно осталось, читается.
        let value = serde_yaml::from_str("directory: D:\\volt\ndirectEnabled: true\n").unwrap();
        assert!(Options::from_value(value).unwrap().direct_enabled);
    }

    #[test]
    fn services_become_rules_and_domains_cannot_smuggle_syntax() {
        let catalog: crate::collections::VoltServices = serde_yaml::from_str(
            "version: 1
services:
  - {id: youtube, title: YouTube, rules: ['DOMAIN-SUFFIX,youtube.com']}
  - {id: discord, title: Discord, rules: ['DOMAIN-SUFFIX,discord.com']}
",
        )
        .unwrap();
        let options = Options {
            services: vec!["discord".into(), "gone".into()],
            domains: vec!["rutracker.org".into()],
            ..Options::default()
        };
        assert_eq!(
            options.rules(&catalog),
            ["DOMAIN-SUFFIX,discord.com", "DOMAIN-SUFFIX,rutracker.org"]
        );
        let everything = Options {
            scope: Scope::Direct,
            ..options.clone()
        };
        assert!(
            everything.rules(&catalog).is_empty(),
            "скрытые сервисы не действуют"
        );
        assert!(options.validate().is_ok());
        for bad in [
            "evil.com,REJECT",
            "has space.ru",
            "Upper.ru",
            "nodot",
            ".lead.ru",
            "x.ru.",
        ] {
            let options = Options {
                domains: vec![bad.into()],
                ..Options::default()
            };
            assert!(options.validate().is_err(), "{bad}");
        }
    }

    /// Подбор проверяет то, что обходится (D-189); свои адреса — сильнее.
    #[test]
    fn probes_follow_what_goes_through_the_bypass() {
        let catalog: crate::collections::VoltServices = serde_yaml::from_str(
            "version: 1
services:
  - {id: youtube, title: YouTube, probe: ['https://www.youtube.com/robots.txt', 'https://redirector.googlevideo.com/report_mapping'], rules: ['DOMAIN-SUFFIX,youtube.com']}
  - {id: discord, title: Discord, rules: ['DOMAIN-SUFFIX,discord.com']}
",
        )
        .unwrap();
        let only = |services: &[&str], domains: &[&str]| Options {
            services: services.iter().map(|id| (*id).into()).collect(),
            domains: domains.iter().map(|domain| (*domain).into()).collect(),
            ..Options::default()
        };
        assert_eq!(
            only(&[], &["rutracker.org"]).probe_targets(&catalog),
            ["https://rutracker.org/"],
            "только свой домен — только он"
        );
        assert_eq!(
            only(&["discord"], &[]).probe_targets(&catalog),
            ["https://discord.com/"],
            "нет probe — первый домен"
        );
        assert_eq!(
            only(&["youtube", "discord"], &["a.ru", "b.ru"]).probe_targets(&catalog),
            [
                "https://www.youtube.com/robots.txt",
                "https://redirector.googlevideo.com/report_mapping",
                "https://discord.com/",
                "https://a.ru/",
            ],
            "не больше четырёх"
        );
        let everything = Options {
            scope: Scope::Direct,
            ..only(&[], &["hidden.ru"])
        };
        assert_eq!(
            everything.probe_targets(&catalog).len(),
            3,
            "весь DIRECT — все сервисы, скрытые домены не в счёт"
        );
        assert_eq!(
            only(&[], &[]).probe_targets(&catalog).len(),
            3,
            "ничего не выбрано — все сервисы, а не пусто"
        );
        let own = Options {
            probe_urls: vec!["https://ya.ru/".into()],
            ..only(&["youtube"], &[])
        };
        assert_eq!(own.probe_targets(&catalog), ["https://ya.ru/"]);
        // Прежнее умолчание в документе — «по выбору», своё — остаётся.
        for (stored, empty) in [
            ("probeUrls: [https://discord.com/api/v10/gateway, https://www.youtube.com/robots.txt, https://redirector.googlevideo.com/report_mapping]
", true),
            ("probeUrls: [https://discord.com/api/v10/gateway, https://www.youtube.com/robots.txt]
", true),
            ("probeUrls: [https://ya.ru/]
", false),
        ] {
            let options = Options::from_value(serde_yaml::from_str(stored).unwrap()).unwrap();
            assert_eq!(options.probe_urls.is_empty(), empty, "{stored}");
            assert!(options.validate().is_ok());
        }
    }

    /// Стратегии — свои документы (D-183): блок `volt` без YAML-строк; строка из прошлых
    /// версий читается и при записи переезжает.
    #[test]
    fn strategies_live_in_their_own_documents() {
        let _sandbox = crate::paths::Sandbox::new("volt-strategies");
        let legacy = "volt:
  directEnabled: true
  relayYaml: |
    version: 1
    profiles: []
";
        Documents::write(CLIENT, legacy).unwrap();
        let options = Options::get().unwrap();
        assert_eq!(
            options.relay_yaml,
            "version: 1
profiles: []
"
        );
        assert_eq!(
            options.vpn_yaml, VPN_DEFAULT,
            "нет строки — документ с образцом"
        );
        options.write().unwrap();
        let client = Yaml::top_mapping(&Documents::read(CLIENT).unwrap()).unwrap();
        let block = client
            .get(Value::from("volt"))
            .unwrap()
            .as_mapping()
            .unwrap();
        assert!(!block.contains_key(Value::from("relayYaml")));
        assert!(!block.contains_key(Value::from("vpnYaml")));
        assert_eq!(
            Documents::read(VOLT_RELAY).unwrap(),
            "version: 1
profiles: []
"
        );
        assert_eq!(Options::get().unwrap(), options);
    }

    #[test]
    fn rejects_capture_of_loopback_and_fake_ips() {
        for endpoint in [
            "127.0.0.1:443",
            "198.18.1.2:443",
            "[::ffff:198.19.1.2]:443",
            "45.86.245.83:0",
        ] {
            assert!(Options {
                endpoints: vec![endpoint.into()],
                ..Options::default()
            }
            .validate()
            .is_err());
        }
        assert!(Options {
            endpoints: vec!["45.86.245.83:443".into()],
            ..Options::default()
        }
        .validate()
        .is_ok());
    }
}
