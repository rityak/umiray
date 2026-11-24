//! Дочерние процессы macOS живут в группе клиентского запуска и завершаются
//! штатным обработчиком выхода либо обработчиком сигналов (`system::session`).

use std::io;
use std::process::{Child, Command};

pub struct Job;

impl Job {
    pub fn spawn(mut command: Command) -> io::Result<Child> {
        command.spawn()
    }
}
