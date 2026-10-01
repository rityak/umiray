//! Скачанные списки — в `.mrs`, формат mihomo (D-157).
//!
//! Текстом antizapret стоит ядру 870 МБ памяти, `.mrs` — 94 (S-028). Собирает само ядро:
//! `mihomo convert-ruleset`, так что формат файла всегда совпадает с тем, кто его читает.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::mihomo::Mihomo;
use crate::error::{AppError, Result};
use crate::lists::store::{ListStore, Part};
use crate::render::mihomo_lists::MihomoLists;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Чем кончилась сборка.
#[derive(Debug, Default)]
pub struct Built {
    /// Списки, чьи файлы изменились: работающее ядро должно их перечитать.
    pub changed: Vec<String>,
    /// Что не собралось — строкой для лога. Остальные списки от этого не страдают.
    pub failed: Vec<String>,
}

pub struct ListBuild;

impl ListBuild {
    /// Собрать то, что отсутствует или старше своего списка (`updated` — когда легли
    /// данные), и убрать собранное из частей, которых больше нет. Без ядра на диске —
    /// ничего: соберёт первый запуск.
    ///
    /// Процесс ядра на каждую часть: звать из блокирующего контекста.
    pub fn prepare() -> Built {
        let mut built = Built::default();
        if !Mihomo::binary().exists() {
            return built;
        }
        for list in ListStore::list() {
            let mut changed = false;
            let fresh = UNIX_EPOCH + Duration::from_secs(list.updated.unwrap_or_default());
            for part in Part::ALL {
                let target = MihomoLists::artifact(&list.id, part);
                let Some(text) = ListStore::part(&list.id, part) else {
                    changed |= std::fs::remove_file(&target).is_ok();
                    continue;
                };
                if modified(&target).is_some_and(|built| built >= fresh) {
                    continue;
                }
                match compile(part, &text, &target) {
                    Ok(()) => changed = true,
                    Err(why) => built.failed.push(format!("{}: {why}", list.id)),
                }
            }
            if changed {
                built.changed.push(list.id);
            }
        }
        built
    }
}

/// Время изменения; нет файла — нет и времени.
fn modified(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Одна часть — один вызов ядра. Текст из базы ядро читает файлом: кладём его рядом
/// на время сборки. Собранное пишем рядом и переименовываем: работающее ядро может
/// как раз читать прежний файл.
fn compile(part: Part, text: &str, target: &std::path::Path) -> Result<()> {
    let behavior = match part {
        Part::Domains => "domain",
        Part::Cidrs => "ipcidr",
    };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let staging = target.with_extension("mrs.part");
    let source = target.with_extension("txt.part");
    std::fs::write(&source, text)?;
    let mut command = Command::new(Mihomo::binary());
    command
        .arg("convert-ruleset")
        .arg(behavior)
        .arg("text")
        .arg(&source)
        .arg(&staging);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output();
    let _ = std::fs::remove_file(&source);
    let output = output.map_err(|e| AppError::io(format!("Не удалось запустить ядро: {e}")))?;
    if !output.status.success() || !staging.exists() {
        let _ = std::fs::remove_file(&staging);
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::io(format!(
            "ядро не собрало .mrs: {}",
            said.lines().last().unwrap_or("без объяснения")
        )));
    }
    std::fs::rename(&staging, target)?;
    Ok(())
}
