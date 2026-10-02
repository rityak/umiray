//! Приведение данных к тому, что ожидает текущая сборка (D-025): разовый переезд с файлов
//! в базу (D-170) и шаги, которые проверяются при каждом запуске.
//!
//! Переезды раскладок до 1.0.0 удалены вместе с файлами (D-170): 1.0.0 их уже провела,
//! а публичных установок старше неё нет.

use serde_yaml::Value;

use crate::app::import::FileImport;
use crate::config::files;
use crate::config::files::Documents;
use crate::config::presets::PresetStore;
use crate::error::{AppError, Result};
use crate::nodes::source_editor::SourceEditor;
use crate::nodes::sources::SourceStore;
use crate::yaml::Yaml;

/// Пересобрать источники из сырья (D-122).
///
/// Не «переезд по флагу», а пересборка производного: файл, который читает ядро, — это
/// разбор сырья текущей сборкой, и меняется он вместе с разбором. Флаг версии здесь
/// был бы лишней сущностью: пересборка идёт из того же сырья и повторный запуск ничего
/// не меняет, а научившийся чему-то новому разбор доезжает сам.
///
/// Источники записей не трогаем: у них сырьё и есть документ, разбирать нечего.
/// Отказ на одном не уносит остальные — испорченный файл не повод не открыть окно.
fn reparse_sources() -> Result<()> {
    for source in SourceStore::list()
        .into_iter()
        .filter(|source| !source.records)
    {
        if let Err(why) = SourceEditor::reparse(&source.id) {
            eprintln!("источник «{}» не пересобрался: {why}", source.name);
        }
    }
    Ok(())
}

pub struct Migration;

impl Migration {
    /// Вызывается один раз при старте. Уже переехавшую установку не трогает.
    pub fn run() -> Result<()> {
        // Файлы — первыми: всё остальное читает и пишет базу (D-170).
        let moved = FileImport::run()?;
        if moved > 0 {
            eprintln!("переезд в базу: {moved} файлов");
        }
        // Коллекции раздаются только в пустую таблицу: переехавшее раздача не тронет (D-100).
        crate::collections::Collections::seed()?;
        crate::collections::Collections::offer()?;
        refresh_stale_templates()?;
        ensure_first_preset()?;
        adopt_ready_sets()?;
        reparse_sources()
    }
}

/// Шаблон прошлой сборки — не правки пользователя.
///
/// Файл, в котором нет ничего кроме комментариев, содержательно пуст: терять там нечего.
/// А человек, открывший «Маршрутизацию», должен видеть боевое правило, а не рассказ о том,
/// что будет, если файл оставить пустым. Свой текст это не трогает: непустой маппинг
/// остаётся как есть, каким бы старым он ни был.
fn refresh_stale_templates() -> Result<()> {
    for id in Documents::templated() {
        let text = Documents::read(id)?;
        let template = Documents::template(id)?;
        let empty = Yaml::top_mapping(&text)
            .map(|map| map.is_empty())
            .unwrap_or(false);
        if empty && text != template {
            Documents::write(id, template)?;
        }
    }
    Ok(())
}

/// Встроенные наборы, включённые глобально (`rulesets:` в `client.yaml`, D-083), — в раздел
/// `ready` каждого набора маршрутизации (D-158). Раньше они действовали при любом наборе;
/// чтобы после переезда маршрут не поменялся, их получает каждый.
///
/// Дописываем текстом, а не пересобираем документ: пересборка потеряла бы комментарии
/// человека. Поле из `client.yaml` уходит последним — прерванный переезд повторится.
fn adopt_ready_sets() -> Result<()> {
    let client = Documents::read(files::CLIENT)?;
    let mut map = Yaml::top_mapping(&client)?;
    let Some(value) = map.remove(Value::from("rulesets")) else {
        return Ok(());
    };
    let names: Vec<String> = value
        .as_sequence()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !names.is_empty() {
        for preset in PresetStore::list() {
            let text = PresetStore::read(&preset.id, "rules")?;
            if let Some(joined) = with_ready(&text, &names) {
                PresetStore::write(&preset.id, "rules", &joined)?;
            }
        }
    }
    let text = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|e| AppError::invalid(e.to_string()))?;
    Documents::write(files::CLIENT, &text)
}

/// Документ с дописанным разделом `ready`. `None` — раздел уже есть: его писал человек или
/// прошлый, прерванный переезд, и второй раз дописывать нельзя.
fn with_ready(text: &str, names: &[String]) -> Option<String> {
    if Yaml::top_mapping(text)
        .ok()?
        .contains_key(Value::from(crate::config::route::READY))
    {
        return None;
    }
    let mut joined = text.trim_end().to_string();
    joined.push_str(
        "\n\n# Готовые наборы, что раньше включались в «Настройках» для всех (D-158).\nready:\n",
    );
    for name in names {
        joined.push_str(&format!("  - id: {name}\n"));
    }
    Some(joined)
}

/// Один набор существует всегда (D-071): разделу «Маршрутизация» иначе нечего показывать.
/// Содержимое первого — то, что клиент собирает сам из источников.
///
/// Проверяется при каждом запуске, а не один раз: набор можно удалить и мимо окна.
fn ensure_first_preset() -> Result<()> {
    if !PresetStore::list().is_empty() {
        return Ok(());
    }
    PresetStore::create(
        PresetStore::default_name(),
        &crate::render::effective::ConfigRenderer::generated_rules()?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D-158: глобально включённые наборы дописываются в документ текстом — комментарии
    /// человека остаются, а второй раз раздел не дописывается.
    #[test]
    fn globally_enabled_sets_move_into_the_route_once() {
        let doc = "# мой маршрут\nrules:\n  - MATCH,umiray\n";
        let names = ["direct-ru".to_string(), "block-ads".to_string()];
        let joined = with_ready(doc, &names).unwrap();
        assert!(joined.starts_with("# мой маршрут\n"), "комментарий потерян");
        let sections = crate::config::route::Sections::read(&joined).unwrap();
        assert_eq!(
            sections
                .ready
                .iter()
                .map(|set| set.id.as_str())
                .collect::<Vec<_>>(),
            ["direct-ru", "block-ads"]
        );
        assert!(
            sections.ready.iter().all(|set| set.target.is_none()),
            "выход — самого набора, как и было"
        );
        assert_eq!(
            with_ready(&joined, &names),
            None,
            "второй раз не дописываем"
        );
    }
}
