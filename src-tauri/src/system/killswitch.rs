//! Kill switch: пока он стоит, наружу выходит только трафик через адаптер ядра (D-073).
//!
//! Зачем вообще: замерено (S-015), что при смерти ядра с поднятым TUN адаптер исчезает
//! вместе с ним, и **через 1.6 секунды** машина уже видна миру своим настоящим адресом.
//! Подъём ядра заново (D-057) сокращает простой, но не закрывает утечку: между смертью
//! и подъёмом трафик идёт открыто. Закрыть её изнутри клиента нечем — нужен брандмауэр.
//!
//! Устройство. Брандмауэр включается, исходящее по умолчанию становится `Block`,
//! и рядом встают четыре разрешения:
//!
//! 1. **сам процесс ядра** — иначе оно не достучится до VPN-сервера, и туннель не поднимется
//!    вовсе. Правило адресует бинарь по пути: умер процесс — разрешать стало нечего;
//! 2. **адаптер ядра по имени** — то, ради чего всё и затевалось. Пропал адаптер (а он
//!    пропадает вместе с процессом) — правило перестаёт совпадать с чем бы то ни было,
//!    и трафик приложений упирается в общий запрет. Именно так «отказ» становится
//!    «закрыто», а не «открыто»;
//! 3. **сеть самого туннеля** — адреса, которые ядро раздаёт своему адаптеру;
//! 4. **локальная сеть** — принтер, роутер, NAS. В TUN она и так цела (S-002), и отнимать
//!    её вместе с интернетом мы не подряжались.
//!
//! **Брандмауэр приходится включать, и это выяснилось измерением** (B-012): на машине
//! разработки он был выключен целиком, и первая версия защиты не запирала ровным счётом
//! ничего — запрет стоял, но исполнять его было некому. Поэтому снимок помнит и `Enabled`:
//! выключить обратно можно только помня, что он был выключен.
//!
//! Почему не запрет на физических адаптерах, что кажется точнее: запрет в брандмауэре
//! Windows **сильнее разрешения**, поэтому такой запрет накрыл бы и само ядро — оно ходит
//! к серверу через ту же физическую карту. Разрешения работают только там, где запрет
//! стоит умолчанием, — отсюда и выбранное устройство.
//!
//! Цена принята осознанно и названа в окне: правило переживает падение клиента, и до
//! следующего запуска машина остаётся без интернета (локальная сеть цела). Лекарство
//! то же, что у системного прокси (D-047), — **снимаем при старте**, поэтому «починить»
//! значит просто открыть umiray ещё раз.

//!
//! На Linux то же самое — своя таблица nftables (D-173, S-035): исходящее по умолчанию
//! `drop`, разрешены адаптер ядра, метка исходящих самого ядра (`CORE_MARK`) и локальные
//! сети. Ставит и снимает её помощник с правами; снимок брандмауэра там не нужен —
//! таблица наша целиком, и снятие удаляет её всю.

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use crate::error::AppError;
use crate::error::Result;

/// Метка, которой ядро помечает свои исходящие (`routing-mark`), — по ней запрет его
/// и выпускает. Нужна только на Linux: nftables не умеют разрешать по пути к бинарю,
/// как брандмауэр Windows. Ставит её сборка конфига в TUN.
pub const CORE_MARK: Option<u32> = if cfg!(target_os = "linux") {
    Some(6666)
} else {
    None
};

#[cfg(any(windows, test))]
/// Общее начало имён наших правил. По нему же они и удаляются — в том числе оставшиеся
/// от прошлой жизни клиента, снимок которой потерян.
const PREFIX: &str = if cfg!(debug_assertions) {
    "umiray-dev killswitch"
} else {
    "umiray killswitch"
};

#[cfg(any(windows, test))]
/// Адреса самого туннеля. Ядро раздаёт их своему адаптеру, и трафик к ним обязан ходить
/// даже когда всё остальное закрыто.
const TUN_NET: &str = "198.18.0.0/15";

#[cfg(any(windows, test))]
/// Чем брандмауэр отвечает на «что у тебя стоит», пока никто ничего не настраивал.
/// **Не `Allow`**: у исходящего по умолчанию значение `NotConfigured`, и вернуть вместо него
/// `Allow` значило бы сделать явно настроенным то, что настроено не было.
const FACTORY: &str = "NotConfigured";

/// Что стояло в брандмауэре до нас: по профилю на строку, `Allow` или `Block`.
///
/// Хранится в настройках и переживает перезапуск — иначе после падения клиента было бы
/// неизвестно, наш ли это запрет, и «открыть на всякий случай» значило бы молча снять
/// чужую защиту.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    /// Профилей у Windows три, и заданы они бывают по-разному: у корпоративной машины
    /// `Domain` вполне может уже стоять в `Block`, а на этой брандмауэр был выключен вовсе.
    ///
    /// `default` обязателен: `settings::load` не падает никогда, а молча возвращает
    /// умолчания (D-024). Снимок, который не разобрался, унёс бы с собой **все** настройки
    /// и оставил машину запертой без единой записи о том, кто её запер.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Что мы разрешили: бинарь ядра и адаптер (D-073). Тот же приём, что `ours`
    /// у системного прокси: адаптер правят в «Настройках», и правила, разрешающие прежний,
    /// заперли бы машину при живом туннеле. Пусто — снимок прошлой сборки, сверять не с чем.
    #[serde(default)]
    pub allowed: Option<Allowed>,
}

/// Кого выпускают наши разрешения.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowed {
    pub core: String,
    pub device: String,
}

/// Профиль брандмауэра, каким он был до нас.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    /// Включён ли брандмауэр для этого профиля. Запоминать обязательно: **выключенный
    /// брандмауэр не запрещает ничего**, поэтому защите приходится его включать —
    /// а выключить обратно можно только помня, что он был выключен.
    pub enabled: String,
    /// `NotConfigured` · `Allow` · `Block`.
    pub action: String,
}

/// Запустить PowerShell с готовым текстом. Не `netsh`: его вывод переведён на язык системы,
/// и разбирать «Блокировать» против `Block` значило бы поставить защиту в зависимость
/// от локали. У командлетов `NetSecurity` значения — перечисление, одинаковое везде.
#[cfg(windows)]
fn powershell(script: &str) -> Result<String> {
    use std::os::windows::process::CommandExt;
    /// Без окна консоли: клиент оконный, и мигать чёрным прямоугольником посреди
    /// подключения он не должен.
    const NO_WINDOW: u32 = 0x0800_0000;

    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|why| AppError::io(format!("Не удалось вызвать PowerShell: {why}")))?;
    if !out.status.success() {
        return Err(AppError::io(format!(
            "Брандмауэр не принял команду: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub struct Firewall;

impl Firewall {
    /// Состояние профилей брандмауэра. Наружу — ради диагностики (D-097): «включён ли
    /// он вообще» спрашивают до того, как поверят галке защиты.
    pub fn profiles() -> Result<Vec<Profile>> {
        #[cfg(windows)]
        return read_profiles();
        // Профилей у nftables нет, а своя таблица исполняется всегда.
        #[cfg(not(windows))]
        Ok(Vec::new())
    }

    /// Поставить защиту. `core` — путь к бинарю ядра, `device` — имя его адаптера.
    ///
    /// Порядок обязателен и обратен интуиции: **сначала разрешения, потом запрет**. Поставь
    /// запрет первым — и между ним и первым разрешением есть окно, в котором машина уже без
    /// сети, а ядро ещё не выведено из-под него.
    /// Поставить защиту. `core` — путь к бинарю ядра, `device` — имя его адаптера.
    ///
    /// Порядок обязателен и обратен интуиции: **сначала разрешения, потом запрет**. Поставь
    /// запрет первым — и между ним и первым разрешением есть окно, в котором машина уже без
    /// сети, а ядро ещё не выведено из-под него.
    #[cfg(all(test, windows))]
    pub fn engage(core: &std::path::Path, device: &str) -> Result<Backup> {
        let previous = read_profiles()?;
        let allowed = Allowed {
            core: core.display().to_string(),
            device: device.to_string(),
        };
        if let Err(why) = Firewall::apply(&allowed) {
            // Полдороги хуже, чем ничего: разрешения без запрета бессмысленны, запрет без
            // разрешений отнимает сеть. Прибираем за собой и отдаём причину наверх.
            let _ = Firewall::release(&Backup {
                profiles: previous,
                allowed: None,
            });
            return Err(why);
        }
        Ok(Backup {
            profiles: previous,
            allowed: None,
        })
    }

    /// Поставить правила без нового снимка. Снимок хранит приложение до изменения ОС,
    /// поэтому reconnect не должен заменять его состоянием уже включённого запрета.
    pub fn apply(allowed: &Allowed) -> Result<()> {
        #[cfg(windows)]
        powershell(&script(&allowed.core, &allowed.device))?;
        #[cfg(target_os = "linux")]
        linux_apply(allowed)?;
        Ok(())
    }

    /// Заменить разрешения, не снимая запрета: адаптер или бинарь ядра сменились, а машина
    /// всё это время заперта (B-042). Новые встают раньше, чем уходят прежние, — окна, где
    /// новое ядро не выпущено, нет.
    pub fn renew(allowed: &Allowed) -> Result<()> {
        #[cfg(windows)]
        powershell(&renew_script(allowed))?;
        // Таблица nftables заменяется одним файлом — это уже замена без окна.
        #[cfg(target_os = "linux")]
        linux_apply(allowed)?;
        Ok(())
    }

    pub fn release(previous: &Backup) -> Result<()> {
        #[cfg(windows)]
        {
            let restore = restore_script(previous);
            powershell(&format!(
                "$ErrorActionPreference='Stop'
         {restore}
         Get-NetFirewallRule -DisplayName '{PREFIX}*' -ErrorAction SilentlyContinue |            Remove-NetFirewallRule"
            ))?;
        }
        #[cfg(target_os = "linux")]
        {
            let _ = previous;
            crate::system::helper::Helper::run(&["killswitch", "release"])?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn linux_apply(allowed: &Allowed) -> Result<()> {
    let mark = CORE_MARK.unwrap_or_default().to_string();
    crate::system::helper::Helper::run(&["killswitch", "apply", &allowed.device, &mark])
}

/// Прочитать состояние профилей — чтобы потом было к чему вернуться.
///
/// Читаем и `Enabled`, и `DefaultOutboundAction`: выключенный брандмауэр не запрещает
/// ничего, и защите приходится его включать. Замерено — на этой машине он был выключен,
/// и первая версия защиты из-за этого не запирала ровным счётом ничего (B-012).
#[cfg(windows)]
fn read_profiles() -> Result<Vec<Profile>> {
    let raw = powershell(
        "Get-NetFirewallProfile -All | ForEach-Object { \
         \"$($_.Name)=$($_.Enabled)=$($_.DefaultOutboundAction)\" }",
    )?;
    let profiles: Vec<Profile> = raw
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('=');
            Some(Profile {
                name: parts.next()?.to_string(),
                enabled: parts.next()?.to_string(),
                action: parts.next()?.to_string(),
            })
        })
        .collect();
    if profiles.is_empty() {
        return Err(AppError::io(
            "Брандмауэр не назвал ни одного профиля".to_string(),
        ));
    }
    Ok(profiles)
}

/// Текст, которым ставится защита. Отдельно от `engage` ради проверки: состав разрешений
/// **и есть** защита, и выпади из него правило по адаптеру — трафик приложений встал бы
/// намертво, а выпади правило ядра — туннель не поднялся бы вовсе. На живом брандмауэре
/// это стоило бы прогона с запертой машиной, здесь стоит одного сравнения строк.
#[cfg(any(windows, test))]
fn script(core: &str, device: &str) -> String {
    // Апостроф закрываем удвоением — так PowerShell экранирует кавычку внутри строки.
    // Имя адаптера пользователь пишет своей рукой в «Настройках» (`tun.device`),
    // и оно не должно превращаться в чужую команду.
    let core = core.replace('\'', "''");
    let device = device.replace('\'', "''");
    format!(
        "$ErrorActionPreference='Stop'
         New-NetFirewallRule -DisplayName '{PREFIX}: ядро' -Direction Outbound -Action Allow \
           -Program '{core}' | Out-Null
         New-NetFirewallRule -DisplayName '{PREFIX}: туннель' -Direction Outbound -Action Allow \
           -InterfaceAlias '{device}' | Out-Null
         New-NetFirewallRule -DisplayName '{PREFIX}: сеть туннеля' -Direction Outbound \
           -Action Allow -RemoteAddress '{TUN_NET}' | Out-Null
         New-NetFirewallRule -DisplayName '{PREFIX}: локальная сеть' -Direction Outbound \
           -Action Allow -RemoteAddress LocalSubnet | Out-Null
         Set-NetFirewallProfile -All -Enabled True -DefaultOutboundAction Block"
    )
}

/// Текст замены разрешений: прежние запоминаются, новые встают, прежние уходят. Запрет
/// стоит всё время — машина не открывается ни на миг (B-042).
#[cfg(any(windows, test))]
fn renew_script(allowed: &Allowed) -> String {
    format!(
        "$old = @(Get-NetFirewallRule -DisplayName '{PREFIX}*' -ErrorAction SilentlyContinue)
         {}
         $old | Remove-NetFirewallRule",
        script(&allowed.core, &allowed.device)
    )
}

/// Снять защиту и вернуть умолчание, каким оно было.
///
/// Вызывается при штатной остановке, при выходе и **при старте клиента** — последнее и есть
/// лекарство после падения: чтобы вернуть машине сеть, достаточно открыть umiray.
///
/// Порядок обратный тому, что в `engage`: сначала открываем умолчание, потом убираем
/// разрешения. Сорвётся второй шаг — останутся лишние разрешающие правила, а это
/// безобидно; обратный порядок оставил бы машину запертой.
/// Текст возврата. Отдельно от `release` по той же причине, что и `script`: это вторая
/// половина той же гарантии, и её тоже надо уметь проверить, не запирая машину.
#[cfg(any(windows, test))]
pub(crate) fn restore_script(previous: &Backup) -> String {
    let profiles = previous
        .profiles
        .iter()
        .map(|profile| {
            let Profile {
                name,
                enabled,
                action,
            } = profile;
            format!(
                "Set-NetFirewallProfile -Name {name} -Enabled {enabled} \
                 -DefaultOutboundAction {action}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n         ");
    // Снимка нет — значит он потерян вместе с настройками. Снимаем **только запрет**:
    // опасен он, а не включённый брандмауэр. Выключать брандмауэр вслепую нельзя — вдруг
    // он и был включён, и мы бы тихо сняли человеку защиту, которую не ставили.
    if profiles.is_empty() {
        format!("Set-NetFirewallProfile -All -DefaultOutboundAction {FACTORY}")
    } else {
        profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Снимок обязан читаться из настроек прошлой сборки, которая его не писала: иначе
    /// обновление клиента оставило бы машину запертой, а вернуть её было бы нечем.
    #[test]
    fn a_missing_backup_still_reads_and_means_factory_default() {
        let empty: Backup = serde_json::from_str("{}").expect("снимок без профилей — тоже снимок");
        assert!(empty.profiles.is_empty());
        assert_eq!(
            FACTORY, "NotConfigured",
            "заводское состояние исходящего — не Allow, замерено на живом брандмауэре"
        );
    }

    /// Состав защиты — не набор пожеланий, а её суть: убери правило по адаптеру, и трафик
    /// приложений встанет намертво при живом туннеле; убери правило ядра, и туннель
    /// не поднимется вовсе; убери запрет — защиты нет совсем. На живом брандмауэре каждая
    /// из этих ошибок стоила бы прогона с запертой машиной.
    #[test]
    fn the_protection_is_exactly_four_allowances_and_one_block() {
        let out = script(r"C:\umiray\mihomo.exe", "Meta");
        assert!(
            out.contains(r"-Program 'C:\umiray\mihomo.exe'"),
            "ядро наружу"
        );
        assert!(out.contains("-InterfaceAlias 'Meta'"), "адаптер ядра");
        assert!(out.contains(TUN_NET), "сеть туннеля");
        assert!(out.contains("-RemoteAddress LocalSubnet"), "локальная сеть");
        assert!(
            out.contains("-DefaultOutboundAction Block"),
            "без запрета разрешения ничего не защищают"
        );
        assert_eq!(
            out.matches("-Action Allow").count(),
            4,
            "разрешений ровно четыре: лишнее — это дыра"
        );
        assert!(
            out.rfind("-Action Allow") < out.find("-DefaultOutboundAction Block"),
            "сначала разрешения, потом запрет: иначе машина без сети раньше, чем ядро выведено"
        );
        // Выключенный брандмауэр не исполняет ничего: первая версия защиты стояла
        // и не запирала (B-012).
        assert!(
            out.contains("-Enabled True"),
            "защита обязана включить брандмауэр, иначе запрет некому исполнять"
        );
    }

    /// Возврат — вторая половина той же гарантии: он обязан вернуть **и** действие,
    /// **и** включённость. Вернёшь одно действие — брандмауэр останется включённым
    /// у того, кто его выключал; вернёшь одну включённость — запрет останется стоять.
    #[test]
    fn the_restore_gives_back_both_halves_of_every_profile() {
        let backup = Backup {
            profiles: vec![
                Profile {
                    name: "Domain".into(),
                    enabled: "False".into(),
                    action: "NotConfigured".into(),
                },
                Profile {
                    name: "Public".into(),
                    enabled: "True".into(),
                    action: "Block".into(),
                },
            ],
            allowed: None,
        };
        let out = restore_script(&backup);
        assert!(out.contains("-Name Domain -Enabled False -DefaultOutboundAction NotConfigured"));
        assert!(
            out.contains("-Name Public -Enabled True -DefaultOutboundAction Block"),
            "чужой запрет возвращаем как был: он не наш, и снимать его мы не вправе"
        );
    }

    /// Потерянный снимок — не повод оставить машину запертой, но и не повод трогать
    /// включённость: опасен запрет, а не включённый брандмауэр.
    #[test]
    fn a_lost_backup_lifts_the_block_and_nothing_else() {
        let out = restore_script(&Backup::default());
        assert!(out.contains(FACTORY), "запрет снят");
        assert!(
            !out.contains("-Enabled"),
            "включённость вслепую не трогаем: вдруг брандмауэр включали не мы"
        );
    }

    /// Имя адаптера приходит из `tun.device`, то есть из файла, который правит человек.
    /// Оттуда оно попадает прямо в текст скрипта.
    #[test]
    fn a_quote_in_the_device_name_cannot_break_out_of_the_script() {
        let out = script(r"C:\core.exe", r"Meta'; Remove-Item C:\ -Recurse; '");
        // Признак побега — закрывшаяся строка: `'Meta'` и сразу за ней команда.
        assert!(
            !out.contains("'Meta'; Remove"),
            "кавычка закрыла строку и впустила чужую команду:\n{out}"
        );
        // А признак порядка — что все литералы закрыты. Одиночная кавычка где угодно
        // сдвинула бы всё остальное внутрь строки или наружу из неё.
        assert_eq!(
            out.matches('\'').count() % 2,
            0,
            "кавычки не сбалансированы — часть скрипта прочтётся не как задумано:\n{out}"
        );
    }

    /// B-042: замена разрешений не открывает машину. Прежние правила запоминаются до новых
    /// и уходят после них, а запрет профиля не снимается вовсе.
    #[test]
    fn the_renewal_puts_new_allowances_before_removing_the_old() {
        let out = renew_script(&Allowed {
            core: r"C:\umiray\mihomo.exe".into(),
            device: "umiray".into(),
        });
        let remembered = out
            .find("$old = @(Get-NetFirewallRule")
            .expect("прежние запомнены");
        let first = out.find("New-NetFirewallRule").expect("новые встают");
        let removed = out
            .find("$old | Remove-NetFirewallRule")
            .expect("прежние уходят");
        assert!(remembered < first && out.rfind("New-NetFirewallRule") < Some(removed));
        assert!(out.contains("-InterfaceAlias 'umiray'"), "новый адаптер");
        assert!(
            !out.contains("NotConfigured"),
            "запрет не снимается:\n{out}"
        );
    }
}
