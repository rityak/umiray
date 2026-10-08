//! Кому достаётся этот запуск процесса: клиенту или тому, кому клиент передаёт себя.
//!
//! Зовётся первым делом в `main`, до Tauri. На Windows — задаче планировщика, которая
//! поднимет клиента с правами (D-087). На Linux — помощнику с правами root: тот же бинарь,
//! запущенный через pkexec (D-173).

pub struct Launch;

impl Launch {
    /// `true` — этот процесс своё сделал и должен выйти, не поднимая окна.
    pub fn handoff() -> bool {
        #[cfg(windows)]
        {
            crate::system::task::SchedulerTask::handoff()
        }
        #[cfg(target_os = "linux")]
        {
            let args: Vec<String> = std::env::args().collect();
            match crate::system::helper::Helper::serve(&args) {
                Some(code) => std::process::exit(code),
                None => false,
            }
        }
    }
}
