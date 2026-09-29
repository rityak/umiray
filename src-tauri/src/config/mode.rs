//! Режим перехвата — поле конфига, а не настройка (D-052).
//!
//! Переключатель в шапке пишет сюда, редактор правит тот же файл руками, и оба видят одно
//! и то же: файл — источник истины, окно — его вид. Поэтому здесь и чтение, и запись.
//!
//! **Комментарии при записи теряются.** Файл пересобирается через `serde_yaml`, а он их
//! не хранит: цена за то, что переключатель и редактор правят один документ (D-052).

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::config::files::LOCAL_PROXY_PORT;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Работает без прав администратора, поэтому и по умолчанию (D-023).
    #[default]
    Local,
    Tun,
}

use crate::config::files;
use crate::config::files::Documents;
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Записать режим, не тронув ничего лишнего.
/// Как ядро называет свой адаптер, если `tun.device` не задан.
pub const DEFAULT_DEVICE: &str = "Meta";

impl Mode {
    pub fn write(mode: Mode) -> Result<()> {
        let mut map = Yaml::top_mapping(&Documents::read(files::ADVANCED)?)?;
        apply(&mut map, mode);
        let text = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(files::ADVANCED, &text)
    }

    /// Режим, записанный в файле. Читать его нужно и до запуска ядра: переключатель в шапке
    /// показывает выбранное, а не работающее (D-060).
    ///
    /// Достаточно одного файла: `tun.enable` — поле конфига ядра, и оно же перекрывает всё
    /// при сборке (D-029). Нечитаемый файл — это `Local`: без TUN клиент работает, с ним —
    /// перехватывает весь трафик машины, и ошибаться в эту сторону нельзя.
    pub fn current() -> Mode {
        Documents::read(files::ADVANCED)
            .and_then(|text| Yaml::top_mapping(&text))
            .map(|map| Mode::of(&map))
            .unwrap_or_default()
    }

    /// Режим документа. Отсутствие `tun.enable` — это local, а не «неизвестно».
    pub fn of(map: &Mapping) -> Mode {
        let on = map
            .get(Value::from("tun"))
            .and_then(Value::as_mapping)
            .and_then(|tun| tun.get(Value::from("enable")))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if on {
            Mode::Tun
        } else {
            Mode::Local
        }
    }

    /// Имя адаптера, который поднимает TUN. Читается отсюда, а не угадывается: пользователь
    /// вправе написать своё `tun.device` в «Настройках», а kill switch привязывается именно
    /// к адаптеру (D-073) — ошибись здесь, и правило разрешит выход не через тот интерфейс.
    ///
    /// Умолчание `Meta` — то, что ставит само ядро, когда поля нет (замерено в S-002).
    pub fn tun_device(map: &Mapping) -> String {
        map.get(Value::from("tun"))
            .and_then(Value::as_mapping)
            .and_then(|tun| tun.get(Value::from("device")))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(DEFAULT_DEVICE)
            .to_string()
    }
}

/// Правило записи: `set` — только то, что и есть режим (`tun.enable`, а для TUN ещё и
/// разрешение имён, без которого он ловит трафик и не может его разрезолвить). Всё
/// остальное `fill`: свой стек или свой порт переживают переключение.
fn apply(map: &mut Mapping, mode: Mode) {
    match mode {
        Mode::Local => {
            Yaml::set(Yaml::sub(map, "tun"), "enable", Value::from(false));
            Yaml::fill(map, "mixed-port", Value::from(LOCAL_PROXY_PORT));
        }
        Mode::Tun => {
            let tun = Yaml::sub(map, "tun");
            Yaml::set(tun, "enable", Value::from(true));
            Yaml::fill(tun, "stack", Value::from("mixed"));
            Yaml::fill(tun, "auto-route", Value::from(true));
            Yaml::fill(tun, "auto-detect-interface", Value::from(true));
            // Windows-default закрывает обход DNS через физический адаптер (D-139).
            Yaml::fill(tun, "strict-route", Value::from(true));
            Yaml::fill(
                tun,
                "dns-hijack",
                Value::Sequence(vec![Value::from("any:53"), Value::from("tcp://any:53")]),
            );

            let dns = Yaml::sub(map, "dns");
            Yaml::set(dns, "enable", Value::from(true));
            Yaml::fill(dns, "enhanced-mode", Value::from("fake-ip"));
            Yaml::fill(
                dns,
                "nameserver",
                Value::Sequence(vec![Value::from("https://1.1.1.1/dns-query")]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switched(document: &str, mode: Mode) -> Value {
        let mut map = Yaml::top_mapping(document).unwrap();
        apply(&mut map, mode);
        Value::Mapping(map)
    }

    #[test]
    fn the_switch_never_touches_what_it_did_not_come_for() {
        let mine = "mixed-port: 7777
log-level: debug
tun:
  stack: gvisor
rules:
  - MATCH,umiray
";
        let out = switched(mine, Mode::Tun);
        assert_eq!(
            out["tun"]["enable"],
            Value::from(true),
            "режим ставится жёстко"
        );
        assert_eq!(
            out["tun"]["stack"],
            Value::from("gvisor"),
            "свой стек переживает переключение"
        );
        assert_eq!(out["mixed-port"], Value::from(7777), "свой порт переживает");
        assert_eq!(out["log-level"], Value::from("debug"));
        assert_eq!(out["rules"][0], Value::from("MATCH,umiray"));
    }

    /// TUN без DNS ловит трафик и не может его разрезолвить — это часть самого режима.
    #[test]
    fn tun_brings_its_own_dns() {
        let out = switched("dns:\n  enable: false\n", Mode::Tun);
        assert_eq!(out["dns"]["enable"], Value::from(true));
        assert_eq!(out["tun"]["strict-route"], Value::from(true));
        assert_eq!(
            out["tun"]["dns-hijack"],
            Value::Sequence(vec![Value::from("any:53"), Value::from("tcp://any:53")])
        );
    }

    #[test]
    fn explicit_tun_routing_choices_survive_the_switch() {
        let out = switched(
            "tun:\n  strict-route: false\n  dns-hijack: [192.0.2.1:53]\n",
            Mode::Tun,
        );
        assert_eq!(out["tun"]["strict-route"], Value::from(false));
        assert_eq!(out["tun"]["dns-hijack"][0], Value::from("192.0.2.1:53"));
    }

    #[test]
    fn switching_back_turns_tun_off_and_opens_the_port() {
        let out = switched("tun:\n  enable: true\n", Mode::Local);
        assert_eq!(out["tun"]["enable"], Value::from(false));
        assert_eq!(out["mixed-port"], Value::from(LOCAL_PROXY_PORT));
    }

    #[test]
    fn a_document_without_tun_reads_as_local() {
        for text in ["", "tun:\n  enable: false\n", "tun: не маппинг\n"] {
            assert_eq!(
                Mode::of(&Yaml::top_mapping(text).unwrap()),
                Mode::Local,
                "«{text}» не должно читаться как TUN"
            );
        }
        assert_eq!(
            Mode::of(&Yaml::top_mapping("tun:\n  enable: true\n").unwrap()),
            Mode::Tun
        );
    }

    /// Круг «прочитали — записали — прочитали» обязан сходиться: иначе переключатель
    /// показывал бы не то, что записал.
    #[test]
    fn what_was_written_reads_back_the_same() {
        for mode in [Mode::Local, Mode::Tun] {
            let mut map = Yaml::top_mapping("").unwrap();
            apply(&mut map, mode);
            assert_eq!(Mode::of(&map), mode);
        }
    }
}
