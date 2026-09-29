//! Ядро qd (github.com/jaywehosl/qd): процесс, его рукопожатие и прокси к локальному API,
//! плюс доставка бинаря. Устройство процесса и API — QD.md.

use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::core::process::CoreProcess;
use crate::core::process::LogRing;
use crate::error::{AppError, Result};
use crate::http::Http;
use crate::paths::Paths;
use crate::system::elevation::Elevation;

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_GRACE: Duration = Duration::from_secs(5);
const HELLO: &str = "{\"qdEmbedded\"";

const RELEASES: &str = "https://api.github.com/repos/jaywehosl/qd/releases?per_page=1";
const ASSET: &str = "qd-core-windows-amd64.exe";
const SUMS: &str = "checksums.txt";
const MAX_BINARY: usize = 128 * 1024 * 1024;

/// Работающий процесс. Всё, что знаем о туннеле, живёт здесь же: умер процесс — вместе
/// с ним ушло и «подключено», и новый процесс начинает с «не подключено».
struct Live {
    child: Child,
    stdin: Option<ChildStdin>,
    api: String,
    token: String,
    connected: bool,
    started: Option<u64>,
}

/// Что qd делает сейчас — для обвязки клиента (D-154).
pub struct Status {
    /// Туннель поднят, и процесс жив.
    pub on: bool,
    /// Туннель **должен** быть поднят: ставит `connect`, снимает `disconnect` (D-057).
    pub wanted: bool,
    pub started: Option<u64>,
}

pub struct Qd {
    /// Один подъём процесса за раз: рукопожатие ждём до 15 с, и второй вызов за это
    /// время поднял бы второй qd. Сам процесс — под обычным замком: состояние
    /// спрашивают синхронно, из статуса.
    spawning: tokio::sync::Mutex<()>,
    live: Mutex<Option<Live>>,
    log: LogRing,
    wanted: AtomicBool,
}

impl Default for Qd {
    fn default() -> Self {
        Self::new()
    }
}

impl Qd {
    pub fn new() -> Self {
        Self {
            spawning: tokio::sync::Mutex::new(()),
            live: Mutex::new(None),
            log: LogRing::default(),
            wanted: AtomicBool::new(false),
        }
    }

    /// Бинарь qd. Где он лежит, знает только qd (D-155).
    pub fn binary() -> std::path::PathBuf {
        Paths::qd()
    }

    pub fn present(&self) -> bool {
        Self::binary().is_file()
    }

    /// Жив ли процесс. Туннель при этом может быть и опущен: разделы qd поднимают процесс
    /// ради его API, не подключаясь.
    pub fn running(&self) -> bool {
        self.endpoint().is_some()
    }

    pub fn status(&self) -> Status {
        let mut live = self.live.lock().unwrap();
        let alive = alive(&mut live);
        let (on, started) = match live.as_ref() {
            Some(held) if alive && held.connected => (true, held.started),
            _ => (false, None),
        };
        Status {
            on,
            wanted: self.wanted.load(Ordering::Relaxed),
            started,
        }
    }

    pub fn log(&self) -> &LogRing {
        &self.log
    }

    /// Поднять туннель: процесс, если его ещё нет, потом `connect`. Состояние после —
    /// из ответа qd, а не из нашего желания.
    pub async fn connect(&self) -> Result<()> {
        self.call("POST", "/client/api/connect", None).await?;
        self.call("GET", "/client/api/state", None).await?;
        self.wanted.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Опустить туннель. Процесс остаётся: он нужен разделам qd ради API.
    pub async fn disconnect(&self) -> Result<()> {
        self.wanted.store(false, Ordering::Relaxed);
        if !self.running() {
            return Ok(());
        }
        self.call("POST", "/client/api/disconnect", None).await?;
        self.call("GET", "/client/api/state", None).await?;
        Ok(())
    }

    /// Освежить «подключено» из ответа qd — туннель мог упасть и без нас. Процесс ради
    /// этого не поднимаем.
    pub async fn refresh(&self) -> Result<()> {
        if !self.running() {
            return Ok(());
        }
        self.call("GET", "/client/api/state", None).await.map(drop)
    }

    pub async fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        if !path.starts_with("/client/api/") {
            return Err(AppError::invalid(format!(
                "qd: путь вне API клиента: {path}"
            )));
        }
        let (api, token) = self.ensure().await?;
        // Мимо системного прокси: в System там может стоять mihomo (GOTCHAS).
        let client = Http::direct()?;
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
                self.heard(up);
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

    /// qd сказал, поднят ли туннель. Время работы — с первого «да».
    fn heard(&self, up: bool) {
        if let Some(held) = self.live.lock().unwrap().as_mut() {
            if up && !held.connected {
                held.started = crate::stamp::Stamp::now();
            }
            held.connected = up;
            if !up {
                held.started = None;
            }
        }
    }

    /// Адрес API живого процесса.
    fn endpoint(&self) -> Option<(String, String)> {
        let mut live = self.live.lock().unwrap();
        if !alive(&mut live) {
            return None;
        }
        live.as_ref()
            .map(|held| (held.api.clone(), held.token.clone()))
    }

    async fn ensure(&self) -> Result<(String, String)> {
        if let Some(found) = self.endpoint() {
            return Ok(found);
        }
        let _spawning = self.spawning.lock().await;
        // Пока ждали замок, процесс мог поднять соседний вызов.
        if let Some(found) = self.endpoint() {
            return Ok(found);
        }
        if !self.present() {
            return Err(AppError::invalid("qd ещё не скачан"));
        }
        if !Elevation::is_elevated() {
            return Err(AppError::NeedsElevation {
                message: "qd перехватывает трафик через WinDivert — нужен запуск с правами администратора".into(),
            });
        }
        let started = self.spawn().await?;
        let found = (started.api.clone(), started.token.clone());
        *self.live.lock().unwrap() = Some(started);
        Ok(found)
    }

    async fn spawn(&self) -> Result<Live> {
        Paths::ensure_root()?;
        std::fs::create_dir_all(Paths::qd_dir())
            .map_err(|e| AppError::io(format!("Не удалось создать папку qd: {e}")))?;

        let mut command = Command::new(Paths::qd());
        command
            .arg("-embedded")
            .arg("-ui-port")
            .arg("0")
            .arg("-state")
            .arg(Paths::qd_dir().join("client.db"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = CoreProcess::spawn(command, 0, "qd")?;

        let hello = match child.stdout.take() {
            Some(out) => self.log.pump_until(out, HELLO),
            None => tokio::sync::oneshot::channel().1,
        };
        if let Some(err) = child.stderr.take() {
            self.log.pump(err);
        }
        let stdin = child.stdin.take();

        let said = match tokio::time::timeout(READY_TIMEOUT, hello).await {
            Ok(Ok(line)) => handshake(&line),
            _ => Err("qd не ответил при запуске".into()),
        };
        let (api, token) = match said {
            Ok(found) => found,
            Err(why) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::CoreFailed {
                    message: why,
                    log: self.log.tail(20),
                });
            }
        };
        Ok(Live {
            child,
            stdin,
            api,
            token,
            connected: false,
            started: None,
        })
    }

    /// Погасить процесс совсем: закрыть stdin — qd выходит сам, — через `STOP_GRACE` убить.
    pub async fn shutdown(&self) {
        self.wanted.store(false, Ordering::Relaxed);
        let taken = self.live.lock().unwrap().take();
        let Some(mut held) = taken else {
            return;
        };
        drop(held.stdin.take());
        let _ =
            tokio::task::spawn_blocking(move || CoreProcess::finish(&mut held.child, STOP_GRACE))
                .await;
    }

    pub async fn install(&self) -> Result<String> {
        let client = Http::client()?;
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

        let sums = Http::fetch(&client, &link(SUMS)?, 64 * 1024).await?;
        let want = checksum(&String::from_utf8_lossy(&sums), ASSET)
            .ok_or_else(|| AppError::invalid(format!("В {SUMS} нет строки для {ASSET}")))?;

        let binary = Http::fetch(&client, &link(ASSET)?, MAX_BINARY).await?;
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
        Paths::ensure_root()?;
        crate::atomic::AtomicFile::write(Paths::qd(), &binary)
            .map_err(|e| AppError::io(format!("Не удалось записать qd: {e}")))?;
        Ok(tag)
    }
}

/// Строка `checksums.txt` для файла → его SHA-256 строчными. Формат `sha256sum`:
/// хеш, пробелы, имя; звёздочка перед именем — двоичный режим.
fn checksum(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_ascii_lowercase())
    })
}

/// Жив ли процесс; мёртвый заодно забываем.
fn alive(live: &mut Option<Live>) -> bool {
    let alive = live
        .as_mut()
        .is_some_and(|held| matches!(held.child.try_wait(), Ok(None)));
    if !alive {
        *live = None;
    }
    alive
}

/// Строка рукопожатия → адрес API и токен, или причина, по которой qd не встал.
/// `{"qdEmbedded":{"api":…,"token":…}}` либо `{"qdEmbedded":{"error":…}}`.
fn handshake(line: &str) -> std::result::Result<(String, String), String> {
    let said = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| value.get("qdEmbedded").cloned())
        .unwrap_or(Value::Null);
    if let Some(why) = said.get("error").and_then(Value::as_str) {
        return Err(format!("qd не запустился: {why}"));
    }
    let field = |name: &str| {
        said.get(name)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    match (field("api"), field("token")) {
        (Some(api), Some(token)) => Ok((api, token)),
        _ => Err("qd прислал неполные данные запуска".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_gives_the_api_or_the_reason() {
        assert_eq!(
            handshake(r#"{"qdEmbedded":{"api":"http://127.0.0.1:5000","token":"t"}}"#),
            Ok(("http://127.0.0.1:5000".into(), "t".into()))
        );
        assert_eq!(
            handshake(r#"{"qdEmbedded":{"error":"another client runs"}}"#),
            Err("qd не запустился: another client runs".into())
        );
        assert!(handshake(r#"{"qdEmbedded":{"api":"http://x"}}"#).is_err());
        assert!(handshake("garbage").is_err());
    }

    #[test]
    fn the_checksum_line_is_found_by_file_name() {
        let sums = "ABC123  qd-core-windows-amd64.exe\n\
                    def456 *qd-windows-amd64.exe\n";
        assert_eq!(
            checksum(sums, "qd-core-windows-amd64.exe").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            checksum(sums, "qd-windows-amd64.exe").as_deref(),
            Some("def456")
        );
        assert_eq!(checksum(sums, "qd-core-linux-amd64"), None);
    }
}
