//! VOLT вокруг mihomo: когда помощники поднимаются, с чем выходят в сеть, как до них
//! доезжает правка (D-176–D-178).
//!
//! Помощники живут вместе с ядром: перехват VPN ставится до первого рукопожатия (хук «до»),
//! Relay — после ядра, когда есть его резолвер (хук «после»); гасит их остановка ядра.

use std::collections::BTreeSet;
use std::net::SocketAddr;

use tauri::{AppHandle, Manager};

use crate::app::state::AppState;
use crate::config::volt::{real_ip, Options};
use crate::core::volt::{Snapshot, Uplink, Volt};
use crate::error::{AppError, Result};
use crate::nodes::sources::SourceStore;
use crate::system::net::{NetInfo, Route};

/// Сколько адресов серверов берёт перехват VPN: длиннее фильтр WinDivert не собирает.
const ENDPOINTS: usize = 64;

/// Включение без прав сохраняется: окно предлагает перезапуск с ними (D-187).
const NEEDS_ADMIN: &str =
    "VOLT работает только от администратора. Включение сохранено и заработает после перезапуска с правами";

/// Сохранить настройки из окна и довести до работающего ядра. Не встало — вернуть прежние.
/// Галка в трее — по тому, что в итоге записано (D-191).
pub async fn update(app: &AppHandle, state: &AppState, options: Options) -> Result<Snapshot> {
    let result = save(app, state, options).await;
    show_in_tray(app);
    result
}

/// Галка «Обход» в трее (D-191): записанное включение; где VOLT нет — пункта нет.
pub fn show_in_tray(app: &AppHandle) {
    use crate::system::features::{Feature, Features};
    let on = Features::has(Feature::Volt)
        .then(|| Options::get().is_ok_and(|options| options.direct_enabled));
    crate::app::tray::Tray::set_bypass(app, on);
}

/// «Обход» из трея — то же, что тумблер в «Соединении». Окна может не быть: отказ — в лог,
/// как у питания из трея.
pub fn toggle_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = match Options::get() {
            Ok(mut options) => {
                options.direct_enabled = !options.direct_enabled;
                update(&app, &state, options).await.map(|_| ())
            }
            Err(why) => Err(why),
        };
        if let Err(why) = result {
            state.note("error", &format!("Обход из трея: {why}"));
        }
    });
}

async fn save(app: &AppHandle, state: &AppState, options: Options) -> Result<Snapshot> {
    let _transition = state.connection.lock().await;
    // Включение без бинарников качает их (D-181): одна галка, а не ручное копирование.
    if options.active() && !crate::core::volt::download::VoltDownload::present() {
        crate::core::volt::download::VoltDownload::install().await?;
    }
    Volt::check(&options)?;
    if options.active() && !crate::system::elevation::Elevation::is_elevated() {
        options.write()?;
        return Err(AppError::NeedsElevation {
            message: NEEDS_ADMIN.into(),
        });
    }
    let previous = Options::get()?;
    options.write()?;
    if let Err(error) = apply(app, state).await {
        previous.write()?;
        if let Err(rollback) = apply(app, state).await {
            state.note("error", &format!("VOLT rollback failed: {rollback}"));
        }
        return Err(error);
    }
    state.volt.snapshot()
}

/// Строгий порядок окна: помощники, ядро, уборка. Отказ помощника здесь — ответ человеку.
async fn apply(app: &AppHandle, state: &AppState) -> Result<()> {
    prepare(state).await?;
    state.connection.apply_locked(app, state).await?;
    Ok(())
}

/// Поднять или перезапустить помощников, которых требуют сохранённые настройки (D-177).
/// Стоит до перезагрузки ядра: та направит в них трафик. Ядро не работает — помощникам
/// жить незачем.
pub async fn prepare(state: &AppState) -> Result<()> {
    if !state.mihomo.status().running {
        state.volt.stop();
        return Ok(());
    }
    let saved = Options::get()?;
    let relay = state.volt.relay_options();
    if saved.direct_enabled
        && relay
            .as_ref()
            .is_none_or(|running| relay_changed(running, &saved))
    {
        if relay.is_some() {
            state.volt.restart_relay(&saved).await?;
        } else {
            state.volt.start_relay(&saved, &uplink(state)).await?;
        }
    }
    let vpn = state.volt.vpn_options();
    if saved.vpn_enabled
        && vpn
            .as_ref()
            .is_none_or(|running| vpn_changed(running, &saved))
    {
        start_vpn(state, &saved).await?;
    }
    Ok(())
}

/// Погасить ненужных — после перезагрузки ядра: маршрутов к ним уже нет.
pub fn settle(state: &AppState) -> Result<()> {
    let saved = Options::get()?;
    if !saved.direct_enabled {
        state.volt.stop_relay();
    }
    if !saved.vpn_enabled {
        state.volt.stop_vpn();
    }
    Ok(())
}

/// Что из настроек читает Relay. Режим `DIRECT-AUTO`/`DIRECT-VOLT` — выбор ядра, не его.
fn relay_changed(running: &Options, saved: &Options) -> bool {
    running.relay_yaml != saved.relay_yaml
        || running.auto_select != saved.auto_select
        || running.probe_urls != saved.probe_urls
        || running.relay_port != saved.relay_port
        || running.auto_port != saved.auto_port
}

fn vpn_changed(running: &Options, saved: &Options) -> bool {
    running.vpn_yaml != saved.vpn_yaml || running.endpoints != saved.endpoints
}

/// Хук запуска ядра «после»: у ядра уже есть резолвер, и Relay спрашивает имена у него.
pub async fn start_relay(state: &AppState) -> Result<()> {
    let options = Options::get()?;
    if !options.direct_enabled {
        state.volt.stop_relay();
        return Ok(());
    }
    state.volt.start_relay(&options, &uplink(state)).await
}

/// Хук запуска ядра «до»: перехват должен стоять раньше первого рукопожатия с сервером.
pub async fn prepare_vpn(state: &AppState) -> Result<()> {
    let options = Options::get()?;
    if !options.vpn_enabled {
        state.volt.stop_vpn();
        return Ok(());
    }
    start_vpn(state, &options).await
}

/// С чем Relay выходит в сеть (D-176): резолвер работающего ядра и физический адаптер —
/// маршрут по умолчанию с наименьшей метрикой, кроме TUN самого ядра.
pub fn uplink(state: &AppState) -> Uplink {
    let tun = state.mihomo.status().device;
    let routes = NetInfo::default_routes().unwrap_or_default();
    let route = physical(&routes, tun.as_deref());
    Uplink {
        dns: state.mihomo.resolver(),
        interface: route.map(|route| route.index),
        network: route
            .map(|route| format!("{}|{}", route.adapter, route.gateway))
            .unwrap_or_default(),
    }
}

/// Маршруты приходят отсортированными по метрике (`NetInfo::default_routes`).
fn physical<'a>(routes: &'a [Route], tun: Option<&str>) -> Option<&'a Route> {
    routes
        .iter()
        .find(|route| route.index != 0 && Some(route.adapter.as_str()) != tun)
}

/// Сеть сменилась под работающим Relay (D-112): он привязан к адаптеру, и на прежнем
/// его прямые соединения уходили бы в никуда. Поднимается заново с новым выходом — подбор,
/// если включён, проверит новую сеть (D-185).
pub async fn network_changed(state: &AppState) {
    let (Some(options), Some(running)) = (state.volt.relay_options(), state.volt.relay_uplink())
    else {
        return;
    };
    let fresh = uplink(state);
    if fresh == running {
        return;
    }
    let _transition = state.connection.lock().await;
    match state.volt.start_relay(&options, &fresh).await {
        Ok(()) => state.note(
            "info",
            &format!(
                "VOLT: сеть сменилась — Relay поднят заново ({})",
                fresh.network
            ),
        ),
        Err(why) => state.note(
            "error",
            &format!("VOLT: сеть сменилась, Relay не поднялся: {why}"),
        ),
    }
}

async fn start_vpn(state: &AppState, options: &Options) -> Result<()> {
    let endpoints = if options.endpoints.is_empty() {
        resolve_endpoints(state).await
    } else {
        options.endpoints.clone()
    };
    if endpoints.is_empty() {
        state.note(
            "warning",
            "VOLT для прокси: адресов серверов не нашлось — перехватывать нечего",
        );
    }
    state.volt.start_vpn(options, endpoints).await
}

/// Узел источника, к которому ходит ядро.
struct Server {
    name: String,
    host: String,
    port: u16,
}

/// Адреса серверов всех источников (D-179). Ни одна беда здесь не отменяет подключение:
/// сервер без адреса и лишние адреса уходят строкой в лог, остальные перехватываются.
async fn resolve_endpoints(state: &AppState) -> Vec<String> {
    let selected = state.routing.selected(state).await;
    let mut servers = servers();
    // Выбранный узел — первым: если адресов больше предела, его соединения обработаются.
    servers.sort_by_key(|server| Some(&server.name) != selected.as_ref());
    let mut lookups = tokio::task::JoinSet::new();
    for (order, server) in servers.into_iter().enumerate() {
        lookups.spawn(async move {
            let result = lookup(&server.host, server.port).await;
            (order, server.host, result)
        });
    }
    let mut found = Vec::new();
    let mut failed = Vec::new();
    while let Some(joined) = lookups.join_next().await {
        let Ok((order, host, result)) = joined else {
            continue;
        };
        match result {
            Ok(addresses) => found.extend(addresses.into_iter().map(|address| (order, address))),
            Err(why) => failed.push(format!("{host} — {why}")),
        }
    }
    if !failed.is_empty() {
        failed.sort();
        state.note(
            "warning",
            &format!(
                "VOLT для прокси: у {} серверов нет адреса, их соединения пойдут без обработки: {}",
                failed.len(),
                failed.join("; ")
            ),
        );
    }
    let (endpoints, total) = in_order(found, ENDPOINTS);
    if total > ENDPOINTS {
        state.note(
            "warning",
            &format!(
                "VOLT для прокси: адресов {total}, перехватываются первые {ENDPOINTS}, выбранный \
                 узел — первым. Нужные серверы задаются в настройках VOLT."
            ),
        );
    }
    endpoints
}

/// Адреса по порядку узлов без повторов, не больше `limit`; второе — сколько было всего.
fn in_order(mut found: Vec<(usize, SocketAddr)>, limit: usize) -> (Vec<String>, usize) {
    found.sort();
    let mut seen = BTreeSet::new();
    let unique: Vec<String> = found
        .into_iter()
        .filter(|(_, address)| seen.insert(*address))
        .map(|(_, address)| address.to_string())
        .collect();
    let total = unique.len();
    (unique.into_iter().take(limit).collect(), total)
}

fn servers() -> Vec<Server> {
    let mut servers = Vec::new();
    for source in SourceStore::list() {
        let Ok(content) =
            serde_yaml::from_str::<serde_yaml::Value>(&SourceStore::content(&source.id))
        else {
            continue;
        };
        let Some(proxies) = content
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for proxy in proxies {
            let field = |key: &str| proxy.get(key).and_then(serde_yaml::Value::as_str);
            let (Some(name), Some(host)) = (field("name"), field("server")) else {
                continue;
            };
            let Some(port) = proxy
                .get("port")
                .and_then(serde_yaml::Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .filter(|port| *port > 0)
            else {
                continue;
            };
            servers.push(Server {
                name: name.to_owned(),
                host: host.to_owned(),
                port,
            });
        }
    }
    servers
}

async fn lookup(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let found: Vec<SocketAddr> = if let Ok(ip) = host.parse() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| AppError::network("DNS не ответил за 5 с"))?
        .map_err(|e| AppError::network(e.to_string()))?
        .collect()
    };
    let real: Vec<SocketAddr> = found
        .into_iter()
        .filter(|address| real_ip(address.ip()))
        .collect();
    if real.is_empty() {
        return Err(AppError::network("только подменные или служебные адреса"));
    }
    Ok(real)
}

pub async fn bootstrap_client(state: &AppState) -> Result<Option<reqwest::Client>> {
    let options = Options::get()?;
    if !options.active() {
        return Ok(None);
    }
    let route = state.volt.bootstrap_relay().await?;
    let addresses: BTreeSet<_> = if options.bootstrap_ips.is_empty() {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host(("api.cloudflareclient.com", 443)),
        )
        .await
        .map_err(|_| AppError::network("Cloudflare DNS timed out"))?
        .map_err(|e| AppError::network(format!("Cloudflare DNS failed: {e}")))?
        .map(|address| address.ip())
        .filter(|ip| real_ip(*ip))
        .take(16)
        .collect()
    } else {
        options.bootstrap_ips.into_iter().collect()
    };
    let mut probes = tokio::task::JoinSet::new();
    for ip in addresses {
        let client = Volt::route_client(&route, Some(ip))?;
        probes.spawn(async move {
            let response = client
                .get("https://api.cloudflareclient.com/v0a4471")
                .send()
                .await
                .ok()?;
            response.bytes().await.ok()?;
            Some(client)
        });
    }
    // Probe TLS with a harmless GET; registration POST must never be raced or replayed.
    while let Some(result) = probes.join_next().await {
        if let Ok(Some(client)) = result {
            return Ok(Some(client));
        }
    }
    Err(AppError::network("Cloudflare TLS bootstrap through VOLT failed; check the relay strategy and real DNS addresses"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Строка уходит человеку: перенос в исходнике не должен оставлять в ней дыру.
    #[test]
    fn the_admin_message_reads_as_one_sentence_pair() {
        assert!(!NEEDS_ADMIN.contains("  "));
    }

    fn route(adapter: &str, metric: u32, index: u32) -> Route {
        Route {
            adapter: adapter.into(),
            gateway: String::new(),
            metric,
            index,
        }
    }

    /// Под TUN маршрут ядра выигрывает по метрике, но Relay обязан выйти мимо него.
    #[test]
    fn the_uplink_skips_the_core_tunnel() {
        let index = |routes: &[Route], tun| physical(routes, tun).map(|route| route.index);
        let routes = [route("Meta", 0, 58), route("Ethernet", 35, 12)];
        assert_eq!(index(&routes, Some("Meta")), Some(12));
        assert_eq!(index(&routes, None), Some(58), "без TUN — как решит ОС");
        assert_eq!(index(&[route("Meta", 0, 58)], Some("Meta")), None);
        assert_eq!(
            index(&[route("eth0", 0, 0)], None),
            None,
            "Linux номера не знает"
        );
    }

    /// Выбранный узел первым, повторы один раз, лишнее отрезано, но посчитано.
    #[test]
    fn endpoints_keep_the_selected_node_within_the_limit() {
        let address = |text: &str| text.parse::<SocketAddr>().unwrap();
        let found = vec![
            (2, address("203.0.113.3:443")),
            (0, address("203.0.113.9:443")),
            (1, address("203.0.113.1:443")),
            (1, address("203.0.113.9:443")),
        ];
        let (endpoints, total) = in_order(found, 2);
        assert_eq!(endpoints, ["203.0.113.9:443", "203.0.113.1:443"]);
        assert_eq!(total, 3);
    }

    /// Правка стратегии не трогает ядро, а режим выхода — не трогает Relay.
    #[test]
    fn each_helper_restarts_only_for_its_own_settings() {
        let base = Options::default();
        let relay = Options {
            relay_yaml: "version: 1\nprofiles: []\n".into(),
            ..base.clone()
        };
        assert!(relay_changed(&base, &relay) && !vpn_changed(&base, &relay));
        let vpn = Options {
            vpn_yaml: "version: 1\nprofiles: []\n".into(),
            ..base.clone()
        };
        assert!(vpn_changed(&base, &vpn) && !relay_changed(&base, &vpn));
        let mode = Options {
            mode: crate::config::volt::Mode::Relay,
            bootstrap_ips: vec!["1.1.1.1".parse().unwrap()],
            ..base.clone()
        };
        assert!(!relay_changed(&base, &mode) && !vpn_changed(&base, &mode));
    }
}
