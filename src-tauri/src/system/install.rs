//! Клиент работает только из каталога данных (D-171): сборка, запущенная в другом месте,
//! кладёт себя туда и запускает копию.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
};

use crate::paths::Paths;
use crate::system::instance::Instance;

/// Сколько ждать, пока работающая копия выйдет: ей нужно погасить ядро.
const LEAVE_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

pub struct Installation;

impl Installation {
    /// Единственное место клиента (D-171). Задача планировщика и автозапуск указывают сюда же.
    pub fn installed_exe() -> PathBuf {
        Paths::client_exe()
    }

    pub fn is_installed(exe: &Path) -> bool {
        fs::canonicalize(exe).ok() == fs::canonicalize(Installation::installed_exe()).ok()
    }

    /// `identifier` — приложения (`tauri.conf.json`): по нему плагин одиночного запуска
    /// называет своё окно, и через него же работающую копию просят уступить место.
    ///
    /// Отвечает `true`, когда эта копия своё сделала и должна выйти.
    pub fn handoff(identifier: &str) -> bool {
        // Задача и проверки `tools/` запускают сборку на месте: перенос им не нужен.
        if std::env::args().any(|arg| arg == "--scheduled") {
            return false;
        }
        match launch_installed(identifier) {
            Ok(launched) => launched,
            Err(error) => {
                eprintln!("Cannot launch installed umiray: {error}");
                false
            }
        }
    }
}

fn launch_installed(identifier: &str) -> io::Result<bool> {
    let source = std::env::current_exe()?;
    let target = Installation::installed_exe();
    if Installation::is_installed(&source) {
        return Ok(false);
    }
    if cfg!(debug_assertions) {
        return launch_debug(identifier, &source, &target);
    }
    let installed = file_version(&target).ok();
    if should_copy(target.exists(), installed) {
        if let Err(error) = copy_to(&source, &target) {
            if !target.is_file() {
                return Err(error);
            }
            // Файл занят — работает установленная копия. Молча открыть её значило бы
            // показать старую версию вместо новой (B-027).
            if !replace_running(identifier, &source, &target, installed) {
                return Ok(true);
            }
        }
    }
    start(&target)?;
    Ok(true)
}

/// Debug: свежая сборка ложится без вопроса, как только файл другой (D-171), и запускатель
/// ждёт копию, держа её в своей клетке (D-058): `tauri dev` следит за ним и, убив его при
/// пересборке или по Ctrl+C, уносит и копию.
fn launch_debug(identifier: &str, source: &Path, target: &Path) -> io::Result<bool> {
    if differs(source, target)
        && copy_to(source, target).is_err()
        && !evict(identifier, source, target)
    {
        eprintln!(
            "Работающая отладочная копия не ушла за {LEAVE_WAIT:?}: закройте её через трей и запустите сборку снова."
        );
        return Ok(true);
    }
    let mut child = start(target)?;
    crate::system::job::Job::attach(child.as_raw_handle() as _);
    child.wait()?;
    // Копия ушла сама — возможно, передав запуск повышенной копии через UAC. Та сидит
    // в нашей клетке (её родителем Windows записывает копию) и умерла бы с нами.
    crate::system::job::Job::release();
    Ok(true)
}

fn start(target: &Path) -> io::Result<process::Child> {
    Command::new(target)
        .args(std::env::args_os().skip(1))
        .current_dir(target.parent().expect("installed exe has a directory"))
        .spawn()
}

/// Спросить, закрыть ли работающую копию, и положить новый файл. `false` — новую не
/// запускаем: человек согласился, а работающая не ушла — ему сказано, что делать.
/// Отказался — запускаем ту, что стоит: он так решил.
fn replace_running(
    identifier: &str,
    source: &Path,
    target: &Path,
    installed: Option<(u16, u16, u16, u16)>,
) -> bool {
    let old = installed
        .map(|(a, b, c, _)| format!("{a}.{b}.{c}"))
        .unwrap_or_else(|| "прежней версии".into());
    let question = format!(
        "Сейчас работает umiray {old}. Закрыть его и запустить {new}?\n\nПодключение прервётся на несколько секунд.",
        new = env!("CARGO_PKG_VERSION"),
    );
    if !ask(&question) {
        return true;
    }
    if evict(identifier, source, target) {
        return true;
    }
    tell(&format!(
        "Не получилось закрыть работающий umiray {old}. Закройте его через значок в трее («Выход») и запустите эту копию снова."
    ));
    false
}

/// Попросить работающую копию уйти и положить файл, как только он освободится.
fn evict(identifier: &str, source: &Path, target: &Path) -> bool {
    Instance::ask_to_leave(identifier);
    let started = std::time::Instant::now();
    while started.elapsed() < LEAVE_WAIT {
        if copy_to(source, target).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    false
}

/// Другая ли сборка лежит на месте: размер или время изменения. Копирование время
/// сохраняет, поэтому та же сборка второй раз не копируется.
fn differs(source: &Path, target: &Path) -> bool {
    let stamp = |path: &Path| {
        fs::metadata(path)
            .ok()
            .map(|meta| (meta.len(), meta.modified().ok()))
    };
    stamp(target).is_none() || stamp(source) != stamp(target)
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

/// Вопрос системным окном: своего у этой копии нет — она выходит до того, как оно поднимется.
fn ask(text: &str) -> bool {
    let text = wide(text);
    let title = wide("umiray");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}

fn tell(text: &str) {
    let text = wide(text);
    let title = wide("umiray");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
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

    /// Та же сборка второй раз не копируется — иначе каждый запуск отладочной сборки
    /// выгонял бы работающую копию; другая копируется всегда (D-171).
    #[test]
    fn only_another_build_is_copied_again() {
        let dir = std::env::temp_dir().join(format!("umiray-differs-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("umiray-dev.exe");
        let target = dir.join("installed").join("umiray-dev.exe");
        fs::write(&source, b"build one").unwrap();
        assert!(differs(&source, &target), "на месте ничего нет");
        copy_to(&source, &target).unwrap();
        assert!(!differs(&source, &target), "копирование сохранило время");
        fs::write(&source, b"build number two").unwrap();
        assert!(differs(&source, &target), "новая сборка");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_windows_file_version() {
        let notepad =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/notepad.exe");
        assert!(file_version(&notepad).unwrap().0 > 0);
    }
}
