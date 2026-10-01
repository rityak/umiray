//! Переезд каталога с файлов в базу (D-170): раскладка 1.x → `umiray.db`.
//!
//! Раскладка не менялась с 1.0.0, поэтому знать её достаточно одну — она здесь списком.
//! Всё ложится одной транзакцией; после неё каждый файл сверяется с записанным и только
//! тогда удаляется. Не сошлось — файл остаётся, следующий запуск попробует снова: файл
//! перезапишет строку, а не наоборот, потому что это копия пользователя.
//!
//! Чего раскладка не знает, остаётся на диске и называется в журнале: молча удалить чужое
//! значило бы потерять, молча забыть — соврать.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::collections::Collections;
use crate::db::{Db, Table};
use crate::error::Result;
use crate::paths::Paths;

/// Куда ложится файл.
#[derive(Debug, PartialEq, Eq)]
enum Fate {
    /// Строка `(таблица, id, часть)`.
    Row(Table, String, String),
    /// Производное, которое пересоберётся само: собранное ядром, отметки старых переездов.
    Drop,
}

/// Файлы корня: имя → строка.
const ROOT: &[(&str, Table, &str)] = &[
    ("advanced.yaml", Table::Documents, "advanced"),
    ("client.yaml", Table::Documents, "client"),
    ("groups.yaml", Table::Documents, "groups"),
    ("settings.json", Table::State, "settings"),
    ("hwid.txt", Table::State, "hwid"),
    ("geo.json", Table::State, "geo"),
];

/// Суффикс файла → часть строки; `None` — файл производный и не переезжает.
type Suffixes = &'static [(&'static str, Option<&'static str>)];

/// Папки «по файлу на часть»: `<id><суффикс>` → `(таблица, id, часть)`. Суффикс длиннее —
/// раньше: `.entry.yaml` и `.patch.json` иначе приняли бы за `.yaml` и `.json`.
const FOLDERS: &[(&str, Table, Suffixes)] = &[
    (
        "presets",
        Table::Presets,
        &[(".rules.yaml", Some("rules")), (".json", Some("meta"))],
    ),
    (
        "sources",
        Table::Sources,
        &[
            (".entry.yaml", Some("entries")),
            // Правки ссылок до D-122: не читаются и не пишутся.
            (".patch.json", None),
            (".json", Some("meta")),
            (".raw", Some("raw")),
            (".txt", Some("provider")),
        ],
    ),
    (
        "lists",
        Table::Lists,
        &[
            (".json", Some("meta")),
            (".domains", Some("domains")),
            (".cidrs", Some("cidrs")),
        ],
    ),
];

/// Отметки разовых шагов у старой папки коллекций: их смысл переезжает в `Collections::upgrade`.
const COLLECTION_MARKS: &[&str] = &[".rule-titles-v1", ".lists-v1", ".dns-v2"];

pub struct FileImport;

impl FileImport {
    /// Перенести всё, что найдётся. Отдаёт, сколько файлов ушло в базу.
    pub fn run() -> Result<usize> {
        let root = Paths::root();
        let (plan, unknown) = plan(&root);
        for file in &unknown {
            eprintln!(
                "переезд в базу: не знаю, что это, оставляю — {}",
                file.display()
            );
        }
        if plan.is_empty() {
            return Ok(0);
        }
        let collections = root.join("collections").is_dir();
        let marks: HashSet<String> = COLLECTION_MARKS
            .iter()
            .filter(|mark| root.join("collections").join(mark).exists())
            .map(|mark| (*mark).to_string())
            .collect();

        let mut rows = Vec::new();
        for (file, fate) in &plan {
            if let Fate::Row(table, id, part) = fate {
                match std::fs::read_to_string(file) {
                    Ok(text) => rows.push((file, *table, id.as_str(), part.as_str(), text)),
                    Err(why) => eprintln!("переезд в базу: не читается {}: {why}", file.display()),
                }
            }
        }
        Db::batch(|batch| {
            for (_, table, id, part, text) in &rows {
                batch.put(*table, id, part, text)?;
            }
            Ok(())
        })?;
        let written: HashSet<&PathBuf> = rows
            .iter()
            .filter(|(_, table, id, part, text)| {
                Db::get(*table, id, part).ok().flatten().as_deref() == Some(text.as_str())
            })
            .map(|(file, ..)| *file)
            .collect();
        if collections {
            Collections::upgrade(|mark| marks.contains(mark))?;
        }

        let mut moved = 0;
        for (file, fate) in &plan {
            let gone = match fate {
                Fate::Row(..) => written.contains(file),
                Fate::Drop => true,
            };
            if gone && std::fs::remove_file(file).is_ok() && matches!(fate, Fate::Row(..)) {
                moved += 1;
            }
        }
        for mark in COLLECTION_MARKS {
            let _ = std::fs::remove_file(root.join("collections").join(mark));
        }
        for dir in [
            "collections/rules",
            "collections",
            "presets",
            "sources",
            "lists",
        ] {
            remove_empty(&root.join(dir));
        }
        Ok(moved)
    }
}

/// Что куда ложится и чего раскладка не знает.
fn plan(root: &Path) -> (Vec<(PathBuf, Fate)>, Vec<PathBuf>) {
    let mut plan = Vec::new();
    let mut unknown = Vec::new();
    for (name, table, id) in ROOT {
        let file = root.join(name);
        if file.is_file() {
            plan.push((file, Fate::Row(*table, (*id).into(), String::new())));
        }
    }
    // Отметка переезда stable/dev (D-150): её роль теперь у самой базы.
    let mark = root.join(".data-v2");
    if mark.is_file() {
        plan.push((mark, Fate::Drop));
    }
    let pending = root.join("qd").join("pending.txt");
    if pending.is_file() {
        plan.push((
            pending,
            Fate::Row(Table::State, "qd-pending".into(), String::new()),
        ));
    }
    for file in files(root) {
        if let Some(id) = archived(root, &file) {
            plan.push((file, Fate::Row(Table::Archive, id, String::new())));
        }
    }
    for (folder, table, suffixes) in FOLDERS {
        let dir = root.join(folder);
        for file in files(&dir) {
            if let Some(id) = archived(root, &file) {
                plan.push((file, Fate::Row(Table::Archive, id, String::new())));
                continue;
            }
            match in_folder(&file, *table, suffixes) {
                Some(fate) => plan.push((file, fate)),
                None => unknown.push(file),
            }
        }
        // Собранное ядрами из списков (`lists/<ядро>/`): пересоберётся в `run/lists/`.
        if *folder == "lists" {
            for engine in dirs(&dir) {
                for file in files(&engine) {
                    plan.push((file, Fate::Drop));
                }
            }
        }
    }
    let collections = root.join("collections");
    for file in files(&collections) {
        let name = file_name(&file);
        match name.strip_suffix(".yaml") {
            Some(id) if !id.is_empty() => plan.push((
                file,
                Fate::Row(Table::Collections, id.into(), String::new()),
            )),
            _ if COLLECTION_MARKS.contains(&name.as_str()) => {}
            _ => unknown.push(file),
        }
    }
    for file in files(&collections.join("rules")) {
        match file_name(&file).strip_suffix(".yaml") {
            Some(id) if !id.is_empty() => plan.push((
                file.clone(),
                Fate::Row(Table::Collections, "rules".into(), id.into()),
            )),
            _ => unknown.push(file),
        }
    }
    (plan, unknown)
}

/// Файл папки «по файлу на часть» → строка, если суффикс знакомый.
fn in_folder(file: &Path, table: Table, suffixes: Suffixes) -> Option<Fate> {
    let name = file_name(file);
    let (suffix, part) = suffixes.iter().find(|(suffix, _)| name.ends_with(suffix))?;
    let id = name.strip_suffix(suffix)?;
    if id.is_empty() || id.contains('.') {
        return None;
    }
    Some(match part {
        Some(part) => Fate::Row(table, id.into(), (*part).into()),
        None => Fate::Drop,
    })
}

/// Копии, которые прежние переезды оставили единственными (`*.migrated`): ключ — путь
/// от корня, чтобы было видно, откуда она.
fn archived(root: &Path, file: &Path) -> Option<String> {
    if !file_name(file).ends_with(".migrated") {
        return None;
    }
    Some(
        file.strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

fn files(dir: &Path) -> Vec<PathBuf> {
    entries(dir, |path| path.is_file())
}

fn dirs(dir: &Path) -> Vec<PathBuf> {
    entries(dir, |path| path.is_dir())
}

fn entries(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| keep(path))
        .collect();
    found.sort();
    found
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Убрать опустевшие папки снизу вверх; непустая остаётся — в ней то, чего раскладка не знает.
fn remove_empty(dir: &Path) {
    for inner in dirs(dir) {
        remove_empty(&inner);
    }
    let _ = std::fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Sandbox;

    fn put(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Каталог 1.x целиком: каждая часть ложится своей строкой, файлы уходят, а то, чего
    /// раскладка не знает, остаётся на месте.
    #[test]
    fn a_file_directory_moves_into_the_database_whole() {
        let sandbox = Sandbox::new("import");
        let root = Paths::root();
        put(&root, "advanced.yaml", "mode: rule\n");
        put(&root, "settings.json", "{\"version\":2}");
        put(&root, "hwid.txt", "0123456789abcdef");
        put(&root, ".data-v2", "1\n");
        put(&root, "config.yaml.migrated", "old: config\n");
        put(
            &root,
            "presets/00000000000000aa.json",
            "{\"id\":\"00000000000000aa\"}",
        );
        put(&root, "presets/00000000000000aa.rules.yaml", "rules: []\n");
        put(
            &root,
            "presets/00000000000000aa.groups.yaml.migrated",
            "proxy-groups: []\n",
        );
        put(
            &root,
            "sources/00000000000000bb.json",
            "{\"id\":\"00000000000000bb\"}",
        );
        put(&root, "sources/00000000000000bb.raw", "vless://raw");
        put(&root, "sources/00000000000000bb.txt", "proxies: []\n");
        put(&root, "sources/00000000000000bb.entry.yaml", "x: {}\n");
        put(&root, "sources/00000000000000bb.patch.json", "{}");
        put(&root, "sources/notes.md", "моё");
        put(&root, "lists/ads.json", "{\"id\":\"ads\"}");
        put(&root, "lists/ads.domains", "+.ads.example\n");
        put(&root, "lists/mihomo/ads.domains.mrs", "built");
        put(&root, "collections/dns.yaml", "version: 1\nproviders: []\n");
        put(
            &root,
            "collections/rules/mine.yaml",
            "title: Мой\nrules: [MATCH,DIRECT]\n",
        );
        put(&root, "collections/.rule-titles-v1", "1\n");
        put(&root, "collections/.dns-v2", "1\n");
        put(&root, "qd/pending.txt", "qd://link");
        put(&root, "qd/client.db", "чужая база");

        let moved = FileImport::run().unwrap();
        assert_eq!(moved, 16);

        let row = |table, id: &str, part: &str| Db::get(table, id, part).unwrap();
        assert_eq!(
            row(Table::Documents, "advanced", "").as_deref(),
            Some("mode: rule\n")
        );
        assert_eq!(
            row(Table::State, "hwid", "").as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(
            row(Table::State, "qd-pending", "").as_deref(),
            Some("qd://link")
        );
        assert_eq!(
            row(Table::Presets, "00000000000000aa", "rules").as_deref(),
            Some("rules: []\n")
        );
        assert_eq!(
            row(Table::Sources, "00000000000000bb", "provider").as_deref(),
            Some("proxies: []\n")
        );
        assert_eq!(
            row(Table::Sources, "00000000000000bb", "entries").as_deref(),
            Some("x: {}\n")
        );
        assert_eq!(
            row(Table::Lists, "ads", "domains").as_deref(),
            Some("+.ads.example\n")
        );
        assert_eq!(
            row(Table::Collections, "rules", "mine").as_deref(),
            Some("title: Мой\nrules: [MATCH,DIRECT]\n")
        );
        assert_eq!(
            row(
                Table::Archive,
                "presets/00000000000000aa.groups.yaml.migrated",
                ""
            )
            .as_deref(),
            Some("proxy-groups: []\n")
        );
        assert!(
            row(Table::Collections, "lists", "").is_some(),
            "без метки .lists-v1 каталог rule sets раздаётся"
        );

        for gone in [
            "advanced.yaml",
            ".data-v2",
            "config.yaml.migrated",
            "presets",
            "lists",
            "collections",
            "qd/pending.txt",
        ] {
            assert!(!root.join(gone).exists(), "{gone} остался");
        }
        assert!(root.join("sources/notes.md").exists(), "чужое не удаляется");
        assert!(
            !root.join("sources/00000000000000bb.raw").exists(),
            "перенесённое удаляется и рядом с чужим"
        );
        assert!(root.join("qd/client.db").exists(), "база qd — не наша");
        assert_eq!(
            FileImport::run().unwrap(),
            0,
            "второй прогон переносить нечего"
        );
        drop(sandbox);
    }

    /// Строка в базе, которой уже касалась сборка (например, шаблон до переезда), уступает
    /// файлу: файл — копия пользователя.
    #[test]
    fn a_file_wins_over_a_row_written_before_the_move() {
        let _sandbox = Sandbox::new("import-wins");
        let root = Paths::root();
        Db::put(Table::Documents, "client", "", "ping: icmp\n").unwrap();
        put(&root, "client.yaml", "ping: tcp\n");
        FileImport::run().unwrap();
        assert_eq!(
            Db::get(Table::Documents, "client", "").unwrap().as_deref(),
            Some("ping: tcp\n")
        );
    }

    /// Суффиксы проверяются от длинного: `.entry.yaml` — правки записей, а не что-то `.yaml`.
    #[test]
    fn the_longest_suffix_decides_the_part() {
        let sources = FOLDERS
            .iter()
            .find(|(name, ..)| *name == "sources")
            .unwrap()
            .2;
        assert_eq!(
            in_folder(Path::new("s/abc.entry.yaml"), Table::Sources, sources),
            Some(Fate::Row(Table::Sources, "abc".into(), "entries".into()))
        );
        assert_eq!(
            in_folder(Path::new("s/abc.patch.json"), Table::Sources, sources),
            Some(Fate::Drop)
        );
        assert_eq!(
            in_folder(Path::new("s/notes.md"), Table::Sources, sources),
            None
        );
    }
}
