//! Запись в планировщике задач: как клиент поднимается с правами администратора (D-087).
//!
//! Права нельзя добавить работающему процессу — их даёт только тот, кто его запускает.
//! Способов ровно два, и мы выбрали второй:
//!
//! - флаг `RUNASADMIN` в реестре — Windows спрашивает UAC при **каждом** запуске,
//!   а при входе в систему просто не поднимает процесс: запись в `Run` с повышением
//!   молча не срабатывает. То есть «всегда от администратора» и «запускать вместе
//!   с Windows» оказались бы взаимоисключающими;
//! - **задача с наивысшими правами** — UAC не спрашивается вообще, и она же служит
//!   автозапуском. Цена: чтобы её завести, права нужны один раз — ровно в тот момент,
//!   когда предложение и всплывает (клиент уже запущен от администратора).
//!
//! Задача описывается XML, а не флагами `schtasks`: `/SC` требует даты и времени
//! в **локальном формате**, и на русской машине `/SD 01/01/2099` уже не разбирается.
//! В XML же можно вовсе не объявлять триггеров — это и есть «поднимать по требованию,
//! но не при входе».
//!
//! Файл XML обязан быть в UTF-16 с меткой порядка байтов: `schtasks` читает объявление
//! кодировки и на UTF-8 отвечает «неверный XML» (GOTCHAS).

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use crate::error::{AppError, Result};
use crate::paths;
use crate::system::elevation;

/// Имя задачи. Совпадает с названием приложения — по нему её и узнают в планировщике.
pub const NAME: &str = paths::APP_NAME;

/// Аргумент, с которым клиента запускает задача. Он же — предохранитель от петли:
/// увидев его, процесс больше не пытается перезапустить сам себя (см. `handoff`).
pub const LAUNCHED: &str = "--scheduled";

/// Не показывать окно консоли `schtasks`. Без этого каждое обращение к планировщику
/// мигало бы чёрным прямоугольником поверх интерфейса.
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

fn schtasks(args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new("schtasks.exe");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(NO_WINDOW);
    }
    command
        .output()
        .map_err(|e| AppError::io(format!("Не удалось вызвать планировщик задач: {e}")))
}

/// Что мы знаем о задаче. Оба признака приходят одним ответом, поэтому и лежат вместе:
/// спрашивать планировщик дважды ради двух булевых значений незачем.
#[derive(Debug, Clone)]
struct Known {
    exists: bool,
    at_logon: bool,
    /// Что задача запускает. Пусто — задачи нет или путь из её описания не вычитался.
    command: Option<String>,
}

/// Память об ответе планировщика.
///
/// Спросить его — это **запуск процесса**, а статус опрашивается раз в 1.5 с: без памяти
/// окно порождало бы `schtasks.exe` сорок раз в минуту. Забываем при каждой своей записи;
/// правку, сделанную руками в планировщике, увидим со следующего запуска клиента —
/// для факта, который меняют раз в жизни, этого достаточно.
static KNOWN: Mutex<Option<Known>> = Mutex::new(None);

fn known() -> Known {
    let mut cached = KNOWN.lock().unwrap();
    if let Some(known) = cached.as_ref() {
        return known.clone();
    }
    let fresh = query();
    *cached = Some(fresh.clone());
    fresh
}

/// Забыть ответ: мы только что его изменили. Видна всему крейту ради живой проверки:
/// она возвращает задачу пользователя мимо нас, и кеш об этом знать неоткуда.
pub(crate) fn forget() {
    *KNOWN.lock().unwrap() = None;
}

/// Ошибку считаем за «задачи нет»: показывать «включено», не сумев это подтвердить, —
/// врать (то же правило, что у автозапуска в реестре).
fn query() -> Known {
    let absent = Known {
        exists: false,
        at_logon: false,
        command: None,
    };
    let Ok(out) = schtasks(&["/Query", "/TN", NAME, "/XML"]) else {
        return absent;
    };
    if !out.status.success() {
        return absent;
    }
    Known {
        exists: true,
        // Ищем прямо в байтах, не переводя их в строку: в какой кодировке придёт вывод,
        // заранее неизвестно (см. `has_ascii`). Полноценный разбор XML ради одного
        // признака был бы лишним и там.
        at_logon: has_ascii(&out.stdout, TRIGGER),
        command: command_of(&out.stdout),
    }
}

/// Признак «поднимается при входе» в описании задачи. Имя тега XML — чистый ASCII,
/// и это единственное, на что здесь можно опереться.
const TRIGGER: &str = "<LogonTrigger>";

/// Есть ли в выводе ASCII-подстрока — в какой бы кодировке он ни пришёл.
///
/// Гадать про кодировку нельзя, и мы уже обожглись: `schtasks /Query /XML` **врёт про
/// себя** — в объявлении документа стоит `encoding="UTF-16"`, а байты приходят
/// однобайтовые, в кодировке консоли (GOTCHAS). Разбор как UTF-16 превращал их
/// в иероглифы, `<LogonTrigger>` не находился никогда, и «запускать вместе с Windows»
/// показывалось выключенным при заведённом триггере.
///
/// Искомое — имя тега, то есть ASCII, а значит в любой из двух раскладок оно лежит
/// байтами либо подряд, либо через нулевой. Этого достаточно, и кодировку знать не надо.
fn has_ascii(haystack: &[u8], needle: &str) -> bool {
    let wide: Vec<u8> = needle.bytes().flat_map(|byte| [byte, 0]).collect();
    let found = |pattern: &[u8]| {
        haystack
            .windows(pattern.len())
            .any(|slice| slice == pattern)
    };
    found(needle.as_bytes()) || found(&wide)
}

/// Путь к файлу, который запускает задача. Ищем в байтах по той же причине, что и триггер:
/// вывод приходит либо однобайтовым в кодировке консоли, либо UTF-16 — заранее неизвестно.
fn command_of(xml: &[u8]) -> Option<String> {
    let narrow = between(xml, b"<Command>", b"</Command>").map(|bytes| from_console(&bytes));
    let wide = || {
        let bytes = between(xml, &widen("<Command>"), &widen("</Command>"))?;
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        Some(String::from_utf16_lossy(&units))
    };
    let text = narrow.or_else(wide)?;
    let text = unescaped(text.trim().trim_matches('"'));
    (!text.is_empty()).then_some(text)
}

fn widen(text: &str) -> Vec<u8> {
    text.bytes().flat_map(|byte| [byte, 0]).collect()
}

fn between(haystack: &[u8], open: &[u8], close: &[u8]) -> Option<Vec<u8>> {
    let at = |from: usize, pattern: &[u8]| {
        haystack[from..]
            .windows(pattern.len())
            .position(|slice| slice == pattern)
            .map(|found| from + found)
    };
    let start = at(0, open)? + open.len();
    let end = at(start, close)?;
    Some(haystack[start..end].to_vec())
}

/// Обратное `escaped`: путь прошёл через разметку данными и возвращается тем же путём.
fn unescaped(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

/// Заведена ли задача. Именно заведена — работает она или нет, отвечает `usable`.
pub fn exists() -> bool {
    known().exists
}

/// Поднимет ли задача клиента на самом деле (B-015).
///
/// `schtasks /Run` докладывает об успехе и тогда, когда файла по `<Command>` больше нет:
/// планировщик берётся запустить, не находит и молча сдаётся. Клиент к этому моменту уже
/// вышел — окна нет, значка нет, сообщения нет. Поэтому «есть задача» и «задача работает»
/// с этого момента разные вопросы, и `handoff` спрашивает второй.
///
/// Путь **не вычитался** — считаем задачу рабочей: описание приходит в кодировке, которую
/// `schtasks` про себя же и путает (GOTCHAS), и ошибка разбора не повод выключить человеку
/// «всегда от администратора». Протухла — только когда путь есть и файла по нему нет.
pub fn usable() -> bool {
    let known = known();
    known.exists
        && !known
            .command
            .as_deref()
            .is_some_and(|exe| !Path::new(exe).exists())
}

/// Поднимает ли задача клиента при входе в систему.
pub fn at_logon() -> bool {
    known().at_logon
}

/// Привести задачу к желаемому виду.
///
/// `at_logon` — поднимать ли клиента при входе в систему. Без него задача остаётся,
/// но триггеров не имеет: запустить её можно только по требованию, чем и пользуется
/// `handoff`.
pub fn apply(at_logon: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    let path = paths::root().join("task.xml");
    paths::ensure_root()?;
    std::fs::write(&path, utf16(&document(&exe, at_logon)))?;

    let file = path.to_string_lossy().to_string();
    let out = schtasks(&["/Create", "/TN", NAME, "/XML", &file, "/F"])?;
    forget();
    // Файл — только переносчик: оставлять описание задачи рядом с конфигами незачем,
    // а на ошибке оно ещё и вводило бы в заблуждение.
    let _ = std::fs::remove_file(&path);

    if !out.status.success() {
        return Err(AppError::NeedsElevation {
            message: format!(
                "Не удалось завести задачу в планировщике: {}",
                reason(&out).unwrap_or_else(|| "нужны права администратора".into())
            ),
        });
    }
    Ok(())
}

/// Убрать задачу. Отсутствие — это успех: мы добивались именно того, чтобы её не было.
pub fn remove() -> Result<()> {
    if !exists() {
        return Ok(());
    }
    let out = schtasks(&["/Delete", "/TN", NAME, "/F"])?;
    forget();
    if !out.status.success() {
        return Err(AppError::NeedsElevation {
            message: format!(
                "Не удалось убрать задачу из планировщика: {}",
                reason(&out).unwrap_or_else(|| "нужны права администратора".into())
            ),
        });
    }
    Ok(())
}

/// Запустить задачу и тем самым поднять вторую копию — уже с правами.
///
/// UAC при этом не спрашивается: права даёт сама задача. Зовётся из `main`, до того
/// как поднимется окно, и вызывающий обязан после этого выйти.
pub fn run() -> Result<()> {
    let out = schtasks(&["/Run", "/TN", NAME])?;
    if out.status.success() {
        return Ok(());
    }
    Err(AppError::io(format!(
        "Задача в планировщике есть, но не запустилась: {}",
        reason(&out).unwrap_or_else(|| "причина неизвестна".into())
    )))
}

/// Отдать запуск задаче: клиент должен работать с правами, а работает без них.
///
/// Отвечает `true`, когда вторая копия поднята, — тогда этой пора выйти. Место вызова
/// обязано быть самым первым в `main`: вторая копия поднимется через мгновение, и если
/// эта успеет объявиться плагином одиночного запуска, новая просто покажет ей окно
/// и умрёт — то есть повышения так и не случится.
pub fn handoff() -> bool {
    // Предохранитель от петли. Запущенного задачей не перезапускаем **никогда** —
    // даже если прав почему-то не досталось: иначе клиент поднимал бы сам себя,
    // пока не кончится терпение у машины.
    if std::env::args().any(|arg| arg == LAUNCHED) {
        return false;
    }
    if elevation::is_elevated() || !usable() {
        return false;
    }
    // Метка — до запуска: повышенная копия может прочитать её раньше, чем `schtasks`
    // вернёт ответ. Не записалась — окно при `smart` не покажется, но клиент поднимется.
    let _ = paths::ensure_root().and_then(|()| std::fs::write(handed_mark(), b""));
    run().is_ok()
}

/// Метка «запуск передан руками» (D-129). Вход в систему и передача прав поднимают задачу
/// одной и той же командой, и повышенной копии больше не по чему их различить.
fn handed_mark() -> std::path::PathBuf {
    paths::root().join("handoff")
}

/// Сколько метка свежая. Повышенная копия встаёт за секунды; метка старше — от передачи,
/// которая не дошла или ушла в уже работающую копию.
const HANDED_FRESH: std::time::Duration = std::time::Duration::from_secs(30);

/// Передан ли этот запуск руками. Метку снимает в любом случае: оставленная, она выдала бы
/// следующий вход в систему за ручной запуск.
pub fn handed_over() -> bool {
    let mark = handed_mark();
    let fresh = std::fs::metadata(&mark)
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age < HANDED_FRESH));
    let _ = std::fs::remove_file(mark);
    fresh
}

/// Первая внятная строка ответа `schtasks`. Он пишет по-разному в stdout и stderr,
/// поэтому смотрим туда, где что-то есть.
fn reason(out: &std::process::Output) -> Option<String> {
    let text = from_console(if out.stderr.is_empty() {
        &out.stdout
    } else {
        &out.stderr
    });
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Вывод консольной программы строкой.
///
/// `schtasks` пишет в **кодировке консоли** — на русской машине это 866. `from_utf8_lossy`
/// превращал её в мозаику из ромбов, и вместо причины отказа пользователь видел кашу.
/// Перевести правильно умеет сама Windows, а кодовую страницу консоли она же и знает.
#[cfg(windows)]
fn from_console(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::MultiByteToWideChar;

    /// Кодовая страница консоли. Именно OEM, а не ANSI: у консоли и у окон Windows они
    /// разные (866 против 1251), и перепутать их — снова мозаика.
    const CP_OEMCP: u32 = 1;

    if bytes.is_empty() {
        return String::new();
    }
    let size = bytes.len() as i32;
    let length =
        unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), size, std::ptr::null_mut(), 0) };
    if length <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut buffer = vec![0u16; length as usize];
    unsafe {
        MultiByteToWideChar(
            CP_OEMCP,
            0,
            bytes.as_ptr(),
            size,
            buffer.as_mut_ptr(),
            length,
        );
    }
    String::from_utf16_lossy(&buffer)
}

#[cfg(not(windows))]
fn from_console(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// UTF-16 с меткой порядка байтов — то, чем описание задачи объявляет само себя,
/// и в этом виде планировщик его принимает. UTF-8 он отвергает, даже если так написано в объявлении (GOTCHAS).
pub(crate) fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// Экранирование для XML. Путь к программе приходит из системы, но пройти через разметку
/// он обязан как данные: `C:\Program Files (x86) & Co` — законное имя каталога.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Под кем работает задача. `DOMAIN\user` — то, что планировщик ждёт в `UserId`;
/// на машине вне домена доменом служит имя компьютера.
fn who() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
        _ => user,
    }
}

/// Описание задачи.
///
/// `MultipleInstancesPolicy = Parallel` — не недосмотр. Соблазн поставить `IgnoreNew`
/// велик, но тогда двойное нажатие по ярлыку при спрятанном в трей клиенте не делало бы
/// **ничего**: задача молча отказалась бы поднимать вторую копию, а показать окно первой
/// некому. С `Parallel` копия поднимается, плагин одиночного запуска показывает окно
/// работающей и тут же её гасит — то самое поведение, которого от ярлыка и ждут (D-046).
///
/// `ExecutionTimeLimit = PT0S` снимает лимит: клиент работает, пока его не выключат.
pub(crate) fn document(exe: &Path, at_logon: bool) -> String {
    let trigger = if at_logon {
        format!(
            "<LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger>",
            escaped(&who())
        )
    } else {
        String::new()
    };
    let directory = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>umiray — запуск VPN-клиента с правами администратора</Description>
  </RegistrationInfo>
  <Triggers>{trigger}</Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>false</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{arg}</Arguments>
      <WorkingDirectory>{dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        trigger = trigger,
        user = escaped(&who()),
        exe = escaped(&exe.to_string_lossy()),
        arg = LAUNCHED,
        dir = escaped(&directory.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ради этого правила `document` и существует отдельной функцией: без триггера
    /// задача не поднимает клиента при входе, а по требованию запускается — на этом
    /// и держится «всегда от администратора» без автозапуска.
    #[test]
    fn the_logon_trigger_is_the_only_difference_between_the_two_shapes() {
        let exe = Path::new(r"C:\Program Files\umiray\umiray.exe");
        let quiet = document(exe, false);
        let boot = document(exe, true);

        assert!(!quiet.contains("LogonTrigger"), "триггер без автозапуска");
        assert!(boot.contains("<LogonTrigger>"), "автозапуск без триггера");
        assert_eq!(
            quiet.replace("<Triggers></Triggers>", ""),
            boot.replace(&format!("<Triggers>{}</Triggers>", trigger_of(&boot)), ""),
            "формы обязаны различаться только триггером"
        );
    }

    /// B-015: задача, чей `<Command>` указывает в никуда, докладывает об успешном запуске
    /// и не запускает ничего. Читаем путь из её же описания — в обеих кодировках, потому
    /// что `schtasks` врёт про свою (GOTCHAS).
    #[test]
    fn the_command_of_a_task_is_read_in_either_encoding() {
        const EXE: &str = r"C:\Program Files\umiray & Co\umiray.exe";
        let xml = document(Path::new(EXE), false);
        assert_eq!(
            command_of(xml.as_bytes()).as_deref(),
            Some(EXE),
            "однобайтовый вывод и экранированный амперсанд"
        );
        assert_eq!(
            command_of(&utf16(&xml)).as_deref(),
            Some(EXE),
            "тот же путь из UTF-16"
        );
        assert_eq!(command_of(b"<Task></Task>"), None, "тега нет — и пути нет");
        assert_eq!(
            command_of(b"<Command></Command>"),
            None,
            "пустой тег не путь"
        );
    }

    fn trigger_of(document: &str) -> String {
        let start = document.find("<Triggers>").unwrap() + "<Triggers>".len();
        let end = document.find("</Triggers>").unwrap();
        document[start..end].to_string()
    }

    /// Права даёт `RunLevel`, а петлю от повторного перезапуска закрывает аргумент.
    /// Пропадёт любое из двух — и задача либо бесполезна, либо запускает клиента вечно.
    #[test]
    fn the_task_runs_elevated_and_marks_what_it_launched() {
        let document = document(Path::new(r"C:\umiray\umiray.exe"), true);
        assert!(document.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(document.contains(&format!("<Arguments>{LAUNCHED}</Arguments>")));
    }

    /// Путь приходит из системы, но через разметку идёт как данные: амперсанд в имени
    /// каталога — законный, а XML с ним разъезжается.
    #[test]
    fn a_path_with_markup_characters_stays_data() {
        let document = document(Path::new(r"C:\Tools & Co\umiray.exe"), false);
        assert!(document.contains(r"C:\Tools &amp; Co\umiray.exe"));
        assert!(!document.contains(r"C:\Tools & Co"));
    }

    /// Файл задачи пишется в UTF-16 с меткой порядка байтов: так объявлено в самом
    /// документе, и в этом виде планировщик его принимает.
    #[test]
    fn the_file_is_utf16_with_a_byte_order_mark() {
        let bytes = utf16("<Task/>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "нет метки порядка байтов");
        assert_eq!(&bytes[2..6], &[b'<', 0, b'T', 0], "не UTF-16 LE");
    }

    /// Та самая ошибка, из-за которой «запускать вместе с Windows» показывалось
    /// выключенным при заведённом триггере: вывод разбирался как UTF-16, а приходил
    /// однобайтовым. Теперь ищем в байтах — и находим в обеих раскладках.
    #[test]
    fn the_logon_trigger_is_found_whatever_the_output_encoding() {
        let with = "<Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>";
        let without = "<Triggers></Triggers>";

        assert!(has_ascii(with.as_bytes(), TRIGGER), "однобайтовый вывод");
        assert!(has_ascii(&utf16(with), TRIGGER), "вывод в UTF-16");
        assert!(!has_ascii(without.as_bytes(), TRIGGER));
        assert!(!has_ascii(&utf16(without), TRIGGER));
        // Пустой ответ — не «триггер есть»: слишком короткий срез не даёт ни одного окна.
        assert!(!has_ascii(b"", TRIGGER));
    }
}
