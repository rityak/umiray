//! Стенд: одноразовое ядро, поднятое ради одного вопроса (D-098).
//!
//! Нужен там, где ответ знает только ядро: DoT, DoQ и DoH3 требуют своих TLS и QUIC,
//! и втаскивать их в клиент ради замера — это две зависимости и своя реализация того,
//! что уже лежит рядом в готовом виде. Стенд поднимает mihomo с **одним** `nameserver`,
//! спрашивает у него имя через `/dns/query` и гасит.
//!
//! **Рабочий VPN он не трогает.** Своя папка, свой порт управления, `mixed-port: 0` —
//! ни одного занятого порта наружу, ни одной строки в чужой конфиг. TUN не поднимается
//! никогда: адаптер в системе один, и стенд, поднявший его, оборвал бы связь.
//!
//! Гаснет он в `Drop`: паника посреди замера не должна оставлять чужой процесс.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::core::controller::Controller;
use crate::error::{AppError, Result};
use crate::paths;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Сколько ждём, пока ядро ответит на `/version`. Ядро без правил и без geosite встаёт
/// за десятки миллисекунд; секунда — потолок на медленную машину.
const READY: Duration = Duration::from_millis(1000);

/// Как часто спрашиваем готовность. Чаще смысла нет — это локальный HTTP.
const TICK: Duration = Duration::from_millis(20);

pub struct Bench {
    child: Child,
    controller: Controller,
    dir: PathBuf,
}

impl Bench {
    /// Поднять ядро с единственным резолвером.
    ///
    /// `nameserver` — строка из коллекции как есть: `tls://dns.google`,
    /// `quic://dns.adguard-dns.com`, `https://…/dns-query`. Ядро понимает их само,
    /// и переводить их во что-то своё было бы лишним шагом с потерями.
    pub async fn start(nameserver: &str) -> Result<Self> {
        Self::with(&document(nameserver)).await
    }

    /// То же, но с готовым конфигом: перебор сочетаний (`diag::matrix`) меняет не только
    /// резолвер, и собирать его строку здесь значило бы держать вторую копию перебора.
    pub async fn with(config: &str) -> Result<Self> {
        let core = paths::core();
        if !core.exists() {
            return Err(AppError::invalid(
                "Ядра нет — шифрованные точки мерить нечем".to_string(),
            ));
        }
        let controller = Controller::new()?;
        // Папка своя у каждого стенда: их поднимают по нескольку разом, и общий каталог
        // они бы затирали друг у друга. Имя берём от порта управления — он уже уникален.
        let dir = paths::run_dir().join(format!("bench-{}", controller.port()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.yaml");
        crate::atomic::write(&path, config)?;

        let mut command = Command::new(&core);
        command
            .arg("-d")
            .arg(&dir)
            .arg("-f")
            .arg(&path)
            .args(controller.args());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command
            .spawn()
            .map_err(|e| AppError::io(format!("Стенд не запустился: {e}")))?;

        let bench = Self {
            child,
            controller,
            dir,
        };
        let started = Instant::now();
        while started.elapsed() < READY {
            if bench.controller.ready().await {
                return Ok(bench);
            }
            tokio::time::sleep(TICK).await;
        }
        Err(AppError::network(
            "Стенд не ответил за секунду — ядро не поднялось".to_string(),
        ))
    }

    /// Спросить имя у резолвера, ради которого стенд и поднимали.
    pub async fn resolve(&self, name: &str) -> Result<Vec<String>> {
        self.controller.resolve(name).await
    }

    /// Внутренности стенда — только замерам (S-020). Рабочему коду они не нужны: он
    /// спрашивает стенд про имена, а не про то, как тот устроен.
    #[cfg(test)]
    pub fn controller(&self) -> &Controller {
        &self.controller
    }

    #[cfg(test)]
    pub fn config_path(&self) -> std::path::PathBuf {
        self.dir.join("config.yaml")
    }

    #[cfg(test)]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Папку убираем целиком: в ней только наш конфиг и то, что ядро в неё написало.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Конфиг стенда: минимальный настолько, насколько ядро позволяет.
///
/// Порты нулевые — стенд ничего не слушает наружу. `dns.listen` пустой: пятьдесят третий
/// порт в системе один, и занимать его ради замера нельзя. `MATCH,DIRECT` стоит потому,
/// что без единого правила ядро отказывается читать конфиг.
fn document(nameserver: &str) -> String {
    format!(
        "# Стенд диагностики. Файл временный: создаётся на один замер и удаляется после.\n\
         mixed-port: 0\n\
         mode: rule\n\
         log-level: silent\n\
         external-ui: \"\"\n\
         dns:\n  \
           enable: true\n  \
           listen: \"\"\n  \
           ipv6: false\n  \
           enhanced-mode: fake-ip\n  \
           nameserver:\n    \
             - \"{nameserver}\"\n\
         rules:\n  \
           - MATCH,DIRECT\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Конфиг стенда не должен ничего слушать и ничего перехватывать: он живёт рядом
    /// с работающим VPN.
    #[test]
    fn the_bench_listens_to_nothing() {
        let yaml = document("tls://dns.google");
        assert!(yaml.contains("mixed-port: 0"));
        assert!(yaml.contains("listen: \"\""));
        assert!(!yaml.contains("tun:"), "стенд не поднимает адаптер");
        assert!(yaml.contains("- \"tls://dns.google\""));
    }

    /// Ядро не читает конфиг без единого правила — проверено им самим, поэтому правило
    /// в шаблоне есть и убирать его нельзя.
    #[test]
    fn there_is_at_least_one_rule() {
        assert!(document("8.8.8.8").contains("- MATCH,DIRECT"));
    }

    /// Строка коллекции уходит в конфиг как есть: перевод её во что-то своё —
    /// лишний шаг с потерями.
    #[test]
    fn the_address_travels_verbatim() {
        for addr in ["quic://dns.adguard-dns.com", "https://dns.google/dns-query"] {
            assert!(document(addr).contains(addr), "{addr}");
        }
    }
}
