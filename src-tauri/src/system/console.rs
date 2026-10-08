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
//! консоль, и после первой мягкой остановки она потеряна — `eprintln!` уходит в никуда
//! (GOTCHAS). В релизе терять нечего.
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
        unsafe {
            // Своя консоль мешает прицепиться к чужой. В релизе её нет, и вызов просто
            // вернёт ноль.
            FreeConsole();
            if AttachConsole(pid) == 0 {
                // Прицепиться не вышло — возвращаем себе то, что было, и сдаёмся.
                AttachConsole(ATTACH_PARENT_PROCESS);
                return false;
            }
            // Пока мы прицеплены, сигнал прилетит и нам: обработчик `NULL` с `TRUE` велит
            // его игнорировать. Иначе клиент погасил бы сам себя вместе с ядром.
            SetConsoleCtrlHandler(None, 1);
            let sent = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0;
            FreeConsole();
            SetConsoleCtrlHandler(None, 0);
            AttachConsole(ATTACH_PARENT_PROCESS);
            sent
        }
    }

    #[cfg(unix)]
    pub fn interrupt(pid: u32) -> bool {
        // SAFETY: только посылка сигнала; чужой pid просто вернёт ошибку.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
    }
}
