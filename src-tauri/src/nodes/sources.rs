//! Хранилище и атомарная публикация источников узлов (D-032, D-140, D-170).
//!
//! Источник — это откуда узлы взялись. В таблице `sources` базы у него части: `meta`
//! с метаданными, `raw` с каноническим ответом и `provider` с нормализованными записями
//! `proxies:` (D-122, D-136). Ядро читает файл, а не базу: `provider` выкладывается
//! в `run/sources/<id>.yaml` при каждой публикации и пересобирается на запуске.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::db::{Db, Table};
use crate::error::{AppError, Result};
use crate::nodes::link::LinkParser;
use crate::nodes::source_id::SourceId;
use crate::paths::Paths;

/// Части источника в базе. Перечисляются источники по `meta`: она пишется вместе
/// с остальными одной транзакцией, и источник без неё не существует.
const META: &str = "meta";
const RAW: &str = "raw";
const PROVIDER: &str = "provider";

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
    /// вернётся следующим обновлением. Не хранится — считается при чтении.
    #[serde(default)]
    pub records: bool,
    /// Ссылки, которые разбор не осилил (D-122). В списке видны помеченными, в конфиг
    /// не идут: узел, собранный наполовину, ядро примет молча и пойдёт не туда.
    #[serde(default)]
    pub skipped: Vec<String>,
    /// Почему последнее обновление подписки не удалось. Пусто — удалось или не обновляли.
    /// Помнит источник, а не память клиента: жалоба переживает перезапуск (D-038).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// Имя дал человек (D-172): название из ответа панели его больше не перетирает.
    #[serde(default)]
    pub renamed: bool,
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
        let mut sources: Vec<Source> = Db::with_part(Table::Sources, META)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, text)| serde_json::from_str::<Source>(&text).ok())
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
        let text = Db::get(Table::Sources, id, META)?
            .ok_or_else(|| AppError::invalid(format!("Источник не найден: {id}")))?;
        serde_json::from_str::<Source>(&text)
            .map(|source| stamped(source, id))
            .map_err(|e| AppError::invalid(format!("Метаданные источника испорчены: {e}")))
    }

    /// Содержимое источника — то, что читает ядро. Отсутствие значит пустой источник.
    pub fn content(id: &str) -> String {
        let Ok(id) = SourceId::parse(id) else {
            return String::new();
        };
        let id = id.as_str();
        Db::get(Table::Sources, id, PROVIDER)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Сырьё от панели, до чистки имён и правок. Всё, что пересобирает источник, читает **его**:
    /// собранный файл на входе означал бы, что наши же правки вплавляются в исходник.
    pub fn raw(id: &str) -> String {
        let Ok(id) = SourceId::parse(id) else {
            return String::new();
        };
        let id = id.as_str();
        Db::get(Table::Sources, id, RAW)
            .ok()
            .flatten()
            .unwrap_or_default()
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

    /// Файл провайдера источника — то, что читает ядро.
    pub fn provider(id: &str) -> std::path::PathBuf {
        Paths::source_provider(id)
    }

    /// Стереть все источники: сброс к состоянию «как после установки».
    pub fn clear() -> Result<()> {
        Db::clear(Table::Sources)?;
        if Paths::sources_dir().exists() {
            std::fs::remove_dir_all(Paths::sources_dir())?;
        }
        Ok(())
    }

    pub fn delete(id: &str) -> Result<()> {
        SourceStore::get(id)?;
        Db::remove_all(Table::Sources, id)?;
        let _ = std::fs::remove_file(Paths::source_provider(id));
        Ok(())
    }

    /// Запомнить, почему обновление не удалось, — только в описании: узлы остаются как были.
    pub fn mark_failed(id: &str, why: &str) -> Result<()> {
        let mut source = SourceStore::get(id)?;
        source.failed = Some(why.to_string());
        put_meta(&source)
    }

    /// Переименовать источник (D-172). Только название: узлы, адрес и id не трогаются, и имя
    /// переживает обновление — панель его больше не перетирает.
    pub fn rename(id: &str, name: &str) -> Result<Source> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::invalid("У источника должно быть название"));
        }
        let mut source = SourceStore::get(id)?;
        source.name = name.to_string();
        source.renamed = true;
        put_meta(&source)?;
        Ok(source)
    }

    /// Пересобрать все источники из сырья и выложить ядру заново: разбор мог научиться
    /// новому, а `run/` — пропасть. Части без `meta` и файлы без источника убираются.
    pub fn repair_all() -> Result<()> {
        let mask = crate::config::awg::Mask::get();
        let mut known = HashSet::new();
        for (id, text) in Db::with_part(Table::Sources, META)? {
            let Ok(mut source) = serde_json::from_str::<Source>(&text) else {
                continue;
            };
            if SourceId::parse(&id).is_err() || source.id != id {
                continue;
            }
            known.insert(id.clone());
            rebuild(&mut source, &id, &mask)?;
        }
        for part in [RAW, PROVIDER] {
            for (id, _) in Db::with_part(Table::Sources, part)? {
                if !known.contains(&id) {
                    Db::remove_all(Table::Sources, &id)?;
                }
            }
        }
        if let Ok(files) = std::fs::read_dir(Paths::sources_dir()) {
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().to_string();
                let id = name.split('.').next().unwrap_or_default();
                if !known.contains(id) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
        Ok(())
    }
}

/// Записать одно описание источника — без узлов и сырья.
fn put_meta(source: &Source) -> Result<()> {
    let meta = serde_json::to_string_pretty(source)
        .map_err(|e| AppError::io(format!("Не удалось записать источник: {e}")))?;
    Db::put(Table::Sources, &source.id, META, &meta)
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

fn publish(
    source: &mut Source,
    id: &str,
    raw: &str,
    mask: &crate::config::awg::Mask,
) -> Result<()> {
    let text = assemble(source, id, raw, mask)?;
    commit(source, id, raw, &text)
}

/// Ссылки сырья такими, какими их разбирает сборка: `mierus://` развёрнута, имена
/// вычищены и разведены с другими источниками. Редактор считает по тем же строкам —
/// иначе позиции узлов разошлись бы со сборкой.
pub(super) fn links_of(id: &str, raw: &str) -> Vec<String> {
    let lines: Vec<String> = raw.lines().map(str::to_string).collect();
    LinkParser::clean(&LinkParser::split(&lines), &mut taken_by_others(id))
}

/// Собрать то, что читает ядро, ничего не записывая: сырьё + чистка имён + правки
/// пользователя. Источнику ставит `skipped` и `nodes` — по ним вызывающий решает, писать ли.
pub(super) fn assemble(
    source: &mut Source,
    id: &str,
    raw: &str,
    mask: &crate::config::awg::Mask,
) -> Result<String> {
    let text = if raw.lines().any(|line| line.contains("://")) {
        let (document, skipped) = crate::nodes::source_build::converted(
            links_of(id, raw),
            mask,
            &crate::nodes::entries::EntryPatch::load(id),
        )?;
        source.skipped = skipped;
        document
    } else {
        // Источник записей: сырьё и есть документ. Разницы от присланного у него нет —
        // присылать было некому (D-120), и правка ложится прямо в него.
        source.skipped = Vec::new();
        raw.lines().collect::<Vec<_>>().join(NL)
    };
    source.nodes = count(&text);
    Ok(text)
}

/// Записать три части одной транзакцией и выложить собранное ядру. Упасть между базой
/// и файлом не страшно: файл пересобирается из базы на запуске.
pub(super) fn commit(source: &Source, id: &str, raw: &str, text: &str) -> Result<()> {
    let meta = serde_json::to_string_pretty(source)
        .map_err(|e| AppError::io(format!("Не удалось записать источник: {e}")))?;
    Db::batch(|batch| {
        batch.put(Table::Sources, id, RAW, raw)?;
        batch.put(Table::Sources, id, PROVIDER, text)?;
        batch.put(Table::Sources, id, META, &meta)
    })?;
    std::fs::create_dir_all(Paths::sources_dir())?;
    Ok(crate::atomic::AtomicFile::write(
        Paths::source_provider(id),
        text,
    )?)
}

/// Записи из **собранного** документа — того, что читает ядро.
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

/// Имена узлов документа `proxies:` — того, что читает ядро (D-122). Порядок файла.
pub(super) fn names(text: &str) -> Vec<String> {
    serde_yaml::from_str::<serde_yaml::Value>(text)
        .ok()
        .and_then(|value| value.get("proxies")?.as_sequence().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|proxy| proxy.get("name")?.as_str().map(str::to_string))
        .collect()
}

/// Имена, занятые другими источниками: ядро складывает все узлы в одну группу выбора,
/// а одинаковые имена делают выбор неоднозначным (S-012). Читаем собранный документ,
/// а не строки ссылок: с D-122 у всякого источника он `proxies:`, и фрагменты `#имя`
/// в нём не встречаются — по ним множество выходило пустым всегда.
pub(super) fn taken_by_others(id: &str) -> HashSet<String> {
    SourceStore::list()
        .iter()
        .filter(|source| source.id != id)
        .flat_map(|source| names(&SourceStore::content(&source.id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::entries::EntryPatch;
    use crate::nodes::source_catalog::SourceCatalog;
    use crate::nodes::source_editor::SourceEditor;
    use crate::nodes::source_import::{host_of, SourceImporter};
    use crate::paths::Sandbox;

    #[test]
    fn startup_repairs_a_source_from_its_canonical_raw_file() {
        let _sandbox = Sandbox::new("source-repair");

        let id = "00000000000000dd";
        let source = Source {
            id: id.into(),
            name: "repair".into(),
            url: None,
            updated: None,
            nodes: 99,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
        };
        let meta = serde_json::to_string(&source).unwrap();
        Db::batch(|batch| {
            batch.put(
                Table::Sources,
                id,
                RAW,
                "vless://11111111-1111-1111-1111-111111111111@example.com:443#Fresh",
            )?;
            batch.put(Table::Sources, id, PROVIDER, "stale")?;
            batch.put(Table::Sources, id, META, &meta)?;
            // Части без описания — недописанное добавление: уходят.
            batch.put(Table::Sources, "00000000000000ee", RAW, "orphan")
        })
        .unwrap();
        std::fs::create_dir_all(Paths::sources_dir()).unwrap();
        std::fs::write(Paths::source_provider("00000000000000ee"), "orphan").unwrap();

        SourceStore::repair_all().unwrap();
        assert!(!SourceStore::content(id).contains("stale"));
        assert!(SourceStore::content(id).contains("name: Fresh"));
        assert_eq!(SourceStore::get(id).unwrap().nodes, 1);
        assert_eq!(
            std::fs::read_to_string(SourceStore::provider(id)).unwrap(),
            SourceStore::content(id),
            "ядро читает то же, что лежит в базе"
        );
        assert!(SourceStore::raw("00000000000000ee").is_empty());
        assert!(!Paths::source_provider("00000000000000ee").exists());
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
        let _sandbox = Sandbox::new("live");

        let (source, notices) = SourceImporter::add_subscription(&url)
            .await
            .expect("подписка не открылась");
        println!("источник: {} | узлов: {}", source.name, source.nodes);
        for notice in &notices {
            println!("служебная запись: {notice}");
        }
        // Собранный файл — документ `proxies:` (D-122): имена из него, а не из ссылок,
        // иначе проверка ниже проходила бы на пустом списке.
        let names = names(&SourceStore::content(&source.id));
        for name in &names {
            println!("  {name}");
        }

        assert!(source.nodes > 0, "узлов не пришло");
        assert_eq!(names.len(), source.nodes, "имена прочитаны все");
        assert!(
            names.iter().all(|name| name
                .chars()
                .all(|c| { c.is_alphanumeric() || " -_.()[]:+/|@#".contains(c) })),
            "в именах остался мусор: {names:?}"
        );
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "имена не уникальны: {names:?}");
    }

    /// Файл конфига становится узлом, который ядро примет, и лежит он записью, а не
    /// ссылкой (D-120). Диск трогаем: это и есть проверка — где узел оказался.
    #[test]
    fn a_wireguard_file_becomes_a_node_of_its_own_source() {
        let sandbox = Sandbox::new("wg-file");
        let dir = sandbox.dir.clone();

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

        let source = SourceImporter::add_proxy(SourceImporter::file_entry(&file).unwrap()).unwrap();
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
        SourceImporter::add_proxy(SourceImporter::file_entry(&file).unwrap()).unwrap();
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
    }

    /// Правка кода узла (D-119): разница доезжает до записи, свежий ключ с сервера
    /// её переживает, «Откатить» снимает обе правки разом. Тоже с диском — правки
    /// и есть файлы.
    #[test]
    fn an_edited_entry_is_kept_as_a_difference_and_rolled_back_whole() {
        let _sandbox = Sandbox::new("entry");

        let wg = "wireguard://c3RhcnlqLWtleQ==@a.example:51820?address=10.0.0.2/32&publickey=cHVi&mtu=1420#Home";
        let mut source = Source {
            id: "00000000000000aa".into(),
            name: "Свои".into(),
            url: None,
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
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
    }

    /// Маскировка рукопожатия — настройка клиента, а лежит она полем узла (D-118).
    /// С D-122 кладётся там, где узел рождается, — при разборе ссылки.
    #[test]
    fn the_handshake_mask_lands_in_the_wireguard_entry() {
        let _sandbox = Sandbox::new("mask");

        let mut source = Source {
            id: "00000000000000bb".into(),
            name: "Панель".into(),
            url: Some("https://example.org/sub".into()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
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
    }

    /// Единственный тест здесь, который трогает диск: он про то, как файлы переживают
    /// обновление, а этого на чистых функциях не покажешь. `LOCALAPPDATA` подменён,
    /// настоящий каталог пользователя не задет.
    ///
    /// С D-122 и правка, и хранение — записи: правка ложится разницей, свежее с сервера
    /// доезжает поверх неё, а «Откатить» снимает разницу целиком.
    #[test]
    fn an_edit_survives_a_refresh_while_the_server_side_change_arrives() {
        let _sandbox = Sandbox::new("patch");

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
            failed: None,
            renamed: false,
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
    }

    /// Подписка из двух узлов — для проверок редактора ниже.
    fn subscription(id: &str, lines: &[&str]) {
        let mut source = Source {
            id: id.into(),
            name: "Панель".into(),
            url: Some("https://example.org/sub".into()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
        };
        let lines = lines.iter().map(|line| line.to_string()).collect();
        write(&mut source, lines, id, &crate::config::awg::Mask::default()).unwrap();
    }

    fn entry_of(id: &str, name: &str) -> serde_yaml::Mapping {
        built_proxies(id)
            .into_iter()
            .find(|entry| entry.get("name").and_then(|value| value.as_str()) == Some(name))
            .unwrap_or_else(|| panic!("нет узла {name}: {}", SourceStore::content(id)))
    }

    fn edit(id: &str, name: &str, change: impl FnOnce(&mut serde_yaml::Mapping)) -> Result<()> {
        let mut entry = entry_of(id, name);
        change(&mut entry);
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(entry)).unwrap();
        SourceEditor::edit_node_code(id, name, &text)
    }

    /// B-036: другой адрес — это другой узел (D-036). Правка искала основу по адресу
    /// **правки** и ложилась на соседа с этим адресом: два «Alpha», Beta пропадал.
    #[test]
    fn a_new_address_is_refused_instead_of_landing_on_another_node() {
        let _sandbox = Sandbox::new("edit-address");
        let id = "00000000000000a1";
        subscription(id, &[
            "vless://11111111-1111-1111-1111-111111111111@a.example:443?encryption=none&security=tls&sni=a.example#Alpha",
            "vless://22222222-2222-2222-2222-222222222222@b.example:443?encryption=none&security=tls&sni=b.example#Beta",
        ]);

        let refused = edit(id, "Alpha", |entry| {
            crate::yaml::Yaml::set(entry, "server", serde_yaml::Value::from("b.example"));
        });
        assert!(refused.is_err(), "{}", SourceStore::content(id));
        assert_eq!(names(&SourceStore::content(id)), ["Alpha", "Beta"]);
        assert_eq!(
            entry_of(id, "Beta")["uuid"],
            serde_yaml::Value::from("22222222-2222-2222-2222-222222222222"),
            "сосед не тронут"
        );
        assert!(EntryPatch::load(id).is_empty(), "отказ ничего не записал");
    }

    /// B-036: два узла за одним адресом (разные uuid и sni) — одно тождество D-036.
    /// Правка второго считалась от первого и ложилась на оба: первый становился копией.
    #[test]
    fn twins_behind_one_address_are_edited_apart() {
        let _sandbox = Sandbox::new("edit-twins");
        let id = "00000000000000a2";
        subscription(id, &[
            "vless://11111111-1111-1111-1111-111111111111@a.example:443?encryption=none&security=tls&sni=one.example#One",
            "vless://22222222-2222-2222-2222-222222222222@a.example:443?encryption=none&security=tls&sni=two.example#Two",
        ]);

        edit(id, "Two", |entry| {
            crate::yaml::Yaml::set(entry, "udp", serde_yaml::Value::from(false));
        })
        .unwrap();
        let one = entry_of(id, "One");
        assert_eq!(
            one["udp"],
            serde_yaml::Value::from(true),
            "первый не тронут"
        );
        assert_eq!(
            one["uuid"],
            serde_yaml::Value::from("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(entry_of(id, "Two")["udp"], serde_yaml::Value::from(false));

        SourceEditor::reset_node(id, "Two").unwrap();
        assert_eq!(entry_of(id, "Two")["udp"], serde_yaml::Value::from(true));
    }

    /// B-037: основа разницы бралась **с** наложенной правкой, а новая разница заменяла
    /// старую целиком — вторая правка стирала первую.
    #[test]
    fn a_second_edit_keeps_the_first() {
        let _sandbox = Sandbox::new("edit-twice");
        let id = "00000000000000a3";
        subscription(id, &[
            "vless://11111111-1111-1111-1111-111111111111@a.example:443?encryption=none&security=tls&sni=a.example#Alpha",
        ]);

        edit(id, "Alpha", |entry| {
            crate::yaml::Yaml::set(
                entry,
                "client-fingerprint",
                serde_yaml::Value::from("safari"),
            );
        })
        .unwrap();
        edit(id, "Alpha", |entry| {
            crate::yaml::Yaml::set(entry, "udp", serde_yaml::Value::from(false));
        })
        .unwrap();
        let alpha = entry_of(id, "Alpha");
        assert_eq!(
            alpha["client-fingerprint"],
            serde_yaml::Value::from("safari")
        );
        assert_eq!(alpha["udp"], serde_yaml::Value::from(false));
    }

    /// Ядро складывает узлы всех источников в одну группу, и одноимённый второй узел там
    /// не выбрать: `PUT /proxies/umiray {"name": …}` всегда берёт первый (живьём, 01.10.2026).
    /// Имена разводятся между источниками любого вида — и ссылками, и записями.
    #[test]
    fn a_name_taken_by_another_source_is_not_reused() {
        let _sandbox = Sandbox::new("names");

        let links = SourceImporter::add_link(
            "vless://11111111-1111-1111-1111-111111111111@a.example.com:443?security=tls#Same",
        )
        .unwrap();
        let mut entry = serde_yaml::Mapping::new();
        for (key, value) in [
            ("name", "Same"),
            ("type", "socks5"),
            ("server", "b.example.com"),
        ] {
            entry.insert(key.into(), value.into());
        }
        entry.insert("port".into(), 1080.into());
        let own = SourceImporter::add_proxy(entry).unwrap();
        assert!(
            SourceStore::content(&own.id).contains("name: Same 2"),
            "узел записью не повторяет имя из ссылок: {}",
            SourceStore::content(&own.id)
        );

        // И в обратную сторону: ссылка не берёт имя, занятое записью.
        SourceImporter::add_link(
            "vless://22222222-2222-2222-2222-222222222222@c.example.com:443?security=tls#Same 2",
        )
        .unwrap();
        let names = names(&SourceStore::content(&links.id));
        assert_eq!(names, ["Same", "Same 2 2"], "имена ссылок: {names:?}");
    }

    /// Список ссылок, вставленный в однострочное поле, приходит одной строкой через пробел.
    #[test]
    fn links_pasted_as_one_line_become_separate_nodes() {
        let _sandbox = Sandbox::new("paste");

        let source = SourceImporter::add_link(
            "vless://11111111-1111-1111-1111-111111111111@a.example.com:443?security=tls#One vless://22222222-2222-2222-2222-222222222222@b.example.com:443?security=tls#Two",
        )
        .unwrap();
        assert_eq!(names(&SourceStore::content(&source.id)), ["One", "Two"]);
    }

    #[test]
    fn a_source_is_named_after_the_panel_host() {
        assert_eq!(host_of("https://sub.example.org/TOKEN"), "sub.example.org");
        assert_eq!(host_of("http://127.0.0.1:8080/x"), "127.0.0.1:8080");
        assert_eq!(host_of("не ссылка"), "Подписка");
    }
}
