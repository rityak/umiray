//! Кто мы для панели подписки: идентификатор устройства и его описание.
//!
//! Панель ограничивает число устройств по заголовку `x-hwid` и **без него отвечает 404**
//! (Remnawave). Формат значения панель проверяет: `[a-zA-Z0-9=-]{10,64}`, а стандарт XTLS
//! ограничивает длину 36 символами — MachineGuid укладывается ровно.

use crate::db::{Db, Table};
use crate::error::{AppError, Result};

/// Строка идентификатора в таблице состояния (D-170).
const ROW: &str = "hwid";

pub struct Device;

impl Device {
    /// Идентификатор устройства для подписок с привязкой (D-016, D-034).
    ///
    /// Идентификатор машины от системы (`Machine::id`: MachineGuid, на Linux — производное
    /// от `/etc/machine-id`). Он переживает переустановку
    /// клиента, поэтому повторное добавление подписки не съедает у провайдера ещё один слот;
    /// прежний случайный идентификатор умирал вместе с каталогом данных и съедал.
    ///
    /// Файл рядом — не источник истины, а слепок: если реестр недоступен, берём из него,
    /// иначе перезаписываем. Два разных значения означали бы два устройства в панели.
    pub fn hwid() -> Result<String> {
        let cached = Db::get(Table::State, ROW, "")
            .ok()
            .flatten()
            .map(|text| text.trim().to_string())
            .filter(|id| is_valid(id));

        let id = match machine_guid() {
            Some(id) => id,
            None => match cached.clone() {
                Some(id) => id,
                None => random()?,
            },
        };

        if cached.as_deref() != Some(id.as_str()) {
            Db::put(Table::State, ROW, "", &id)?;
        }
        Ok(id)
    }

    /// Описание устройства. Панели это не обязательно, но по нему она различает устройства
    /// в списке — человеку иначе не понять, какой слот чей.
    pub fn os_version() -> String {
        crate::system::machine::Machine::os_version()
    }

    /// Имя ОС для панели.
    pub fn os() -> &'static str {
        crate::system::machine::Machine::os()
    }
}

/// Панель проверяет значение регуляркой — мусор она отвергнет вместе со всей подпиской.
fn is_valid(id: &str) -> bool {
    (10..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '=' || c == '-')
}

fn machine_guid() -> Option<String> {
    crate::system::machine::Machine::id().filter(|id| is_valid(id))
}

/// Запасной вариант, если реестр недоступен: случайный идентификатор установки.
/// Он переживает перезапуск, но не переустановку — и это лучше, чем не работающая подписка.
fn random() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| AppError::io(format!("Не удалось создать идентификатор: {e}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_regex_is_what_we_check_against() {
        assert!(
            is_valid("4c4c4544-0043-4a10-8058-b7c04f4a3258"),
            "MachineGuid"
        );
        assert!(is_valid("9f1c2d3e4a5b6c7d8e9f0a1b2c3d4e5f"), "случайный");
        assert!(!is_valid("короткий"), "меньше десяти символов");
        assert!(!is_valid("с кириллицей и пробелом"), "не тот алфавит");
        assert!(!is_valid(&"a".repeat(65)), "длиннее шестидесяти четырёх");
    }

    /// Если реестр читается — значение обязано подойти панели без правок.
    #[test]
    fn the_machine_guid_is_shaped_the_way_the_panel_wants() {
        match machine_guid() {
            Some(id) => assert!(is_valid(&id), "MachineGuid не прошёл проверку"),
            None => println!("реестр недоступен — проверять нечего"),
        }
    }
}
