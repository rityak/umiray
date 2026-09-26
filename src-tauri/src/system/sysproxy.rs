//! Системный прокси Windows: включить, выключить, прибрать за собой (D-047).
//!
//! D-008 отказывался это трогать, и его довод верен: запись в реестр переживает падение
//! клиента, и появляется класс багов «прокси залип». Отменён не довод, а вывод — потому
//! что без этого режим Local Proxy требует, чтобы пользователь сам прописал адрес,
//! и «Подключено» в окне не означает, что трафик идёт.
//!
//! Лекарство от залипания здесь одно и оно обязательное: **прибираемся при каждом старте**,
//! а не только при аккуратном выходе. Если в реестре остался наш адрес — снимаем.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::system::registry;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// Локальные адреса мимо прокси: без этого само окно не достучится до `external-controller`,
/// а он слушает на 127.0.0.1.
const BYPASS: &str = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;\
172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;\
172.31.*;192.168.*;<local>";

/// Что стояло в реестре до нас и что мы записали вместо этого.
///
/// Прежнюю настройку храним, чтобы вернуть её, а не стереть: у пользователя вполне может
/// быть **другой** VPN в режиме системного прокси, и «выключить наш» не значит
/// «выключить любой».
///
/// Свой адрес храним ради обратного случая: пока мы работали, прокси мог сменить кто-то
/// третий. Тогда возвращать «как было до нас» уже нельзя — это затрёт чужую свежую
/// настройку нашей несвежей.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub enabled: bool,
    pub server: String,
    /// Адрес, который записали мы. Пусто — у записи из старой сборки, тогда проверить
    /// нечем и мы возвращаем как было (прежнее поведение).
    #[serde(default)]
    pub ours: String,
    /// Список исключений до нас. `None` — значения не было вовсе (или снимок оставлен
    /// сборкой, которая его не помнила): тогда возврат его удаляет, а не оставляет наш.
    /// Хранить обязательно: `enable` его перезаписывает, и без снимка свой список
    /// исключений пользователь теряет навсегда (B-011).
    #[serde(default)]
    pub bypass: Option<String>,
}

/// Наш ли адрес сейчас в реестре. По этому признаку и убираем за собой после падения:
/// чужой прокси трогать нельзя, свой — обязаны.
pub fn is_ours(address: &str) -> bool {
    matches!(read(), Ok(current) if current.enabled && current.server == address)
}

/// Прочитать текущее состояние — чтобы потом было к чему вернуться.
pub fn read() -> Result<Backup> {
    Ok(Backup {
        enabled: registry::read_dword(KEY, "ProxyEnable")?.unwrap_or(0) == 1,
        server: registry::read_string(KEY, "ProxyServer")?.unwrap_or_default(),
        // Заполняет `enable`: здесь мы только читаем чужое состояние.
        ours: String::new(),
        bypass: registry::read_string(KEY, "ProxyOverride")?,
    })
}

/// Включить наш прокси. Возвращает снимок: что стояло раньше и что поставили мы.
#[cfg(test)]
pub fn enable(address: &str) -> Result<Backup> {
    let mut previous = read()?;
    previous.ours = address.to_string();
    if let Err(why) = apply(address) {
        let _ = put(&previous);
        return Err(why);
    }
    Ok(previous)
}

/// Записать адрес без нового снимка. Нужен при повторном подъёме: исходное состояние
/// уже сохранено, и подменять его снимком нашей же настройки нельзя.
pub fn apply(address: &str) -> Result<()> {
    let current = read()?;
    if let Err(why) = write(address) {
        let _ = put(&current);
        return Err(why);
    }
    Ok(())
}

fn write(address: &str) -> Result<()> {
    registry::write_string(KEY, "ProxyServer", address)?;
    registry::write_string(KEY, "ProxyOverride", BYPASS)?;
    registry::write_dword(KEY, "ProxyEnable", 1)?;
    notify();
    Ok(())
}

fn put(previous: &Backup) -> Result<()> {
    registry::write_string(KEY, "ProxyServer", &previous.server)?;
    match &previous.bypass {
        Some(bypass) => registry::write_string(KEY, "ProxyOverride", bypass)?,
        None => registry::delete_value(KEY, "ProxyOverride")?,
    }
    registry::write_dword(KEY, "ProxyEnable", u32::from(previous.enabled))?;
    notify();
    Ok(())
}

/// Вернуть как было.
///
/// **Чужое не трогаем.** Если в реестре уже не наш адрес, значит прокси сменил кто-то
/// другой — другой VPN-клиент, например, — и восстанавливать «как было до нас» нельзя:
/// мы затрём его свежую настройку своей несвежей. В этом случае просто забываем снимок.
///
/// Снимок обязателен: без него неизвестно, наша ли запись в реестре, а «выключить любой
/// прокси» — не наше дело. Кто прибирается, тот и предъявляет снимок.
pub fn restore(previous: &Backup) -> Result<()> {
    if !previous.ours.is_empty() && !is_ours(&previous.ours) {
        return Ok(());
    }
    put(previous)
}

/// Сказать системе, что настройки изменились. Без этого уже запущенный браузер продолжит
/// ходить напрямую до своего перезапуска — и включение будет выглядеть не сработавшим.
#[cfg(windows)]
fn notify() {
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}

#[cfg(not(windows))]
fn notify() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_keeps_the_loopback_out_of_the_proxy() {
        // Через прокси окно не достучится до `external-controller` — он на 127.0.0.1.
        assert!(BYPASS.contains("127.*"));
        assert!(BYPASS.contains("<local>"));
    }

    #[test]
    fn an_empty_backup_restores_a_switched_off_proxy() {
        // Снимок «прокси не было» — это тоже снимок: возвращаем выключённое состояние,
        // а не отсутствие действия.
        let empty = Backup::default();
        assert!(!empty.enabled);
        assert!(empty.server.is_empty());
    }

    /// Снимок из сборки, которая ещё не писала свой адрес, обязан читаться: иначе
    /// обновление клиента оставило бы прокси включённым, а вернуть его было бы нечем.
    #[test]
    fn an_old_backup_without_our_address_still_reads() {
        let old: Backup = serde_json::from_str(r#"{"enabled":true,"server":"127.0.0.1:2080"}"#)
            .expect("снимок прошлой сборки должен читаться");
        assert_eq!(old.server, "127.0.0.1:2080");
        assert!(old.ours.is_empty(), "проверять нечем — вернём как было");
        assert!(
            old.bypass.is_none(),
            "список исключений такой снимок не помнит — возврат его удалит, а не оставит наш"
        );
    }

    /// Снимок обязан помнить **всё**, что перезаписывает `enable`. Иначе возврат чинит
    /// половину: адрес возвращается, а список исключений остаётся наш (B-011).
    #[test]
    fn the_backup_remembers_every_value_enable_overwrites() {
        let round: Backup = serde_json::from_str(
            r#"{"enabled":true,"server":"127.0.0.1:2080","ours":"127.0.0.1:3090",
                "bypass":"*.corp.example;<local>"}"#,
        )
        .unwrap();
        assert_eq!(round.bypass.as_deref(), Some("*.corp.example;<local>"));
    }
}
