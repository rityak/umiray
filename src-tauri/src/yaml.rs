//! Мелкая работа с `serde_yaml::Mapping`.
//!
//! Отдельный модуль, потому что одни и те же три операции нужны и конфигу, и разбору ссылок:
//! поставить поле, поставить только если его нет, достать вложенный маппинг.

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, Result};

/// Поставить поле, затерев прежнее значение.
pub fn set(map: &mut Mapping, key: &str, value: Value) {
    map.insert(Value::from(key), value);
}

/// Поставить поле, только если его ещё нет: так значение пользователя не затирается.
pub fn fill(map: &mut Mapping, key: &str, value: Value) {
    if !map.contains_key(Value::from(key)) {
        map.insert(Value::from(key), value);
    }
}

/// Вложенный маппинг по ключу, создавая его, если там пусто или лежит не маппинг.
pub fn sub<'a>(map: &'a mut Mapping, key: &str) -> &'a mut Mapping {
    let key = Value::from(key);
    if !map.get(&key).is_some_and(Value::is_mapping) {
        map.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    map.get_mut(&key).unwrap().as_mapping_mut().unwrap()
}

/// Верхний уровень документа как маппинг. Пустой документ — пустой маппинг, а не ошибка.
pub fn top_mapping(text: &str) -> Result<Mapping> {
    let parsed = serde_yaml::from_str(text).map_err(|e| AppError::invalid(format!("YAML: {e}")))?;
    match parsed {
        Value::Mapping(map) => Ok(map),
        Value::Null => Ok(Mapping::new()),
        _ => Err(AppError::invalid(
            "Конфиг должен быть набором полей, а не одиночным значением",
        )),
    }
}

/// Наложить один документ на другой. Маппинги сливаются вглубь, всё прочее заменяется.
///
/// Вглубь — потому что оверрайд обычно уточняет одно поле раздела: `tun: {stack: gvisor}`
/// не должен стирать `tun.enable`, поставленный слоем режима.
pub fn merge(base: &mut Mapping, over: Mapping) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Mapping(into)), Value::Mapping(from)) => merge(into, from),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}
