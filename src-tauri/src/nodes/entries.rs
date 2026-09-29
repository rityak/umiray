//! Правка **записи** узла: то, что клиент пишет в `proxies:` сам (D-119).
//!
//! Рядом лежит `patches.rs` — правка параметров ссылки. Разница не в хранилище, а в том,
//! кто собирает узел: ссылку читает ядро (D-031) и своего YAML туда не положить, а запись
//! `wireguard://` клиент собирает сам (D-063) и знает её целиком.
//!
//! Храним **разницу от собранного**, а не копию, — то же правило, что у D-036. Копия
//! заморозила бы ключи и адрес: подписка их сменит, а узел останется на старых и молча
//! перестанет работать. Убранный ключ хранится как `null`: иначе «удалить поле»
//! неотличимо от «не трогал».

use std::collections::BTreeMap;

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};
use crate::paths::Paths;

/// Разница для одной записи: ключ → значение, `null` — убрать ключ.
pub type Patch = Mapping;

/// Все правки записей одного источника, по тождеству узла (`link::identity_of`).
pub type Entries = BTreeMap<String, Patch>;

pub struct EntryPatch;

impl EntryPatch {
    pub fn load(source: &str) -> Entries {
        std::fs::read_to_string(Paths::source_entries(source))
            .ok()
            .and_then(|text| serde_yaml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(source: &str, entries: &Entries) -> Result<()> {
        Paths::ensure_sources_dir()?;
        let path = Paths::source_entries(source);
        if entries.is_empty() {
            // Пустой файл не нужен: отсутствие правок и есть отсутствие файла.
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        let text = serde_yaml::to_string(entries)
            .map_err(|e| AppError::io(format!("Не удалось записать правки узла: {e}")))?;
        Ok(crate::atomic::AtomicFile::write(path, text)?)
    }

    /// Чем правленая запись отличается от собранной. Пустая разница означает «не правили».
    pub fn diff(built: &Mapping, edited: &Mapping) -> Patch {
        let mut patch = Mapping::new();
        for (key, value) in edited {
            if built.get(key) != Some(value) {
                patch.insert(key.clone(), value.clone());
            }
        }
        for key in built.keys() {
            if !edited.contains_key(key) {
                patch.insert(key.clone(), Value::Null);
            }
        }
        patch
    }

    /// Наложить разницу на собранную запись.
    pub fn apply(patch: &Patch, built: &mut Mapping) {
        for (key, value) in patch {
            if value.is_null() {
                built.remove(key);
            } else {
                built.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, Value)]) -> Mapping {
        pairs
            .iter()
            .map(|(k, v)| (Value::from(*k), v.clone()))
            .collect()
    }

    #[test]
    fn only_the_difference_is_kept() {
        let built = map(&[
            ("name", Value::from("wg")),
            ("mtu", Value::from(1420)),
            ("udp", Value::from(true)),
        ]);
        let edited = map(&[
            ("name", Value::from("wg")),
            ("mtu", Value::from(1280)),
            ("udp", Value::from(true)),
            ("dialer-proxy", Value::from("sock")),
        ]);
        let patch = EntryPatch::diff(&built, &edited);
        assert_eq!(patch.len(), 2, "только изменённое и добавленное: {patch:?}");
        assert_eq!(patch.get(Value::from("mtu")), Some(&Value::from(1280)));
        assert_eq!(
            patch.get(Value::from("dialer-proxy")),
            Some(&Value::from("sock"))
        );
    }

    /// Убранный ключ обязан отличаться от нетронутого — иначе его не удалить.
    #[test]
    fn a_removed_key_is_remembered_as_null() {
        let built = map(&[("name", Value::from("wg")), ("mtu", Value::from(1420))]);
        let edited = map(&[("name", Value::from("wg"))]);
        let patch = EntryPatch::diff(&built, &edited);
        assert_eq!(patch.get(Value::from("mtu")), Some(&Value::Null));

        let mut again = built.clone();
        EntryPatch::apply(&patch, &mut again);
        assert_eq!(again, edited);
    }

    /// Ради этого правка и хранится разницей: свежий ключ с сервера должен доезжать.
    #[test]
    fn a_rotated_key_survives_the_patch() {
        let built = map(&[
            ("private-key", Value::from("старый")),
            ("mtu", Value::from(1420)),
        ]);
        let patch = EntryPatch::diff(
            &built,
            &map(&[
                ("private-key", Value::from("старый")),
                ("mtu", Value::from(1280)),
            ]),
        );

        let mut fresh = map(&[
            ("private-key", Value::from("новый")),
            ("mtu", Value::from(1420)),
        ]);
        EntryPatch::apply(&patch, &mut fresh);
        assert_eq!(
            fresh.get(Value::from("private-key")),
            Some(&Value::from("новый")),
            "ключ с сервера уцелел"
        );
        assert_eq!(fresh.get(Value::from("mtu")), Some(&Value::from(1280)));
    }

    #[test]
    fn an_untouched_entry_leaves_no_patch() {
        let built = map(&[("name", Value::from("wg")), ("mtu", Value::from(1420))]);
        assert!(EntryPatch::diff(&built, &built.clone()).is_empty());
    }
}
