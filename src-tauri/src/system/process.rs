//! Processes running on Windows: names for PROCESS-NAME rules (D-153) and killing an
//! orphaned core by its full path (D-059, D-154).

use std::path::Path;

use serde::Serialize;

use crate::error::Result;

#[derive(Serialize)]
pub struct Process {
    pub name: String,
}

pub struct ProcessTable;

impl ProcessTable {
    #[cfg(windows)]
    pub fn running() -> Result<Vec<Process>> {
        let names: std::collections::BTreeSet<String> =
            snapshot()?.into_iter().map(|(_, name)| name).collect();
        Ok(names.into_iter().map(|name| Process { name }).collect())
    }

    #[cfg(not(windows))]
    pub fn running() -> Result<Vec<Process>> {
        Ok(Vec::new())
    }

    /// Kill every process started from this very file. By path, not by name: a process with
    /// the same file name elsewhere is someone else's — qd has a standalone client (D-154).
    /// An elevated process we have no rights to stays; `taskkill` could not do more either.
    #[cfg(windows)]
    pub fn kill_by_path(path: &Path) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };

        let (Ok(wanted), Some(name)) = (std::fs::canonicalize(path), path.file_name()) else {
            return;
        };
        let name = name.to_string_lossy();
        let Ok(processes) = snapshot() else {
            return;
        };
        for (pid, exe) in processes {
            if pid == std::process::id() || !exe.eq_ignore_ascii_case(&name) {
                continue;
            }
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                    0,
                    pid,
                )
            };
            if handle.is_null() {
                continue;
            }
            if image(handle).and_then(|image| std::fs::canonicalize(image).ok())
                == Some(wanted.clone())
            {
                unsafe { TerminateProcess(handle, 1) };
            }
            unsafe { CloseHandle(handle) };
        }
    }

    #[cfg(not(windows))]
    pub fn kill_by_path(_path: &Path) {}
}

/// Full path of the process's executable.
#[cfg(windows)]
fn image(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;

    let mut buffer = [0u16; 32_768];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
    (ok != 0).then(|| std::ffi::OsString::from_wide(&buffer[..length as usize]).into())
}

/// Every running process: its id and executable file name.
#[cfg(windows)]
fn snapshot() -> Result<Vec<(u32, String)>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = Vec::new();
    if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if !name.is_empty() {
                found.push((entry.th32ProcessID, name));
            }
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snapshot) };
    Ok(found)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn running_processes_include_this_test() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy();
        assert!(super::ProcessTable::running()
            .unwrap()
            .iter()
            .any(|process| process.name.eq_ignore_ascii_case(&name)));
    }

    /// The whole point of going by path: a process of the same name started from another
    /// file survives, the one from our file does not.
    #[test]
    fn only_a_process_from_this_very_file_is_killed() {
        let dir = std::env::temp_dir().join(format!("umiray-sweep-{}", std::process::id()));
        let ours = dir.join("ours");
        let theirs = dir.join("theirs");
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::create_dir_all(&theirs).unwrap();
        let ping = std::path::Path::new(r"C:\Windows\System32\PING.EXE");
        let (a, b) = (ours.join("PING.EXE"), theirs.join("PING.EXE"));
        std::fs::copy(ping, &a).unwrap();
        std::fs::copy(ping, &b).unwrap();
        let spawn = |exe: &std::path::Path| {
            std::process::Command::new(exe)
                .args(["-n", "30", "127.0.0.1"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        };
        let (mut mine, mut other) = (spawn(&a), spawn(&b));

        super::ProcessTable::kill_by_path(&a);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while mine.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mine_gone = mine.try_wait().unwrap().is_some();
        let other_alive = other.try_wait().unwrap().is_none();
        let _ = mine.kill();
        let _ = other.kill();
        let _ = mine.wait();
        let _ = other.wait();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(mine_gone, "процесс из нашего файла пережил уборку");
        assert!(other_alive, "уборка задела одноимённый чужой процесс");
    }
}
