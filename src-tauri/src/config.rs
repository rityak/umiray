//! Пользовательские документы и точечные настройки клиента.
//!
//! Сборка итогового конфига ядра живёт в `render::effective`: этот модуль не знает ни
//! каталог источников, ни формат результата.

pub mod advanced;
pub mod auto;
pub mod awg;
pub mod direction;
pub mod files;
pub mod groups;
pub mod mode;
pub mod presets;
pub mod recommended;
pub mod rename;
pub mod route;
pub mod route_doc;
pub mod rules;
pub mod rulesets;
pub mod udp;
