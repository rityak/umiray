//! Где лежат файлы приложения. Единственная ответственность модуля — пути.
//!
//! Всё живёт под `%LOCALAPPDATA%\umiray-client` (D-014). Не `app_local_data_dir` из Tauri —
//! тот подставил бы identifier из tauri.conf.json.
//!
//! **У отладочной сборки каталог свой** — `umiray-client-dev` (D-116). Разработка идёт
//! на той же машине, где клиентом пользуются, и `tauri dev` иначе правил бы живые
//! подписки, выбор узла и настройки того, кто просто хотел, чтобы VPN работал.
//! Признак — профиль сборки, а не переменная окружения: забыть её ровно так же легко,
//! как и не заметить, что правишь настоящий конфиг.

use std::io;
use std::path::PathBuf;

const APP_DIR: &str = if cfg!(debug_assertions) {
    "umiray-client-dev"
} else {
    "umiray-client"
};

pub fn root() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(APP_DIR)
}

/// Конфиг до разделения на профиль и оверрайд. Остался только ради разовой миграции.
pub fn legacy_config() -> PathBuf {
    root().join("config.yaml")
}

/// Настройки приложения: поведение, переживающее перезапуск (D-024).
pub fn settings() -> PathBuf {
    root().join("settings.json")
}

/// Каталог источников: список ссылок и метаданные на каждый (D-032).
/// Список ссылок читает само ядро — это `path:` его провайдера.
pub fn sources_dir() -> PathBuf {
    root().join("sources")
}

pub fn source_links(id: &str) -> PathBuf {
    sources_dir().join(format!("{id}.txt"))
}

/// Что прислала панель, до наших правок. Без него кнопка «Сброс» не может ничего вернуть.
pub fn source_raw(id: &str) -> PathBuf {
    sources_dir().join(format!("{id}.raw"))
}

/// Правки пользователя: разница, а не поправленные узлы.
pub fn source_patches(id: &str) -> PathBuf {
    sources_dir().join(format!("{id}.patch.json"))
}

/// Правка **записи** узла — та, что клиент пишет сам (D-119). Тоже разница, а не копия.
pub fn source_entries(id: &str) -> PathBuf {
    sources_dir().join(format!("{id}.entry.yaml"))
}

pub fn source_meta(id: &str) -> PathBuf {
    sources_dir().join(format!("{id}.json"))
}

/// Настройки ядра: конфиг целиком, кроме источников, групп и правил.
/// Правит его и редактор, и переключатель режима — файл один на обоих (D-052).
pub fn advanced() -> PathBuf {
    root().join("advanced.yaml")
}

/// Настройки самого клиента: то, чего нет в конфиге ядра (D-068). Ядру не уходит.
pub fn client() -> PathBuf {
    root().join("client.yaml")
}

/// Тот же файл до переименования. Остался ради разового переезда.
pub fn legacy_override() -> PathBuf {
    root().join("override.yaml")
}

/// Группы выбора пользователя. Пусто — группу собирает клиент (D-044).
pub fn groups() -> PathBuf {
    root().join("groups.yaml")
}

/// Маршрутизация пользователя. Пусто — весь трафик идёт через группу клиента (D-044).
pub fn rules() -> PathBuf {
    root().join("rules.yaml")
}

pub fn ensure_sources_dir() -> io::Result<()> {
    std::fs::create_dir_all(sources_dir())
}

/// Наборы конфигов: пара «группы + маршрутизация» под направление (D-056).
pub fn presets_dir() -> PathBuf {
    root().join("presets")
}

pub fn preset_meta(id: &str) -> PathBuf {
    presets_dir().join(format!("{id}.json"))
}

/// Файл набора. `part` — идентификатор пользовательского файла: `groups` или `rules`.
pub fn preset_part(id: &str, part: &str) -> PathBuf {
    presets_dir().join(format!("{id}.{part}.yaml"))
}

pub fn ensure_presets_dir() -> io::Result<()> {
    std::fs::create_dir_all(presets_dir())
}

/// Коллекции: всё, что клиент поставляет данными, а владеет ими пользователь (D-100).
/// Резолверы, эталонные ресурсы, встроенные наборы правил — читается отсюда, а не из бинаря.
pub fn collections_dir() -> PathBuf {
    root().join("collections")
}

/// Коллекция-документ: один файл с известной схемой.
pub fn collection_file(name: &str) -> PathBuf {
    collections_dir().join(format!("{name}.yaml"))
}

/// Коллекция-папка: много файлов одной схемы, которые перечисляются на лету.
pub fn collection_folder(name: &str) -> PathBuf {
    collections_dir().join(name)
}

/// Где лежали коллекции до D-100. Нужны переезду, и только ему.
pub fn legacy_catalog_dir() -> PathBuf {
    root().join("catalog")
}

pub fn legacy_rulesets_dir() -> PathBuf {
    root().join("rulesets")
}

/// Рабочий каталог ядра: сюда кладётся сгенерированный конфиг, сюда ядро пишет своё.
pub fn run_dir() -> PathBuf {
    root().join("run")
}

/// Конфиг, который реально запускается: пользовательский плюс поля режима.
pub fn effective_config() -> PathBuf {
    run_dir().join("config.yaml")
}

pub fn core() -> PathBuf {
    root().join("mihomo.exe")
}

/// Идентификатор установки для подписок с привязкой по устройству.
/// Страна по адресу узла, кэшем (D-084). Ключ — `host:port`: имя узла панель меняет,
/// адрес нет.
pub fn geo() -> PathBuf {
    root().join("geo.json")
}

pub fn hwid() -> PathBuf {
    root().join("hwid.txt")
}

pub fn ensure_root() -> io::Result<()> {
    std::fs::create_dir_all(root())
}

pub fn ensure_run_dir() -> io::Result<()> {
    std::fs::create_dir_all(run_dir())
}
