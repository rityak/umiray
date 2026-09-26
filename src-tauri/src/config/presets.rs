//! Наборы маршрутизации (D-056, переустроено D-071, сужено D-075).
//!
//! Набор — это **и есть** то, что правится в разделе «Маршрутизация»: другого файла под него
//! больше нет. Группы из набора уехали в общий документ клиента (D-075): они про источники,
//! а не про маршрут, и заводить их заново в каждом наборе значило бы переписывать одно и то же.
//!
//! Раньше рядом жила пара «живых» `groups.yaml` и `rules.yaml`, в которые набор подкладывался
//! при переключении, а при уходе снимался обратно. Смысла в этой возне не было никакого:
//! живой файл всегда был копией либо набора, либо того, что клиент собирает сам, — и обе
//! копии умели разъезжаться с оригиналом.
//!
//! Теперь сгенерированный набор не хранится вовсе (его собирает рендер в момент сборки),
//! а именованные лежат здесь и правятся на месте.
//!
//! `advanced.yaml` в набор **не входит**: он про режим перехвата и DNS ядра, а не про
//! маршрут, и возить его вместе с направлением значило бы ронять TUN при смене выхода.
//!
//! Проверка на диске живёт в `live.rs`: она подменяет `LOCALAPPDATA` всему процессу,
//! а такой тест в общем прогоне подрался бы за переменную с соседями (GOTCHAS).

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::paths;
use crate::stamp;

/// Из чего состоит набор. Имя части — оно же идентификатор раздела окна; `files.rs`
/// строит по нему список документов и стережёт совпадение тестом.
///
/// Часть одна: группы уехали в общий документ клиента (D-075), и набор — это ровно
/// маршрутизация, ничего больше.
pub const PARTS: [&str; 1] = ["rules"];

/// Единственная часть набора. Отдельным именем, чтобы «rules» не расползлось строкой
/// по трём модулям.
pub const RULES: &str = "rules";

/// Имя первого набора. Придумывает его клиент, а не человек: набор заводится сам,
/// чтобы разделу было что показывать, и спрашивать имя в этот момент — мешать.
const FIRST: &str = "Мой набор";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// Секунды эпохи. Форматирует интерфейс, бэкенд времени не знает.
    pub created: Option<u64>,
}

/// Идентификатор приходит из вебвью, а это недоверенные данные: он не должен превращаться
/// в путь. Форма закрытая — ровно то, что выдаёт `stamp::id`.
fn valid(id: &str) -> bool {
    id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())
}

fn check(id: &str) -> Result<()> {
    if valid(id) {
        Ok(())
    } else {
        Err(AppError::invalid(format!("Неизвестный набор: {id}")))
    }
}

/// Часть набора — тоже с границы, и тоже не должна превращаться в путь.
fn check_part(part: &str) -> Result<()> {
    if PARTS.contains(&part) {
        Ok(())
    } else {
        Err(AppError::invalid(format!(
            "Неизвестная часть набора: {part}"
        )))
    }
}

pub fn list() -> Vec<Preset> {
    let Ok(entries) = std::fs::read_dir(paths::presets_dir()) else {
        return Vec::new();
    };
    let mut presets: Vec<Preset> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    // Порядок в окне не должен скакать от того, как файлы легли на диске.
    presets.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
    presets
}

pub fn get(id: &str) -> Result<Preset> {
    check(id)?;
    let text = std::fs::read_to_string(paths::preset_meta(id))
        .map_err(|_| AppError::invalid(format!("Набора «{id}» нет")))?;
    serde_json::from_str(&text).map_err(|e| AppError::invalid(format!("Набор не читается: {e}")))
}

/// Часть набора как текст. Отсутствие файла — пустой документ, а не ошибка: часть мог
/// не записать снимок более ранней сборки, и разделу всё равно есть что показать.
pub fn read(id: &str, part: &str) -> Result<String> {
    check(id)?;
    check_part(part)?;
    Ok(std::fs::read_to_string(paths::preset_part(id, part)).unwrap_or_default())
}

pub fn write(id: &str, part: &str, text: &str) -> Result<()> {
    check(id)?;
    check_part(part)?;
    // Набор без описания — не набор: писать в несуществующий значит завести файлы,
    // которых потом никто не покажет.
    get(id)?;
    paths::ensure_presets_dir()?;
    Ok(crate::atomic::write(paths::preset_part(id, part), text)?)
}

/// Содержимое набора — то, что уходит в сборку. Это ровно маршрутизация (D-075).
pub fn content(id: &str) -> Result<String> {
    read(id, RULES)
}

/// Завести набор с готовым содержимым.
///
/// Содержимое приходит снаружи, а не собирается здесь: клиенту его собирает рендер,
/// а хранилищу знать про рендер незачем.
pub fn create(name: &str, rules: &str) -> Result<Preset> {
    let preset = Preset {
        id: stamp::id()?,
        name: unique_name(name, &list()),
        created: stamp::now(),
    };
    paths::ensure_presets_dir()?;
    write_meta(&preset)?;
    crate::atomic::write(paths::preset_part(&preset.id, RULES), rules)?;
    Ok(preset)
}

/// Переименовать. Имя приходит от человека, поэтому приводим его к уникальному:
/// два набора с одинаковой подписью в списке неразличимы.
pub fn rename(id: &str, name: &str) -> Result<Preset> {
    let mut preset = get(id)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("У набора должно быть имя"));
    }
    // Себя из списка занятых исключаем, иначе переименование в то же самое даст «имя 2».
    let others: Vec<Preset> = list().into_iter().filter(|other| other.id != id).collect();
    preset.name = unique_name(name, &others);
    write_meta(&preset)?;
    Ok(preset)
}

/// Удалить набор целиком: описание и его часть.
///
/// Отсутствие части — не ошибка: её мог не записать более ранний снимок, а результат
/// нужен один и тот же — набора больше нет.
pub fn delete(id: &str) -> Result<()> {
    check(id)?;
    for part in PARTS {
        let _ = std::fs::remove_file(paths::preset_part(id, part));
    }
    Ok(std::fs::remove_file(paths::preset_meta(id))?)
}

/// Имя первого набора.
pub fn default_name() -> &'static str {
    FIRST
}

/// Имя, которого ещё нет. Два набора с одинаковой подписью в списке неразличимы.
fn unique_name(base: &str, taken: &[Preset]) -> String {
    let occupied = |name: &str| taken.iter().any(|preset| preset.name == name);
    if !occupied(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !occupied(name))
        .unwrap_or_else(|| base.to_string())
}

fn write_meta(preset: &Preset) -> Result<()> {
    let text = serde_json::to_string_pretty(preset)
        .map_err(|e| AppError::io(format!("Не удалось записать набор: {e}")))?;
    Ok(crate::atomic::write(paths::preset_meta(&preset.id), text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(names: &[&str]) -> Vec<Preset> {
        names
            .iter()
            .map(|name| Preset {
                id: "0".repeat(16),
                name: (*name).to_string(),
                created: None,
            })
            .collect()
    }

    /// `id` приходит из вебвью: он не должен превращаться в путь на диске.
    #[test]
    fn an_identifier_that_is_not_ours_is_refused() {
        assert!(valid("0123456789abcdef"));
        assert!(!valid("../../settings"));
        assert!(!valid(""));
        assert!(!valid("0123456789abcde"), "короче нашего");
        assert!(!valid("0123456789abcdefg"), "длиннее нашего");
        assert!(!valid("0123456789abcdeZ"), "не шестнадцатеричный");
        assert!(check("../../settings").is_err());
    }

    /// Имя части — тоже с границы: `rules/../../settings` не должно стать путём.
    #[test]
    fn a_part_that_is_not_ours_is_refused() {
        assert!(check_part(RULES).is_ok());
        assert!(
            check_part("groups").is_err(),
            "группы — общий документ клиента, а не часть набора (D-075)"
        );
        assert!(
            check_part("advanced").is_err(),
            "оно про ядро, а не про маршрут"
        );
        assert!(check_part("../settings").is_err());
        assert!(check_part("").is_err());
    }

    #[test]
    fn a_second_set_does_not_take_the_same_name() {
        assert_eq!(unique_name(FIRST, &[]), FIRST);
        assert_eq!(unique_name(FIRST, &named(&[FIRST])), "Мой набор 2");
        assert_eq!(
            unique_name(FIRST, &named(&[FIRST, "Мой набор 2"])),
            "Мой набор 3"
        );
        assert_eq!(
            unique_name(FIRST, &named(&["Другое"])),
            FIRST,
            "чужое имя занятым не считается"
        );
    }

    // Где искать ссылку на источник — вопрос уже не набора: группы стали общим документом
    // клиента (D-075). Правило переехало в `state::mentions` вместе с тестом.
}
