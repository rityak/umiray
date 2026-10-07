//! Жизнь самого клиента: состояние окна, настройки на диске и то, что происходит вокруг
//! ядра, но не внутри него.
//!
//! Здесь нет ни разбора конфига, ни разговора с ядром — только приложение как таковое:
//! что оно помнит между запусками, что делает по расписанию и как переезжает со старой
//! раскладки.

pub mod boot;
pub mod catalog;
pub mod client;
pub mod connect;
pub mod data;
pub mod diagnostics;
pub mod engine;
pub mod groups;
pub mod guard;
pub mod import;
pub mod killswitch;
pub mod lifecycle;
pub mod lists;
pub mod maintenance;
pub mod measure;
pub mod migrate;
pub mod mihomo;
pub mod mode;
pub mod node_check;
pub mod notice;
pub mod presets;
pub mod proxy;
pub mod qd;
pub mod refresher;
pub mod routing;
pub mod settings;
pub mod sources;
pub mod state;
pub mod status;
pub mod tick;
pub mod tray;
pub mod updates;
pub mod wake;
