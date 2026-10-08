//! Помощник с правами root (D-173): тот же бинарь клиента, запущенный `pkexec … --helper`.
//!
//! Клиент работает от пользователя. То немногое, что требует root, делает эта короткая
//! жизнь: отдать ядру права на TUN, прописать DNS туннеля в systemd-resolved, поставить
//! и снять запрет nftables. Каждая команда — фиксированный список действий: из аргументов
//! берутся только проверенные имя адаптера, метка и путь к ядру своего пользователя.
//!
//! Polkit пропускает без пароля только сессию у экрана (`allow_active`), и только этот
//! бинарь с первым аргументом `--helper` (аннотация `exec.argv1` в действии пакета).

use std::ffi::{CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};

/// Первый аргумент помощника — его и ждёт polkit.
pub const FLAG: &str = "--helper";

/// Адрес внутри сети туннеля, куда resolved шлёт запросы: ядро перехватывает любой `:53`
/// (`dns-hijack`), а маршрут к нему идёт через адаптер. ponytail: сеть туннеля — умолчание
/// ядра `198.18.0.1/30`; свой `inet4-address` в конфиге потребует брать адрес оттуда.
const TUN_DNS: &str = "198.18.0.2";

/// Сколько ждём адаптер, чтобы прописать ему DNS: ядро поднимает его за доли секунды.
const DEVICE_WAIT: Duration = Duration::from_secs(20);

/// Таблица nftables клиента (своя у отладочной сборки, D-150). Удаляется целиком — так
/// снимается и запрет, оставшийся от прошлой жизни клиента.
const TABLE: &str = if cfg!(debug_assertions) {
    "umiray_dev"
} else {
    "umiray"
};

/// Локальные сети и сеть туннеля: принтер, роутер, NAS остаются доступны и при запрете,
/// как на Windows.
const LOCAL4: &str =
    "10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, 224.0.0.0/4, 198.18.0.0/15";
const LOCAL6: &str = "fe80::/10, fc00::/7, ff00::/8";

pub struct Helper;

impl Helper {
    /// Команда, которая выполнит `args` от root. Клиент, сам запущенный от root, — без pkexec.
    pub fn command(args: &[&OsStr]) -> Command {
        let me = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("umiray"));
        let mut command = if is_root() {
            Command::new(me)
        } else {
            let mut pkexec = Command::new("pkexec");
            pkexec.arg(me);
            pkexec
        };
        command.arg(FLAG).args(args);
        command
    }

    /// Выполнить и дождаться. Отказ — с тем, что сказал помощник или polkit.
    pub fn run(args: &[&str]) -> Result<()> {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let out = Helper::command(&args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| AppError::io(format!("Не удалось вызвать pkexec: {e}")))?;
        if out.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // 126 и 127 — ответ самого pkexec: человек отказал или polkit не разрешил. Не
        // `NeedsElevation`: кнопка «перезапустить с правами» здесь ничего бы не дала.
        if matches!(out.status.code(), Some(126 | 127)) {
            return Err(AppError::io(format!("Система не дала прав: {said}")));
        }
        Err(AppError::io(format!(
            "Помощник с правами не справился: {said}"
        )))
    }

    /// Есть ли чем получить права.
    pub fn available() -> bool {
        is_root() || tool("pkexec").is_some()
    }

    /// Чем pkexec объяснил отказ: 126 — человек нажал «Отмена», 127 — polkit не разрешил
    /// (сеанс не у экрана или нет агента, который спросил бы пароль).
    pub fn refused(code: Option<i32>) -> Option<&'static str> {
        match code? {
            126 => Some("Запрос прав отклонён — TUN не поднят."),
            127 => Some(
                "Система не дала прав на TUN: polkit не разрешил запуск. Так бывает вне сеанса \
                 у экрана (ssh, другой пользователь) или без агента polkit в окружении.",
            ),
            _ => None,
        }
    }

    /// Точка входа помощника. `None` — это не помощник, клиент идёт дальше.
    pub fn serve(args: &[String]) -> Option<i32> {
        if args.get(1).map(String::as_str) != Some(FLAG) {
            return None;
        }
        let done = match args.get(2..).unwrap_or_default() {
            [command, device, core, rest @ ..] if command == "core" => run_core(device, core, rest),
            [command, action, device, mark] if command == "killswitch" && action == "apply" => {
                block(device, mark)
            }
            [command, action] if command == "killswitch" && action == "release" => unblock(),
            _ => Err("неизвестная команда помощника".to_string()),
        };
        Some(match done {
            Ok(()) => 0,
            Err(why) => {
                eprintln!("{why}");
                1
            }
        })
    }
}

fn is_root() -> bool {
    // SAFETY: только чтение идентификатора.
    unsafe { libc::geteuid() == 0 }
}

/// Пользователь, позвавший pkexec: uid, gid и домашний каталог. Без pkexec — root.
fn caller() -> std::result::Result<(u32, u32, PathBuf), String> {
    let uid: u32 = std::env::var("PKEXEC_UID")
        .ok()
        .and_then(|uid| uid.parse().ok())
        .unwrap_or(0);
    // SAFETY: getpwuid отдаёт указатель на статическую запись; читаем её сразу.
    unsafe {
        let entry = libc::getpwuid(uid);
        if entry.is_null() {
            return Err(format!("пользователь {uid} не найден"));
        }
        let home = std::ffi::CStr::from_ptr((*entry).pw_dir);
        Ok((
            uid,
            (*entry).pw_gid,
            PathBuf::from(OsStr::from_bytes(home.to_bytes())),
        ))
    }
}

/// Ядро в TUN: права на адаптер, DNS туннеля, смерть вместе с клиентом (S-035).
///
/// Бинарь — только ядро клиента в доме позвавшего пользователя и принадлежащее ему:
/// чужой файл под этим именем прав не получит.
fn run_core(device: &str, core: &str, rest: &[String]) -> std::result::Result<(), String> {
    let device = checked_device(device)?;
    let (uid, gid, home) = caller()?;
    let core = Path::new(core);
    let meta = std::fs::metadata(core).map_err(|e| format!("ядро {}: {e}", core.display()))?;
    if core.file_name() != Some(OsStr::new(crate::paths::CORE_NAME))
        || !core.starts_with(&home)
        || meta.uid() != uid
        || !meta.is_file()
    {
        return Err(format!("{} — не ядро клиента", core.display()));
    }
    tell_resolved_later(device);
    // exec возвращается только с ошибкой: на успехе этот процесс уже стал ядром.
    let error = Command::new(setpriv())
        .arg(format!("--reuid={uid}"))
        .arg(format!("--regid={gid}"))
        .arg("--init-groups")
        .arg("--inh-caps=+net_admin,+net_bind_service")
        .arg("--ambient-caps=+net_admin,+net_bind_service")
        .arg("--pdeathsig=TERM")
        .arg(core)
        .args(rest)
        .env("HOME", &home)
        .exec();
    Err(format!("setpriv не запустился: {error}"))
}

fn setpriv() -> PathBuf {
    tool("setpriv").unwrap_or_else(|| PathBuf::from("setpriv"))
}

/// Системная утилита по полному пути. `PATH` не годится: у пользователя Debian в нём нет
/// `/usr/sbin`, где лежит `nft`, а pkexec выдаёт помощнику свой урезанный.
pub fn tool(name: &str) -> Option<PathBuf> {
    ["/usr/sbin", "/usr/bin", "/sbin", "/bin"]
        .iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|path| path.is_file())
}

/// Прописать DNS туннеля, когда адаптер появится, — отдельным процессом: этот через миг
/// станет ядром. Ребёнок сразу отпускает вывод, иначе клиент ждал бы конца лога ядра, пока
/// ребёнок жив. Адаптер исчезнет вместе с ядром — resolved забудет и его DNS: снимать нечего.
fn tell_resolved_later(device: &str) {
    let Ok(path) = CString::new(format!("/sys/class/net/{device}")) else {
        return;
    };
    // SAFETY: до fork процесс однонитевой — помощник не поднимает ни Tauri, ни рантайм.
    if unsafe { libc::fork() } != 0 {
        return;
    }
    // SAFETY: в ребёнке — только перенаправление своих дескрипторов на /dev/null.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        libc::dup2(null, 1);
        libc::dup2(null, 2);
    }
    let began = Instant::now();
    // SAFETY: access только проверяет путь.
    while unsafe { libc::access(path.as_ptr(), libc::F_OK) } != 0 {
        if began.elapsed() > DEVICE_WAIT {
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for args in [
        ["dns", device, TUN_DNS],
        ["domain", device, "~."],
        ["default-route", device, "yes"],
    ] {
        // Без systemd-resolved (Debian по умолчанию) прописывать некому, и не нужно:
        // запросы к резолверу из resolv.conf идут в туннель как любой `:53` (S-035).
        if let Some(resolvectl) = tool("resolvectl") {
            let _ = Command::new(resolvectl).args(args).status();
        }
    }
    std::process::exit(0);
}

/// Имя адаптера пишет человек (`tun.device`), а уходит оно в команды от root.
fn checked_device(device: &str) -> std::result::Result<&str, String> {
    let fits = !device.is_empty()
        && device.len() <= 15
        && device
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    fits.then_some(device)
        .ok_or_else(|| format!("имя адаптера не подходит: {device}"))
}

/// Текст запрета. Отдельно ради проверки: состав разрешений **и есть** защита (как
/// `killswitch::script` на Windows).
fn ruleset(device: &str, mark: u32) -> String {
    format!(
        "table inet {TABLE} {{
  chain output {{
    type filter hook output priority 0; policy drop;
    oif \"lo\" accept
    oifname \"{device}\" accept
    meta mark {mark} accept
    ip daddr {{ {LOCAL4} }} accept
    ip6 daddr {{ {LOCAL6} }} accept
  }}
}}
"
    )
}

/// Поставить запрет одним файлом nft: прежняя таблица заменяется атомарно, без мгновения,
/// в котором машина открыта (B-042).
fn block(device: &str, mark: &str) -> std::result::Result<(), String> {
    let device = checked_device(device)?;
    let mark: u32 = mark
        .parse()
        .map_err(|_| format!("метка не число: {mark}"))?;
    nft(&format!(
        "table inet {TABLE}\ndelete table inet {TABLE}\n{}",
        ruleset(device, mark)
    ))
}

/// Снять запрет. Пустая таблица заводится и удаляется — так снятие успешно и тогда,
/// когда запрета не было.
fn unblock() -> std::result::Result<(), String> {
    nft(&format!("table inet {TABLE}\ndelete table inet {TABLE}\n"))
}

fn nft(script: &str) -> std::result::Result<(), String> {
    use std::io::Write;
    let nft = tool("nft").ok_or("nft не найден — поставьте пакет nftables")?;
    let mut child = Command::new(nft)
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("nft не запустился: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(script.as_bytes())
            .map_err(|e| format!("nft: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("nft: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "nft: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Выпади правило адаптера — встанет трафик приложений; выпади метка — туннель
    /// не поднимется; умолчание — запрет.
    #[test]
    fn the_block_lets_out_only_the_tunnel_the_core_and_the_local_network() {
        let rules = ruleset("Meta", 6666);
        assert!(rules.contains("policy drop;"));
        assert!(rules.contains("oifname \"Meta\" accept"));
        assert!(rules.contains("meta mark 6666 accept"));
        assert!(rules.contains("192.168.0.0/16"));
        assert!(rules.contains("198.18.0.0/15"), "сеть туннеля");
    }

    #[test]
    fn a_device_name_cannot_carry_a_command() {
        assert!(checked_device("Meta").is_ok());
        assert!(checked_device("utun-1.2").is_ok());
        assert!(checked_device("Meta\" accept; flush ruleset").is_err());
        assert!(checked_device("").is_err());
        assert!(checked_device("a-very-long-name-x").is_err());
    }

    #[test]
    fn not_a_helper_call_goes_on_to_the_client() {
        assert_eq!(Helper::serve(&["umiray".into()]), None);
        assert_eq!(
            Helper::serve(&["umiray".into(), FLAG.into(), "rm".into()]),
            Some(1)
        );
    }
}
