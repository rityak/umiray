use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::config::volt::{Options, Route};
use crate::core::process::{CoreProcess, LogRing};
use crate::core::volt_tune::TuneReport;
use crate::error::{AppError, Result};
use crate::paths::Paths;

pub mod download;
pub mod tune_memory;

use tune_memory::{Remembered, TuneMemory};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub options: Options,
    pub available: bool,
    pub elevated: bool,
    pub relay_running: bool,
    pub vpn_running: bool,
    pub endpoints: Vec<String>,
    pub log: Vec<String>,
    pub relay_default: &'static str,
    pub vpn_default: &'static str,
    pub domain_pools: Vec<crate::config::volt::DomainPool>,
    pub tuning: Option<TuneReport>,
    /// Почему Relay не работает (D-187): ответ последнего запуска; удачный его стирает.
    pub relay_error: Option<String>,
    /// Что проверит подбор (D-189): окно показывает это без «Кода».
    pub probe_targets: Vec<String>,
    /// Последние счётчики работающего Relay (D-188); до первых и без Relay — нет.
    pub relay_stats: Option<RelayStats>,
    /// Сервисы коллекции `volt` (D-182): окно показывает их галками.
    pub services: Vec<crate::collections::VoltService>,
}

/// С чем Relay выходит в сеть (D-176). Пусто — как система: системный DNS и маршрут ОС;
/// так работают подбор и выпуск WARP, когда ядра нет.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Uplink {
    /// `/dns/query` работающего ядра и его секрет: имена Relay узнаёт у того же резолвера,
    /// что и ядро, — подмену на 53 и подменный адрес TUN он не видит.
    pub dns: Option<(String, String)>,
    /// Физический адаптер: в TUN сокет Relay иначе ушёл бы обратно в туннель, и ядро
    /// пересобрало бы поток — разрезы и ложные пакеты пропали бы молча.
    pub interface: Option<u32>,
    /// Какая это сеть — адаптер и шлюз: ответ подбора принадлежит ей (D-185).
    pub network: String,
}

/// Счётчики работающего Relay (D-188): событие `stats` раз в [`STATS_EVERY`]. По ним окно
/// говорит, что обход действительно что-то делает, а не только запущен.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayStats {
    pub connections: u64,
    pub active: u64,
    /// Изменённые пакеты: обход не просто пропускал.
    pub modified: u64,
    pub faked: u64,
    /// Соединения, которые не открылись, и пакеты, которые не перехватились.
    pub failures: u64,
    pub last_error: Option<String>,
    /// AUTO: сколько рукопожатий ушло напрямую и сколько — через обход.
    pub auto_direct: u64,
    pub auto_bypassed: u64,
    /// Ротация стратегий по хосту: сколько раз переключились и сколько ранних
    /// блокировок засёк детектор (ядро, #6).
    pub rotations: u64,
    pub detected_failures: u64,
}

const STATS_EVERY: &str = "5s";

/// Помощник и то, с чем он запущен: по этому сверяется сохранённое (D-177) и им же
/// помощник поднимается после падения (D-178).
struct Running {
    child: Child,
    options: Options,
    uplink: Uplink,
    endpoints: Vec<String>,
}

#[derive(Default)]
pub struct Volt {
    relay: Mutex<Option<Running>>,
    vpn: Mutex<Option<Running>>,
    /// Пароль SOCKS Relay — на сессию клиента: ядро, которому его пишет сборка, живёт
    /// не дольше клиента (D-058), а в базе ему делать нечего.
    password: Mutex<Option<String>>,
    log: LogRing,
    memory: TuneMemory,
    tune_lock: tokio::sync::Mutex<()>,
    domain_pools: Mutex<Option<(String, Vec<crate::config::volt::DomainPool>)>>,
    failure: Mutex<Option<String>>,
    /// Пишет поток вывода Relay, читает снимок.
    stats: Arc<Mutex<Option<RelayStats>>>,
}

impl Volt {
    pub fn logs(&self) -> Vec<String> {
        self.log.lines()
    }

    /// Каталог бинарников: его наполняет загрузка с выпуска (D-181), свой путь не задаётся.
    pub fn directory() -> PathBuf {
        Paths::volt_dir()
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let options = Options::get()?;
        let directory = Self::directory();
        let uplink = self.relay_uplink().unwrap_or_default();
        let tuning = self
            .memory
            .find(&TuneMemory::key(&options, &uplink))
            .map(|remembered| remembered.report);
        let catalog = crate::collections::Collections::volt()?;
        let probe_targets = options.probe_targets(&catalog);
        Ok(Snapshot {
            available: cfg!(windows) && download::VoltDownload::present(),
            elevated: crate::system::elevation::Elevation::is_elevated(),
            options,
            relay_running: alive(&self.relay),
            vpn_running: alive(&self.vpn),
            endpoints: self
                .vpn
                .lock()
                .unwrap()
                .as_ref()
                .map(|running| running.endpoints.clone())
                .unwrap_or_default(),
            log: self.log.tail(30),
            relay_default: crate::config::volt::RELAY_DEFAULT,
            vpn_default: crate::config::volt::VPN_DEFAULT,
            domain_pools: self.domain_pools(&directory)?,
            tuning,
            relay_error: self.failure.lock().unwrap().clone(),
            probe_targets,
            relay_stats: alive(&self.relay)
                .then(|| self.stats.lock().unwrap().clone())
                .flatten(),
            services: catalog.services,
        })
    }

    /// С какими настройками работает Relay; `None` — не работает.
    pub fn relay_options(&self) -> Option<Options> {
        running_options(&self.relay)
    }

    /// С чем работающий Relay вышел в сеть: сеть сменилась — его пора поднять заново.
    pub fn relay_uplink(&self) -> Option<Uplink> {
        self.relay
            .lock()
            .unwrap()
            .as_ref()
            .map(|running| running.uplink.clone())
    }

    /// С какими настройками работает перехват VPN; `None` — не работает.
    pub fn vpn_options(&self) -> Option<Options> {
        running_options(&self.vpn)
    }

    /// Выходы Relay для сборки конфига: есть, только когда VOLT для прямых соединений
    /// включён. Сборка только читает — пароль заводится здесь, один на сессию.
    pub fn route(&self, options: &Options) -> Result<Option<Route>> {
        if !options.direct_enabled {
            return Ok(None);
        }
        Ok(Some(Route {
            exit: options.scope == crate::config::volt::Scope::Direct,
            rules: options.rules(&crate::collections::Collections::volt()?),
            ..self.relay_route(options)?
        }))
    }

    fn relay_route(&self, options: &Options) -> Result<Route> {
        let mut password = self.password.lock().unwrap();
        if password.is_none() {
            *password = Some(format!(
                "{}{}",
                crate::stamp::Stamp::id()?,
                crate::stamp::Stamp::id()?
            ));
        }
        Ok(Route {
            mode: options.mode,
            relay_port: options.relay_port,
            auto_port: options.auto_port,
            password: password.clone().unwrap_or_default(),
            exit: false,
            rules: Vec::new(),
        })
    }

    fn domain_pools(
        &self,
        directory: &std::path::Path,
    ) -> Result<Vec<crate::config::volt::DomainPool>> {
        let binary = directory.join("volt-relay.exe");
        let Ok(metadata) = std::fs::metadata(&binary) else {
            return Ok(Vec::new());
        };
        let key = format!(
            "{}:{}:{:?}",
            binary.display(),
            metadata.len(),
            metadata.modified().ok()
        );
        let mut cache = self.domain_pools.lock().unwrap();
        if let Some((_, pools)) = cache.as_ref().filter(|(cached, _)| cached == &key) {
            return Ok(pools.clone());
        }
        let mut command = Command::new(&binary);
        command.arg("-domain-pools");
        crate::system::console::Console::hide(&mut command, false);
        let result = command
            .output()
            .map_err(|e| AppError::io(format!("VOLT dictionaries: {e}")))?;
        if !result.status.success() || result.stdout.len() > 16_384 {
            return Err(AppError::invalid(
                "VOLT binaries need an update: domain dictionary metadata is unavailable",
            ));
        }
        let pools: Vec<crate::config::volt::DomainPool> = serde_json::from_slice(&result.stdout)
            .map_err(|e| AppError::invalid(format!("VOLT dictionary metadata: {e}")))?;
        *cache = Some((key, pools.clone()));
        Ok(pools)
    }

    pub fn check(options: &Options) -> Result<()> {
        options.validate()?;
        if !options.active() {
            return Ok(());
        }
        if !cfg!(windows) {
            return Err(AppError::invalid(
                "VOLT is currently available only on Windows",
            ));
        }
        let directory = Self::directory();
        for (binary, text, name) in [
            ("volt-relay.exe", relay_yaml(options)?, "relay-check.yaml"),
            ("volt.exe", options.vpn_yaml.clone(), "vpn-check.yaml"),
        ] {
            if (binary == "volt.exe" && !options.vpn_enabled)
                || (binary == "volt-relay.exe" && !options.direct_enabled)
            {
                continue;
            }
            let file = strategy_file(name, &text)?;
            let mut command = Command::new(directory.join(binary));
            command.args(["-config", &file.to_string_lossy(), "-check-config"]);
            crate::system::console::Console::hide(&mut command, false);
            let result = command.output().map_err(|e| {
                AppError::io(format!(
                    "VOLT не запустился для проверки стратегии: {e}. Бинарники — в {}",
                    directory.display()
                ))
            })?;
            if !result.status.success() {
                return Err(AppError::invalid(format!(
                    "VOLT strategy: {}",
                    String::from_utf8_lossy(&result.stderr)
                )));
            }
        }
        Ok(())
    }

    /// Поднять Relay с этими настройками и выходом в сеть (D-176). Работающий гасится:
    /// ядро зовёт это, когда Relay нужен другим.
    pub async fn start_relay(&self, options: &Options, uplink: &Uplink) -> Result<()> {
        let result = self.try_start_relay(options, uplink).await;
        *self.failure.lock().unwrap() = result.as_ref().err().map(ToString::to_string);
        result
    }

    /// Relay упал и не поднимается (D-178): окно скажет об этом, пока его не запустят снова.
    pub fn fail(&self, why: &str) {
        *self.failure.lock().unwrap() = Some(why.to_owned());
    }

    async fn try_start_relay(&self, options: &Options, uplink: &Uplink) -> Result<()> {
        let mut checked = options.clone();
        checked.direct_enabled = true;
        Self::check(&checked)?;
        if !crate::system::elevation::Elevation::is_elevated() {
            return Err(AppError::NeedsElevation {
                message: "VOLT requires administrator privileges for WinDivert".into(),
            });
        }
        if options.auto_select && options.direct_enabled {
            self.tune_cached(options, uplink, false).await?;
        }
        self.spawn_relay(options, uplink).await
    }

    /// Перезапустить Relay с новыми настройками, но тем же выходом в сеть: стратегия
    /// сменилась, а сеть и ядро — нет (D-177).
    pub async fn restart_relay(&self, options: &Options) -> Result<()> {
        let uplink = self
            .relay
            .lock()
            .unwrap()
            .as_ref()
            .map(|running| running.uplink.clone())
            .unwrap_or_default();
        self.start_relay(options, &uplink).await
    }

    /// Relay для выпуска WARP: работающий — как есть, иначе свой, как у системы. Без
    /// VPN ядра нет, и спрашивать имена не у кого.
    pub async fn bootstrap_relay(&self) -> Result<Route> {
        let running = self.relay_options().filter(|_| alive(&self.relay));
        let options = match running {
            Some(options) => options,
            None => {
                let options = Options::get()?;
                self.start_relay(&options, &Uplink::default()).await?;
                options
            }
        };
        self.relay_route(&options)
    }

    async fn spawn_relay(&self, options: &Options, uplink: &Uplink) -> Result<()> {
        stop_slot(&self.relay);
        let route = self.relay_route(options)?;
        let mut effective = options.clone();
        effective.relay_yaml = self.effective_yaml(options, uplink);
        let file = strategy_file("relay.yaml", &relay_yaml(&effective)?)?;
        let directory = Self::directory();
        let mut command = Command::new(directory.join("volt-relay.exe"));
        command.args([
            "-config",
            &file.to_string_lossy(),
            "-listen",
            &format!("127.0.0.1:{}", route.relay_port),
            "-username",
            "umiray",
            "-dll",
            &directory.join("WinDivert.dll").to_string_lossy(),
            "-stop-on-stdin",
            "-stats-interval",
            STATS_EVERY,
        ]);
        command.args(uplink_args(uplink));
        command.env("UMIRAY_VOLT_PASSWORD", route.password);
        if let Some((_, token)) = &uplink.dns {
            command.env("UMIRAY_VOLT_DNS_TOKEN", token);
        }
        *self.stats.lock().unwrap() = None;
        let child = self
            .launch(command, "VOLT Relay", Some(self.stats.clone()))
            .await?;
        *self.relay.lock().unwrap() = Some(Running {
            child,
            options: options.clone(),
            uplink: uplink.clone(),
            endpoints: Vec::new(),
        });
        Ok(())
    }

    pub async fn tune_now(&self) -> Result<TuneReport> {
        let options = Options::get()?;
        let running = self.relay_uplink();
        let uplink = running.clone().unwrap_or_default();
        let key = TuneMemory::key(&options, &uplink);
        let previous = self.memory.find(&key);
        let report = self.tune_cached(&options, &uplink, true).await?;
        if report.selected.is_some() && running.is_some() {
            if let Err(error) = self.spawn_relay(&options, &uplink).await {
                // Победитель не встал — Relay возвращается к тому, с чем работал.
                self.memory.remember(previous.unwrap_or(Remembered {
                    key,
                    checked: 0,
                    report: report.clone(),
                    yaml: None,
                }));
                if let Err(rollback) = self.spawn_relay(&options, &uplink).await {
                    self.log.push(event_line(
                        "VOLT strategy selection",
                        &format!("Relay rollback failed: {rollback}"),
                        "error",
                    ));
                }
                return Err(error);
            }
        }
        Ok(report)
    }

    /// «Проверить сайт» (D-190): с той стратегией, с которой Relay работает в этой сети.
    pub async fn check_site(&self, input: &str) -> Result<crate::core::volt_tune::SiteCheck> {
        if !download::VoltDownload::present() {
            return Err(AppError::invalid(
                "VOLT is not downloaded yet: turn it on first",
            ));
        }
        let _tune = self.tune_lock.lock().await;
        let options = Options::get()?;
        let uplink = self.relay_uplink().unwrap_or_default();
        let strategy = self.effective_yaml(&options, &uplink);
        crate::core::volt_tune::check_site(input, &strategy, &uplink, &self.log).await
    }

    async fn tune_cached(
        &self,
        options: &Options,
        uplink: &Uplink,
        force: bool,
    ) -> Result<TuneReport> {
        let _tune = self.tune_lock.lock().await;
        let key = TuneMemory::key(options, uplink);
        if !force {
            if let Some(remembered) = self.memory.fresh(&key) {
                return Ok(remembered.report);
            }
        }
        let mut checked = options.clone();
        checked.probe_urls = options.probe_targets(&crate::collections::Collections::volt()?);
        let report = crate::core::volt_tune::tune_logged(&checked, uplink, &self.log).await?;
        // Никто не прошёл или блокировки нет — Relay остаётся с прежним выбором для этой сети.
        let yaml = match report.selected.as_deref() {
            Some(id) => Some(crate::core::volt_tune::selected_yaml(
                &options.relay_yaml,
                id,
            )?),
            None => self
                .memory
                .find(&key)
                .and_then(|remembered| remembered.yaml),
        };
        self.memory.remember(Remembered {
            key,
            checked: crate::stamp::Stamp::now().unwrap_or_default(),
            report: report.clone(),
            yaml,
        });
        Ok(report)
    }

    fn effective_yaml(&self, options: &Options, uplink: &Uplink) -> String {
        self.memory
            .find(&TuneMemory::key(options, uplink))
            .and_then(|remembered| remembered.yaml)
            .unwrap_or_else(|| options.relay_yaml.clone())
    }

    /// Перехват VPN по адресам серверов. Пустой список — перехватывать нечего: не запуск.
    pub async fn start_vpn(&self, options: &Options, endpoints: Vec<String>) -> Result<()> {
        self.stop_vpn();
        if !options.vpn_enabled || endpoints.is_empty() {
            return Ok(());
        }
        let directory = Self::directory();
        let mut vpn_yaml = options.vpn_yaml.clone();
        if options.auto_ttl {
            // Probe the path to the endpoints once and bound the decoys so they
            // expire before the server (#5). A failed probe leaves the strategy
            // as written.
            if let Some(ttl) = crate::core::hops::probe_decoy_ttl(&endpoints).await {
                vpn_yaml = crate::core::hops::inject_ttl(&vpn_yaml, ttl);
            }
        }
        let file = strategy_file("vpn.yaml", &vpn_yaml)?;
        let mut command = Command::new(directory.join("volt.exe"));
        command.args([
            "-config",
            &file.to_string_lossy(),
            "-dll",
            &directory.join("WinDivert.dll").to_string_lossy(),
            "-stop-on-stdin",
        ]);
        for endpoint in &endpoints {
            command.args(["-endpoint", endpoint]);
        }
        let child = self.launch(command, "VOLT Proxy", None).await?;
        *self.vpn.lock().unwrap() = Some(Running {
            child,
            options: options.clone(),
            uplink: Uplink::default(),
            endpoints,
        });
        Ok(())
    }

    /// Помощник умер не по нашей команде: слот занят, процесс вышел (D-178).
    pub fn crashed(&self) -> bool {
        [&self.relay, &self.vpn]
            .iter()
            .any(|slot| slot.lock().unwrap().is_some() && !alive(slot))
    }

    /// Поднять умерших так же, как они были запущены. Подбор стратегии не повторяется:
    /// его результат уже в кэше, а падение — не повод гонять проверки.
    pub async fn revive(&self) -> Result<()> {
        let relay = dead(&self.relay);
        let vpn = dead(&self.vpn);
        if let Some(running) = relay {
            self.spawn_relay(&running.options, &running.uplink).await?;
        }
        if let Some(running) = vpn {
            self.start_vpn(&running.options, running.endpoints).await?;
        }
        Ok(())
    }

    async fn launch(
        &self,
        mut command: Command,
        name: &str,
        stats: Option<Arc<Mutex<Option<RelayStats>>>>,
    ) -> Result<Child> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = CoreProcess::spawn(command, false, name)?;
        let source = name.to_owned();
        self.log
            .pump_map_until(child.stderr.take().unwrap(), "\0", move |line| {
                Some(event_line(&source, &line, "error"))
            });
        let source = name.to_owned();
        let ready = self.log.pump_map_until(
            child.stdout.take().unwrap(),
            "{\"event\":\"ready\"",
            // Счётчики раз в несколько секунд — в окно, а не в журнал.
            move |line| match relay_stats(&line) {
                Some(fresh) => {
                    if let Some(sink) = &stats {
                        *sink.lock().unwrap() = Some(fresh);
                    }
                    None
                }
                None => Some(event_line(&source, &line, "info")),
            },
        );
        match tokio::time::timeout(Duration::from_secs(10), ready).await {
            Ok(Ok(line)) if serde_json::from_str::<serde_json::Value>(&line).is_ok() => {
                self.log.push(event_line(name, &line, "info"));
                Ok(child)
            }
            _ => {
                stop_child(&mut child);
                Err(AppError::CoreFailed {
                    message: format!("{name} did not become ready"),
                    log: self.log.tail(12),
                })
            }
        }
    }

    pub fn stop_vpn(&self) {
        stop_slot(&self.vpn);
    }

    /// Погашенный по нашей команде Relay ни о чём не жалуется: причина прошлого отказа
    /// больше не про него.
    pub fn stop_relay(&self) {
        stop_slot(&self.relay);
        *self.failure.lock().unwrap() = None;
        *self.stats.lock().unwrap() = None;
    }

    pub fn stop(&self) {
        self.stop_vpn();
        self.stop_relay();
    }

    pub fn route_client(route: &Route, ip: Option<std::net::IpAddr>) -> Result<reqwest::Client> {
        let proxy = reqwest::Proxy::all(format!("socks5://127.0.0.1:{}", route.relay_port))
            .map_err(|e| AppError::network(e.to_string()))?
            .basic_auth("umiray", &route.password);
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(ip) = ip {
            builder = builder.resolve(
                "api.cloudflareclient.com",
                std::net::SocketAddr::new(ip, 443),
            );
        }
        builder
            .build()
            .map_err(|e| AppError::network(e.to_string()))
    }
}

impl Drop for Volt {
    fn drop(&mut self) {
        self.stop();
    }
}

fn alive(slot: &Mutex<Option<Running>>) -> bool {
    slot.lock()
        .unwrap()
        .as_mut()
        .is_some_and(|running| matches!(running.child.try_wait(), Ok(None)))
}

fn running_options(slot: &Mutex<Option<Running>>) -> Option<Options> {
    slot.lock()
        .unwrap()
        .as_ref()
        .map(|running| running.options.clone())
}

/// Забрать из слота умершего — вместе с тем, с чем он был запущен.
fn dead(slot: &Mutex<Option<Running>>) -> Option<Running> {
    let mut slot = slot.lock().unwrap();
    let exited = slot
        .as_mut()
        .is_some_and(|running| !matches!(running.child.try_wait(), Ok(None)));
    if exited {
        slot.take()
    } else {
        None
    }
}

/// Аргументы выхода в сеть. Секрет резолвера едет окружением, а не строкой процесса.
pub(super) fn uplink_args(uplink: &Uplink) -> Vec<String> {
    let mut args = Vec::new();
    if let Some((url, _)) = &uplink.dns {
        args.extend(["-dns-server".to_owned(), url.clone()]);
    }
    if let Some(index) = uplink.interface {
        args.extend(["-interface-index".to_owned(), index.to_string()]);
    }
    args
}

fn stop_slot(slot: &Mutex<Option<Running>>) {
    if let Some(mut running) = slot.lock().unwrap().take() {
        stop_child(&mut running.child);
    }
}

pub(super) fn stop_child(child: &mut Child) {
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(b"stop\n");
    }
    CoreProcess::finish(child, Duration::from_secs(2));
}

pub(super) fn strategy_file(name: &str, text: &str) -> Result<PathBuf> {
    let directory = Paths::root().join("run/volt");
    std::fs::create_dir_all(&directory)?;
    let file = directory.join(name);
    crate::atomic::AtomicFile::write(&file, text.as_bytes())?;
    Ok(file)
}

/// Адреса слушателей Relay — клиента, не стратегии. Порт фиксированный, а не `:0`: конфиг
/// ядра с этим портом собирается раньше, чем поднимается Relay (D-176, D-184).
fn relay_yaml(options: &Options) -> Result<String> {
    let mut map = crate::yaml::Yaml::top_mapping(&options.relay_yaml)?;
    let auto = map
        .entry(serde_yaml::Value::from("auto"))
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
    let auto = auto
        .as_mapping_mut()
        .ok_or_else(|| AppError::invalid("VOLT auto must be a mapping"))?;
    auto.insert("enabled".into(), true.into());
    auto.insert(
        "listen".into(),
        format!("127.0.0.1:{}", options.auto_port).into(),
    );
    serde_yaml::to_string(&map).map_err(|e| AppError::invalid(e.to_string()))
}

/// Событие `stats` Relay; любая другая строка — `None`.
fn relay_stats(raw: &str) -> Option<RelayStats> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    if value["event"] != "stats" {
        return None;
    }
    let (stats, auto) = (&value["stats"], &value["auto"]);
    let count = |from: &serde_json::Value, key: &str| from[key].as_u64().unwrap_or(0);
    Some(RelayStats {
        connections: count(stats, "connections"),
        active: count(stats, "active"),
        modified: count(stats, "modified"),
        faked: count(stats, "faked"),
        failures: count(stats, "dial_failures") + count(stats, "capture_failures"),
        last_error: stats["last_error"]
            .as_str()
            .filter(|error| !error.is_empty())
            .map(str::to_owned),
        auto_direct: count(auto, "direct_succeeded"),
        auto_bypassed: count(auto, "fallbacks"),
        rotations: count(stats, "rotations"),
        detected_failures: count(stats, "detected_failures"),
    })
}

pub(super) fn event_line(source: &str, raw: &str, fallback: &str) -> String {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok();
    let event = value.as_ref().and_then(|v| v["event"].as_str());
    let mut level = fallback;
    let message = match event {
        Some("ready") => {
            let value = value.as_ref().unwrap();
            let addresses: Vec<&str> = ["listen", "auto_listen"]
                .iter()
                .filter_map(|key| value[key].as_str())
                .filter(|address| !address.is_empty())
                .collect();
            let mut message = if addresses.is_empty() {
                format!("{source} started")
            } else {
                format!("{source} started; listening on {}", addresses.join(", "))
            };
            // С чем Relay вышел в сеть (D-176): по этой строке видно, что TUN он обходит.
            if let Some(index) = value["interface_index"].as_u64().filter(|index| *index > 0) {
                message.push_str(&format!("; adapter {index}"));
            }
            if let Some(dns) = value["dns_server"].as_str().filter(|dns| !dns.is_empty()) {
                message.push_str(&format!("; DNS {dns}"));
            }
            // Версия сборки `umiray-core`: по ней видно, что лежит устаревший бинарник (GOTCHAS).
            if let Some(version) = value["version"]
                .as_str()
                .filter(|version| !version.is_empty())
            {
                message.push_str(&format!("; version {version}"));
            }
            message
        }
        Some("stopped") => {
            let stats = &value.as_ref().unwrap()["stats"];
            let count = |key: &str| stats[key].as_u64().unwrap_or(0);
            if count("capture_failures") > 0 {
                level = "error";
            } else if count("dial_failures") > 0 {
                level = "warn";
            }
            let mut message = format!("{source} stopped; packets captured: {}, transformed: {}, passed: {}, injected: {}, noise: {}, capture failures: {}",
                count("captured"), count("modified"), count("passed"), count("injected"), count("faked"), count("capture_failures"));
            if stats["connections"].is_number() {
                message.push_str(&format!(
                    "; connections: {}, failed: {}",
                    count("connections"),
                    count("dial_failures")
                ));
            }
            if let Some(error) = stats["last_error"]
                .as_str()
                .filter(|error| !error.is_empty())
            {
                message.push_str(&format!("; last error: {error}"));
            }
            let auto = &value.as_ref().unwrap()["auto"];
            if auto.is_object() {
                let count = |key: &str| auto[key].as_u64().unwrap_or(0);
                message.push_str(&format!(
                    "; AUTO direct: {}, Relay: {}, failed: {}, cache hits: {}",
                    count("direct_succeeded"),
                    count("fallbacks"),
                    count("fallback_failures"),
                    count("cache_hits")
                ));
            }
            message
        }
        _ => raw.to_owned(),
    };
    format!(
        "time=\"{}\" level={level} msg={}",
        crate::stamp::Stamp::local(),
        serde_json::to_string(&format!("volt: {message}")).unwrap()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Счётчики на ходу — в окно; остальные события — в журнал, как были (D-188).
    #[test]
    fn stats_events_feed_the_window_not_the_log() {
        let line = r#"{"event":"stats","stats":{"connections":34,"active":2,"modified":120,"faked":40,"dial_failures":1,"capture_failures":1,"last_error":"timeout"},"auto":{"direct_succeeded":20,"fallbacks":9}}"#;
        assert_eq!(
            relay_stats(line),
            Some(RelayStats {
                connections: 34,
                active: 2,
                modified: 120,
                faked: 40,
                failures: 2,
                last_error: Some("timeout".into()),
                auto_direct: 20,
                auto_bypassed: 9,
                rotations: 0,
                detected_failures: 0,
            })
        );
        assert_eq!(
            relay_stats(r#"{"event":"stats","stats":{"last_error":""}}"#),
            Some(RelayStats::default()),
            "AUTO нет, ошибки нет — нули"
        );
        for other in [
            r#"{"event":"stopped","stats":{"connections":34}}"#,
            r#"{"event":"ready"}"#,
            "plain text",
        ] {
            assert_eq!(relay_stats(other), None, "{other}");
        }
    }

    #[test]
    fn lifecycle_logs_are_readable_and_keep_error_levels() {
        let ready = event_line(
            "VOLT Relay",
            r#"{"event":"ready","password":"secret"}"#,
            "info",
        );
        assert!(ready.contains("level=info"));
        assert!(ready.contains("volt: VOLT Relay started"));
        assert!(!ready.contains("secret"));
        let bound = event_line(
            "VOLT Relay",
            r#"{"event":"ready","version":"v0.1.0","listen":"127.0.0.1:3101","interface_index":12,"dns_server":"http://127.0.0.1:9090/dns/query"}"#,
            "info",
        );
        assert!(bound.contains("adapter 12; DNS http://127.0.0.1:9090/dns/query; version v0.1.0"));
        let stopped = event_line(
            "VOLT Proxy",
            r#"{"event":"stopped","stats":{"captured":8,"modified":2,"capture_failures":1}}"#,
            "info",
        );
        assert!(stopped.contains("level=error"));
        assert!(stopped.contains("packets captured: 8, transformed: 2"));
    }

    #[test]
    fn listener_addresses_are_owned_by_the_client() {
        let options = Options {
            auto_port: 4567,
            ..Options::default()
        };
        let yaml = relay_yaml(&options).unwrap();
        let config: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config["auto"]["listen"].as_str(), Some("127.0.0.1:4567"));
        assert_eq!(config["auto"]["enabled"].as_bool(), Some(true));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn waits_for_ready_and_stops_the_owned_process_through_stdin() {
        let volt = Volt::default();
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "Write-Output '{\"event\":\"ready\"}'; if ([Console]::ReadLine() -eq 'stop') { Write-Output '{\"event\":\"stopped\"}'; exit 0 }; exit 1"]);
        let mut child = volt.launch(command, "test helper", None).await.unwrap();
        assert!(child.try_wait().unwrap().is_none());
        stop_child(&mut child);
        assert!(child.try_wait().unwrap().unwrap().success());
        let endpoints = vec!["45.86.245.83:443".to_owned()];
        *volt.vpn.lock().unwrap() = Some(Running {
            child,
            options: Options::default(),
            uplink: Uplink::default(),
            endpoints: endpoints.clone(),
        });
        assert!(
            volt.crashed(),
            "a process that exited on its own is a crash"
        );
        let taken = dead(&volt.vpn).expect("the dead helper leaves the slot");
        assert_eq!(
            taken.endpoints, endpoints,
            "and keeps what to revive it with"
        );
        assert!(!volt.crashed() && volt.vpn_options().is_none());
    }

    #[test]
    fn the_resolver_secret_travels_outside_the_command_line() {
        let uplink = Uplink {
            dns: Some(("http://127.0.0.1:9090/dns/query".into(), "secret".into())),
            interface: Some(7),
            network: "Ethernet|192.168.1.1".into(),
        };
        let args = uplink_args(&uplink);
        assert_eq!(
            args,
            [
                "-dns-server",
                "http://127.0.0.1:9090/dns/query",
                "-interface-index",
                "7"
            ]
        );
        assert!(!args.iter().any(|arg| arg.contains("secret")));
        assert!(uplink_args(&Uplink::default()).is_empty());
    }

    #[test]
    fn the_relay_password_lives_for_the_session_and_only_when_direct_is_on() {
        let volt = Volt::default();
        let off = Options::default();
        assert!(volt.route(&off).unwrap().is_none());
        let on = Options {
            direct_enabled: true,
            ..Options::default()
        };
        let first = volt.route(&on).unwrap().unwrap();
        let second = volt.route(&on).unwrap().unwrap();
        assert_eq!(first.password, second.password, "one config, one password");
        assert_eq!(first.password.len(), 32);
        assert_ne!(
            Volt::default().route(&on).unwrap().unwrap().password,
            first.password
        );
    }
}
