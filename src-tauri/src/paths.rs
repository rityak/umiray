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

use std::io;
use std::path::PathBuf;

pub const APP_NAME: &str = if cfg!(debug_assertions) {
    "umiray-dev"
} else {
    "umiray"
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

    /// Конфиг до разделения на профиль и оверрайд. Остался только ради разовой миграции.
    pub fn legacy_config() -> PathBuf {
        Paths::root().join("config.yaml")
    }

    pub fn qd() -> PathBuf {
        Paths::root().join(QD_NAME)
    }

    /// Состояние qd: его база, `client.db`. Пишет сам qd, мы только даём каталог.
    pub fn qd_dir() -> PathBuf {
        Paths::root().join("qd")
    }

    /// Настройки приложения: поведение, переживающее перезапуск (D-024).
    pub fn settings() -> PathBuf {
        Paths::root().join("settings.json")
    }

    /// Каталог источников: список ссылок и метаданные на каждый (D-032).
    /// Список ссылок читает само ядро — это `path:` его провайдера.
    pub fn sources_dir() -> PathBuf {
        Paths::root().join("sources")
    }

    pub fn source_links(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.txt"))
    }

    /// Что прислала панель, до наших правок. Без него кнопка «Сброс» не может ничего вернуть.
    pub fn source_raw(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.raw"))
    }

    /// Правки пользователя: разница, а не поправленные узлы.
    pub fn source_patches(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.patch.json"))
    }

    /// Правка **записи** узла — та, что клиент пишет сам (D-119). Тоже разница, а не копия.
    pub fn source_entries(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.entry.yaml"))
    }

    pub fn source_meta(id: &str) -> PathBuf {
        Paths::sources_dir().join(format!("{id}.json"))
    }

    /// Настройки ядра: конфиг целиком, кроме источников, групп и правил.
    /// Правит его и редактор, и переключатель режима — файл один на обоих (D-052).
    pub fn advanced() -> PathBuf {
        Paths::root().join("advanced.yaml")
    }

    /// Настройки самого клиента: то, чего нет в конфиге ядра (D-068). Ядру не уходит.
    pub fn client() -> PathBuf {
        Paths::root().join("client.yaml")
    }

    /// Тот же файл до переименования. Остался ради разового переезда.
    pub fn legacy_override() -> PathBuf {
        Paths::root().join("override.yaml")
    }

    /// Группы выбора пользователя. Пусто — группу собирает клиент (D-044).
    pub fn groups() -> PathBuf {
        Paths::root().join("groups.yaml")
    }

    /// Маршрутизация пользователя. Пусто — весь трафик идёт через группу клиента (D-044).
    pub fn rules() -> PathBuf {
        Paths::root().join("rules.yaml")
    }

    pub fn ensure_sources_dir() -> io::Result<()> {
        std::fs::create_dir_all(Paths::sources_dir())
    }

    /// Наборы конфигов: пара «группы + маршрутизация» под направление (D-056).
    pub fn presets_dir() -> PathBuf {
        Paths::root().join("presets")
    }

    pub fn preset_meta(id: &str) -> PathBuf {
        Paths::presets_dir().join(format!("{id}.json"))
    }

    /// Файл набора. `part` — идентификатор пользовательского файла: `groups` или `rules`.
    pub fn preset_part(id: &str, part: &str) -> PathBuf {
        Paths::presets_dir().join(format!("{id}.{part}.yaml"))
    }

    pub fn ensure_presets_dir() -> io::Result<()> {
        std::fs::create_dir_all(Paths::presets_dir())
    }

    /// Коллекции: всё, что клиент поставляет данными, а владеет ими пользователь (D-100).
    /// Резолверы, эталонные ресурсы, встроенные наборы правил — читается отсюда, а не из бинаря.
    pub fn collections_dir() -> PathBuf {
        Paths::root().join("collections")
    }

    /// Коллекция-документ: один файл с известной схемой.
    pub fn collection_file(name: &str) -> PathBuf {
        Paths::collections_dir().join(format!("{name}.yaml"))
    }

    /// Коллекция-папка: много файлов одной схемы, которые перечисляются на лету.
    pub fn collection_folder(name: &str) -> PathBuf {
        Paths::collections_dir().join(name)
    }

    /// Где лежали коллекции до D-100. Нужны переезду, и только ему.
    pub fn legacy_catalog_dir() -> PathBuf {
        Paths::root().join("catalog")
    }

    pub fn legacy_rulesets_dir() -> PathBuf {
        Paths::root().join("rulesets")
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

    /// Идентификатор установки для подписок с привязкой по устройству.
    /// Страна по адресу узла, кэшем (D-084). Ключ — `host:port`: имя узла панель меняет,
    /// адрес нет.
    pub fn geo() -> PathBuf {
        Paths::root().join("geo.json")
    }

    pub fn hwid() -> PathBuf {
        Paths::root().join("hwid.txt")
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

#[cfg(test)]
mod tests {
    /// Кто вправе знать, где лежит каждый вид данных (D-155). Путь знает только его
    /// хранилище: ради этого переезд части данных в SQLite — замена одного хранилища,
    /// а не поиск путей по всему коду. Новый путь без строки здесь — провал теста:
    /// у данных обязан быть владелец.
    const OWNERS: &[(&str, &[&str])] = &[
        ("settings", &["app/settings.rs"]),
        ("sources_dir", &["nodes/sources.rs"]),
        (
            "ensure_sources_dir",
            &["nodes/sources.rs", "nodes/entries.rs"],
        ),
        ("source_links", &["nodes/sources.rs"]),
        ("source_raw", &["nodes/sources.rs"]),
        ("source_patches", &["nodes/sources.rs"]),
        ("source_meta", &["nodes/sources.rs"]),
        ("source_entries", &["nodes/entries.rs"]),
        ("advanced", &["config/files.rs"]),
        ("client", &["config/files.rs"]),
        ("groups", &["config/files.rs"]),
        ("presets_dir", &["config/presets.rs"]),
        ("preset_meta", &["config/presets.rs"]),
        ("preset_part", &["config/presets.rs"]),
        ("ensure_presets_dir", &["config/presets.rs"]),
        ("collections_dir", &["collections.rs"]),
        ("collection_file", &["collections.rs"]),
        ("collection_folder", &["collections.rs"]),
        ("core", &["core/mihomo.rs"]),
        ("run_dir", &["core/mihomo.rs"]),
        ("ensure_run_dir", &["core/mihomo.rs"]),
        ("effective_config", &["core/mihomo.rs"]),
        ("qd", &["core/qd.rs"]),
        ("qd_dir", &["core/qd.rs"]),
        ("geo", &["nodes/geo.rs"]),
        ("hwid", &["nodes/device.rs"]),
    ];
    /// Сам каталог данных общий: «убедиться, что он есть» — не знание о чужих данных.
    const SHARED: &[&str] = &["root", "ensure_root"];
    /// Переезд по определению знает и старую раскладку, и новую.
    const MIGRATION: &[&str] = &["app/migrate.rs", "app/data.rs"];

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
