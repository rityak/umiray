//! Права на TUN под macOS: только ядро запускается с правами администратора.
//! Пароль или Touch ID запрашивает системный диалог `osascript` при подключении.

use std::path::Path;
use std::process::Command;

use crate::error::{AppError, Result};

pub struct Elevation;

impl Elevation {
    pub fn is_elevated() -> bool {
        Path::new("/usr/bin/osascript").is_file()
    }

    pub fn relaunch_as_admin() -> Result<()> {
        Err(AppError::NeedsElevation {
            message: Elevation::missing(),
        })
    }

    pub fn missing() -> String {
        "Для TUN нужно разрешить запуск ядра в системном диалоге администратора".into()
    }

    pub fn refused(_status: std::process::ExitStatus) -> Option<String> {
        None
    }

    pub fn privileged(binary: &Path, _device: &str) -> Command {
        let active = crate::paths::Paths::run_dir().join("privileged-core.active");
        let stop = crate::paths::Paths::run_dir().join("privileged-core.stop");
        let mut command = Command::new("/usr/bin/osascript");
        command
            .arg("-e")
            .arg(
                r#"on run argv
set appPID to item 1 of argv
set activeFile to item 2 of argv
set stopFile to item 3 of argv
set commandLine to ""
repeat with argumentIndex from 4 to count of argv
    set argumentValue to item argumentIndex of argv
    set commandLine to commandLine & quoted form of (argumentValue as text) & " "
end repeat
set cleanupCommand to "rm -f " & quoted form of activeFile & " " & quoted form of stopFile
set monitorCommand to "(while kill -0 $core_pid 2>/dev/null && kill -0 " & appPID & " 2>/dev/null && [ ! -e " & quoted form of stopFile & " ]; do sleep 0.2; done; if kill -0 $core_pid 2>/dev/null; then kill -TERM $core_pid; fi; " & cleanupCommand & ") & "
set commandToRun to "core_pid=$$; " & cleanupCommand & "; echo " & appPID & " > " & quoted form of activeFile & "; " & monitorCommand & "exec " & commandLine
do shell script commandToRun with administrator privileges
end run"#,
            )
            .arg("--")
            .arg(std::process::id().to_string())
            .arg(active)
            .arg(stop)
            .arg(binary);
        command
    }

    pub fn interrupt(pid: u32) -> bool {
        let stop = crate::paths::Paths::run_dir().join("privileged-core.stop");
        if crate::paths::Paths::run_dir()
            .join("privileged-core.active")
            .exists()
        {
            return std::fs::write(stop, pid.to_string()).is_ok();
        }
        // SAFETY: локальное ядро запущено в собственной группе процессов.
        unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM) == 0 }
    }
}
