//! Разговор копий клиента на Linux. Второй запуск передаёт аргументы работающей копии
//! через D-Bus плагина одиночного запуска, и этого хватает. Замены работающей копии новой
//! версией (B-027) нет: версию меняет пакет, и он же останавливает старую.

/// С этим аргументом копия просит работающую уступить место (B-027).
pub const REPLACE: &str = "--replace";

pub struct Instance;

impl Instance {
    pub fn asked_to_leave(args: &[String]) -> bool {
        args.iter().any(|arg| arg == REPLACE)
    }

    /// Канал копий открыт всем копиям одного пользователя — открывать нечего.
    pub fn hear_lower(_identifier: &str) -> bool {
        true
    }

    /// Другой копии, которую надо было бы просить уйти, при перезапуске на Linux нет:
    /// права даются ядру, а не клиенту (D-173).
    pub fn send_away(_identifier: &str, _wait: std::time::Duration) -> bool {
        true
    }
}
