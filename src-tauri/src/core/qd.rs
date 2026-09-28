use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::http;
use crate::paths;
use crate::system::elevation;

const LOG_LINES: usize = 500;
const READY_TIMEOUT: Duration = Duration::from_secs(15);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(5);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const HELLO: &str = "{\"qdEmbedded\"";

const RELEASES: &str = "https://api.github.com/repos/jaywehosl/qd/releases?per_page=1";
const ASSET: &str = "qd-core-windows-amd64.exe";
const SUMS: &str = "checksums.txt";
const MAX_BINARY: usize = 128 * 1024 * 1024;

struct Live {
    child: Child,
    stdin: Option<ChildStdin>,
    api: String,
    token: String,
}

pub struct Qd {
    live: tokio::sync::Mutex<Option<Live>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    connected: AtomicBool,
}

impl Default for Qd {
    fn default() -> Self {
        Self::new()
    }
}

impl Qd {
    pub fn new() -> Self {
        Self {
            live: tokio::sync::Mutex::new(None),
            logs: Arc::new(Mutex::new(VecDeque::new())),
            connected: AtomicBool::new(false),
        }
    }

    pub fn present(&self) -> bool {
        paths::qd().is_file()
    }

    pub async fn running(&self) -> bool {
        let mut live = self.live.lock().await;
        match live.as_mut() {
            Some(held) => held.child.try_wait().ok().flatten().is_none(),
            None => false,
        }
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    pub async fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        if !path.starts_with("/client/api/") {
            return Err(AppError::invalid(format!(
                "qd: путь вне API клиента: {path}"
            )));
        }
        let (api, token) = self.ensure().await?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(CALL_TIMEOUT)
            .build()
            .map_err(|e| AppError::network(format!("qd: HTTP-клиент не создан: {e}")))?;
        let url = format!("{api}{path}");
        let request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            _ => {
                return Err(AppError::invalid(format!(
                    "qd: метод {method} не поддержан"
                )))
            }
        };
        let mut request = request.header("X-QD-Token", token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let reply: Value = request
            .send()
            .await
            .map_err(|e| AppError::network(format!("qd не ответил: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::network(format!("qd ответил не JSON: {e}")))?;
        if reply.get("success").and_then(Value::as_bool) == Some(true) {
            let obj = reply.get("obj").cloned().unwrap_or(Value::Null);
            if let Some(up) = obj.get("connected").and_then(Value::as_bool) {
                self.connected.store(up, Ordering::Relaxed);
            }
            return Ok(obj);
        }
        let message = reply
            .get("msg")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .unwrap_or("qd отказал без объяснения");
        Err(AppError::invalid(message.to_string()))
    }

    async fn ensure(&self) -> Result<(String, String)> {
        let mut live = self.live.lock().await;
        if let Some(held) = live.as_mut() {
            if held.child.try_wait().ok().flatten().is_none() {
                return Ok((held.api.clone(), held.token.clone()));
            }
            *live = None;
        }
        if !self.present() {
            return Err(AppError::invalid("qd ещё не скачан"));
        }
        if !elevation::is_elevated() {
            return Err(AppError::NeedsElevation {
                message: "qd перехватывает трафик через WinDivert — нужен запуск с правами администратора".into(),
            });
        }
        let started = self.spawn().await?;
        let found = (started.api.clone(), started.token.clone());
        *live = Some(started);
        Ok(found)
    }

    async fn spawn(&self) -> Result<Live> {
        paths::ensure_root()?;
        std::fs::create_dir_all(paths::qd_dir())
            .map_err(|e| AppError::io(format!("Не удалось создать папку qd: {e}")))?;

        let mut command = Command::new(paths::qd());
        command
            .arg("-embedded")
            .arg("-ui-port")
            .arg("0")
            .arg("-state")
            .arg(paths::qd_dir().join("client.db"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| AppError::CoreFailed {
            message: format!("Не удалось запустить qd: {e}"),
            log: Vec::new(),
        })?;
        if !crate::system::job::attach(child.as_raw_handle()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: "Не удалось привязать qd к процессу клиента".into(),
                log: Vec::new(),
            });
        }

        let (tell, hello) = tokio::sync::oneshot::channel();
        if let Some(out) = child.stdout.take() {
            pump(out, self.logs.clone(), Some(tell));
        }
        if let Some(err) = child.stderr.take() {
            pump(err, self.logs.clone(), None);
        }
        let stdin = child.stdin.take();

        let said = match tokio::time::timeout(READY_TIMEOUT, hello).await {
            Ok(Ok(said)) => said,
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::CoreFailed {
                    message: "qd не ответил при запуске".into(),
                    log: self.tail(20),
                });
            }
        };
        if let Some(why) = said.get("error").and_then(Value::as_str) {
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: format!("qd не запустился: {why}"),
                log: self.tail(20),
            });
        }
        let api = said
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let token = said
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if api.is_empty() || token.is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: "qd прислал неполные данные запуска".into(),
                log: self.tail(20),
            });
        }
        Ok(Live {
            child,
            stdin,
            api,
            token,
        })
    }

    pub async fn shutdown(&self) {
        self.connected.store(false, Ordering::Relaxed);
        let taken = self.live.lock().await.take();
        let Some(mut held) = taken else {
            return;
        };
        drop(held.stdin.take());
        let _ = tokio::task::spawn_blocking(move || {
            let deadline = std::time::Instant::now() + STOP_GRACE;
            while std::time::Instant::now() < deadline {
                if held.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = held.child.kill();
            let _ = held.child.wait();
        })
        .await;
    }

    pub async fn install(&self) -> Result<String> {
        let client = http::client()?;
        let releases: Value = client
            .get(RELEASES)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| AppError::network(format!("Не удалось узнать версию qd: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::network(format!("GitHub ответил не JSON: {e}")))?;
        let release = releases
            .as_array()
            .and_then(|all| all.first())
            .ok_or_else(|| AppError::network("У qd нет ни одного релиза"))?;
        let tag = release
            .get("tag_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let link = |name: &str| {
            release
                .get("assets")
                .and_then(Value::as_array)
                .and_then(|assets| {
                    assets
                        .iter()
                        .find(|asset| asset.get("name").and_then(Value::as_str) == Some(name))
                })
                .and_then(|asset| asset.get("browser_download_url"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| AppError::network(format!("В релизе {tag} нет {name}")))
        };

        let sums = fetch(&client, &link(SUMS)?, 64 * 1024).await?;
        let sums = String::from_utf8_lossy(&sums);
        let want = sums
            .lines()
            .find_map(|line| {
                let (hash, name) = line.split_once(char::is_whitespace)?;
                (name.trim().trim_start_matches('*') == ASSET).then(|| hash.to_ascii_lowercase())
            })
            .ok_or_else(|| AppError::invalid(format!("В {SUMS} нет строки для {ASSET}")))?;

        let binary = fetch(&client, &link(ASSET)?, MAX_BINARY).await?;
        let got: String = Sha256::digest(&binary)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if got != want {
            return Err(AppError::invalid(
                "Контрольная сумма qd не совпала — файл не записан",
            ));
        }
        if !binary.starts_with(b"MZ") {
            return Err(AppError::invalid(
                "Скачанный qd не похож на программу Windows",
            ));
        }

        self.shutdown().await;
        paths::ensure_root()?;
        crate::atomic::write(paths::qd(), &binary)
            .map_err(|e| AppError::io(format!("Не удалось записать qd: {e}")))?;
        Ok(tag)
    }

    fn tail(&self, lines: usize) -> Vec<String> {
        let logs = self.logs.lock().unwrap();
        logs.iter()
            .skip(logs.len().saturating_sub(lines))
            .cloned()
            .collect()
    }
}

async fn fetch(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::network(format!("Не удалось скачать {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::network(format!(
            "Загрузка вернула {}: {url}",
            response.status()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AppError::network(format!("Загрузка оборвалась: {e}")))?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AppError::network(format!("Файл неожиданно велик: {url}")));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn pump(
    stream: impl Read + Send + 'static,
    logs: Arc<Mutex<VecDeque<String>>>,
    hello: Option<tokio::sync::oneshot::Sender<Value>>,
) {
    std::thread::spawn(move || {
        let mut hello = hello;
        for line in BufReader::new(stream).lines().map_while(|line| line.ok()) {
            if line.starts_with(HELLO) {
                if let Some(tell) = hello.take() {
                    let said = serde_json::from_str::<Value>(&line)
                        .ok()
                        .and_then(|value| value.get("qdEmbedded").cloned())
                        .unwrap_or(Value::Null);
                    let _ = tell.send(said);
                }
                continue;
            }
            let mut logs = logs.lock().unwrap();
            if logs.len() >= LOG_LINES {
                logs.pop_front();
            }
            logs.push_back(line);
        }
    });
}
