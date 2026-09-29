//! Чем доедет правка: перезагрузкой конфига или подъёмом ядра заново (D-102).
//!
//! Таблица не выдумана — она измерена на живом ядре (S-020) и сторожится живой проверкой
//! `live_which_keys_a_reload_carries`. Здесь только вывод из замера, одной строкой:
//! **всё доезжает перезагрузкой, кроме двух ключей**.
//!
//! Сравнивается не «что нажали», а два конфига: тот, на котором ядро работает, и тот,
//! который собрался бы сейчас. Так решение не зависит от того, кто и через какую форму
//! правил файл, — это то же правило, по которому шапка читает режим из конфига (D-052).

use serde_yaml::{Mapping, Value};

use crate::error::Result;
use crate::yaml::Yaml;

/// Что нужно сделать, чтобы разница доехала до ядра.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// `PUT /configs`: PID тот же, открытые соединения целы (S-019).
    Reload,
    /// Только подъём заново. Причина едет вместе с ним: окно обязано **сказать**, почему
    /// предлагает кнопку, а молча рвать все соединения в системе клиент не вправе (D-102).
    /// Строка готовая, а не имя ключа: имён полей ядра окно не видит (Т7).
    Restart(&'static str),
}

/// Поля, которые живое ядро **не перечитывает** (S-020). Путь, а не имя верхнего ключа:
/// разница внутри `tun` бывает разной ценой.
///
/// `mixed-port` не доезжает вовсе: после перезагрузки `/configs` показывает старый номер,
/// и слушает ядро по-прежнему старый порт.
///
/// `tun.enable` — это появление и исчезновение адаптера, то есть ровно то, про что D-060
/// всегда говорил «перезапуском». Замер показал, что с правами администратора ядро
/// принимает и его, но проверено там было только «ядро не выругалось»: целы ли маршруты
/// и перехват имён — нет. Соседние ключи под `tun` (стек, имя, MTU) в этом списке
/// **не стоят**: они доезжают, и держать из-за них предложение перезапуска значило бы
/// показывать его после каждого сохранения формы.
const ON_START: &[(&[&str], &str)] = &[
    (
        &["mixed-port"],
        "Порт локального прокси изменился, а ядро читает его при запуске.",
    ),
    (
        &["tun", "enable"],
        "Режим перехвата изменился, а адаптер ядро поднимает при запуске.",
    ),
];

/// Значение по пути. Пусто — поля нет, и это тоже ответ: появившийся ключ отличается
/// от отсутствовавшего.
fn at<'a>(map: &'a Mapping, path: &[&str]) -> Option<&'a Value> {
    let (last, sections) = path.split_last()?;
    let mut here = map;
    for section in sections {
        here = here.get(Value::from(*section))?.as_mapping()?;
    }
    here.get(Value::from(*last))
}

impl Apply {
    /// Чем доедет разница между работающим конфигом и собранным. Пусто — доезжать нечему.
    pub fn needed(launched: &str, assembled: &str) -> Result<Option<Apply>> {
        let (was, now) = (Yaml::top_mapping(launched)?, Yaml::top_mapping(assembled)?);
        if was == now {
            return Ok(None);
        }
        let stale = ON_START
            .iter()
            .find(|(path, _)| at(&was, path) != at(&now, path));
        match stale {
            Some((_, why)) => Ok(Some(Apply::Restart(why))),
            None => Ok(Some(Apply::Reload)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: &str = "mixed-port: 3090
log-level: info
tun:
  enable: false
  stack: mixed
rules:
  - MATCH,DIRECT
";

    fn asked(changed: &str) -> Option<Apply> {
        Apply::needed(RUNNING, changed).unwrap()
    }

    #[test]
    fn the_same_config_asks_for_nothing() {
        assert_eq!(asked(RUNNING), None);
        // Порядок ключей — не разница: сборка пересобирает YAML, и сравнение обязано
        // смотреть на значения, а не на текст.
        assert_eq!(
            asked("rules:\n  - MATCH,DIRECT\nlog-level: info\nmixed-port: 3090\ntun:\n  enable: false\n  stack: mixed\n"),
            None
        );
    }

    /// Ради этого правила всё и затевалось: тумблер набора правит `rules`, и рвать из-за
    /// него каждое соединение в системе больше не нужно (S-020).
    #[test]
    fn a_route_change_travels_by_reload() {
        assert_eq!(
            asked(&RUNNING.replace("MATCH,DIRECT", "MATCH,REJECT")),
            Some(Apply::Reload)
        );
        assert_eq!(
            asked(&RUNNING.replace("log-level: info", "log-level: warning")),
            Some(Apply::Reload)
        );
    }

    /// А эти два — нет, и замер показал именно это. Заодно проверяется, что причина
    /// приходит **разная**: одна кнопка на два повода объясняла бы не то.
    #[test]
    fn the_port_and_the_tunnel_ask_for_a_restart() {
        let port = asked(&RUNNING.replace("mixed-port: 3090", "mixed-port: 7890"));
        let tun = asked(&RUNNING.replace("enable: false", "enable: true"));
        assert!(matches!(port, Some(Apply::Restart(why)) if why.contains("Порт")));
        assert!(
            matches!(tun, Some(Apply::Restart(why)) if why.contains("перехвата")),
            "адаптер на ходу не появится (D-060)"
        );
        assert_eq!(
            asked(&RUNNING.replace("stack: mixed", "stack: gvisor")),
            Some(Apply::Reload),
            "стек доезжает: держать из-за него предложение перезапуска значило бы              показывать его после каждого сохранения формы"
        );
    }

    /// Форма пишет все свои поля, и на файле, где их не было, это добавляет ключи.
    /// Если бы новый ключ под `tun` считался за перезапуск, окно предлагало бы его
    /// после каждого сохранения — и убрать это предложение было бы нечем.
    #[test]
    fn a_key_that_the_form_added_does_not_ask_for_a_restart() {
        let saved = RUNNING.replace(
            "tun:
  enable: false
  stack: mixed",
            "tun:
  enable: false
  stack: mixed
  strict-route: false
  mtu: 1400",
        );
        assert_eq!(asked(&saved), Some(Apply::Reload));
    }

    /// Перезапуск сильнее перезагрузки: если изменилось и то и другое, отдаём его.
    #[test]
    fn a_restart_wins_over_a_reload() {
        let both = RUNNING
            .replace("mixed-port: 3090", "mixed-port: 7890")
            .replace("MATCH,DIRECT", "MATCH,REJECT");
        assert!(matches!(asked(&both), Some(Apply::Restart(_))));
    }
}
