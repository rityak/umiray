//! Где лежат файлы приложения. Единственная ответственность модуля — пути.
//!
//! Всё живёт под `%LOCALAPPDATA%\umiray` (D-150). Не `app_local_data_dir` из Tauri —
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
pub const CLIENT_NAME: &str = if cfg!(debug_assertions) {
    "umiray-dev.exe"
} else {
    "umiray.exe"
};

pub const CORE_NAME: &str = if cfg!(debug_assertions) {
    "mihomo-dev.exe"
} else {
    "mihomo.exe"
};

pub struct Paths;

impl Paths {
    pub fn root() -> PathBuf {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(APP_NAME)
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
pub const QD_NAME: &str = if cfg!(debug_assertions) {
    "qd-dev.exe"
} else {
    "qd.exe"
};

/// Свой `LOCALAPPDATA` для проверки с диском — под замком на весь процесс: переменная одна
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
        std::env::set_var("LOCALAPPDATA", &dir);
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
