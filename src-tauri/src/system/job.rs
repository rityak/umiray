//! Клетка для дочерних процессов: что клиент породил, то с ним и умрёт (D-058).
//!
//! `RunEvent::Exit` гасит ядро только при штатном выходе. Паника, `taskkill /F`, перезапуск
//! бинаря дев-сервером обработчиков не вызывают — и `mihomo.exe` остаётся жить, держа порт
//! (GOTCHAS). Здесь этим занимается не наш код, а ядро Windows: пока процесс клиента держит
//! ручку job object с `KILL_ON_JOB_CLOSE`, всё, что внутри, живо; закрылась ручка — убито.
//! Своего кода при этом не исполняется вовсе, поэтому клетка переживает и падение, и `/F`.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// Ручку держим до конца процесса и намеренно не закрываем: её закрытие и есть сигнал
/// «убить всех внутри». `usize`, потому что `HANDLE` — сырой указатель и `Sync` не является.
static JOB: OnceLock<usize> = OnceLock::new();

pub struct Job;

impl Job {
    /// Посадить процесс в клетку. `false` — клетки нет, и ядро переживёт падение клиента:
    /// вызывающий обязан сказать об этом вслух, а не молча продолжить.
    pub fn attach(process: HANDLE) -> bool {
        let job = *JOB.get_or_init(create) as HANDLE;
        if job.is_null() {
            return false;
        }
        unsafe { AssignProcessToJobObject(job, process) != 0 }
    }

    /// Отпустить всех, кто в клетке: выход владельца их больше не убьёт. Нужно отладочному
    /// запускателю (D-171), когда копия ушла сама: процесс, поднятый ею через UAC, Windows
    /// сажает в клетку родителя, и без этого он умирал бы вместе с запускателем.
    pub fn release() {
        let Some(job) = JOB.get().map(|job| *job as HANDLE) else {
            return;
        };
        unsafe {
            let limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
        }
    }
}

/// Ноль — «не получилось»: клетка на этой системе не создаётся, и второй раз пробовать
/// незачем.
fn create() -> usize {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if set == 0 {
            // Клетка без флага бесполезна: она никого не убьёт, а процессы соберёт.
            CloseHandle(job);
            return 0;
        }
        job as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};

    /// Что клетка действительно убивает, проверяется целиком снаружи
    /// (`node tools/crash-recovery.mjs`): для этого нужен второй процесс, который умрёт.
    /// Здесь — что она вообще создаётся и принимает процесс: ошибись мы во флагах или
    /// в размере структуры, `attach` вернул бы `false` уже тут.
    #[test]
    fn a_live_process_goes_into_the_cage() {
        let mut child = Command::new("cmd")
            .args(["/c", "ping", "-n", "5", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .expect("cmd есть на любой windows");
        assert!(Job::attach(child.as_raw_handle()), "клетка приняла процесс");
        let _ = child.kill();
        let _ = child.wait();
    }
}
