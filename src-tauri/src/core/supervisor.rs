//! Процесс ядра: запуск в нужном режиме, ожидание готовности, лог и остановка.
//!
//! Владеет дочерним процессом и его выводом. Наружу отдаёт только состояние и строки лога —
//! команды Tauri про `Child` ничего не знают.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::controller::{self, Controller};

use crate::config::mode::Mode;
use crate::error::{AppError, Result};
use crate::paths;
use crate::render::mihomo::Effective;
use crate::system::elevation;

/// Сколько строк вывода ядра держим. Единственный источник правды о том, почему оно не встало.
const LOG_LINES: usize = 500;
/// Сколько ждём, пока ядро начнёт отвечать: большой конфиг грузится небыстро.
const READY_TIMEOUT: Duration = Duration::from_secs(10);
/// Сколько ждём, пока ядро выйдет само после Ctrl+Break. Замер даёт около ста
/// миллисекунд (S-021); полсекунды — потолок на медленную машину, после него гасим.
const GRACE: Duration = Duration::from_millis(500);
/// Без этого флага при каждом запуске мигает окно консоли.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
}

pub struct Supervisor {
    running: Mutex<Running>,
    logs: Arc<Mutex<VecDeque<String>>>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(Running::default()),
            logs: Arc::new(Mutex::new(VecDeque::new())),
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
            running.controller = None;
            running.started = None;
            running.launched = None;
        }
        Status {
            running: alive,
            mode: running.mode,
            port: running.port,
            started: running.started,
        }
    }

    /// Конфиг, на котором ядро **работает сейчас**. Пусто — не работает. По нему считается,
    /// доедет ли правка перезагрузкой (D-102): сравнивать надо с работающим, а не с файлом.
    pub fn launched(&self) -> Option<String> {
        self.running.lock().unwrap().launched.clone()
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    /// Ядро умерло само, хотя должно работать. Единственный способ отличить падение
    /// от штатной остановки — флаг `wanted`: смерть процесса его не снимает (D-057).
    pub fn crashed(&self) -> bool {
        // `status` заодно подчищает состояние, поэтому он первым — и не под своим замком.
        let alive = self.status().running;
        !alive && self.running.lock().unwrap().wanted
    }

    /// Своя строка в логе ядра. Формат — тот же `time=… level=… msg=…`, которым пишет
    /// ядро: окно уже умеет его разбирать и фильтровать, время встаёт в ту же колонку,
    /// а `umiray:` говорит, кто автор строки.
    pub fn note(&self, level: &str, message: &str) {
        push(
            &self.logs,
            format!(
                "time=\"{}\" level={level} msg=\"umiray: {message}\"",
                crate::stamp::local()
            ),
        );
    }

    /// Вернуть в кольцо строки прошлой жизни ядра. Запуск кольцо чистит, поэтому надзор
    /// снимает хвост до подъёма и возвращает после — иначе причина падения пропадёт
    /// вместе с ним (D-057).
    pub fn recall(&self, lines: Vec<String>) {
        for line in lines {
            push(&self.logs, line);
        }
    }

    /// Хвост лога списком строк: интерфейс отрисует их сам, склеивать в текст ошибки нечего.
    pub fn tail(&self, lines: usize) -> Vec<String> {
        let logs = self.logs.lock().unwrap();
        logs.iter()
            .skip(logs.len().saturating_sub(lines))
            .cloned()
            .collect()
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
        paths::ensure_run_dir()?;
        crate::atomic::write(paths::effective_config(), &effective.yaml)?;
        controller.apply(&paths::effective_config()).await?;
        // Запомненное «чем запускали» обязано догнать: по нему считается, нужен ли
        // перезапуск (D-102), и разъехаться с работающим ядром ему нельзя.
        let mut running = self.running.lock().unwrap();
        running.mode = Some(effective.mode);
        running.port = effective.port;
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
        check_privileges(effective.mode, elevation::is_elevated())?;
        self.stop();

        let core = paths::core();
        if !core.exists() {
            return Err(AppError::CoreMissing {
                path: core.display().to_string(),
            });
        }

        paths::ensure_run_dir()?;
        crate::atomic::write(paths::effective_config(), &effective.yaml)?;

        self.logs.lock().unwrap().clear();
        let controller = Controller::new()?;
        let mut child = self.spawn(&core, &controller)?;

        if let Err(why) = wait_ready(&mut child, &controller).await {
            // Не оставляем висеть ядро, которое запустилось, но так и не заработало.
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: why,
                log: self.tail(6),
            });
        }

        let mut running = self.running.lock().unwrap();
        running.child = Some(child);
        running.mode = Some(effective.mode);
        running.port = effective.port;
        running.probe = effective.probe;
        running.controller = Some(controller);
        running.started = crate::stamp::now();
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
            if crate::system::console::interrupt(child.id()) {
                let began = std::time::Instant::now();
                while began.elapsed() < GRACE {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        running.wanted = false;
        running.mode = None;
        running.port = None;
        running.probe = None;
        running.controller = None;
        running.started = None;
        running.launched = None;
    }

    fn spawn(&self, core: &std::path::Path, controller: &Controller) -> Result<Child> {
        let mut command = Command::new(core);
        command
            .arg("-d")
            .arg(paths::run_dir())
            .arg("-f")
            .arg(paths::effective_config())
            // Ядро отказывается читать файлы провайдеров вне своего рабочего каталога:
            // «path is not subpath of home directory or SAFE_PATHS». Рабочий каталог —
            // `run/`, а источники лежат в соседнем `sources/` (D-014), и это правильно:
            // они наши, а не его. Открываем ему ровно один каталог, а не расширяем `-d`
            // до корня — иначе ядро начало бы писать свой кэш вперемешку с нашими файлами.
            .env("SAFE_PATHS", paths::sources_dir())
            .args(controller.args())
            // `null`, а не наследование: после мягкой остановки (S-021) клиент отцепляется
            // от консоли ядра, и его собственные стандартные дескрипторы становятся
            // недействительными. Унаследованный `stdin` тогда роняет **запуск**:
            // «Неверный дескриптор (os error 6)», то есть VPN, который выключили,
            // больше не включается (B-017). Ядру ввод не нужен вовсе.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Своя группа процессов — ради мягкой остановки: без неё Ctrl+Break
            // прилетел бы и клиенту (S-021).
            command.creation_flags(CREATE_NO_WINDOW | crate::system::console::NEW_PROCESS_GROUP);
        }

        let mut child = command.spawn().map_err(|e| AppError::CoreFailed {
            message: format!("Не удалось запустить ядро: {e}"),
            log: Vec::new(),
        })?;
        // Клетка до всего остального: с этой секунды ядро не переживёт падение клиента
        // (D-058). Между спавном и этой строкой окно всё-таки есть — микросекунды,
        // и закрыть его можно только запуском в приостановленном виде.
        // Потолок: окно в микросекунды, лечится CREATE_SUSPENDED + ResumeThread.
        if !crate::system::job::attach(child.as_raw_handle()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::CoreFailed {
                message: "Не удалось привязать ядро к процессу клиента".into(),
                log: vec!["Windows job object не принял процесс ядра".into()],
            });
        }
        if let Some(out) = child.stdout.take() {
            pump(out, self.logs.clone());
        }
        if let Some(err) = child.stderr.take() {
            pump(err, self.logs.clone());
        }
        Ok(child)
    }
}

/// Ядро, пережившее прошлую жизнь клиента, — прибрать при запуске (D-059).
///
/// Клетка (D-058) осиротеть ему не даёт, но она появилась не всегда и может не создаться,
/// а осиротевшее ядро ломает ровно всё: держит порт, держит рабочий каталог, а в TUN —
/// адаптер со всем трафиком машины, и выключить его из окна нельзя, потому что клиент
/// про него ничего не знает. Поэтому бьём по имени файла ядра, не разбираясь, чей процесс:
/// stable и dev имеют разные имена ядра и не трогают друг друга (D-150).
///
/// Зовётся **только** из `setup`, и это важно: вторая копия приложения гасится плагином
/// одиночного запуска раньше (D-046), а вызов до сборки приложения убивал бы ядро первой.
pub fn sweep() {
    let Some(name) = paths::core().file_name().map(std::ffi::OsString::from) else {
        return;
    };
    let mut command = Command::new("taskkill");
    command
        .arg("/IM")
        .arg(name)
        .arg("/F")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // Код возврата не смотрим: «процесса нет» — обычный случай, а не ошибка.
    let _ = command.status();
}

/// TUN настраивает виртуальный адаптер и без прав администратора падает уже после запуска,
/// в логе ядра: `configure tun interface: Access is denied`. Ловим раньше и говорим понятнее.
///
/// Открыта ради перезапуска (D-143): он проверяет права **до** остановки работающего
/// ядра — иначе переключение на TUN без прав гасило бы VPN, а поднять не могло.
pub fn check_privileges(mode: Mode, elevated: bool) -> Result<()> {
    if mode == Mode::Tun && !elevated {
        return Err(AppError::NeedsElevation {
            message: "Для режима TUN нужны права администратора — перезапустите приложение".into(),
        });
    }
    Ok(())
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
            return Err(format!("Ядро завершилось ({code})."));
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

/// Вывод ядра — единственный способ понять, почему оно не поднялось.
fn pump(stream: impl Read + Send + 'static, logs: Arc<Mutex<VecDeque<String>>>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(|line| line.ok()) {
            push(&logs, line);
        }
    });
}

/// Кольцо на `LOG_LINES` строк: пишут в него и ядро, и клиент (`note`).
fn push(logs: &Mutex<VecDeque<String>>, line: String) {
    let mut logs = logs.lock().unwrap();
    if logs.len() >= LOG_LINES {
        logs.pop_front();
    }
    logs.push_back(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tun_demands_administrator() {
        assert!(
            check_privileges(Mode::Local, false).is_ok(),
            "local работает без прав"
        );
        assert!(
            check_privileges(Mode::Tun, true).is_ok(),
            "с правами TUN разрешён"
        );

        let refusal = check_privileges(Mode::Tun, false).unwrap_err();
        assert_eq!(
            refusal.kind(),
            "needsElevation",
            "интерфейс покажет кнопку по этому виду"
        );
        let refusal = refusal.to_string();
        assert!(
            refusal.contains("администратора"),
            "причина названа: {refusal}"
        );
    }
}
