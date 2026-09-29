//! Дисковое хранилище и атомарная публикация источников узлов (D-032, D-140).
//!
//! Источник — это откуда узлы взялись: `sources/<id>.json` с метаданными,
//! `sources/<id>.raw` с каноническим ответом и `sources/<id>.txt` с нормализованными
//! записями `proxies:`, которые читает ядро (D-122, D-136).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::nodes::link::LinkParser;
use crate::nodes::source_id::SourceId;
use crate::paths::Paths;

/// Перевод строки отдельной константой: файл правится скриптами, и экранирование
/// внутри строкового литерала переживает это хуже, чем имя.
const NL: &str = "
";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    /// Адрес подписки. Пусто у источника «мои ссылки» — его обновлять неоткуда.
    pub url: Option<String>,
    /// Когда последний раз обновляли, в секундах эпохи. Форматирует интерфейс.
    pub updated: Option<u64>,
    pub nodes: usize,
    /// Источник хранит **записи** `proxies:`, а не ссылки, — то есть узлы в нём написал
    /// клиент, и удалить их можно (D-121). У подписки удаление было бы враньём: узел
    /// вернётся следующим обновлением. На диске не лежит — считается при чтении.
    #[serde(default)]
    pub records: bool,
    /// Ссылки, которые разбор не осилил (D-122). В списке видны помеченными, в конфиг
    /// не идут: узел, собранный наполовину, ядро примет молча и пойдёт не туда.
    #[serde(default)]
    pub skipped: Vec<String>,
}

/// Проставить признак, который не хранится, а выводится из содержимого.
fn stamped(mut source: Source, id: &str) -> Source {
    source.records = source.url.is_none() && is_records(id);
    source
}

/// Источник хранит записи, а не ссылки.
///
/// Спрашиваем **сырьё**, а не собранный файл: с D-122 собранный файл — документ `proxies:`
/// у всякого источника, и по нему уже ничего не различить. Различие осталось в том, что
/// пришло: список ссылок или готовый документ.
///
/// И не имя, и не отсутствие адреса: имя пользователь вправе сменить, а «без адреса»
/// с D-120 значит и «мои ссылки», и «свои узлы». Пустой источник не является ни тем ни
/// другим — иначе первая же ссылка уедет в документ `proxies:` и сломает его (GOTCHAS).
fn is_records(id: &str) -> bool {
    serde_yaml::from_str::<serde_yaml::Value>(&SourceStore::raw(id))
        .ok()
        .and_then(|value| value.get("proxies").cloned())
        .is_some()
}

pub struct SourceStore;

impl SourceStore {
    pub fn list() -> Vec<Source> {
        let Ok(entries) = std::fs::read_dir(Paths::sources_dir()) else {
            return Vec::new();
        };
        let mut sources: Vec<Source> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<Source>(&text).ok())
            .filter(|source| SourceId::parse(&source.id).is_ok())
            .map(|source| {
                let id = source.id.clone();
                stamped(source, &id)
            })
            .collect();
        // Подписки сверху, «мои ссылки» в конце: так список не прыгает при добавлении.
        sources.sort_by(|a, b| {
            a.url
                .is_none()
                .cmp(&b.url.is_none())
                .then(a.name.cmp(&b.name))
        });
        sources
    }

    pub fn get(id: &str) -> Result<Source> {
        let id = SourceId::parse(id)?;
        let id = id.as_str();
        let text = std::fs::read_to_string(Paths::source_meta(id))
            .map_err(|_| AppError::invalid(format!("Источник не найден: {id}")))?;
        serde_json::from_str::<Source>(&text)
            .map(|source| stamped(source, id))
            .map_err(|e| AppError::invalid(format!("Метаданные источника испорчены: {e}")))
    }

    /// Содержимое источника — то, что читает ядро. Отсутствующий файл значит пустой источник.
    pub fn content(id: &str) -> String {
        let Ok(id) = SourceId::parse(id) else {
            return String::new();
        };
        let id = id.as_str();
        std::fs::read_to_string(Paths::source_links(id)).unwrap_or_default()
    }

    /// Сырьё от панели, до чистки имён и правок. Всё, что пересобирает источник, читает **его**:
    /// собранный файл на входе означал бы, что наши же правки вплавляются в исходник.
    pub fn raw(id: &str) -> String {
        let Ok(id) = SourceId::parse(id) else {
            return String::new();
        };
        let id = id.as_str();
        std::fs::read_to_string(Paths::source_raw(id)).unwrap_or_default()
    }

    /// Переписать источник руками (D-065).
    ///
    /// Правится именно сырьё: собранный файл пересобирается из него при каждом обновлении,
    /// и правка в нём жила бы до первого «Обновить» **молча**. Дальше текст проходит тот же
    /// путь, что и ответ панели: чистка имён, разведение дубликатов, наложение правок.
    pub fn write_raw(id: &str, text: &str) -> Result<Source> {
        let mut source = SourceStore::get(id)?;
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        write(&mut source, lines, id, &crate::config::awg::Mask::get())?;
        SourceStore::get(id)
    }

    /// Порядок ключей в записи: сначала то, по чему узел узнают, потом остальное как есть.
    ///
    /// Нужен не для красоты: объект приезжает из окна через JSON, и по дороге порядок ключей
    /// теряется — в файле оказывалось `name, port, server, type`, и читать это глазами больно.
    pub fn ordered(entry: serde_yaml::Mapping) -> serde_yaml::Mapping {
        const FIRST: [&str; 4] = ["name", "type", "server", "port"];
        let mut out = serde_yaml::Mapping::new();
        for key in FIRST {
            if let Some(value) = entry.get(serde_yaml::Value::from(key)) {
                out.insert(serde_yaml::Value::from(key), value.clone());
            }
        }
        for (key, value) in entry {
            if !key.as_str().is_some_and(|key| FIRST.contains(&key)) {
                out.insert(key, value);
            }
        }
        out
    }

    /// Каталог источников. Ядру он нужен как разрешённый путь провайдеров (`SAFE_PATHS`),
    /// а раскладку внутри знает только это хранилище (D-155).
    pub fn dir() -> std::path::PathBuf {
        Paths::sources_dir()
    }

    /// Файл провайдера источника — то, что читает ядро.
    pub fn provider(id: &str) -> std::path::PathBuf {
        Paths::source_links(id)
    }

    /// Стереть все источники: сброс к состоянию «как после установки». Каталог целиком —
    /// выборочная чистка означала бы помнить список файлов источника в двух местах.
    pub fn clear() -> Result<()> {
        if Paths::sources_dir().exists() {
            std::fs::remove_dir_all(Paths::sources_dir())?;
        }
        Paths::ensure_sources_dir()?;
        Ok(())
    }

    pub fn delete(id: &str) -> Result<()> {
        SourceId::parse(id)?;
        let _ = std::fs::remove_file(Paths::source_links(id));
        let _ = std::fs::remove_file(Paths::source_raw(id));
        let _ = std::fs::remove_file(Paths::source_patches(id));
        std::fs::remove_file(Paths::source_meta(id))
            .map_err(|_| AppError::invalid(format!("Источник не найден: {id}")))?;
        Ok(())
    }

    /// Довести прерванную публикацию до состояния, которое описывает raw. Metadata пишется
    /// последней, поэтому источник с ней — существующий; файлы без неё были незавершённым
    /// добавлением и в список никогда не попадали.
    pub fn repair_all() -> Result<()> {
        Paths::ensure_sources_dir()?;
        let mask = crate::config::awg::Mask::get();
        let entries = std::fs::read_dir(Paths::sources_dir())?;
        let mut known = HashSet::new();
        for entry in entries.flatten() {
            if entry.path().extension().is_none_or(|ext| ext != "json")
                || entry
                    .path()
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".patch.json"))
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(mut source) = serde_json::from_str::<Source>(&text) else {
                continue;
            };
            let Ok(id) = SourceId::parse(&source.id) else {
                continue;
            };
            known.insert(id.as_str().to_string());
            rebuild(&mut source, id.as_str(), &mask)?;
        }
        for entry in std::fs::read_dir(Paths::sources_dir())?.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = name.split('.').next() else {
                continue;
            };
            if SourceId::parse(id).is_ok() && !known.contains(id) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }
}

/// Записи, которые уже лежат в источнике.
pub(super) fn own_proxies(id: &str) -> Vec<serde_yaml::Value> {
    serde_yaml::from_str::<serde_yaml::Value>(&SourceStore::raw(id))
        .ok()
        .and_then(|value| {
            value
                .get("proxies")
                .and_then(serde_yaml::Value::as_sequence)
                .cloned()
        })
        .unwrap_or_default()
}

/// Записать источник: сырьё от панели кладём как есть, а то, что читает ядро, генерируем.
///
/// Раздельно — потому что иначе «Сброс» нечем откатывать: наши правки оказались бы вплавлены
/// в единственную копию, и исходного значения не осталось бы нигде.
pub(super) fn write(
    source: &mut Source,
    lines: Vec<String>,
    id: &str,
    mask: &crate::config::awg::Mask,
) -> Result<()> {
    SourceId::parse(id)?;
    Paths::ensure_sources_dir()?;
    publish(source, id, &lines.join("\n"), mask)
}

/// Собрать то, что читает ядро: сырьё + чистка имён + правки пользователя.
///
/// Пересобирается каждый раз целиком, а не правится на месте: так результат зависит только
/// от сырья и правок, и любая из них снимается без следа.
pub(super) fn rebuild(
    source: &mut Source,
    id: &str,
    mask: &crate::config::awg::Mask,
) -> Result<()> {
    SourceId::parse(id)?;
    publish(source, id, &SourceStore::raw(id), mask)
}

/// Подготовить все три представления до первой замены и опубликовать metadata последней.
/// Ошибка обычной записи возвращает весь набор к прежним байтам; падение процесса между
/// заменами чинится следующей пересборкой из raw.
fn publish(
    source: &mut Source,
    id: &str,
    raw: &str,
    mask: &crate::config::awg::Mask,
) -> Result<()> {
    let lines: Vec<String> = raw.lines().map(str::to_string).collect();

    let text = if lines.iter().any(|line| line.contains("://")) {
        let (document, skipped) = crate::nodes::source_build::converted(
            id,
            LinkParser::clean(&lines, &mut taken_by_others(id)),
            mask,
        )?;
        source.skipped = skipped;
        document
    } else {
        // Источник записей: сырьё и есть документ. Разницы от присланного у него нет —
        // присылать было некому (D-120), и правка ложится прямо в него.
        source.skipped = Vec::new();
        lines.join(NL)
    };

    source.nodes = count(&text);
    let meta = serde_json::to_string_pretty(source)
        .map_err(|e| AppError::io(format!("Не удалось записать источник: {e}")))?;
    let files = [
        (Paths::source_raw(id), raw.as_bytes()),
        (Paths::source_links(id), text.as_bytes()),
        (Paths::source_meta(id), meta.as_bytes()),
    ];
    let before: Vec<Option<Vec<u8>>> = files
        .iter()
        .map(|(path, _)| std::fs::read(path).ok())
        .collect();
    for (at, (path, contents)) in files.iter().enumerate() {
        if let Err(why) = crate::atomic::AtomicFile::write(path, contents) {
            for ((path, _), old) in files.iter().zip(&before).take(at + 1) {
                match old {
                    Some(contents) => {
                        let _ = crate::atomic::AtomicFile::write(path, contents);
                    }
                    None => {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            return Err(why.into());
        }
    }
    Ok(())
}

/// Записи из **собранного** файла — того, что читает ядро.
pub(super) fn built_proxies(id: &str) -> Vec<serde_yaml::Mapping> {
    serde_yaml::from_str::<serde_yaml::Value>(&SourceStore::content(id))
        .ok()
        .and_then(|value| value.get("proxies")?.as_sequence().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|proxy| proxy.as_mapping().cloned())
        .collect()
}

/// Сколько узлов в содержимом. Ссылки считаются построчно, а YAML — по записям `proxies`:
/// в таком виде источник появляется только после переезда со старой раскладки.
fn count(text: &str) -> usize {
    serde_yaml::from_str::<serde_yaml::Value>(text)
        .ok()
        .and_then(|value| value.get("proxies")?.as_sequence().map(Vec::len))
        .unwrap_or(0)
}

/// Имена, занятые другими источниками: ядро складывает все узлы в одну группу выбора,
/// а одинаковые имена делают выбор неоднозначным (S-012).
pub(super) fn taken_by_others(id: &str) -> HashSet<String> {
    SourceStore::list()
        .iter()
        .filter(|source| source.id != id)
        .flat_map(|source| {
            SourceStore::content(&source.id)
                .lines()
                .filter_map(LinkParser::name_of)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// `LOCALAPPDATA` один на процесс, а тестов с диском стало два: без замка они
    /// подменяют каталог друг у друга на ходу.
    static DISK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    use super::*;
    use crate::nodes::entries::EntryPatch;
    use crate::nodes::source_catalog::SourceCatalog;
    use crate::nodes::source_editor::SourceEditor;
    use crate::nodes::source_import::{host_of, SourceImporter};

    #[test]
    fn startup_repairs_a_source_from_its_canonical_raw_file() {
        let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("umiray-source-repair-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);
        Paths::ensure_sources_dir().unwrap();

        let id = "00000000000000dd";
        let source = Source {
            id: id.into(),
            name: "repair".into(),
            url: None,
            updated: None,
            nodes: 99,
            records: false,
            skipped: Vec::new(),
        };
        crate::atomic::AtomicFile::write(
            Paths::source_raw(id),
            "vless://11111111-1111-1111-1111-111111111111@example.com:443#Fresh",
        )
        .unwrap();
        crate::atomic::AtomicFile::write(Paths::source_links(id), "stale").unwrap();
        crate::atomic::AtomicFile::write(
            Paths::source_meta(id),
            serde_json::to_string(&source).unwrap(),
        )
        .unwrap();

        SourceStore::repair_all().unwrap();
        assert!(!SourceStore::content(id).contains("stale"));
        assert!(SourceStore::content(id).contains("name: Fresh"));
        assert_eq!(SourceStore::get(id).unwrap().nodes, 1);
    }

    /// Живая проверка всего пути: скачать настоящую подписку, вычистить имена, записать.
    ///
    /// В обычный прогон не входит — ходит в сеть и занимает слот устройства у провайдера.
    /// Запуск: `cargo test live_subscription -- --ignored --nocapture --test-threads=1`,
    /// адрес подписки в переменной `UMIRAY_SUB`.
    #[tokio::test]
    #[ignore]
    async fn live_subscription_becomes_clean_named_nodes() {
        // Пропускаем, а не падаем. Проверка стоит слота устройства у провайдера, поэтому
        // адрес даётся осознанно — и без него `cargo test live` краснел **всегда**,
        // а набор, который всегда красный, перестают читать целиком.
        let Ok(url) = std::env::var("UMIRAY_SUB") else {
            println!("пропущено: нет UMIRAY_SUB — проверка стоит слота устройства у провайдера");
            return;
        };
        let dir = std::env::temp_dir().join("umiray-live-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);

        let (source, notices) = SourceImporter::add_subscription(&url)
            .await
            .expect("подписка не открылась");
        println!("источник: {} | узлов: {}", source.name, source.nodes);
        for notice in &notices {
            println!("служебная запись: {notice}");
        }
        for line in SourceStore::content(&source.id).lines() {
            println!(
                "  {:10} {:28} {}",
                line.split("://").next().unwrap_or("?"),
                LinkParser::endpoint_of(line).unwrap_or_default(),
                LinkParser::name_of(line).unwrap_or_default()
            );
        }

        assert!(source.nodes > 0, "узлов не пришло");
        let names: Vec<String> = SourceStore::content(&source.id)
            .lines()
            .filter_map(LinkParser::name_of)
            .collect();
        assert!(
            names.iter().all(|name| name
                .chars()
                .all(|c| { c.is_alphanumeric() || " -_.()[]:+/,|@#".contains(c) })),
            "в именах остался мусор: {names:?}"
        );
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "имена не уникальны: {names:?}");
    }

    /// Файл конфига становится узлом, который ядро примет, и лежит он записью, а не
    /// ссылкой (D-120). Диск трогаем: это и есть проверка — где узел оказался.
    #[test]
    fn a_wireguard_file_becomes_a_node_of_its_own_source() {
        let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("umiray-import-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);

        let file = dir.join("Дом.conf");
        std::fs::write(
            &file,
            "[Interface]
PrivateKey = cHJpdmF0ZQ==
Address = 10.0.0.2/32
Jc = 4

             [Peer]
PublicKey = cHVibGlj
Endpoint = a.example:51820
",
        )
        .unwrap();

        let source = SourceImporter::import_file(&file).unwrap();
        assert_eq!(source.nodes, 1, "узел приехал");
        assert!(source.url.is_none(), "это не подписка");
        assert!(
            !SourceStore::content(&source.id).contains("://"),
            "узел лежит записью, а не ссылкой: {}",
            SourceStore::content(&source.id)
        );

        let node = SourceCatalog::nodes()
            .into_iter()
            .find(|node| node.name == "Дом")
            .expect("узел в списке");
        assert!(node.supported, "ядро такой узел поднимет");

        let code = SourceEditor::node_code(&source.id, "Дом").unwrap();
        assert!(code.editable, "свой узел правится целиком");
        assert!(code.text.contains("type: wireguard"), "{}", code.text);
        assert!(
            code.text.contains("jc: 4"),
            "маска из файла доехала: {}",
            code.text
        );

        // Запись едет и объектом — из неё окно заполняет форму (D-121).
        let object = code.entry.expect("запись объектом");
        assert_eq!(object["type"], "wireguard");
        assert_eq!(object["amnezia-wg-option"]["jc"], 4);
        assert!(code.why.is_none(), "форма есть — объяснять нечего");
        assert!(
            SourceStore::get(&source.id).unwrap().records,
            "источник хранит записи"
        );

        // Второй такой же файл не затирает первый и не даёт ядру двух одинаковых имён.
        SourceImporter::import_file(&file).unwrap();
        let names: Vec<String> = SourceCatalog::nodes()
            .into_iter()
            .map(|node| node.name)
            .collect();
        assert_eq!(
            names,
            vec!["Дом".to_string(), "Дом 2".to_string()],
            "{names:?}"
        );

        // Форма пишет узел объектом — и тем же путём, что и код (D-121).
        let mut changed = object.clone();
        changed["mtu"] = serde_json::json!(1300);
        changed["name"] = serde_json::json!("Дом");
        SourceEditor::set_node_entry(&source.id, "Дом", changed).unwrap();
        assert!(
            SourceEditor::node_code(&source.id, "Дом")
                .unwrap()
                .text
                .contains("mtu: 1300"),
            "правка формой доехала"
        );

        // Свой узел убирается целиком, и соседа это не трогает (D-121).
        SourceEditor::delete_node(&source.id, "Дом").unwrap();
        let left: Vec<String> = SourceCatalog::nodes()
            .into_iter()
            .map(|node| node.name)
            .collect();
        assert_eq!(left, vec!["Дом 2".to_string()], "{left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Правка кода узла (D-119): разница доезжает до записи, свежий ключ с сервера
    /// её переживает, «Откатить» снимает обе правки разом. Тоже с диском — правки
    /// и есть файлы.
    #[test]
    fn an_edited_entry_is_kept_as_a_difference_and_rolled_back_whole() {
        let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("umiray-entry-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);

        let wg = "wireguard://c3RhcnlqLWtleQ==@a.example:51820?address=10.0.0.2/32&publickey=cHVi&mtu=1420#Home";
        let mut source = Source {
            id: "00000000000000aa".into(),
            name: "Свои".into(),
            url: None,
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
        };
        write(
            &mut source,
            vec![wg.to_string()],
            "00000000000000aa",
            &crate::config::awg::Mask::default(),
        )
        .unwrap();

        let code = SourceEditor::node_code("00000000000000aa", "Home").unwrap();
        assert!(
            code.editable,
            "запись wireguard собирает клиент — она правится"
        );
        assert!(code.text.contains("mtu: 1420"), "{}", code.text);

        // Правим код: меняем MTU и дописываем поле, которого в ссылке нет вовсе.
        let edited = code.text.replace("mtu: 1420", "mtu: 1280").replace(
            "udp: true",
            "udp: true
remote-dns-resolve: true",
        );
        SourceEditor::edit_node_code("00000000000000aa", "Home", &edited).unwrap();

        let stored = EntryPatch::load("00000000000000aa");
        let patch = stored.values().next().expect("правка записи сохранена");
        assert_eq!(patch.len(), 2, "хранится только разница: {patch:?}");

        let again = SourceEditor::node_code("00000000000000aa", "Home").unwrap();
        assert!(again.text.contains("mtu: 1280"), "{}", again.text);
        assert!(again.text.contains("remote-dns-resolve: true"));
        assert!(
            SourceCatalog::nodes()
                .iter()
                .any(|node| node.name == "Home" && node.edited),
            "узел помечен правленым"
        );

        // Подписка сменила ключ: он обязан доехать поверх нашей правки.
        let rotated = wg.replace("c3RhcnlqLWtleQ==", "bm92eWotа2V5");
        write(
            &mut source,
            vec![rotated],
            "00000000000000aa",
            &crate::config::awg::Mask::default(),
        )
        .unwrap();
        let fresh = SourceEditor::node_code("00000000000000aa", "Home").unwrap();
        assert!(fresh.text.contains("bm92eWotа2V5"), "{}", fresh.text);
        assert!(fresh.text.contains("mtu: 1280"), "правка уцелела");

        SourceEditor::reset_node("00000000000000aa", "Home").unwrap();
        assert!(
            EntryPatch::load("00000000000000aa").is_empty(),
            "откат снял правку записи"
        );
        let back = SourceEditor::node_code("00000000000000aa", "Home").unwrap();
        assert!(back.text.contains("mtu: 1420"), "{}", back.text);
        assert!(!back.text.contains("remote-dns-resolve"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Маскировка рукопожатия — настройка клиента, а лежит она полем узла (D-118).
    /// С D-122 кладётся там, где узел рождается, — при разборе ссылки.
    #[test]
    fn the_handshake_mask_lands_in_the_wireguard_entry() {
        let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("umiray-mask-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);

        let mut source = Source {
            id: "00000000000000bb".into(),
            name: "Панель".into(),
            url: Some("https://example.org/sub".into()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
        };
        write(
            &mut source,
            vec![
                "wireguard://a2V5@a.example:51820?address=10.0.0.2/32&publickey=cHVi#W".into(),
                "vless://u@b.example:443?encryption=none#V".into(),
            ],
            "00000000000000bb",
            &crate::config::awg::Mask::default(),
        )
        .unwrap();

        let built = SourceStore::content("00000000000000bb");
        assert!(built.contains("amnezia-wg-option"), "{built}");
        assert_eq!(
            built.matches("amnezia-wg-option").count(),
            1,
            "только узлу WireGuard: у vless такого поля нет"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Единственный тест здесь, который трогает диск: он про то, как файлы переживают
    /// обновление, а этого на чистых функциях не покажешь. `LOCALAPPDATA` подменён,
    /// настоящий каталог пользователя не задет.
    ///
    /// С D-122 и правка, и хранение — записи: правка ложится разницей, свежее с сервера
    /// доезжает поверх неё, а «Откатить» снимает разницу целиком.
    #[test]
    fn an_edit_survives_a_refresh_while_the_server_side_change_arrives() {
        let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("umiray-patch-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCALAPPDATA", &dir);

        // Панель прислала узел.
        let first = "vless://старый@a.example:443?encryption=none&fp=chrome&security=tls&sni=a.example#Sweden 0";
        let mut source = Source {
            id: "00000000000000cc".into(),
            name: "Панель".into(),
            url: Some("https://example.org/sub".into()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
        };
        write(
            &mut source,
            vec![first.to_string()],
            "00000000000000cc",
            &crate::config::awg::Mask::default(),
        )
        .unwrap();
        assert_eq!(source.nodes, 1);
        assert!(
            SourceStore::content("00000000000000cc").contains("type: vless"),
            "источник хранит запись, а не ссылку: {}",
            SourceStore::content("00000000000000cc")
        );

        // Пользователь поправил отпечаток — формой или кодом, дорога одна.
        let mut edited = built_proxies("00000000000000cc").remove(0);
        crate::yaml::Yaml::set(
            &mut edited,
            "client-fingerprint",
            serde_yaml::Value::from("safari"),
        );
        SourceEditor::edit_node_code(
            "00000000000000cc",
            "Sweden 0",
            &serde_yaml::to_string(&serde_yaml::Value::Mapping(edited)).unwrap(),
        )
        .unwrap();
        assert!(SourceStore::content("00000000000000cc").contains("client-fingerprint: safari"));

        // Обновление: панель сменила секрет и переименовала узел.
        let second = "vless://новый@a.example:443?encryption=none&fp=chrome&security=tls&sni=a.example#%F0%9F%87%B8%F0%9F%87%AA Sweden 0";
        write(
            &mut source,
            vec![second.to_string()],
            "00000000000000cc",
            &crate::config::awg::Mask::default(),
        )
        .unwrap();

        let built = SourceStore::content("00000000000000cc");
        assert!(
            built.contains("uuid: новый"),
            "свежий секрет доехал: {built}"
        );
        assert!(
            built.contains("client-fingerprint: safari"),
            "правка пережила обновление: {built}"
        );
        assert!(
            !built.contains("старый"),
            "перевыпущенный секрет вытеснил прежний: {built}"
        );
        assert_eq!(
            built_proxies("00000000000000cc").len(),
            1,
            "узел один, а не два"
        );

        // Сброс возвращает то, что прислала панель.
        SourceEditor::reset_node("00000000000000cc", "Sweden 0").unwrap();
        let built = SourceStore::content("00000000000000cc");
        assert!(
            built.contains("client-fingerprint: chrome"),
            "откат вернул присланное: {built}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_source_is_named_after_the_panel_host() {
        assert_eq!(host_of("https://sub.example.org/TOKEN"), "sub.example.org");
        assert_eq!(host_of("http://127.0.0.1:8080/x"), "127.0.0.1:8080");
        assert_eq!(host_of("не ссылка"), "Подписка");
    }
}
