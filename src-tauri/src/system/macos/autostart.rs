//! Автозапуск macOS через пользовательский LaunchAgent.

use std::path::PathBuf;

use crate::error::{AppError, Result};

pub const AT_LOGON: &str = "--autostart";
pub const RESTORE: &str = "--restore";

pub struct Autostart;

impl Autostart {
    pub fn by_system() -> bool {
        std::env::args().any(|arg| arg == AT_LOGON)
    }

    pub fn refresh() -> Result<()> {
        Ok(())
    }

    pub fn enabled() -> bool {
        file("").is_file()
    }

    pub fn always_admin() -> bool {
        false
    }

    pub fn set_always_admin(_on: bool) -> Result<()> {
        Err(AppError::invalid(
            "На macOS права даются ядру при каждом запуске — отдельной настройки нет",
        ))
    }

    pub fn set(on: bool) -> Result<()> {
        put("", on, AT_LOGON)
    }

    pub fn set_restore(on: bool) -> Result<()> {
        put("-restore", on, RESTORE)
    }
}

fn put(suffix: &str, on: bool, flag: &str) -> Result<()> {
    let path = &file(suffix);
    if !on {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let exe = std::env::current_exe()?;
    let label = path.file_stem().unwrap().to_string_lossy().to_string();
    std::fs::write(path, entry(&label, &exe.to_string_lossy(), flag))?;
    Ok(())
}

fn file(suffix: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let profile = if cfg!(debug_assertions) { ".dev" } else { "" };
    home.join("Library/LaunchAgents")
        .join(format!("com.umiray.client{profile}{suffix}.plist"))
}

fn entry(label: &str, exe: &str, flag: &str) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>{}</string>\n\
         <key>ProgramArguments</key><array>\n\
         <string>{}</string><string>{}</string>\n\
         </array>\n\
         <key>RunAtLoad</key><true/>\n\
         </dict></plist>\n",
        escape(label),
        escape(exe),
        escape(flag)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_agent_preserves_paths_and_escapes_xml() {
        let text = entry(
            "com.umiray.client",
            "/Applications/My & Apps/umiray",
            AT_LOGON,
        );
        assert!(text.contains("<string>/Applications/My &amp; Apps/umiray</string>"));
        assert!(text.contains("<string>--autostart</string>"));
        assert!(text.contains("<key>RunAtLoad</key><true/>"));
    }
}
