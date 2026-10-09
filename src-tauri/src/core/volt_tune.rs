use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::config::volt::Options;
use crate::core::process::{CoreProcess, LogRing};
use crate::error::{AppError, Result};
use crate::stamp::Stamp;

const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const BODY_LIMIT: usize = 1_048_576;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub url: String,
    pub attempt: u8,
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateResult {
    pub id: String,
    pub label: String,
    pub successes: usize,
    pub total: usize,
    pub latency_ms: Option<u64>,
    pub checks: Vec<ProbeResult>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuneReport {
    pub checked_at: String,
    pub scope: String,
    pub urls: Vec<String>,
    pub selected: Option<String>,
    /// Всё открылось напрямую, без VOLT: блокировки нет, стратегия не менялась (D-185).
    #[serde(default)]
    pub unblocked: bool,
    pub candidates: Vec<CandidateResult>,
}

pub(super) struct Candidate {
    id: &'static str,
    label: &'static str,
    pub yaml: String,
}

pub(super) async fn tune_logged(
    options: &Options,
    uplink: &super::volt::Uplink,
    log: &LogRing,
) -> Result<TuneReport> {
    options.validate()?;
    if !cfg!(windows) {
        return Err(AppError::invalid(
            "VOLT strategy selection requires Windows",
        ));
    }
    if !crate::system::elevation::Elevation::is_elevated() {
        return Err(AppError::NeedsElevation {
            message: "VOLT strategy selection requires administrator privileges".into(),
        });
    }
    let mut candidates = candidates(&options.relay_yaml)?;
    // Контроль первым (D-185): что открывается напрямую, обходить незачем.
    candidates.insert(
        0,
        Candidate {
            id: DIRECT,
            label: "Direct, without VOLT",
            yaml: DIRECT_YAML.into(),
        },
    );
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        log.push(super::volt::event_line(
            "VOLT strategy selection",
            &format!(
                "checking {} against {} HTTPS resources twice",
                candidate.label,
                options.probe_urls.len()
            ),
            "info",
        ));
        let checks = match ProbeRelay::start(uplink, &probe_yaml(&candidate.yaml)?, log).await {
            Ok(relay) => probe_all(&relay.address, &relay.password, &options.probe_urls).await,
            Err(error) => failed_checks(&options.probe_urls, &error.to_string()),
        };
        let successes = checks.iter().filter(|check| check.ok).count();
        let total = options.probe_urls.len() * 2;
        let latency_ms = (successes > 0).then(|| {
            checks
                .iter()
                .filter(|check| check.ok)
                .map(|check| check.latency_ms)
                .sum::<u64>()
                / successes as u64
        });
        log.push(super::volt::event_line(
            "VOLT strategy selection",
            &format!("{}: {successes}/{total} successful checks", candidate.label),
            if successes == total { "info" } else { "warn" },
        ));
        results.push(CandidateResult {
            id: candidate.id.into(),
            label: candidate.label.into(),
            successes,
            total,
            latency_ms,
            checks,
        });
        if verdict(&results).1 {
            break;
        }
    }
    let (selected, unblocked) = verdict(&results);
    log.push(super::volt::event_line(
        "VOLT strategy selection",
        &if unblocked {
            "everything opened directly; nothing is blocked, the strategy is kept".to_owned()
        } else {
            selected
                .as_ref()
                .map(|id| format!("selected {id}; all HTTPS checks passed"))
                .unwrap_or_else(|| {
                    "no candidate passed every HTTPS check; keeping the previous strategy".into()
                })
        },
        if selected.is_some() || unblocked {
            "info"
        } else {
            "warn"
        },
    ));
    Ok(TuneReport {
        checked_at: Stamp::utc(),
        scope: "direct-https".into(),
        urls: options.probe_urls.clone(),
        selected,
        unblocked,
        candidates: results,
    })
}

/// Итог двух попыток к одному адресу (D-190).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteCheck {
    pub url: String,
    /// Что добавить в свои домены: имя без `www.`; у адреса-IP — нет.
    pub domain: Option<String>,
    pub direct: Attempt,
    pub bypass: Attempt,
}

/// Адрес от человека: имя или HTTPS-ссылка. Без схемы — `https://<имя>/`.
pub(crate) fn site_url(input: &str) -> Result<(String, Option<String>)> {
    let input = input.trim();
    let url = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}/")
    };
    crate::config::volt::validate_probe_urls(std::slice::from_ref(&url))?;
    let host = reqwest::Url::parse(&url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    let domain =
        (host.parse::<std::net::IpAddr>().is_err() && !host.starts_with('[') && host.contains('.'))
            .then(|| host.strip_prefix("www.").unwrap_or(&host).to_owned());
    Ok((url, domain))
}

/// Две попытки — одно слово: открылся, только если обе; время — среднее удачных.
fn attempt(checks: &[ProbeResult]) -> Attempt {
    let ok: Vec<u64> = checks
        .iter()
        .filter(|check| check.ok)
        .map(|check| check.latency_ms)
        .collect();
    Attempt {
        ok: !checks.is_empty() && ok.len() == checks.len(),
        latency_ms: (!ok.is_empty()).then(|| ok.iter().sum::<u64>() / ok.len() as u64),
        error: checks.iter().find_map(|check| check.error.clone()),
    }
}

/// «Проверить сайт» (D-190): напрямую и с этой стратегией, тем же временным Relay, что
/// у подбора.
pub(super) async fn check_site(
    input: &str,
    strategy: &str,
    uplink: &super::volt::Uplink,
    log: &LogRing,
) -> Result<SiteCheck> {
    let (url, domain) = site_url(input)?;
    if !crate::system::elevation::Elevation::is_elevated() {
        return Err(AppError::NeedsElevation {
            message: "VOLT site check requires administrator privileges".into(),
        });
    }
    let urls = [url.clone()];
    let run = async |yaml: &str| -> Result<Attempt> {
        let relay = ProbeRelay::start(uplink, &probe_yaml(yaml)?, log).await?;
        Ok(attempt(
            &probe_all(&relay.address, &relay.password, &urls).await,
        ))
    };
    let direct = run(DIRECT_YAML).await?;
    let bypass = run(strategy).await?;
    log.push(super::volt::event_line(
        "VOLT site check",
        &format!(
            "{url}: directly {}, through the bypass {}",
            if direct.ok { "opens" } else { "fails" },
            if bypass.ok { "opens" } else { "fails" }
        ),
        "info",
    ));
    Ok(SiteCheck {
        url,
        domain,
        direct,
        bypass,
    })
}

/// Контроль без VOLT: Relay пропускает соединения как есть — тот же адаптер и резолвер,
/// что у стратегий, без разрезов и шума.
const DIRECT: &str = "direct";
const DIRECT_YAML: &str = "version: 1\ndefault_action: pass\nprofiles:\n  - name: direct\n    match: {network: tcp}\n    stages:\n      - action: pass\n";

/// Победитель и «блокировки нет». Контроль прошёл всё — выбирать нечего; иначе лучший
/// из стратегий, прошедший всё.
fn verdict(results: &[CandidateResult]) -> (Option<String>, bool) {
    let unblocked = results.first().is_some_and(|direct| {
        direct.id == DIRECT && direct.total > 0 && direct.successes == direct.total
    });
    if unblocked {
        return (None, true);
    }
    let strategies: Vec<CandidateResult> = results
        .iter()
        .filter(|result| result.id != DIRECT)
        .cloned()
        .collect();
    (winner(&strategies).map(|result| result.id.clone()), false)
}

/// Шаблон из окна и победитель подбора — одно преобразование (D-180): меняются только
/// этапы TLS, остальное в стратегии человека остаётся как было.
pub(crate) fn selected_yaml(text: &str, id: &str) -> Result<String> {
    crate::config::volt::strategy_parse(text)?;
    if id == "current" {
        return Ok(text.into());
    }
    let (action, noise) = match id {
        "tls-split" => ("split", false),
        "tls-disorder" => ("disorder", false),
        "tls-fake" => ("fake", true),
        "tls-fake-split" => ("split", true),
        "tls-auto" => ("disorder", true),
        _ => return Err(AppError::invalid("Unknown VOLT strategy candidate")),
    };
    template_yaml(&crate::yaml::Yaml::top_mapping(text)?, action, noise)?
        .ok_or_else(|| AppError::invalid("VOLT strategy has no TLS transforms for this preset"))
}

fn winner(results: &[CandidateResult]) -> Option<&CandidateResult> {
    results
        .iter()
        .filter(|result| result.total > 0 && result.successes == result.total)
        .min_by_key(|result| result.latency_ms.unwrap_or(u64::MAX))
}

fn candidates(text: &str) -> Result<Vec<Candidate>> {
    let map = crate::yaml::Yaml::top_mapping(text)?;
    let mut candidates = vec![Candidate {
        id: "current",
        label: "Current strategy",
        yaml: text.into(),
    }];
    for (id, label, action, noise) in [
        ("tls-split", "TLS fragmentation", "split", false),
        ("tls-disorder", "TLS packet reordering", "disorder", false),
        ("tls-auto", "TLS decoys and reordering", "disorder", true),
    ] {
        if let Some(yaml) = template_yaml(&map, action, noise)? {
            candidates.push(Candidate { id, label, yaml });
        }
    }
    Ok(candidates)
}

fn template_yaml(map: &Mapping, action: &str, noise: bool) -> Result<Option<String>> {
    let mut modified = map.clone();
    let profiles = modified
        .get_mut(Value::from("profiles"))
        .and_then(Value::as_sequence_mut)
        .ok_or_else(|| AppError::invalid("VOLT strategy selection requires profiles"))?;
    let mut changed = false;
    for profile in profiles {
        let Some(profile) = profile.as_mapping_mut() else {
            continue;
        };
        let Some(matcher) = profile.get(Value::from("match")) else {
            continue;
        };
        if matcher.get("network").and_then(Value::as_str) != Some("tcp") {
            continue;
        }
        let matched_tls = tls_only(matcher);
        if let Some(stages) = profile
            .get_mut(Value::from("stages"))
            .and_then(Value::as_sequence_mut)
        {
            for stage in stages {
                if only_tls(stage.get("payloads"))
                    || matched_tls
                        && stage
                            .get("payloads")
                            .and_then(Value::as_sequence)
                            .is_none_or(|items| items.is_empty())
                {
                    replace_tls(stage, action, noise)?;
                    changed = true;
                }
            }
        } else if let Some(transform) = profile.get_mut(Value::from("transform")) {
            if only_tls(transform.get("payloads"))
                || matched_tls
                    && transform
                        .get("payloads")
                        .and_then(Value::as_sequence)
                        .is_none_or(|items| items.is_empty())
            {
                replace_tls(transform, action, noise)?;
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(None);
    }
    serde_yaml::to_string(&modified)
        .map(Some)
        .map_err(|error| AppError::invalid(error.to_string()))
}

fn only_tls(value: Option<&Value>) -> bool {
    value.and_then(Value::as_sequence).is_some_and(|items| {
        !items.is_empty() && items.iter().all(|item| item.as_str() == Some("tls"))
    })
}

fn tls_only(matcher: &Value) -> bool {
    if only_tls(matcher.get("payloads")) {
        return true;
    }
    if matcher
        .get("all")
        .and_then(Value::as_sequence)
        .is_some_and(|items| items.iter().any(tls_only))
    {
        return true;
    }
    matcher
        .get("any")
        .and_then(Value::as_sequence)
        .is_some_and(|items| !items.is_empty() && items.iter().all(tls_only))
}

fn replace_tls(value: &mut Value, action: &str, noise: bool) -> Result<()> {
    let map = value
        .as_mapping_mut()
        .ok_or_else(|| AppError::invalid("VOLT TLS transform must be a mapping"))?;
    map.insert("action".into(), action.into());
    if action == "fake" {
        map.remove(Value::from("positions"));
    } else {
        map.insert(
            "positions".into(),
            Value::Sequence(vec!["1".into(), "midsld".into()]),
        );
    }
    map.insert("packet_limit".into(), 2.into());
    map.insert("byte_limit".into(), 16384.into());
    map.remove(Value::from("sequence_overlap"));
    if noise {
        let mut fake = map
            .get(Value::from("fake"))
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        fake.insert("kind".into(), "tls-auto".into());
        fake.insert("repeats".into(), 3.into());
        fake.remove(Value::from("ttl"));
        fake.remove(Value::from("hex"));
        fake.remove(Value::from("payload_file"));
        if !fake.contains_key(Value::from("server_name"))
            && !fake.contains_key(Value::from("server_names"))
            && !fake.contains_key(Value::from("server_name_file"))
            && !fake.contains_key(Value::from("server_name_source"))
        {
            fake.insert("server_name_source".into(), "noise-extended".into());
        }
        map.insert("fake".into(), Value::Mapping(fake));
    } else {
        map.remove(Value::from("fake"));
    }
    Ok(())
}

fn probe_yaml(text: &str) -> Result<String> {
    let mut map = crate::yaml::Yaml::top_mapping(text)?;
    for section in ["auto", "udp"] {
        let value = map
            .entry(Value::from(section))
            .or_insert_with(|| Value::Mapping(Mapping::default()));
        let fields = value
            .as_mapping_mut()
            .ok_or_else(|| AppError::invalid(format!("VOLT {section} must be a mapping")))?;
        fields.insert("enabled".into(), false.into());
    }
    serde_yaml::to_string(&map).map_err(|error| AppError::invalid(error.to_string()))
}

struct ProbeRelay {
    child: Option<Child>,
    file: PathBuf,
    address: std::net::SocketAddr,
    password: String,
}

impl ProbeRelay {
    async fn start(uplink: &super::volt::Uplink, yaml: &str, log: &LogRing) -> Result<Self> {
        let directory = super::volt::Volt::directory();
        let password = format!("{}{}", Stamp::id()?, Stamp::id()?);
        let file = super::volt::strategy_file(&format!("probe-{}.yaml", Stamp::id()?), yaml)?;
        let mut relay = Self {
            child: None,
            file,
            address: ([127, 0, 0, 1], 0).into(),
            password,
        };
        let mut command = Command::new(directory.join("volt-relay.exe"));
        command.args([
            "-config",
            &relay.file.to_string_lossy(),
            "-listen",
            "127.0.0.1:0",
            "-username",
            "umiray-probe",
            "-dll",
            &directory.join("WinDivert.dll").to_string_lossy(),
            "-max-clients",
            "4",
            "-stop-on-stdin",
        ]);
        // Проверка видит сеть так же, как рабочий Relay: тот же резолвер и адаптер (D-176).
        command.args(super::volt::uplink_args(uplink));
        if let Some((_, token)) = &uplink.dns {
            command.env("UMIRAY_VOLT_DNS_TOKEN", token);
        }
        command
            .env("UMIRAY_VOLT_PASSWORD", &relay.password)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = CoreProcess::spawn(command, false, "VOLT strategy probe")?;
        log.pump_map_until(child.stderr.take().unwrap(), "\0", |line| {
            Some(super::volt::event_line(
                "VOLT strategy probe",
                &line,
                "error",
            ))
        });
        let ready = log.pump_map_until(
            child.stdout.take().unwrap(),
            "{\"event\":\"ready\"",
            |line| {
                Some(super::volt::event_line(
                    "VOLT strategy probe",
                    &line,
                    "info",
                ))
            },
        );
        relay.child = Some(child);
        let line = tokio::time::timeout(Duration::from_secs(10), ready)
            .await
            .map_err(|_| AppError::network("VOLT probe startup timed out"))?
            .map_err(|_| AppError::network("VOLT probe exited before becoming ready"))?;
        let ready: serde_json::Value = serde_json::from_str(&line)
            .map_err(|_| AppError::invalid("Invalid VOLT probe ready event"))?;
        let address = ready["listen"]
            .as_str()
            .and_then(|text| text.parse::<std::net::SocketAddr>().ok())
            .filter(|address| address.ip().is_loopback() && address.port() != 0)
            .ok_or_else(|| AppError::invalid("VOLT probe did not report its loopback listener"))?;
        relay.address = address;
        Ok(relay)
    }
}

impl Drop for ProbeRelay {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            super::volt::stop_child(child);
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

async fn probe_all(
    address: &std::net::SocketAddr,
    password: &str,
    urls: &[String],
) -> Vec<ProbeResult> {
    let proxy = format!("socks5h://{address}");
    let mut checks = Vec::with_capacity(urls.len() * 2);
    for attempt in [1, 2] {
        let mut tasks = tokio::task::JoinSet::new();
        for url in urls {
            let (url, proxy, password) = (url.clone(), proxy.clone(), password.to_owned());
            tasks.spawn(async move {
                let began = Instant::now();
                let result = async {
                    let proxy = reqwest::Proxy::all(proxy)
                        .map_err(|error| AppError::network(error.to_string()))?
                        .basic_auth("umiray-probe", &password);
                    let client = reqwest::Client::builder()
                        .no_proxy()
                        .proxy(proxy)
                        .timeout(PROBE_TIMEOUT)
                        .connect_timeout(PROBE_TIMEOUT)
                        .http1_only()
                        .pool_max_idle_per_host(0)
                        .redirect(reqwest::redirect::Policy::none())
                        .user_agent(crate::http::USER_AGENT)
                        .build()
                        .map_err(|error| AppError::network(error.to_string()))?;
                    probe_https(&client, &url).await
                }
                .await;
                ProbeResult {
                    url,
                    attempt,
                    ok: result.is_ok(),
                    latency_ms: began.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    error: result.err().map(|error| error.to_string()),
                }
            });
        }
        let mut round = Vec::with_capacity(urls.len());
        let mut error = "HTTPS probe task did not complete".to_owned();
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(check) => round.push(check),
                Err(failure) => error = format!("HTTPS probe task failed: {failure}"),
            }
        }
        complete_checks(&mut round, urls, attempt, &error);
        checks.extend(round);
    }
    checks.sort_by(|left, right| {
        left.url
            .cmp(&right.url)
            .then(left.attempt.cmp(&right.attempt))
    });
    checks
}

/// Доступность, а не ответ сайта (D-185): TLS сошёлся с проверкой сертификата и тело
/// дочиталось — путь работает, какой бы ни был статус. Заглушка провайдера по HTTPS
/// сертификат не пройдёт. Длинное тело читается до предела: обрыв на середине — тоже отказ.
async fn probe_https(client: &reqwest::Client, url: &str) -> Result<()> {
    let mut response = client.get(url).send().await.map_err(probe_error)?;
    let mut size = 0usize;
    while size <= BODY_LIMIT {
        match response.chunk().await.map_err(probe_error)? {
            Some(chunk) => size = size.saturating_add(chunk.len()),
            None => break,
        }
    }
    Ok(())
}

fn probe_error(error: reqwest::Error) -> AppError {
    let category = if error.is_timeout() {
        "HTTPS request timed out"
    } else if error.is_connect() {
        "HTTPS connection failed"
    } else {
        "HTTPS request failed"
    };
    let error = error.without_url();
    let mut details = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        details.push_str(": ");
        details.push_str(&cause.to_string());
        source = cause.source();
    }
    AppError::network(format!("{category}: {details}"))
}

fn complete_checks(checks: &mut Vec<ProbeResult>, urls: &[String], attempt: u8, error: &str) {
    for (index, url) in urls.iter().enumerate() {
        let expected = urls[..=index]
            .iter()
            .filter(|candidate| *candidate == url)
            .count();
        if checks.iter().filter(|check| check.url == *url).count() < expected {
            checks.push(ProbeResult {
                url: url.clone(),
                attempt,
                ok: false,
                latency_ms: 0,
                error: Some(error.into()),
            });
        }
    }
}

fn failed_checks(urls: &[String], error: &str) -> Vec<ProbeResult> {
    urls.iter()
        .flat_map(|url| {
            [1, 2].map(|attempt| ProbeResult {
                url: url.clone(),
                attempt,
                ok: false,
                latency_ms: 0,
                error: Some(error.into()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::volt::validate_probe_urls;

    fn result(id: &str, successes: usize, latency: u64) -> CandidateResult {
        CandidateResult {
            id: id.into(),
            label: id.into(),
            successes,
            total: 4,
            latency_ms: Some(latency),
            checks: Vec::new(),
        }
    }

    /// Контроль прошёл всё — блокировки нет, выбирать нечего; иначе выбирает лучший из
    /// стратегий, а контроль в выбор не входит, даже самый быстрый (D-185).
    /// Имя или HTTPS-ссылка; домен для своих — без `www.`, у IP — нет (D-190).
    #[test]
    fn a_site_is_a_name_or_an_https_link() {
        assert_eq!(
            site_url(" www.RuTracker.org ").unwrap(),
            (
                "https://www.RuTracker.org/".into(),
                Some("rutracker.org".into())
            )
        );
        assert_eq!(
            site_url("https://discord.com/app").unwrap().1,
            Some("discord.com".into())
        );
        assert_eq!(site_url("https://1.1.1.1/").unwrap().1, None);
        for bad in ["http://ya.ru/", "https://u:p@ya.ru/", "", "ya ru"] {
            assert!(site_url(bad).is_err(), "{bad}");
        }
        let check = |ok, latency_ms| ProbeResult {
            url: "u".into(),
            attempt: 1,
            ok,
            latency_ms,
            error: (!ok).then(|| "timed out".into()),
        };
        assert_eq!(
            attempt(&[check(true, 100), check(false, 4000)]),
            Attempt {
                ok: false,
                latency_ms: Some(100),
                error: Some("timed out".into())
            },
            "одна неудача из двух — не открылся"
        );
        assert!(attempt(&[check(true, 100), check(true, 200)]).ok);
        assert!(!attempt(&[]).ok);
    }

    #[test]
    fn a_direct_pass_means_nothing_is_blocked() {
        let open = [result(DIRECT, 4, 90), result("tls-split", 4, 120)];
        assert_eq!(verdict(&open), (None, true));
        let blocked = [
            result(DIRECT, 2, 50),
            result("tls-split", 4, 300),
            result("tls-auto", 4, 200),
        ];
        assert_eq!(verdict(&blocked), (Some("tls-auto".into()), false));
        let hopeless = [result(DIRECT, 0, 0), result("tls-split", 3, 100)];
        assert_eq!(verdict(&hopeless), (None, false));
        assert!(crate::config::volt::strategy_parse(DIRECT_YAML).is_ok());
    }

    #[test]
    fn candidates_preserve_matching_udp_and_non_tls_stages() {
        let text = "version: 1\nprofiles:\n- name: tls\n  match:\n    network: tcp\n    all:\n    - payloads: [tls]\n    - any: [{hosts: [youtube.com]}, {signatures: [{offset: 0, hex: '16'}]}]\n    not: {hosts: [skip.ru]}\n  stages:\n  - action: split\n    positions: ['1']\n  - action: fake\n    payloads: [http]\n    fake: {kind: http, server_name: yandex.ru}\n- name: udp\n  match: {network: udp, payloads: [quic]}\n  transform: {action: fake, fake: {kind: quic, server_name: yandex.ru}}\n";
        let original: Value = serde_yaml::from_str(text).unwrap();
        for candidate in candidates(text).unwrap() {
            let modified: Value = serde_yaml::from_str(&candidate.yaml).unwrap();
            assert_eq!(
                modified["profiles"][0]["match"],
                original["profiles"][0]["match"]
            );
            assert_eq!(
                modified["profiles"][0]["stages"][1],
                original["profiles"][0]["stages"][1]
            );
            assert_eq!(modified["profiles"][1], original["profiles"][1]);
        }
        let value: Value = serde_yaml::from_str(&probe_yaml(text).unwrap()).unwrap();
        assert_eq!(value["auto"]["enabled"].as_bool(), Some(false));
        assert_eq!(value["udp"]["enabled"].as_bool(), Some(false));
    }

    #[test]
    fn selection_requires_every_probe_to_pass() {
        let result = |id: &str, successes, total, latency_ms| CandidateResult {
            id: id.into(),
            label: id.into(),
            successes,
            total,
            latency_ms,
            checks: vec![],
        };
        let results = [
            result("fast-failure", 3, 4, Some(1)),
            result("complete", 4, 4, Some(20)),
            result("slow", 4, 4, Some(30)),
        ];
        assert_eq!(winner(&results).unwrap().id, "complete");
        assert!(winner(&[result("failure", 0, 4, None), result("empty", 0, 0, None)]).is_none());
    }

    #[test]
    fn probe_urls_are_bounded_https_get_targets() {
        assert!(validate_probe_urls(&["https://discord.com/api/v10/gateway".into()]).is_ok());
        for url in [
            "http://youtube.com",
            "https://user:secret@youtube.com",
            "https://youtube.com/#part",
            "https://youtube.com:0",
        ] {
            assert!(validate_probe_urls(&[url.into()]).is_err(), "{url}");
        }
        assert!(validate_probe_urls(&[]).is_err());
        assert!(validate_probe_urls(&vec!["https://youtube.com/robots.txt".into(); 5]).is_err());
    }

    #[test]
    fn tls_auto_retains_domain_dictionary_and_user_yaml() {
        let text = "profiles:\n- name: tls\n  match: {network: tcp, payloads: [tls]}\n  transform: {action: fake, fake: {kind: tls, server_name_source: noise-compact, repeats: 1}}\n";
        let yaml = selected_yaml(text, "tls-auto").unwrap();
        let value: Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            value["profiles"][0]["transform"]["fake"]["server_name_source"].as_str(),
            Some("noise-compact")
        );
        assert_eq!(selected_yaml(text, "current").unwrap(), text);
    }

    #[test]
    fn tls_auto_resets_decoy_lifetime_without_changing_the_saved_strategy() {
        let text = "# User strategy\nprofiles:\n- name: tls\n  match: {network: tcp, payloads: [tls]}\n  transform: {action: disorder, positions: ['1', midsld], fake: {kind: tls-auto, server_name_source: noise-compact, repeats: 3, ttl: 3}}\n";
        assert_eq!(selected_yaml(text, "current").unwrap(), text);
        let preset: Value =
            serde_yaml::from_str(&selected_yaml(text, "tls-auto").unwrap()).unwrap();
        let fake = &preset["profiles"][0]["transform"]["fake"];
        assert!(fake.get("ttl").is_none());
        assert_eq!(fake["server_name_source"].as_str(), Some("noise-compact"));
        assert_eq!(fake["repeats"].as_u64(), Some(3));
        assert!(selected_yaml(text, "unknown").is_err());
        assert!(selected_yaml(&" ".repeat(262_145), "current").is_err());
        assert!(selected_yaml("profiles: [", "current").is_err());
    }

    #[test]
    fn manual_tls_presets_preserve_matching_and_other_traffic() {
        let text = "version: 1\ndefault_action: pass\ncustom_root: keep\nprofiles:\n- name: tls\n  custom_profile: keep\n  match:\n    network: tcp\n    all: [{payloads: [tls]}, {hosts: [test.invalid]}]\n  stages:\n  - action: disorder\n    positions: ['7', midsld]\n    sequence_overlap: 32\n    custom_step: keep\n    fake: {kind: tls, server_name_source: noise-compact, repeats: 11, ttl: 3}\n  - action: fake\n    payloads: [http]\n    fake: {kind: http, server_name: test.invalid}\n- name: udp\n  match: {network: udp, payloads: [quic]}\n  transform: {action: fake, fake: {kind: quic, server_name_source: noise-extended}}\n";
        let original: Value = serde_yaml::from_str(text).unwrap();
        for (id, action, noise) in [
            ("tls-split", "split", false),
            ("tls-disorder", "disorder", false),
            ("tls-fake", "fake", true),
            ("tls-fake-split", "split", true),
            ("tls-auto", "disorder", true),
        ] {
            let modified: Value = serde_yaml::from_str(&selected_yaml(text, id).unwrap()).unwrap();
            assert_eq!(modified["custom_root"], original["custom_root"]);
            assert_eq!(modified["default_action"], original["default_action"]);
            assert_eq!(modified["profiles"][0]["custom_profile"], "keep");
            assert_eq!(
                modified["profiles"][0]["match"],
                original["profiles"][0]["match"]
            );
            assert_eq!(
                modified["profiles"][0]["stages"][1],
                original["profiles"][0]["stages"][1]
            );
            assert_eq!(modified["profiles"][1], original["profiles"][1]);
            let step = &modified["profiles"][0]["stages"][0];
            assert_eq!(step["action"], action);
            assert_eq!(step["custom_step"], "keep");
            assert_eq!(step["packet_limit"].as_u64(), Some(2));
            assert_eq!(step["byte_limit"].as_u64(), Some(16384));
            assert!(step.get("sequence_overlap").is_none());
            if action == "fake" {
                assert!(step.get("positions").is_none());
            } else {
                assert_eq!(
                    step["positions"],
                    serde_yaml::to_value(["1", "midsld"]).unwrap()
                );
            }
            if noise {
                assert_eq!(step["fake"]["kind"], "tls-auto");
                assert_eq!(step["fake"]["server_name_source"], "noise-compact");
                assert_eq!(step["fake"]["repeats"].as_u64(), Some(3));
                assert!(step["fake"].get("ttl").is_none());
            } else {
                assert!(step.get("fake").is_none());
            }
        }
        assert_eq!(selected_yaml(text, "current").unwrap(), text);
        assert_eq!(
            candidates(text)
                .unwrap()
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>(),
            ["current", "tls-split", "tls-disorder", "tls-auto"]
        );
    }

    #[test]
    fn noise_presets_remove_custom_payloads_and_preserve_the_dictionary() {
        for field in ["hex: '00'", "payload_file: decoy.bin"] {
            let text = format!("profiles:\n- name: tls\n  match: {{network: tcp, payloads: [tls]}}\n  transform:\n    action: fake\n    fake: {{kind: custom, server_name_file: domains.txt, {field}}}\n");
            for id in ["tls-fake", "tls-fake-split", "tls-auto"] {
                let modified: Value =
                    serde_yaml::from_str(&selected_yaml(&text, id).unwrap()).unwrap();
                let fake = &modified["profiles"][0]["transform"]["fake"];
                assert_eq!(fake["kind"], "tls-auto");
                assert_eq!(fake["server_name_file"], "domains.txt");
                assert!(fake.get("hex").is_none());
                assert!(fake.get("payload_file").is_none());
            }
        }
    }

    #[test]
    fn manual_presets_reject_non_tls_strategies_and_preserve_mixed_stages() {
        let mixed = "profiles:\n- name: mixed\n  match: {network: tcp, payloads: [tls, http]}\n  stages:\n  - {action: split, positions: ['1'], payloads: [tls]}\n  - {action: split, positions: [host+1], payloads: [http]}\n";
        let original: Value = serde_yaml::from_str(mixed).unwrap();
        for id in [
            "tls-split",
            "tls-disorder",
            "tls-fake",
            "tls-fake-split",
            "tls-auto",
        ] {
            let modified: Value = serde_yaml::from_str(&selected_yaml(mixed, id).unwrap()).unwrap();
            assert_eq!(
                modified["profiles"][0]["match"],
                original["profiles"][0]["match"]
            );
            assert_eq!(
                modified["profiles"][0]["stages"][0]["payloads"],
                original["profiles"][0]["stages"][0]["payloads"]
            );
            assert_eq!(
                modified["profiles"][0]["stages"][1],
                original["profiles"][0]["stages"][1]
            );
            let unfiltered = "profiles:\n- name: mixed\n  match: {network: tcp, payloads: [tls, http]}\n  transform: {action: split, positions: ['1']}\n";
            assert!(selected_yaml(unfiltered, id).is_err());
            assert!(selected_yaml("profiles: []", id).is_err());
        }
    }

    #[tokio::test]
    async fn probe_errors_show_timeout_and_connection_causes_without_the_url() {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let error = client.get(&url).send().await.unwrap_err();
        assert!(error.is_timeout());
        let cause = std::error::Error::source(&error).unwrap().to_string();
        let message = probe_error(error).to_string();
        assert!(message.starts_with("HTTPS request timed out:"));
        assert!(message.contains(&cause));
        assert!(!message.contains(&url));

        let tls_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tls_url = format!("https://{}/", tls_listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let (mut socket, _) = tls_listener.accept().await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        });
        let error = client
            .get(&tls_url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .unwrap_err();
        server.await.unwrap();
        assert!(error.is_connect() && !error.is_timeout());
        let cause = std::error::Error::source(&error).unwrap().to_string();
        let message = probe_error(error).to_string();
        assert!(message.starts_with("HTTPS connection failed:"));
        assert!(message.contains(&cause));
        assert!(!message.contains(&tls_url));
    }

    #[test]
    fn a_missing_probe_result_is_a_failure_even_for_duplicate_urls() {
        let urls: Vec<String> = vec!["https://ya.ru/".into(), "https://ya.ru/".into()];
        let mut checks = vec![ProbeResult {
            url: urls[0].clone(),
            attempt: 1,
            ok: true,
            latency_ms: 1,
            error: None,
        }];
        complete_checks(&mut checks, &urls, 1, "task cancelled");
        assert_eq!(checks.len(), 2);
        let result = CandidateResult {
            id: "current".into(),
            label: "Current".into(),
            successes: checks.iter().filter(|check| check.ok).count(),
            total: urls.len(),
            latency_ms: Some(1),
            checks,
        };
        assert!(winner(&[result]).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn dropping_a_probe_stops_only_its_owned_child_and_removes_its_config() {
        let file =
            std::env::temp_dir().join(format!("volt-probe-test-{}.yaml", Stamp::id().unwrap()));
        std::fs::write(&file, b"probe").unwrap();
        let marker = file.with_extension("stopped");
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "if ([Console]::ReadLine() -eq 'stop') { [IO.File]::WriteAllText($env:UMIRAY_PROBE_MARKER, 'stopped'); exit 0 }; exit 1"]);
        command
            .env("UMIRAY_PROBE_MARKER", &marker)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = CoreProcess::spawn(command, false, "VOLT test probe").unwrap();
        let guard = ProbeRelay {
            child: Some(child),
            file: file.clone(),
            address: ([127, 0, 0, 1], 0).into(),
            password: "test".into(),
        };
        drop(guard);
        assert!(!file.exists());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "stopped");
        std::fs::remove_file(marker).unwrap();
    }
}
