//! Разговор с Windows: реестр, системный прокси, автозапуск, права администратора.
//!
//! Всё, что знает про эту операционную систему, собрано здесь — чтобы при переносе
//! на другую было видно, что именно придётся написать заново.

pub mod autostart;
pub mod browser;
pub mod console;
pub mod elevation;
pub mod install;
pub mod job;
pub mod killswitch;
pub mod lang;
pub mod net;
pub mod pick;
pub mod process;
pub mod registry;
pub mod sysproxy;
pub mod task;
pub mod wake;
pub mod webview;
