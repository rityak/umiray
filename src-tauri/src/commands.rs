//! Граница с окном: тонкие обёртки над модулями, по файлу на домен (D-030).
//!
//! Команда разбирает аргументы и делает **один** вызов. Логика внутри команды означает,
//! что модуля не хватает, — так это правило и проверяется: файл, который начал считать
//! что-то сам, здесь сразу видно.

pub mod advanced;
pub mod client;
pub mod config;
pub mod connection;
pub mod core;
pub mod diag;
pub mod direction;
pub mod groups;
pub mod lists;
pub mod mode;
pub mod nodes;
pub mod presets;
pub mod qd;
pub mod rules;
pub mod rulesets;
pub mod settings;
pub mod sources;
pub mod system;
pub mod udp;
pub mod updates;

pub mod volt;
