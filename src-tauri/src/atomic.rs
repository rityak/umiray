//! Замена файла целиком без промежуточного пустого состояния (D-136).

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Записать во временный файл рядом, вытолкнуть содержимое на диск и заменить цель.
/// Соседство обязательно: только внутри одного тома rename остаётся атомарным.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let (temporary, mut file) = temporary(path)?;
    let result = (|| {
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        drop(file);
        replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn temporary(path: &Path) -> io::Result<(PathBuf, std::fs::File)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    for _ in 0..16 {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).map_err(|why| io::Error::other(why.to_string()))?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let temporary = parent.join(format!(".{name}.{suffix}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(why) if why.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(why) => return Err(why),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "не удалось выбрать имя временного файла",
    ))
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_existing_file_is_replaced_whole() {
        let path =
            std::env::temp_dir().join(format!("umiray-atomic-{}.txt", crate::stamp::id().unwrap()));
        std::fs::write(&path, "before").unwrap();
        write(&path, "after").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
        std::fs::remove_file(path).unwrap();
    }
}
