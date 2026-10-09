//! Ядро qd (github.com/jaywehosl/qd): процесс, его рукопожатие и прокси к локальному API,
//! плюс доставка бинаря. Устройство процесса и API — QD.md.

use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::core::process::CoreProcess;
use crate::core::process::LogRing;
use crate::db::{Db, Table};
use crate::error::{AppError, Result};
use crate::http::Http;
use crate::paths::Paths;
use crate::system::elevation::Elevation;

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_GRACE: Duration = Duration::from_secs(5);
const HELLO: &str = "{\"qdEmbedded\"";

/// Лента релизов, а не API (D-161): `api.github.com` режут по DNS, а `/releases/latest`
/// у qd пуст — все релизы pre-release. Лента знает их и идёт от нового к старому.
const REPO: &str = "https://github.com/jaywehosl/qd";
const ASSET: &str = "qd-core-windows-amd64.exe";
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
    /// Процесс жив, а туннеля нет: потерянный туннель qd возвращает сам, пока его
    /// не опустили (с 0.1.5), — это не падение (D-057).
    pub recovering: bool,
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
            recovering: alive && !on,
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
        let obj = request(&api, &token, method, path, body).await?;
        if let Some(up) = obj.get("connected").and_then(Value::as_bool) {
            self.heard(up);
        }
        Ok(obj)
    }

    /// Ссылка, которую сейчас некому принять: qd встаёт только с правами (D-161). Ждёт
    /// в базе и уходит в qd при первом его подъёме.
    pub fn hold(&self, link: &str) -> Result<()> {
        Db::put(Table::State, PENDING, "", link)
    }

    /// Удалить бинарь (D-161). Процесс гасим: без этого файл занят. Каталог `qd/` остаётся —
    /// в нём настройки qd. Работает ли туннель, проверяет вызывающий под замком
    /// перехода.
    pub async fn remove(&self) -> Result<()> {
        self.shutdown().await;
        match std::fs::remove_file(Self::binary()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(AppError::io(format!("Не удалось удалить qd: {e}")))
            }
            _ => Ok(()),
        }
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
        adopt_pending(&found.0, &found.1).await;
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
        let mut child = CoreProcess::spawn(command, false, "qd")?;

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
        use crate::system::features::{Feature, Features};
        // Сборка ядра qd есть только под Windows (D-174): качать на Linux нечего.
        if !Features::has(Feature::Qd) {
            return Err(AppError::invalid("qd на этой системе не работает"));
        }
        let (tag, binary) =
            crate::core::release::Release::fetch(REPO, ASSET, MAX_BINARY, "qd").await?;
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

/// Один запрос к API живого qd. Ответ `{success, obj, msg}` → `obj` или отказ словами qd.
async fn request(
    api: &str,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
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
        return Ok(reply.get("obj").cloned().unwrap_or(Value::Null));
    }
    let message = reply
        .get("msg")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or("qd отказал без объяснения");
    Err(AppError::invalid(message.to_string()))
}

/// Ссылка qd, ждущая его подъёма (D-161): строка в таблице состояния (D-170).
const PENDING: &str = "qd-pending";

/// Отдать qd ссылку, которая ждала его подъёма. Неудача подъём не отменяет: ссылка ждёт
/// следующего, а причину скажет сам qd, когда её вставят снова.
async fn adopt_pending(api: &str, token: &str) {
    let Ok(Some(link)) = Db::get(Table::State, PENDING, "") else {
        return;
    };
    let body = serde_json::json!({ "uri": link.trim() });
    if request(api, token, "POST", "/client/api/import", Some(body))
        .await
        .is_ok()
    {
        let _ = Db::remove(Table::State, PENDING, "");
    }
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

    /// Туннель упал, процесс жив — qd поднимает его сам. Надзор, принимавший это за падение,
    /// после трёх своих попыток гасил VPN насовсем (D-057). Умер процесс — вот это падение.
    #[cfg(windows)]
    #[test]
    fn a_live_qd_without_a_tunnel_is_recovering_and_a_dead_one_is_not() {
        let qd = Qd::new();
        qd.wanted.store(true, Ordering::Relaxed);
        let child = Command::new(r"C:\Windows\System32\PING.EXE")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        *qd.live.lock().unwrap() = Some(Live {
            child,
            stdin: None,
            api: String::new(),
            token: String::new(),
            connected: false,
            started: None,
        });
        let lost = qd.status();
        assert!(
            !lost.on && lost.recovering,
            "процесс жив — туннель вернёт сам qd"
        );

        let mut held = qd.live.lock().unwrap().take().unwrap();
        held.child.kill().unwrap();
        held.child.wait().unwrap();
        *qd.live.lock().unwrap() = Some(held);
        let dead = qd.status();
        assert!(
            dead.wanted && !dead.on && !dead.recovering,
            "процесс умер — это падение"
        );
    }
}
