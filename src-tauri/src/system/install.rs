//! Stable executable location (D-152). A build started elsewhere launches the installed copy.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};

use crate::paths::Paths;
use crate::system::registry::Registry;

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\umiray";

pub struct Installation;

impl Installation {
    pub fn installed_exe() -> PathBuf {
        let registered = Registry::read_string(UNINSTALL, "InstallLocation")
            .ok()
            .flatten()
            .map(|text| PathBuf::from(text.trim_matches('"')))
            .filter(|dir| dir.join("umiray.exe").is_file());
        registered.unwrap_or_else(Paths::root).join("umiray.exe")
    }

    pub fn is_installed(exe: &Path) -> bool {
        fs::canonicalize(exe).ok() == fs::canonicalize(Installation::installed_exe()).ok()
    }

    pub fn handoff() -> bool {
        if cfg!(debug_assertions) || std::env::args().any(|arg| arg == "--scheduled") {
            return false;
        }
        match launch_installed() {
            Ok(launched) => launched,
            Err(error) => {
                eprintln!("Cannot launch installed umiray: {error}");
                false
            }
        }
    }
}

fn launch_installed() -> io::Result<bool> {
    let source = std::env::current_exe()?;
    let target = Installation::installed_exe();
    if Installation::is_installed(&source) {
        return Ok(false);
    }
    let installed = file_version(&target).ok();
    if should_copy(target.exists(), installed) {
        if let Err(error) = copy_to(&source, &target) {
            // ponytail: a running installed exe cannot be replaced; retry on a later launch.
            if !target.is_file() {
                return Err(error);
            }
        }
    }
    Command::new(&target)
        .args(std::env::args_os().skip(1))
        .current_dir(target.parent().expect("installed exe has a directory"))
        .spawn()?;
    Ok(true)
}

fn should_copy(exists: bool, installed: Option<(u16, u16, u16, u16)>) -> bool {
    !exists || installed.is_some_and(|version| own_version() > version)
}

fn own_version() -> (u16, u16, u16, u16) {
    let mut parts = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|part| part.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        0,
    )
}

fn copy_to(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target.parent().expect("installed exe has a directory"))?;
    let staged = target.with_extension(format!("{}.tmp", process::id()));
    fs::copy(source, &staged)?;
    let result = fs::rename(&staged, target);
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn file_version(path: &Path) -> io::Result<(u16, u16, u16, u16)> {
    let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let mut handle = 0;
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle) };
    if size == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut data = vec![0u8; size as usize];
    let mut info = std::ptr::null_mut();
    let mut length = 0;
    let root = ['\\' as u16, 0];
    if unsafe {
        GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0
            || VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut info, &mut length) == 0
    } || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { std::ptr::read_unaligned(info.cast::<VS_FIXEDFILEINFO>()) };
    if info.dwSignature != 0xFEEF04BD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid version",
        ));
    }
    Ok((
        (info.dwFileVersionMS >> 16) as u16,
        info.dwFileVersionMS as u16,
        (info.dwFileVersionLS >> 16) as u16,
        info.dwFileVersionLS as u16,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_build_cannot_replace_the_installed_client() {
        assert!(!should_copy(true, Some(own_version())));
        assert!(!should_copy(true, Some((u16::MAX, 0, 0, 0))));
        assert!(should_copy(true, Some((0, 0, 0, 0))));
        assert!(should_copy(false, None));
    }

    #[test]
    fn a_new_build_replaces_the_old_file() {
        let dir = std::env::temp_dir().join(format!("umiray-install-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("new.exe");
        let target = dir.join("umiray.exe");
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        copy_to(&source, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_windows_file_version() {
        let notepad =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/notepad.exe");
        assert!(file_version(&notepad).unwrap().0 > 0);
    }
}
