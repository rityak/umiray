//! Второй запуск передаёт аргументы через плагин одиночного запуска Tauri.

pub const REPLACE: &str = "--replace";

pub struct Instance;

impl Instance {
    pub fn asked_to_leave(args: &[String]) -> bool {
        args.iter().any(|arg| arg == REPLACE)
    }

    pub fn hear_lower(_identifier: &str) -> bool {
        true
    }

    pub fn send_away(_identifier: &str, _wait: std::time::Duration) -> bool {
        true
    }
}
