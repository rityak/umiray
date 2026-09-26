//! Маскировка рукопожатия WireGuard: поля `amnezia-wg-option` в `client.yaml` (D-118).
//!
//! Зачем вообще: на пути до сервера WireGuard узнают по рукопожатию и после него душат
//! поток — 204 Б/с против 9.4 МБ/с на одном и том же сервере, где меняется только
//! протокол (S-023). Детектор ломается мусорными пакетами перед рукопожатием, и **сервер
//! при этом остаётся ванильным**: сами пакеты рукопожатия не меняются, а мусор ядерный
//! WireGuard молча роняет.
//!
//! Два яруса, и разница между ними не косметическая:
//!
//! - `jc/jmin/jmax` — мусорные пакеты. Работают с **любым** сервером WireGuard;
//! - `s1…s4` и `h1…h4` — паддинг рукопожатия и магические заголовки. Меняют формат самих
//!   пакетов, поэтому требуют **сервера с AmneziaWG**: на ванильном туннель не встанет.
//!
//! Настройка клиентская и одна на все узлы: она про то, как клиент представляется на пути,
//! а путь у всех узлов один.
//!
//! Потолок: два сервера AmneziaWG с разными `h1…h4` одной настройкой не покрыть.
//! Понадобится — поле уедет к узлу поверх механизма правок (D-036); пока такого сервера
//! нет ни одного.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::config::files::{self, CLIENT};
use crate::error::{AppError, Result};
use crate::yaml::{set, top_mapping};

const KEY: &str = "wireguard-mask";

/// Предел мусорного пакета у AmneziaWG: он уезжает одной датаграммой и больше MTU быть
/// не может.
const MAX_JUNK: u32 = 1280;
/// Стандартные типы пакетов WireGuard — 1…4. Магический заголовок обязан их обойти,
/// иначе он не заголовок, а обычный WireGuard с лишним шагом.
const RESERVED_HEADER: u32 = 4;
/// Разница длин между рукопожатием (148 байт) и ответом (92). Паддинг, который делает их
/// одинаковыми, склеивает два разных пакета в один вид — AmneziaWG это запрещает.
const HANDSHAKE_GAP: u32 = 56;

/// Маска рукопожатия целиком. Ноль в поле — «не трогать»: у AmneziaWG это и есть
/// выключенное состояние, отдельного флага у него нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    /// Сколько мусорных пакетов слать перед рукопожатием. Ноль — не слать.
    pub jc: u32,
    pub jmin: u32,
    pub jmax: u32,
    /// Паддинг: рукопожатие, ответ, cookie, транспорт.
    pub s1: u32,
    pub s2: u32,
    pub s3: u32,
    pub s4: u32,
    /// Магические заголовки тех же четырёх типов пакетов.
    pub h1: u32,
    pub h2: u32,
    pub h3: u32,
    pub h4: u32,
}

/// Умолчание — **включённый** мусор одним пакетом (D-118): цена ему один пакет на
/// рукопожатие, а без него узел WireGuard на задушенном пути не работает и не объясняет
/// почему.
impl Default for Mask {
    fn default() -> Self {
        Self {
            jc: 1,
            jmin: 40,
            jmax: 70,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            h1: 0,
            h2: 0,
            h3: 0,
            h4: 0,
        }
    }
}

impl Mask {
    /// Есть ли что писать в конфиг. Всё по нулям — блока не будет вовсе.
    pub fn on(&self) -> bool {
        self.pairs().iter().any(|(_, value)| *value > 0)
    }

    /// Поля в том виде, в каком их называет ядро. Порядок — как в конфигах AmneziaWG.
    fn pairs(&self) -> [(&'static str, u32); 11] {
        [
            ("jc", self.jc),
            ("jmin", self.jmin),
            ("jmax", self.jmax),
            ("s1", self.s1),
            ("s2", self.s2),
            ("s3", self.s3),
            ("s4", self.s4),
            ("h1", self.h1),
            ("h2", self.h2),
            ("h3", self.h3),
            ("h4", self.h4),
        ]
    }

    /// `amnezia-wg-option` для записи узла. Нули не пишем: у ядра отсутствие поля и ноль —
    /// одно и то же, а блок из одних нулей читался бы как «включено, но ничего не делает».
    pub fn option(&self) -> Option<Value> {
        if !self.on() {
            return None;
        }
        let mut map = serde_yaml::Mapping::new();
        for (name, value) in self.pairs() {
            if value > 0 {
                set(&mut map, name, Value::from(value));
            }
        }
        Some(Value::Mapping(map))
    }

    /// Сочетания, при которых туннель не встанет или встанет наполовину. Проверяем
    /// на записи, а не при сборке: молча собранный и не работающий конфиг — ровно та беда,
    /// от которой заведены формы (D-052).
    fn check(&self) -> Result<()> {
        let bad = |text: String| Err(AppError::invalid(text));
        if self.jc > 128 {
            return bad("Мусорных пакетов не больше 128".into());
        }
        if self.jc > 0 && self.jmin > self.jmax {
            return bad("Наименьший размер мусора больше наибольшего".into());
        }
        if self.jmax > MAX_JUNK {
            return bad(format!("Мусорный пакет не длиннее {MAX_JUNK} байт"));
        }
        if [self.s1, self.s2, self.s3, self.s4]
            .iter()
            .any(|value| *value > MAX_JUNK)
        {
            return bad(format!("Паддинг не длиннее {MAX_JUNK} байт"));
        }
        if self.s1 > 0 && self.s1 + HANDSHAKE_GAP == self.s2 {
            return bad(
                "S1 + 56 = S2: с таким паддингом рукопожатие и ответ становятся одной длины".into(),
            );
        }
        let headers = [self.h1, self.h2, self.h3, self.h4];
        if headers.iter().any(|value| *value > 0) {
            if headers.iter().any(|value| *value <= RESERVED_HEADER) {
                return bad(
                    "Заголовки задаются все четыре и числом больше 4: 1…4 занял сам WireGuard"
                        .into(),
                );
            }
            let mut seen = headers;
            seen.sort_unstable();
            if seen.windows(2).any(|pair| pair[0] == pair[1]) {
                return bad("Заголовки H1…H4 должны отличаться друг от друга".into());
            }
        }
        Ok(())
    }
}

/// Что лежит в файле. Мусор в поле — это умолчание, а не отказ собрать конфиг: из-за
/// настройки маскировки не поднять VPN было бы хуже, чем её не применить.
pub fn get() -> Mask {
    files::read(CLIENT)
        .ok()
        .and_then(|text| of(&text).ok())
        .unwrap_or_default()
}

/// Записать и отдать записанное: окно показывает файл, а не свою память о нём.
pub fn set_mask(mask: Mask) -> Result<Mask> {
    mask.check()?;
    let mut map = top_mapping(&files::read(CLIENT)?)?;
    set(
        &mut map,
        KEY,
        serde_yaml::to_value(mask).map_err(|e| AppError::invalid(e.to_string()))?,
    );
    let text = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|e| AppError::invalid(e.to_string()))?;
    files::write(CLIENT, &text)?;
    Ok(mask)
}

/// Разбор отделён от диска — правила проверяются обычным `cargo test`.
fn of(text: &str) -> Result<Mask> {
    let map = top_mapping(text)?;
    match map.get(Value::from(KEY)) {
        Some(value) => {
            serde_yaml::from_value(value.clone()).map_err(|e| AppError::invalid(e.to_string()))
        }
        None => Ok(Mask::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_is_on_until_it_is_switched_off() {
        assert_eq!(of("").unwrap().jc, 1, "умолчание — один мусорный пакет");
        assert_eq!(of("ping: tcp").unwrap().jc, 1);
        assert_eq!(of("wireguard-mask:\n  jc: 0\n").unwrap().jc, 0);
        // Частично записанный блок не теряет остальные поля.
        let part = of("wireguard-mask:\n  jc: 4\n").unwrap();
        assert_eq!((part.jc, part.jmin, part.jmax), (4, 40, 70));
    }

    #[test]
    fn nothing_is_written_when_nothing_is_set() {
        let off = Mask {
            jc: 0,
            jmin: 0,
            jmax: 0,
            ..Mask::default()
        };
        assert!(!off.on());
        assert!(off.option().is_none());
    }

    #[test]
    fn only_the_fields_that_are_set_reach_the_core() {
        let option = Mask::default().option().expect("маска включена");
        let map = option.as_mapping().expect("отображение");
        assert_eq!(map.get(Value::from("jc")), Some(&Value::from(1u32)));
        assert_eq!(map.get(Value::from("jmax")), Some(&Value::from(70u32)));
        assert!(map.get(Value::from("h1")).is_none(), "нули не пишем");
        assert!(map.get(Value::from("s1")).is_none());
    }

    #[test]
    fn the_combinations_that_break_the_tunnel_are_refused() {
        let bad = |mask: Mask| mask.check().is_err();
        assert!(bad(Mask {
            jmin: 90,
            jmax: 70,
            ..Mask::default()
        }));
        assert!(bad(Mask {
            jmax: 4000,
            ..Mask::default()
        }));
        // S1 + 56 = S2 — рукопожатие и ответ становятся одной длины.
        assert!(bad(Mask {
            s1: 15,
            s2: 71,
            ..Mask::default()
        }));
        // Заголовок из занятых WireGuard 1…4 и повтор.
        assert!(bad(Mask {
            h1: 3,
            h2: 100,
            h3: 200,
            h4: 300,
            ..Mask::default()
        }));
        assert!(bad(Mask {
            h1: 100,
            h2: 100,
            h3: 200,
            h4: 300,
            ..Mask::default()
        }));
        assert!(Mask {
            s1: 15,
            s2: 30,
            h1: 100,
            h2: 200,
            h3: 300,
            h4: 400,
            ..Mask::default()
        }
        .check()
        .is_ok());
    }
}
