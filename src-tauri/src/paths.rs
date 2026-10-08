//! Где лежат файлы приложения. Единственная ответственность модуля — пути.
//!
//! Всё живёт под `%LOCALAPPDATA%\umiray` на Windows и `$XDG_DATA_HOME/umiray`
//! (`~/.local/share/umiray`) на Linux (D-150). Не `app_local_data_dir` из Tauri —
//! тот подставил бы identifier из tauri.conf.json.
//!
//! **У отладочной сборки каталог свой** — `umiray-dev` (D-150). Разработка идёт
//! на той же машине, где клиентом пользуются, и `tauri dev` иначе правил бы живые
//! подписки, выбор узла и настройки того, кто просто хотел, чтобы VPN работал.
//! Признак — профиль сборки, а не переменная окружения: забыть её ровно так же легко,
//! как и не заметить, что правишь настоящий конфиг.
//!
//! В каталоге — клиент, ядра, база и то, что читают ядра (D-170, D-171): данные клиента
//! лежат в базе, на диске остаётся только сгенерированное для ядра в `run/`.

use std::io;
use std::path::PathBuf;

pub const APP_NAME: &str = if cfg!(debug_assertions) {
    "umiray-dev"
} else {
    "umiray"
};

/// Имя клиента в каталоге данных (D-171). Совпадает с именем бинаря сборки: так копия
/// из `target/debug` и установленная — один и тот же процесс для `taskkill` и проверок.
/// Только Windows: на Linux клиент лежит там, куда его положил пакет.
#[cfg(windows)]
pub const CLIENT_NAME: &str = if cfg!(debug_assertions) {
    "umiray-dev.exe"
} else {
    "umiray.exe"
};

/// Имя бинаря: своё у отладочной сборки (D-150), `.exe` только на Windows.
macro_rules! binary {
    ($debug:literal, $release:literal) => {
        match (cfg!(debug_assertions), cfg!(windows)) {
            (true, true) => concat!($debug, ".exe"),
            (false, true) => concat!($release, ".exe"),
            (true, false) => $debug,
            (false, false) => $release,
        }
    };
}

pub const CORE_NAME: &str = binary!("mihomo-dev", "mihomo");

/// Переменная с корнем данных пользователя. Её же подменяют проверки (`Sandbox`).
#[cfg(windows)]
pub const BASE: &str = "LOCALAPPDATA";
#[cfg(not(windows))]
pub const BASE: &str = "XDG_DATA_HOME";

pub struct Paths;

impl Paths {
    pub fn root() -> PathBuf {
        Paths::base().join(APP_NAME)
    }

    /// Корень данных пользователя. На Linux без `XDG_DATA_HOME` — его умолчание
    /// по спецификации XDG, `~/.local/share`.
    fn base() -> PathBuf {
        if let Some(base) = std::env::var_os(BASE).filter(|base| !base.is_empty()) {
            return PathBuf::from(base);
        }
        if cfg!(windows) {
            return PathBuf::from(".");
        }
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/share"))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Предыдущий каталог этого же окружения, только для разового переезда (D-150).
    pub fn legacy_root() -> PathBuf {
        Paths::root().with_file_name(if cfg!(debug_assertions) {
            "umiray-client-dev"
        } else {
            "umiray-client"
        })
    }

    /// База клиента: все его данные (D-170).
    pub fn db() -> PathBuf {
        Paths::root().join("umiray.db")
    }

    /// Единственное место, откуда клиент работает (D-171).
    #[cfg(windows)]
    pub fn client_exe() -> PathBuf {
        Paths::root().join(CLIENT_NAME)
    }

    pub fn qd() -> PathBuf {
        Paths::root().join(QD_NAME)
    }

    /// Состояние qd: его база, `client.db`. Пишет сам qd, мы только даём каталог.
    pub fn qd_dir() -> PathBuf {
        Paths::root().join("qd")
    }

    /// Провайдеры источников для ядра: то, что хранилище источников выкладывает из базы
    /// при каждой публикации (D-170). Внутри рабочего каталога ядра — читать вне его
    /// mihomo отказывается (B-002).
    pub fn sources_dir() -> PathBuf {
        Paths::run_dir().join("sources")
    }

    pub fn source_provider(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.yaml"))
    }

    /// Собранное ядрами из rule sets (D-157): `<ядро>/<id>.<часть>.mrs`. Сами списки — в базе.
    pub fn lists_dir() -> PathBuf {
        Paths::run_dir().join("lists")
    }

    /// Рабочий каталог ядра: сюда кладётся сгенерированный конфиг, сюда ядро пишет своё.
    pub fn run_dir() -> PathBuf {
        Paths::root().join("run")
    }

    /// Конфиг, который реально запускается: пользовательский плюс поля режима.
    pub fn effective_config() -> PathBuf {
        Paths::run_dir().join("config.yaml")
    }

    pub fn core() -> PathBuf {
        Paths::root().join(CORE_NAME)
    }

    pub fn ensure_root() -> io::Result<()> {
        std::fs::create_dir_all(Paths::root())
    }

    pub fn ensure_run_dir() -> io::Result<()> {
        std::fs::create_dir_all(Paths::run_dir())
    }
}

/// Бинарь второго ядра (D-154). У debug своё имя, как у mihomo (D-150): уборка сирот
/// по пути одной сборки не тронет процесс другой.
pub const QD_NAME: &str = binary!("qd-dev", "qd");

/// Свой корень данных (`BASE`) для проверки с диском — под замком на весь процесс: переменная одна
/// на процесс, и две проверки иначе подменяли бы каталог друг у друга на ходу.
#[cfg(test)]
pub struct Sandbox {
    _lock: std::sync::MutexGuard<'static, ()>,
    pub dir: PathBuf,
}

#[cfg(test)]
impl Sandbox {
    pub fn new(name: &str) -> Sandbox {
        static DISK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("umiray-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var(BASE, &dir);
        Sandbox { _lock: lock, dir }
    }
}

#[cfg(test)]
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    /// Кто вправе знать, где лежит каждый вид данных (D-155). Путь знает только его
    /// хранилище: ради этого переезд в SQLite (D-170) заменил хранилища, а не искал пути
    /// по всему коду. Новый путь без строки здесь — провал теста: у данных обязан быть владелец.
    const OWNERS: &[(&str, &[&str])] = &[
        ("db", &["db.rs"]),
        ("client_exe", &["system/install.rs"]),
        ("sources_dir", &["nodes/sources.rs"]),
        ("source_provider", &["nodes/sources.rs"]),
        ("lists_dir", &["lists/store.rs"]),
        ("core", &["core/mihomo.rs"]),
        ("run_dir", &["core/mihomo.rs"]),
        ("ensure_run_dir", &["core/mihomo.rs"]),
        ("effective_config", &["core/mihomo.rs"]),
        ("qd", &["core/qd.rs"]),
        ("qd_dir", &["core/qd.rs"]),
    ];
    /// Сам каталог данных общий: «убедиться, что он есть» — не знание о чужих данных.
    const SHARED: &[&str] = &["root", "ensure_root"];
    /// Переезд по определению знает и старую раскладку, и новую.
    const MIGRATION: &[&str] = &["app/import.rs", "app/data.rs"];

    #[test]
    fn only_the_owner_knows_where_its_data_lives() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src.clone()];
        let mut wrong = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if !name.ends_with(".rs") || name == "paths.rs" || name == "live.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
                for used in code.split("Paths::").skip(1) {
                    let item: String = used
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if SHARED.contains(&item.as_str()) || MIGRATION.contains(&name.as_str()) {
                        continue;
                    }
                    match OWNERS.iter().find(|(path, _)| *path == item) {
                        Some((_, owners)) if owners.contains(&name.as_str()) => {}
                        Some(_) => {
                            wrong.push(format!("{name}: Paths::{item} — путь чужого хранилища"))
                        }
                        None => wrong.push(format!("{name}: Paths::{item} — у пути нет владельца")),
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
