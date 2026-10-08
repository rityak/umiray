//! Чем клиента поднимает система: запись в автозагрузке — или задача в планировщике,
//! если он должен работать с правами администратора (D-087).
//!
//! Состояние живёт **в системе, а не в наших настройках**. Причина та же, по которой
//! `corePresent` и `elevated` лежат в статусе, а не в `settings.json` (D-028): это факт
//! операционной системы, и хранить его копию значило бы иметь два источника истины.
//! Пользователь вправе убрать запись через диспетчер задач — окно обязано это показать,
//! а не настаивать на своём.
//!
//! Два способа запуска, и одновременно они не работают: если есть задача, запись
//! в `Run` подняла бы **вторую** копию без прав. Поэтому переключение одного всегда
//! перекладывает и другой — здесь, в одном месте, а не в двух командах.
//!
//! «Есть задача» здесь означает `SchedulerTask::usable`, а не `SchedulerTask::exists`: задача, чей файл
//! исчез, не поднимает ничего и прав не даёт (B-015). Для всех вопросов «кто сейчас
//! за запуск» такая задача — отсутствующая; убрать её из планировщика по-прежнему
//! можно, и «всегда от администратора» её же и перезаводит поверх.

use crate::error::Result;
use crate::system::registry::Registry;
use crate::system::task;
use crate::system::task::SchedulerTask;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Имя записи. Совпадает с названием приложения — по нему её и узнают в диспетчере задач.
const NAME: &str = crate::paths::APP_NAME;

/// Флаг, с которым клиента поднимает запись в `Run`: по нему `smart` узнаёт, что окно
/// показывать не надо (D-129).
pub const AT_LOGON: &str = "--autostart";

/// Флаг записи «вернуть сеть при входе» (D-175).
pub const RESTORE: &str = "--restore";

pub struct Autostart;

impl Autostart {
    /// ponytail: на Windows записи нет — залипший прокси снимает следующий запуск клиента.
    /// Окажется, что выход из сеанса его оставляет, — та же запись через `RunOnce`.
    pub fn set_restore(_on: bool) -> Result<()> {
        Ok(())
    }

    /// Подняла ли клиента система, а не человек (D-129).
    pub fn by_system() -> bool {
        let args: Vec<String> = std::env::args().collect();
        decide(&args, SchedulerTask::handed_over, SchedulerTask::at_logon)
    }

    /// Запись в `Run`, заведённая до D-129, флага не несёт — дописываем его. Путь при этом
    /// не трогаем: отладочная сборка иначе перевела бы на себя автозапуск установленного клиента.
    pub fn refresh() -> Result<()> {
        let Some(next) = Registry::read_string(RUN, NAME)?
            .as_deref()
            .and_then(with_flag)
        else {
            return Ok(());
        };
        // Запуском заведует задача — запись в `Run` тогда не наша забота (D-087).
        if SchedulerTask::usable() {
            return Ok(());
        }
        Registry::write_string(RUN, NAME, &next)
    }

    /// Стоит ли автозапуск сейчас. Ошибку чтения считаем за «нет»: показывать «включено»,
    /// не сумев это подтвердить, — врать.
    ///
    /// Спрашиваем у того, кто сейчас за запуск и отвечает: есть задача — у неё, нет —
    /// у реестра. Смотреть в оба места и складывать ответы значило бы показывать «включено»
    /// по забытой записи, которая всё равно не сработает.
    pub fn enabled() -> bool {
        if SchedulerTask::usable() {
            return SchedulerTask::at_logon();
        }
        in_registry()
    }

    /// Поднимается ли клиент с правами администратора всегда. Это и есть наличие задачи:
    /// другого её назначения нет.
    ///
    /// Спрашиваем `usable`, а не `exists`: задача, чей файл исчез, прав не даёт и клиента
    /// не поднимает (B-015). Показывать по ней «включено» — то же враньё, что и по забытой
    /// записи в реестре, только дороже: человек уверен, что TUN встанет без UAC.
    pub fn always_admin() -> bool {
        SchedulerTask::usable()
    }

    /// Переложить запуск с одного способа на другой, сохранив автозапуск как он есть.
    ///
    /// Права нужны, чтобы **завести или убрать** задачу, а не чтобы ей пользоваться.
    /// Отказ приходит как `NeedsElevation` — у окна на него уже есть кнопка (D-028).
    pub fn set_always_admin(on: bool) -> Result<()> {
        let at_logon = Autostart::enabled();
        if on {
            SchedulerTask::apply(at_logon)?;
            // Запись в реестре теперь лишняя и вредная: она подняла бы вторую копию,
            // и уже без прав.
            return Registry::delete_value(RUN, NAME);
        }
        SchedulerTask::remove()?;
        write_registry(at_logon)
    }

    pub fn set(on: bool) -> Result<()> {
        if SchedulerTask::usable() {
            // Задача уже есть — значит запуском заведует она, и автозапуск для неё
            // это наличие триггера, а не строчка в реестре.
            return SchedulerTask::apply(on);
        }
        // Протухшая задача (B-015) считается отсутствующей: она всё равно ничего не поднимет,
        // а требовать прав ради её починки там, где человек просил всего лишь автозапуск,
        // незачем. Починится сама, когда включат «всегда от администратора».
        write_registry(on)
    }
}

/// Разбор без побочных действий — ради теста. Метка и триггер спрашиваются лениво: первое
/// снимает файл, второе запускает `schtasks`, и без `--scheduled` не нужно ни то ни другое.
///
/// `--scheduled` без метки — ещё не вход в систему: с ним же руками поднимают отладочную
/// сборку на машине с задачей (`tools/*.mjs`). Автозапуском он считается, только
/// когда у задачи правда есть триггер входа.
fn decide(args: &[String], handed: impl FnOnce() -> bool, at_logon: impl FnOnce() -> bool) -> bool {
    if args.iter().any(|arg| arg == AT_LOGON) {
        return true;
    }
    if !args.iter().any(|arg| arg == task::LAUNCHED) {
        return false;
    }
    !handed() && at_logon()
}

/// Команда с флагом автозапуска. `None` — флаг уже есть или записи нет вовсе.
fn with_flag(value: &str) -> Option<String> {
    let value = value.trim_end();
    if value.is_empty() || value.split_whitespace().any(|part| part == AT_LOGON) {
        return None;
    }
    Some(format!("{value} {AT_LOGON}"))
}

fn in_registry() -> bool {
    matches!(Registry::read_string(RUN, NAME), Ok(Some(value)) if !value.is_empty())
}

/// Строка команды для автозагрузки.
///
/// Кавычки обязательны: путь по умолчанию содержит пробел («Program Files»), и без них
/// система прочитает `C:\Program` как команду, а `Files\umiray\umiray.exe` — как аргумент.
fn command(exe: &std::path::Path) -> String {
    format!("\"{}\" {AT_LOGON}", exe.display())
}

fn write_registry(on: bool) -> Result<()> {
    if !on {
        return Registry::delete_value(RUN, NAME);
    }
    // Запись указывает на место клиента (D-171), а не на того, кто её пишет.
    Registry::write_string(
        RUN,
        NAME,
        &command(&crate::system::install::Installation::installed_exe()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ради этого правила `command` и существует отдельной функцией: путь с пробелом
    /// без кавычек Windows читает как команду с аргументом и запускает не то.
    #[test]
    fn a_path_with_spaces_stays_one_argument() {
        let path = std::path::Path::new(r"C:\Program Files\umiray\umiray.exe");
        assert_eq!(
            command(path),
            r#""C:\Program Files\umiray\umiray.exe" --autostart"#
        );
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    /// Четыре случая D-129. Замыкание, которое звать нельзя, падает: так видно, что разбор
    /// не трогает метку и планировщик там, где ответ ясен без них.
    #[test]
    fn who_started_the_client() {
        let never = || -> bool { panic!("спрашивать не нужно") };
        assert!(decide(&args(&["umiray.exe", "--autostart"]), never, never));
        assert!(!decide(&args(&["umiray.exe"]), never, never), "ярлык");
        assert!(
            !decide(&args(&["umiray.exe", "--scheduled"]), || true, never),
            "передача прав оставила метку — это ярлык"
        );
        assert!(
            decide(&args(&["umiray.exe", "--scheduled"]), || false, || true),
            "задача без метки и с триггером входа — вход в систему"
        );
        assert!(
            !decide(&args(&["umiray.exe", "--scheduled"]), || false, || false),
            "без триггера входа `--scheduled` поставил человек — отладочный запуск"
        );
    }

    #[test]
    fn an_old_run_entry_gets_the_flag_and_keeps_its_path() {
        let old = r#""C:\Program Files\umiray\umiray.exe""#;
        assert_eq!(
            with_flag(old).as_deref(),
            Some(r#""C:\Program Files\umiray\umiray.exe" --autostart"#)
        );
        assert_eq!(
            with_flag(&with_flag(old).unwrap()),
            None,
            "второй раз не дописывает"
        );
        assert_eq!(with_flag(""), None);
    }
}
