//! Разговор с операционной системой: прокси, автозапуск, права, брандмауэр, процессы.
//!
//! Всё, что знает про ОС, собрано здесь (D-173): выше этого слоя нет ни `windows_sys`,
//! ни `libc`, ни имени системы. У каждого модуля один договор; что делает каждая ОС,
//! решает `#[cfg]` внутри модуля или его файл в `system/linux/` — для модулей, у которых
//! на Linux нет ничего общего с Windows.
//!
//! Чего ОС хорошо не умеет, окно не показывает — список в `features` (D-174).

#[cfg_attr(target_os = "linux", path = "system/linux/autostart.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/autostart.rs")]
pub mod autostart;
pub mod browser;
pub mod console;
#[cfg_attr(target_os = "linux", path = "system/linux/elevation.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/elevation.rs")]
pub mod elevation;
pub mod features;
#[cfg(target_os = "linux")]
#[path = "system/linux/helper.rs"]
mod helper;
#[cfg_attr(target_os = "linux", path = "system/linux/install.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/install.rs")]
pub mod install;
#[cfg_attr(target_os = "linux", path = "system/linux/instance.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/instance.rs")]
pub mod instance;
#[cfg_attr(target_os = "linux", path = "system/linux/job.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/job.rs")]
pub mod job;
pub mod killswitch;
pub mod lang;
pub mod launch;
pub mod machine;
pub mod net;
pub mod pick;
pub mod process;
#[cfg(windows)]
pub mod registry;
pub mod session;
#[cfg_attr(target_os = "linux", path = "system/linux/sysproxy.rs")]
#[cfg_attr(target_os = "macos", path = "system/macos/sysproxy.rs")]
pub mod sysproxy;
#[cfg(windows)]
pub mod task;
pub mod taskbar;
pub mod wake;
pub mod webview;
