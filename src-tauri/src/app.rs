//! Жизнь самого клиента: состояние окна, настройки на диске и то, что происходит вокруг
//! ядра, но не внутри него.
//!
//! Здесь нет ни разбора конфига, ни разговора с ядром — только приложение как таковое:
//! что оно помнит между запусками, что делает по расписанию и как переезжает со старой
//! раскладки.

pub mod boot;
pub mod client;
pub mod connect;
pub mod data;
pub mod guard;
pub mod lifecycle;
pub mod measure;
pub mod migrate;
pub mod mode;
pub mod notice;
pub mod refresher;
pub mod reset;
pub mod settings;
pub mod state;
pub mod status;
pub mod tick;
pub mod tray;
pub mod updates;
pub mod wake;
