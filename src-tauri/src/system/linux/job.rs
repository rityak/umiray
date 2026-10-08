//! Клетка для дочерних процессов на Linux: что клиент породил, то с ним и умрёт (D-058).
//!
//! Job object здесь нет; его место занимает `PR_SET_PDEATHSIG` — ядро ОС само пошлёт
//! процессу `SIGTERM`, когда умрёт родитель. Своего кода при этом не исполняется, поэтому
//! клетка переживает и падение клиента, и `kill -9`.
//!
//! Подвох (S-035): «родитель» здесь — **нить**, которая звала `fork`, а не процесс. Нить
//! рантайма или `spawn_blocking` умирает раньше клиента — и ядро погасло бы посреди работы.
//! Поэтому все запуски идут через одну нить, живущую столько же, сколько клиент.
//!
//! Второй подвох: признак сбрасывается при `exec` бинаря с file capabilities и при смене
//! пользователя. Ядру в TUN права даёт помощник (`elevation`) — он ставит признак заново
//! сам, уже после смены прав (`setpriv --pdeathsig`).

use std::io;
use std::process::{Child, Command};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};

type Order = (Command, Sender<io::Result<Child>>);

/// Нить запусков: принимает команду, отдаёт процесс.
static SPAWNER: OnceLock<Mutex<Sender<Order>>> = OnceLock::new();

pub struct Job;

impl Job {
    /// Запустить так, чтобы процесс умер вместе с клиентом.
    pub fn spawn(mut command: Command) -> io::Result<Child> {
        use std::os::unix::process::CommandExt;
        // SAFETY: между fork и exec зовётся только prctl — он async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let (tell, heard) = mpsc::channel();
        spawner()
            .lock()
            .unwrap()
            .send((command, tell))
            .map_err(|_| io::Error::other("нить запусков не отвечает"))?;
        heard
            .recv()
            .map_err(|_| io::Error::other("нить запусков не отвечает"))?
    }
}

fn spawner() -> &'static Mutex<Sender<Order>> {
    SPAWNER.get_or_init(|| {
        let (orders, inbox) = mpsc::channel::<Order>();
        std::thread::Builder::new()
            .name("umiray-spawner".into())
            .spawn(move || {
                for (mut command, reply) in inbox {
                    let _ = reply.send(command.spawn());
                }
            })
            .expect("нить запусков не создалась");
        Mutex::new(orders)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    /// Признак ставится, и процесс запускается из нити, которая переживёт вызывающего.
    /// Что ребёнок правда умирает с родителем, проверяет живая проверка снаружи.
    #[test]
    fn a_child_starts_from_the_long_lived_thread() {
        let caller = std::thread::spawn(|| {
            let mut command = Command::new("sleep");
            command.arg("5").stdout(Stdio::null());
            Job::spawn(command).expect("sleep есть на любом linux")
        });
        let mut child = caller.join().unwrap();
        // Нить вызывающего уже умерла — а процесс жив: родитель у него нить запусков.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            child.try_wait().unwrap().is_none(),
            "процесс пережил вызвавшую нить"
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}
