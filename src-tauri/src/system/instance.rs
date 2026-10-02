//! Как копии клиента говорят друг с другом: скрытое окно плагина одиночного запуска
//! (`tauri-plugin-single-instance`). Второй запуск передаёт через него свои аргументы,
//! новая версия — просьбу уступить место (B-027).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, FindWindowW, SendMessageW, MSGFLT_ALLOW, WM_COPYDATA,
};

/// С этим аргументом новая копия просит работающую уступить место (B-027): та выходит
/// штатно — гасит ядро и снимает системный прокси, как по «Выходу» в трее.
pub const REPLACE: &str = "--replace";

/// Метка сообщения плагина одиночного запуска: им же копии говорят друг с другом всегда.
const SINGLE_INSTANCE_DATA: usize = 1542;

pub struct Instance;

impl Instance {
    /// Новая копия просит эту уступить место (B-027).
    pub fn asked_to_leave(args: &[String]) -> bool {
        args.iter().any(|arg| arg == REPLACE)
    }

    /// Попросить работающую копию выйти — тем же сообщением, каким плагин передаёт ей
    /// аргументы второго запуска: `каталог|аргументы…`.
    pub fn ask_to_leave(identifier: &str) {
        let window = window(identifier);
        if window.is_null() {
            return;
        }
        let data = leave_message(&std::env::current_dir().unwrap_or_default());
        let message = COPYDATASTRUCT {
            dwData: SINGLE_INSTANCE_DATA,
            cbData: data.len() as u32,
            lpData: data.as_ptr() as *mut _,
        };
        unsafe {
            SendMessageW(window, WM_COPYDATA, 0, &message as *const _ as isize);
        }
    }

    /// Попросить работающую копию выйти и дождаться, пока её окно исчезнет: к этому моменту
    /// она погасила ядро и вернула прокси и брандмауэр. `true` — ушла или её не было.
    pub fn send_away(identifier: &str, wait: std::time::Duration) -> bool {
        Instance::ask_to_leave(identifier);
        let asked = std::time::Instant::now();
        while !window(identifier).is_null() {
            if asked.elapsed() > wait {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        true
    }

    /// Слышать просьбу уступить место и от копии без прав (B-030). Клиент «всегда
    /// от администратора» повышен, а новая версия запускается обычной: Windows не доставляет
    /// `WM_COPYDATA` процессу выше уровнем (UIPI), и замена молча не удавалась. Канал умеет
    /// только «покажи окно» и «выйди штатно» — открыть его ниже уровнем не страшно.
    pub fn hear_lower(identifier: &str) -> bool {
        let window = window(identifier);
        !window.is_null()
            && unsafe {
                ChangeWindowMessageFilterEx(window, WM_COPYDATA, MSGFLT_ALLOW, std::ptr::null_mut())
                    != 0
            }
    }
}

/// Скрытое окно плагина. Нет окна — пустой указатель: копии с этим `identifier` не работают.
fn window(identifier: &str) -> HWND {
    let class = wide(&format!("{identifier}-sic"));
    let title = wide(&format!("{identifier}-siw"));
    unsafe { FindWindowW(class.as_ptr(), title.as_ptr()) }
}

/// Текст сообщения: каталог, потом аргументы через `|`, в конце ноль — плагин читает его
/// как строку C и делит по `|`.
fn leave_message(cwd: &Path) -> String {
    format!("{}|umiray|{REPLACE}\0", cwd.to_string_lossy())
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Работающая копия получает аргументы так, как их разбирает плагин: первое — каталог,
    /// остальное — аргументы, и среди них просьба уйти.
    #[test]
    fn the_leave_request_reads_as_arguments_of_a_second_launch() {
        let data = leave_message(Path::new(r"C:\Users\me"));
        let text = data.strip_suffix('\0').expect("строка C кончается нулём");
        let mut parts = text.split('|');
        assert_eq!(parts.next(), Some(r"C:\Users\me"));
        let args: Vec<String> = parts.map(str::to_string).collect();
        assert!(Instance::asked_to_leave(&args), "{args:?}");
        assert!(!Instance::asked_to_leave(&["umiray".into()]));
    }

    /// Живая: настоящая отладочная копия получает просьбу и выходит штатно, с кодом 0 —
    /// то есть через `RunEvent::Exit`, который гасит ядро. Работающую копию закрывает.
    #[test]
    #[ignore]
    fn live_a_running_copy_leaves_when_asked() {
        const WAIT: std::time::Duration = std::time::Duration::from_secs(15);
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("umiray-dev.exe");
        assert!(exe.is_file(), "нет {exe:?} — сначала `cargo build`");
        let identifier = crate::app::boot::Boot::context()
            .config()
            .identifier
            .clone();
        let found = || !window(&identifier).is_null();

        // Своя копия, если её нет; уже работающая — тоже годится, просьба одна и та же.
        let mut own = (!found()).then(|| {
            std::process::Command::new(&exe)
                .arg("--scheduled")
                .spawn()
                .expect("копия не запустилась")
        });
        let up = std::time::Instant::now();
        while !found() && up.elapsed() < std::time::Duration::from_secs(30) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(found(), "окно одиночного запуска так и не появилось");

        Instance::ask_to_leave(&identifier);
        let asked = std::time::Instant::now();
        while found() && asked.elapsed() < WAIT {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if found() {
            // Своя копия не должна пережить провал: она держит вывод прогона открытым.
            if let Some(child) = own.as_mut() {
                let _ = child.kill();
            }
            panic!("копия не ушла за {WAIT:?}");
        }
        if let Some(child) = own.as_mut() {
            let status = child.wait().unwrap();
            assert!(status.success(), "выход не штатный: {status}");
        }
        println!("копия ушла за {:?}", asked.elapsed());
    }
}
