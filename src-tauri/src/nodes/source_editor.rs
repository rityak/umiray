//! Редактор записей узлов и разниц поверх подписки.

use serde::Serialize;

use crate::error::{AppError, Result};
use crate::nodes::entries::EntryPatch;
use crate::nodes::link::LinkParser;
use crate::nodes::source_build::converted;
use crate::nodes::source_build::SourceBuilder;
use crate::nodes::source_id::SourceId;
use crate::nodes::sources::built_proxies;
use crate::nodes::sources::own_proxies;
use crate::nodes::sources::rebuild;
use crate::nodes::sources::taken_by_others;
use crate::nodes::sources::write;
use crate::nodes::sources::SourceStore;

/// Шапка кода у узла, который клиент написал сам: откатывать его не к чему.
const OWN_HEAD: &str =
    "# Ваш узел: этот документ и есть его конфиг. Правится целиком, имена полей — как у ядра.
";

/// Шапка правимого кода узла. Объяснение стоит в самом документе, а не подсказкой рядом:
/// его читают там же, где правят.
const ENTRY_HEAD: &str =
    "# Запись узла так, как её увидит ядро. Правится: всё, чего нет в подписке, остаётся вашим,
# а изменённое хранится разницей — свежий ключ с сервера доедет и поверх правки.
";

/// Код узла и то, принимает ли он ввод (D-119).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Code {
    pub text: String,
    pub editable: bool,
    /// Запись узла объектом — из неё окно заполняет форму (D-121). Пусто там, где формы
    /// быть не может: ссылку читает ядро, и записи у нас просто нет.
    pub entry: Option<serde_json::Value>,
    /// Почему формы нет. Показывается вместо неё, а не вместо всего окна.
    pub why: Option<String>,
}

pub struct SourceEditor;

impl SourceEditor {
    /// Снять все правки с узла — вернуть его к тому, что прислала панель.
    /// Пересобрать источник из сырья — тем же путём, что и после обновления подписки.
    /// Нужен переезду: у профилей до D-122 собранный файл ещё список ссылок.
    pub fn reparse(id: &str) -> Result<()> {
        SourceId::parse(id)?;
        rebuild(
            &mut SourceStore::get(id)?,
            id,
            &crate::config::awg::Mask::get(),
        )
    }

    pub fn reset_node(id: &str, node: &str) -> Result<()> {
        SourceId::parse(id)?;
        let identity = identity(id, node)?;
        let mut written = EntryPatch::load(id);
        written.remove(&identity);
        EntryPatch::save(id, &written)?;
        rebuild(
            &mut SourceStore::get(id)?,
            id,
            &crate::config::awg::Mask::get(),
        )
    }

    /// Записать узел целиком, приняв его объектом (D-121). Дальше — той же дорогой, что
    /// и правка кода: у записи своей хранилище, у собранной из ссылки — разница.
    pub fn set_node_entry(id: &str, node: &str, entry: serde_json::Value) -> Result<()> {
        SourceId::parse(id)?;
        let mapping = serde_yaml::to_value(&entry)
            .ok()
            .and_then(|value| value.as_mapping().cloned())
            .ok_or_else(|| AppError::invalid("Это не запись узла"))?;
        let text =
            serde_yaml::to_string(&serde_yaml::Value::Mapping(SourceStore::ordered(mapping)))
                .map_err(|e| AppError::invalid(e.to_string()))?;
        SourceEditor::edit_node_code(id, node, &text)
    }

    /// Убрать узел, который клиент написал сам (D-121). Только у источника записей: узел
    /// подписки вернётся следующим обновлением, и «удаление» было бы враньём.
    pub fn delete_node(id: &str, node: &str) -> Result<()> {
        SourceId::parse(id)?;
        let (at, _) = own_entry(id, node)?;
        let mut proxies = own_proxies(id);
        proxies.remove(at);
        let mut document = serde_yaml::Mapping::new();
        crate::yaml::Yaml::set(
            &mut document,
            "proxies",
            serde_yaml::Value::Sequence(proxies),
        );
        let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        let mut source = SourceStore::get(id)?;
        write(
            &mut source,
            vec![yaml],
            id,
            &crate::config::awg::Mask::get(),
        )?;
        // Правки удалённого узла уходят вместе с ним: иначе они ждали бы одноимённого.
        if let Ok(identity) = identity(id, node) {
            let mut written = EntryPatch::load(id);
            written.remove(&identity);
            EntryPatch::save(id, &written)?;
        }
        Ok(())
    }

    /// Код узла: запись, которую читает ядро, и та же запись объектом (D-121, D-122).
    ///
    /// Разницы между «своим» узлом и узлом подписки больше нет: и то и другое — запись.
    /// Разница осталась в хранении: у источника записей правка ложится в него самого,
    /// у подписки — разницей поверх разобранного.
    pub fn node_code(id: &str, node: &str) -> Result<Code> {
        SourceId::parse(id)?;
        let entry = built_proxies(id)
            .into_iter()
            .find(|entry| {
                entry
                    .get(serde_yaml::Value::from("name"))
                    .and_then(serde_yaml::Value::as_str)
                    == Some(node)
            })
            .ok_or_else(|| AppError::invalid(format!("Узел не найден: {node}")))?;
        let head = if SourceStore::get(id)?.records {
            OWN_HEAD
        } else {
            ENTRY_HEAD
        };
        Ok(Code {
            text: format!(
                "{head}{}",
                serde_yaml::to_string(&serde_yaml::Value::Mapping(entry.clone()))
                    .map_err(|e| AppError::invalid(e.to_string()))?
            ),
            editable: true,
            entry: object(&entry),
            why: None,
        })
    }

    /// Переписать запись узла целиком (D-119, D-122).
    ///
    /// У источника записей хранилище и есть его код — пишем прямо в него. У подписки храним
    /// **разницу** от разобранного: свежий ключ с сервера должен доезжать и поверх правки.
    pub fn edit_node_code(id: &str, node: &str, text: &str) -> Result<()> {
        SourceId::parse(id)?;
        let edited: serde_yaml::Mapping = serde_yaml::from_str(text)
            .map_err(|e| AppError::invalid(format!("Это не конфиг узла: {e}")))?;
        let named = edited
            .get(serde_yaml::Value::from("name"))
            .and_then(serde_yaml::Value::as_str);
        if named != Some(node) {
            return Err(AppError::invalid(
                "Имя узла здесь не меняется: по нему узел находят группы и правила",
            ));
        }

        if SourceStore::get(id)?.records {
            let (at, _) = own_entry(id, node)?;
            let mut proxies = own_proxies(id);
            proxies[at] = serde_yaml::Value::Mapping(edited);
            let mut document = serde_yaml::Mapping::new();
            crate::yaml::Yaml::set(
                &mut document,
                "proxies",
                serde_yaml::Value::Sequence(proxies),
            );
            let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
                .map_err(|e| AppError::invalid(e.to_string()))?;
            let mut source = SourceStore::get(id)?;
            return write(
                &mut source,
                vec![yaml],
                id,
                &crate::config::awg::Mask::get(),
            );
        }

        // Разницу считаем от **разобранного без правок**: иначе вторая правка легла бы
        // поверх первой и «Откатить» вернул бы не туда.
        let (clean, _) = converted(
            id,
            LinkParser::clean(&raw_lines(id), &mut taken_by_others(id)),
            &crate::config::awg::Mask::get(),
        )?;
        let built = serde_yaml::from_str::<serde_yaml::Value>(&clean)
            .ok()
            .and_then(|value| value.get("proxies")?.as_sequence().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|proxy| proxy.as_mapping().cloned())
            .find(|entry| SourceBuilder::identity_of(entry) == SourceBuilder::identity_of(&edited))
            .ok_or_else(|| AppError::invalid(format!("Узел не найден: {node}")))?;

        let identity = SourceBuilder::identity_of(&built)
            .ok_or_else(|| AppError::invalid(format!("Не удалось опознать узел: {node}")))?;
        let mut all = EntryPatch::load(id);
        let patch = EntryPatch::diff(&built, &edited);
        if patch.is_empty() {
            all.remove(&identity);
        } else {
            all.insert(identity, patch);
        }
        EntryPatch::save(id, &all)?;
        rebuild(
            &mut SourceStore::get(id)?,
            id,
            &crate::config::awg::Mask::get(),
        )
    }
}

/// Запись объектом — тем, что перейдёт границу (D-121). YAML разбирают здесь, а не в окне.
fn object(entry: &serde_yaml::Mapping) -> Option<serde_json::Value> {
    serde_json::to_value(serde_yaml::Value::Mapping(entry.clone())).ok()
}

/// Сырьё построчно — им пользуются и пересборка, и подсчёт разницы.
fn raw_lines(id: &str) -> Vec<String> {
    SourceStore::raw(id).lines().map(str::to_string).collect()
}

/// Запись узла в источнике записей — вместе с её местом в списке.
fn own_entry(id: &str, node: &str) -> Result<(usize, serde_yaml::Mapping)> {
    own_proxies(id)
        .into_iter()
        .enumerate()
        .find_map(|(at, proxy)| {
            let map = proxy.as_mapping()?;
            (map.get(serde_yaml::Value::from("name"))?.as_str()? == node).then(|| (at, map.clone()))
        })
        .ok_or_else(|| AppError::invalid(format!("Узел не найден: {node}")))
}

/// Тождество узла по его нынешнему имени. Считается от **сырья**: имя в ключ не входит,
/// но найти нужную строку в собранном списке проще всего именно по имени, а порядок строк
/// сырья и результата совпадает — результат из сырья и получен.
fn identity(id: &str, node: &str) -> Result<String> {
    // Читаем **собранное**, а не сырьё: у подписки сырьё — это ссылки, а тождество
    // с D-122 считается от записи.
    built_proxies(id)
        .into_iter()
        .find_map(|entry| {
            (entry.get(serde_yaml::Value::from("name"))?.as_str()? == node)
                .then(|| SourceBuilder::identity_of(&entry))?
        })
        .ok_or_else(|| AppError::invalid(format!("Не удалось опознать узел: {node}")))
}
