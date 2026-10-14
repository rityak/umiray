//! Ядро mihomo. Сам модуль — процесс: запуск в нужном режиме, ожидание готовности, лог
//! и остановка; владеет дочерним процессом и его выводом, наружу отдаёт только состояние
//! и строки лога — команды Tauri про `Child` ничего не знают.
//!
//! Остальное разделено по тому, чем управляет: `controller` — работающим ядром через
//! `external-controller` (D-007), `download` — файлом ядра на диске (D-006), `apply` —
//! тем, доедет ли правка до живого ядра перезагрузкой (D-102), `lists` — скачанными
//! списками в формате ядра (D-157).

pub mod apply;
pub mod controller;
pub mod download;
pub mod keys;
pub mod lists;

use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use controller::Controller;

use crate::config::mode::Mode;
use crate::core::process::CoreProcess;
use crate::core::process::LogRing;
use crate::error::{AppError, Result};
use crate::paths::Paths;
use crate::render::mihomo::Effective;
use crate::system::elevation::Elevation;

/// Имена geo-баз mihomo в рабочем каталоге: mmdb-режим, dat-режим и база ASN.
const GEO_FILES: [&str; 5] = [
    "geoip.metadb",
    "GeoIP.dat",
    "GeoSite.dat",
    "GeoLite2-ASN.mmdb",
    "ASN.mmdb",
];

/// Geo-база ядра для окна.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoFile {
    pub name: String,
    /// Когда файл менялся, в секундах эпохи.
    pub modified: Option<u64>,
}

/// Сколько ждём, пока ядро начнёт отвечать: большой конфиг грузится небыстро.
const READY_TIMEOUT: Duration = Duration::from_secs(10);
/// Сколько ждём, пока ядро выйдет само после Ctrl+Break. Замер даёт около ста
/// миллисекунд (S-021); полсекунды — потолок на медленную машину, после него гасим.
const GRACE: Duration = Duration::from_millis(500);

#[derive(Default)]
struct Running {
    child: Option<Child>,
    mode: Option<Mode>,
    /// Порт локального прокси в том виде, в каком он оказался в конфиге: пользователь мог
    /// переопределить его оверрайдом, и шапка обязана показывать настоящий адрес.
    port: Option<u16>,
    /// Порт служебного входа под замер «через прокси, keep-alive» (D-072). Живёт столько же,
    /// сколько процесс: вход заводится в конфиге при каждом запуске.
    probe: Option<u16>,
    /// Адаптер TUN работающего конфига; пусто — не TUN.
    device: Option<String>,
    /// Живёт ровно столько же, сколько процесс: секрет одноразовый.
    controller: Option<Controller>,
    /// Когда ядро поднялось, в секундах эпохи. Живёт столько же, сколько процесс:
    /// поднятое заново после падения (D-057) — это новое время работы, а не продолжение
    /// прежнего.
    started: Option<u64>,
    /// Ядро **должно** работать: ставит удачный запуск, снимает явная остановка.
    /// Смерть процесса флаг не трогает — по нему и отличается падение от «выключили»
    /// (D-057). На диск не едет: желание живёт ровно одну сессию.
    wanted: bool,
    /// Конфиг, на котором ядро **сейчас работает**. Не то же, что файл на диске: файл
    /// правится в любой момент, а работающее ядро живёт с тем, что ему дали (D-102).
    /// По этой разнице и считается, нужен ли перезапуск.
    launched: Option<String>,
}

pub struct Status {
    pub running: bool,
    pub mode: Option<Mode>,
    pub port: Option<u16>,
    /// Когда ядро поднялось. Считает здесь, а не окно: окно переживает перезапуск ядра
    /// и живёт дольше него, и таймер на его стороне врал бы после каждого падения.
    pub started: Option<u64>,
    /// Ядро **должно** работать (D-057): живое при `running: false` — упало само.
    pub wanted: bool,
    pub device: Option<String>,
}

pub struct Mihomo {
    running: Mutex<Running>,
    log: LogRing,
}

impl Mihomo {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(Running::default()),
            log: LogRing::default(),
        }
    }

    /// Живо ли ядро. Заодно подчищает состояние, если процесс умер сам.
    pub fn status(&self) -> Status {
        let mut running = self.running.lock().unwrap();
        let alive = match running.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if !alive {
            running.child = None;
            running.mode = None;
            running.port = None;
            running.probe = None;
            running.device = None;
            running.controller = None;
            running.started = None;
            running.launched = None;
        }
        Status {
            running: alive,
            mode: running.mode,
            port: running.port,
            started: running.started,
            wanted: running.wanted,
            device: running.device.clone(),
        }
    }

    /// Конфиг, на котором ядро **работает сейчас**. Пусто — не работает. По нему считается,
    /// доедет ли правка перезагрузкой (D-102): сравнивать надо с работающим, а не с файлом.
    pub fn launched(&self) -> Option<String> {
        self.running.lock().unwrap().launched.clone()
    }

    pub fn log(&self) -> &LogRing {
        &self.log
    }

    /// Ручка к API ядра. Приватна намеренно: наружу из `core` торчат доменные операции
    /// (`select`, `traffic`, `reload`), а не устройство ядра (D-033).
    fn controller(&self) -> Option<Controller> {
        self.running.lock().unwrap().controller.clone()
    }

    pub async fn traffic(&self) -> Result<Option<controller::Traffic>> {
        match self.controller() {
            Some(controller) => controller.traffic().await.map(Some),
            None => Ok(None),
        }
    }

    /// Перепроверить живость узлов немедленно (D-112). Ноль — ядро не работает,
    /// и проверять нечего.
    pub async fn recheck(&self) -> Result<usize> {
        match self.controller() {
            Some(controller) => controller.recheck().await,
            None => Ok(0),
        }
    }

    /// Где сейчас стоит галочка в группе выбора. Пусто — ядро не запущено.
    pub async fn selected(&self) -> Option<String> {
        let controller = self.controller()?;
        let groups = controller.groups().await.ok()?;
        groups
            .into_iter()
            .find(|group| group.name == crate::config::direction::SELECTOR)
            .map(|group| group.now)
    }

    /// Куда уходит трафик группы — до узла (D-145). Ядро не работает или не ответило —
    /// известна только сама группа.
    pub async fn route(&self, from: &str) -> Vec<String> {
        match self.controller() {
            Some(controller) => controller
                .route(from)
                .await
                .unwrap_or_else(|_| vec![from.to_string()]),
            None => vec![from.to_string()],
        }
    }

    /// Переключить выход. Правка конфига и перезапуск ядра для этого не нужны (D-007).
    pub async fn select(&self, node: &str) -> Result<()> {
        self.require_controller()?
            .select(crate::config::direction::SELECTOR, node)
            .await
    }

    /// Перечитать источник живым ядром — без перезапуска (S-012).
    /// Порт служебного входа под замер (D-072). Пусто — ядро не работает.
    pub fn probe_port(&self) -> Option<u16> {
        self.running.lock().unwrap().probe
    }

    /// Навести служебную группу на узел. Маршрут пользователя это не трогает: через неё
    /// ходит только наш собственный вход.
    pub async fn probe_select(&self, node: &str) -> Result<()> {
        self.require_controller()?
            .select(crate::config::direction::PROBE, node)
            .await
    }

    /// Померить задержку у всех узлов источника — руками ядра (D-069).
    pub async fn healthcheck(&self, source: &str) -> Result<()> {
        self.require_controller()?.healthcheck(source).await
    }

    /// Что ядро намеряло по каждому узлу.
    pub async fn delays(&self) -> Result<std::collections::HashMap<String, Vec<u32>>> {
        self.require_controller()?.delays().await
    }

    /// Дать живому ядру перечитать конфиг целиком (S-019). Ядро не запущено — тишина:
    /// применять нечего, файл и так лежит на диске и уедет следующим запуском.
    pub async fn apply(&self, effective: &Effective) -> Result<()> {
        let Some(controller) = self.controller() else {
            return Ok(());
        };
        Paths::ensure_run_dir()?;
        crate::atomic::AtomicFile::write(Paths::effective_config(), &effective.yaml)?;
        controller.apply(&Paths::effective_config()).await?;
        // Запомненное «чем запускали» обязано догнать: по нему считается, нужен ли
        // перезапуск (D-102), и разъехаться с работающим ядром ему нельзя.
        let mut running = self.running.lock().unwrap();
        running.mode = Some(effective.mode);
        running.port = effective.port;
        running.device = effective.device.clone();
        running.launched = Some(effective.yaml.clone());
        Ok(())
    }

    /// Забыть карту подменных адресов. Ядро не запущено — забывать нечего.
    pub async fn flush_fake_ip(&self) -> Result<()> {
        self.require_controller()?.flush_fake_ip().await
    }

    /// Оборвать все соединения, чтобы новые пошли по новому маршруту (D-143).
    /// Ядро не запущено — рвать нечего.
    pub async fn close_connections(&self) -> Result<()> {
        match self.controller() {
            Some(controller) => controller.close_all().await,
            None => Ok(()),
        }
    }

    pub async fn reload(&self, source: &str) -> Result<()> {
        match self.controller() {
            Some(controller) => controller.reload(source).await,
            // Ядро не запущено — перечитывать нечему, файл и так на диске.
            None => Ok(()),
        }
    }

    /// Перечитать собранный список (D-157). Не запущено — нечего: файл и так на диске.
    pub async fn reload_rules(&self, name: &str) -> Result<()> {
        match self.controller() {
            Some(controller) => controller.reload_rules(name).await,
            None => Ok(()),
        }
    }

    /// Обновить geo-базы руками ядра (D-157). Без работающего ядра — нечем. Отдаёт даты
    /// файлов после обновления — ради них кнопку и нажимали.
    pub async fn update_geo(&self) -> Result<Vec<GeoFile>> {
        self.require_controller()?.update_geo().await?;
        Ok(Mihomo::geo_files())
    }

    fn require_controller(&self) -> Result<Controller> {
        self.controller()
            .ok_or_else(|| AppError::invalid("Ядро не запущено"))
    }

    /// Собранный конфиг приходит **снаружи** (D-071): что в него попало, решают настройки
    /// клиента — какой набор применён, — а дочерний процесс про настройки не знает ничего.
    ///
    /// Режим при этом читается из самого конфига, а не из настроек рядом (D-052): правку
    /// руками в разделе «Настройки» и нажатие в шапке ядро видит одинаково.
    pub async fn start(&self, effective: &Effective) -> Result<()> {
        Mihomo::check_privileges(effective.mode, Elevation::is_elevated())?;
        self.stop();

        let core = Paths::core();
        if !core.exists() {
            return Err(AppError::CoreMissing {
                path: core.display().to_string(),
            });
        }

        Paths::ensure_run_dir()?;
        crate::atomic::AtomicFile::write(Paths::effective_config(), &effective.yaml)?;

        self.log.clear();
        let controller = Controller::new()?;
        let mut child = self.spawn(&core, &controller, effective.device.as_deref())?;

        if let Err(why) = wait_ready(&mut child, &controller).await {
            // Не оставляем висеть ядро, которое запустилось, но так и не заработало.
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: why,
                log: self.log.tail(6),
            });
        }

        let mut running = self.running.lock().unwrap();
        running.child = Some(child);
        running.mode = Some(effective.mode);
        running.port = effective.port;
        running.probe = effective.probe;
        running.device = effective.device.clone();
        running.controller = Some(controller);
        running.started = crate::stamp::Stamp::now();
        running.wanted = true;
        running.launched = Some(effective.yaml.clone());
        Ok(())
    }

    /// Остановка по чужой воле: нашей команде, выходу, сдаче надзора. Снимает «должно
    /// работать» — после неё смерть процесса падением уже не считается (D-057).
    ///
    /// Сначала просим выйти самому (S-021): только так ядро сохраняет карту подменных
    /// адресов, а потерянная карта — это тихая подмена маршрута, а не медленный выход.
    /// Не вышло за `GRACE` — гасим как раньше: остановка обязана состояться.
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        if let Some(mut child) = running.child.take() {
            let asked = crate::system::console::Console::interrupt(child.id());
            CoreProcess::finish(&mut child, if asked { GRACE } else { Duration::ZERO });
        }
        running.wanted = false;
        running.mode = None;
        running.port = None;
        running.probe = None;
        running.device = None;
        running.controller = None;
        running.started = None;
        running.launched = None;
    }

    /// `device` — адаптер TUN: такому ядру нужны права, и запускает его система
    /// (`Elevation::privileged`: на Windows права уже у клиента, на Linux — помощник, D-173).
    fn spawn(
        &self,
        core: &std::path::Path,
        controller: &Controller,
        device: Option<&str>,
    ) -> Result<Child> {
        let mut command = match device {
            Some(device) => Elevation::privileged(core, device),
            None => Command::new(core),
        };
        command
            .arg("-d")
            .arg(Paths::run_dir())
            .arg("-f")
            .arg(Paths::effective_config())
            // Ядро отказывается читать файлы вне своего рабочего каталога: «path is not
            // subpath of home directory or SAFE_PATHS» (B-002). Провайдеры источников
            // и собранные списки поэтому выкладываются внутрь `run/` (D-170).
            .args(controller.args())
            // `null`, а не наследование: после мягкой остановки (S-021) клиент отцепляется
            // от консоли ядра, и его собственные стандартные дескрипторы становятся
            // недействительными. Унаследованный `stdin` тогда роняет **запуск**:
            // «Неверный дескриптор (os error 6)», то есть VPN, который выключили,
            // больше не включается (B-017). Ядру ввод не нужен вовсе.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Своя группа процессов — ради мягкой остановки: без неё Ctrl+Break
        // прилетел бы и клиенту (S-021).
        let mut child = CoreProcess::spawn(command, true, "mihomo")?;
        if let Some(out) = child.stdout.take() {
            self.log.pump(out);
        }
        if let Some(err) = child.stderr.take() {
            self.log.pump(err);
        }
        Ok(child)
    }
}

impl Mihomo {
    /// Бинарь ядра. Где он лежит, знает только mihomo (D-155): диагностике и статусу
    /// его отдают отсюда.
    pub fn binary() -> std::path::PathBuf {
        Paths::core()
    }

    /// Рабочий каталог ядра: кэш, собранный конфиг, пробные прогоны.
    pub fn workdir() -> std::path::PathBuf {
        Paths::run_dir()
    }

    /// Geo-базы в рабочем каталоге и когда каждая менялась (D-157). Какие именно лежат,
    /// решает ядро по режиму `geodata-mode` — поэтому показываем те, что нашлись.
    pub fn geo_files() -> Vec<GeoFile> {
        GEO_FILES
            .iter()
            .filter_map(|name| {
                let meta = std::fs::metadata(Paths::run_dir().join(name)).ok()?;
                Some(GeoFile {
                    name: (*name).to_string(),
                    modified: meta
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|since| since.as_secs()),
                })
            })
            .collect()
    }

    /// TUN настраивает виртуальный адаптер и без прав администратора падает уже после запуска,
    /// в логе ядра: `configure tun interface: Access is denied`. Ловим раньше и говорим понятнее.
    ///
    /// Открыта ради перезапуска (D-143): он проверяет права **до** остановки работающего
    /// ядра — иначе переключение на TUN без прав гасило бы VPN, а поднять не могло.
    pub fn check_privileges(mode: Mode, elevated: bool) -> Result<()> {
        if mode == Mode::Tun && !elevated {
            return Err(AppError::NeedsElevation {
                message: Elevation::missing(),
            });
        }
        Ok(())
    }
}

/// Ждём не «процесс жив» и даже не «порт принимает», а «ядро отвечает» — на своём API.
///
/// Так проверка одинакова в обоих режимах: в TUN слушающего прокси-порта нет, и раньше там
/// оставалось только «не упало за 1.2 с» (долг-потолок из D-009, теперь закрыт).
/// Что порт открывается заметно позже спавна — измерено, B-001.
///
/// Единственное место, оставшееся на `String`: причину знает эта функция, а лог — супервизор,
/// поэтому `CoreFailed` собирается у вызывающего. Возвращать отсюда `AppError` без лога значило
/// бы отдавать наружу заведомо неполную ошибку.
async fn wait_ready(child: &mut Child, controller: &Controller) -> std::result::Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Ok(Some(code)) = child.try_wait() {
            return Err(Elevation::refused(code).unwrap_or(format!("Ядро завершилось ({code}).")));
        }
        if controller.ready().await {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Ядро не ответило за 10 с.".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tun_demands_administrator() {
        assert!(
            Mihomo::check_privileges(Mode::Local, false).is_ok(),
            "local работает без прав"
        );
        assert!(
            Mihomo::check_privileges(Mode::Tun, true).is_ok(),
            "с правами TUN разрешён"
        );

        let refusal = Mihomo::check_privileges(Mode::Tun, false).unwrap_err();
        assert_eq!(
            refusal.kind(),
            "needsElevation",
            "интерфейс покажет кнопку по этому виду"
        );
        assert_eq!(refusal.to_string(), Elevation::missing(), "причина названа");
    }
}
