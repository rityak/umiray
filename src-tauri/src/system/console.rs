//! Мягкая остановка дочернего процесса: Ctrl+Break вместо расстрела (D-103, S-021).
//!
//! Зачем вообще: ядро сохраняет карту подменных адресов только при штатном выходе.
//! Убитое `/F` теряет её целиком, и после подъёма `198.18.0.4` достаётся тому, кто
//! спросил первым, — приложение, помнящее старый адрес, уезжает по чужому правилу
//! без единой ошибки (S-019, S-021).
//!
//! Почему так неудобно. Консольный сигнал доставляется **консоли**, а не процессу:
//! отправитель обязан быть к ней подключён. Ядро мы запускаем с `CREATE_NO_WINDOW` —
//! у него своя консоль, просто невидимая, — а клиент в релизе консоли не имеет вовсе.
//! Отсюда пляска: отцепиться от своей, прицепиться к его, послать сигнал, отцепиться.
//! Плюс `CREATE_NEW_PROCESS_GROUP` при запуске: без него сигнал ушёл бы всей группе,
//! то есть и нам.
//!
//! Цена, которую мы платим осознанно: в отладочной сборке у клиента **есть** своя
//! консоль, и после первой мягкой остановки она может быть потеряна — `eprintln!`
//! молчит (GOTCHAS). Молчит, а не пишет в чужой файл: слоты std после пляски
//! возвращаются или пустеют (`StdHandles`, B-054).
//!
//! На Linux всё это — один `SIGTERM`: сигнал адресуется процессу, а не консоли.

use std::process::Command;

/// Без этого флага при каждом запуске мигает окно консоли.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Своя группа процессов. Без неё Ctrl+Break получила бы и наша.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

pub struct Console;

impl Console {
    /// Дочерний процесс без окна консоли. `own_group` — в своей группе процессов: ради
    /// мягкой остановки на Windows (S-021) и чтобы Ctrl+C в терминале отладки не гасил
    /// ядро раньше клиента на Linux.
    pub fn hide(command: &mut Command, own_group: bool) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let group = if own_group {
                CREATE_NEW_PROCESS_GROUP
            } else {
                0
            };
            command.creation_flags(CREATE_NO_WINDOW | group);
        }
        #[cfg(unix)]
        if own_group {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
    }

    /// Попросить процесс выйти самому. `false` — не вышло попросить; звать `kill` всё равно
    /// придётся, эта функция только даёт шанс выйти по-человечески.
    #[cfg(windows)]
    pub fn interrupt(pid: u32) -> bool {
        use windows_sys::Win32::System::Console::{
            AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
            ATTACH_PARENT_PROCESS, CTRL_BREAK_EVENT,
        };
        let saved = StdHandles::save();
        let sent = unsafe {
            // Своя консоль мешает прицепиться к чужой. В релизе её нет, и вызов просто
            // вернёт ноль.
            FreeConsole();
            if AttachConsole(pid) == 0 {
                // Прицепиться не вышло — возвращаем себе то, что было, и сдаёмся.
                AttachConsole(ATTACH_PARENT_PROCESS);
                false
            } else {
                // Пока мы прицеплены, сигнал прилетит и нам: обработчик `NULL` с `TRUE`
                // велит его игнорировать. Иначе клиент погасил бы сам себя вместе с ядром.
                SetConsoleCtrlHandler(None, 1);
                let sent = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0;
                FreeConsole();
                SetConsoleCtrlHandler(None, 0);
                AttachConsole(ATTACH_PARENT_PROCESS);
                sent
            }
        };
        saved.restore();
        sent
    }
}

/// Слоты stdin/stdout/stderr вокруг пляски с консолью (B-054). `AttachConsole` кладёт
/// в них дескрипторы чужой консоли, `FreeConsole` их закрывает, а номера в слотах
/// остаются. Windows отдаёт освободившийся номер следующему открытому файлу — и `eprintln!`
/// пишет в него: так строка такта легла поверх заголовка `umiray.db`.
#[cfg(windows)]
struct StdHandles([Slot; 3]);

#[cfg(windows)]
struct Slot {
    id: u32,
    handle: windows_sys::Win32::Foundation::HANDLE,
    /// Узнать это можно только до пляски: закрытый дескриптор уже ни на что не отвечает.
    console: bool,
}

#[cfg(windows)]
impl StdHandles {
    fn save() -> Self {
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        Self(
            [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|id| {
                // SAFETY: только чтение слота.
                let handle = unsafe { GetStdHandle(id) };
                Slot {
                    id,
                    handle,
                    console: is_console(handle),
                }
            }),
        )
    }

    /// Файл, канал или пустоту `FreeConsole` не трогает — они возвращаются как были.
    /// Свой дескриптор консоли закрыт: остаётся то, что дал `ATTACH_PARENT_PROCESS`, если
    /// это живая консоль, иначе слот пуст — `eprintln!` молчит, но в чужой файл не пишет.
    fn restore(self) {
        use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle};
        for slot in self.0 {
            let keep = if !slot.console {
                slot.handle
            } else {
                // SAFETY: только чтение слота.
                let now = unsafe { GetStdHandle(slot.id) };
                if is_console(now) {
                    now
                } else {
                    std::ptr::null_mut()
                }
            };
            // SAFETY: запись своего слота.
            unsafe { SetStdHandle(slot.id, keep) };
        }
    }
}

/// Дескриптор живой консоли: на всё остальное `GetConsoleMode` отвечает отказом.
#[cfg(windows)]
fn is_console(handle: windows_sys::Win32::Foundation::HANDLE) -> bool {
    use windows_sys::Win32::System::Console::GetConsoleMode;
    let mut mode = 0;
    // SAFETY: только спрашивает; пустой или закрытый дескриптор вернёт ноль.
    !handle.is_null() && unsafe { GetConsoleMode(handle, &mut mode) } != 0
}

impl Console {
    #[cfg(target_os = "macos")]
    pub fn interrupt(pid: u32) -> bool {
        crate::system::elevation::Elevation::interrupt(pid)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn interrupt(pid: u32) -> bool {
        // SAFETY: только посылка сигнала; чужой pid просто вернёт ошибку.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    const SLOTS: [u32; 3] = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];

    /// Слоты прогона тестов возвращаются и после провала: через них отвечает сам `cargo test`.
    struct Restore([windows_sys::Win32::Foundation::HANDLE; 3]);

    impl Drop for Restore {
        fn drop(&mut self) {
            for (slot, handle) in SLOTS.into_iter().zip(self.0) {
                unsafe { SetStdHandle(slot, handle) };
            }
        }
    }

    /// B-054: после мягкой остановки stderr клиента указывал на закрытый дескриптор
    /// консоли ядра. Windows отдавала тот же номер следующему файлу — базе, — и строка
    /// такта ложилась поверх её заголовка.
    ///
    /// Слоты пусты, как у релиза из Проводника: GUI-процесс std не получает. Канал
    /// `cargo test` в слоте `AttachConsole` не трогает, и без этого тест зелёный и без фикса.
    #[test]
    fn a_soft_stop_leaves_no_stale_std_handle() {
        let _restore = Restore(SLOTS.map(|slot| unsafe { GetStdHandle(slot) }));
        for slot in SLOTS {
            unsafe { SetStdHandle(slot, std::ptr::null_mut()) };
        }
        let mut command = Command::new("ping");
        command.args(["-n", "30", "127.0.0.1"]);
        command.stdout(std::process::Stdio::null());
        Console::hide(&mut command, true);
        let mut child = command.spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            Console::interrupt(child.id()),
            "сигнал не ушёл — проверять нечего"
        );
        let _ = child.kill();
        let _ = child.wait();

        // Какой файл получит закрытый номер, зависит от всего, что процесс закрывал
        // и открывал рядом, — ловить совпадение ненадёжно. Проверяется сам слот: пустой,
        // как был, или живая консоль; закрытый номер — то, что раздаётся чужим файлам.
        let stale: Vec<isize> = SLOTS
            .map(|slot| unsafe { GetStdHandle(slot) })
            .into_iter()
            .filter(|handle| !handle.is_null() && !is_console(*handle))
            .map(|handle| handle as isize)
            .collect();
        assert!(
            stale.is_empty(),
            "в слотах std закрытые дескрипторы: {stale:?}"
        );
    }
}
