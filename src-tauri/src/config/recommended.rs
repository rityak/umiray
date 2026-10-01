//! Рекомендованный конфиг ядра (D-169): что «Рекомендованная» мастера кладёт в «Настройки
//! mihomo» поверх того, что там есть. Значения и основания — RECOMMENDED.md.
//!
//! Конфиг один на все режимы: человек, начавший с Proxy, переключится в TUN без второго
//! мастера, поэтому DNS здесь включён всегда, а не вместе с TUN. Подборов здесь нет:
//! резолверы `dns.nameserver` и `tun.mtu` мерит `diag::smart` после этой записи. Спорного
//! тоже нет — сниффер, `prefer-h3` и открытый NAT выбирает человек на своём шаге мастера
//! и в «Настройках mihomo» (`config/advanced.rs`).

use crate::config::files::{self, Documents};
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Что накладывается. Данные, а не код: список стареет быстрее программы, и правится он
/// вместе с RECOMMENDED.md, а не поиском по ветвлениям. Списки заменяются целиком,
/// разделы сливаются вглубь (`Yaml::merge`) — `tun.enable` переключателя режима цел.
const OVERLAY: &str = r#"
log-level: warning
tcp-concurrent: true
unified-delay: true
geo-auto-update: true
geo-update-interval: 24
tun:
  stack: mixed
  strict-route: true
dns:
  enable: true
  enhanced-mode: fake-ip
  default-nameserver:
    - 77.88.8.8
    - 223.5.5.5
  proxy-server-nameserver:
    - quic://223.5.5.5
    - https://1.1.1.1/dns-query
    - https://8.8.8.8/dns-query
  direct-nameserver:
    - 77.88.8.8
    - 8.8.8.8
  fake-ip-filter:
    - geosite:private
    - '*.lan'
    - '*.local'
    - '*.localdomain'
    - '*.localhost'
    - '*.home.arpa'
    - '*.internal'
    - '*.test'
    - '*.invalid'
    - '*.example'
    - +.msftconnecttest.com
    - +.msftncsi.com
    - time.windows.com
    - time.*.com
    - time.*.gov
    - time.*.apple.com
    - time-ios.apple.com
    - ntp.*.com
    - +.pool.ntp.org
    - +.stun.*.*
    - +.stun.*.*.*
    - +.stun.*.*.*.*
    - +.srv.nintendo.net
    - '*.n.n.srv.nintendo.net'
    - +.stun.playstation.net
    - xbox.*.microsoft.com
    - '*.*.xboxlive.com'
    - xnotify.xboxlive.com
    - +.battle.net
    - +.steamcontent.com
    - my.keenetic.net
    - '*.router.asus.com'
    - +.tplinkwifi.net
    - +.tplinklogin.net
    - +.routerlogin.net
    - +.miwifi.com
sniffer:
  override-destination: false
  sniff:
    HTTP:
      ports: [80, 8080-8880]
      override-destination: true
    TLS:
      ports: [443, 8443]
    QUIC:
      ports: [443, 8443]
  skip-domain:
    - Mijia Cloud
    - +.push.apple.com
"#;

pub struct Recommended;

impl Recommended {
    /// Наложить рекомендованное на «Настройки mihomo».
    pub fn apply() -> Result<()> {
        let text = Documents::read(files::ADVANCED)?;
        let next = Recommended::over(&text)?;
        Documents::write(files::ADVANCED, &next)
    }

    /// Документ с наложенным рекомендованным. Отдельно от диска — ради теста.
    fn over(text: &str) -> Result<String> {
        let mut map = Yaml::top_mapping(text)?;
        Yaml::merge(&mut map, Yaml::top_mapping(OVERLAY)?);
        serde_yaml::to_string(&serde_yaml::Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    fn applied() -> Value {
        let template = Documents::template(files::ADVANCED).unwrap();
        serde_yaml::from_str(&Recommended::over(template).unwrap()).unwrap()
    }

    /// Один конфиг на все режимы: DNS не ждёт TUN, а выбор режима — его переключателя,
    /// а не рекомендованного.
    #[test]
    fn the_recommended_config_serves_every_capture_mode() {
        let out = applied();
        assert_eq!(out["dns"]["enable"], Value::from(true));
        assert_eq!(out["tun"]["enable"], Value::from(false), "режим не тронут");
        assert_eq!(
            out["mode"],
            Value::from("rule"),
            "остальное шаблона на месте"
        );
        assert_eq!(
            out["dns"]["nameserver"][0],
            Value::from("https://1.1.1.1/dns-query"),
            "резолверы — подбор мастера, наложение их не трогает"
        );
    }

    /// Спорное выбирает человек: наложение его не трогает, что бы ни стояло в файле.
    #[test]
    fn the_overlay_leaves_the_disputed_options_alone() {
        let mine = "sniffer: {enable: false}
tun: {endpoint-independent-nat: false}
dns: {prefer-h3: false}
";
        let out: Value = serde_yaml::from_str(&Recommended::over(mine).unwrap()).unwrap();
        assert_eq!(out["sniffer"]["enable"], Value::from(false));
        assert_eq!(out["tun"]["endpoint-independent-nat"], Value::from(false));
        assert_eq!(out["dns"]["prefer-h3"], Value::from(false));
    }

    /// Живая: ядро принимает шаблон с наложенным целиком. Битый вариант рядом — без него
    /// зелёный `-t` ничего не значит (GOTCHAS).
    #[test]
    #[ignore]
    fn live_the_core_accepts_the_recommended_config() {
        let core = crate::paths::Paths::core();
        assert!(core.exists(), "ядро не скачано, проверять нечем");
        let dir = std::env::temp_dir().join("umiray-recommended-check");
        std::fs::create_dir_all(&dir).unwrap();
        let check = |body: &str| -> bool {
            let path = dir.join("config.yaml");
            std::fs::write(&path, body).unwrap();
            std::process::Command::new(&core)
                .args(["-t", "-d"])
                .arg(&dir)
                .arg("-f")
                .arg(&path)
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).contains("test is successful"))
                .unwrap_or(false)
        };
        let config = serde_yaml::to_string(&applied()).unwrap();
        assert!(
            check(&config),
            "ядро отвергло рекомендованный конфиг:\n{config}"
        );
        assert!(
            !check(&config.replace("fake-ip", "fake-ipp")),
            "битый тоже прошёл"
        );
    }
}
