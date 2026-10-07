//! Переименование группы (D-172): проверка имени и перенос ссылок на неё в документах.
//!
//! Имя группы — адрес: на него смотрят другие группы (`proxies:`), правила наборов и выбор
//! в «Соединении». Переименовать только саму группу значило бы молча увести её правила
//! в `umiray` (D-156). Здесь — чистые функции над текстом; кто какие документы правит,
//! решает сервис.

use crate::config::direction::{AUTO, PROBE, SELECTOR};
use crate::config::groups::GroupsCodec;
use crate::config::rules::RulesCodec;
use crate::error::{AppError, Result};

/// Выходы ядра: группа с таким именем спорила бы с ними за правило.
const CORE: [&str; 6] = [
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "COMPATIBLE",
    "GLOBAL",
];

/// Служебный префикс клиента: `umiray-udp`, `umiray-geo-pl` (D-113, D-172).
const OWN_PREFIX: &str = "umiray-";

/// Годится ли имя для группы. `taken` — имена, которые уже что-то значат: другие группы
/// и узлы (правило с целью-узлом собирается в группу с его именем, D-082).
pub fn check(name: &str, taken: &[String]) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("У группы должно быть имя"));
    }
    // Ядро режет строку правила по запятой, и цель стала бы своим началом (B-043).
    if name.contains(',') {
        return Err(AppError::invalid("Запятой в имени группы быть не может"));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "В имени группы не может быть переноса строки",
        ));
    }
    let reserved = [AUTO, SELECTOR, PROBE].iter().chain(CORE.iter());
    if reserved.clone().any(|word| word.eq_ignore_ascii_case(name))
        || name.to_lowercase().starts_with(OWN_PREFIX)
    {
        return Err(AppError::invalid(format!(
            "«{name}» — служебное имя клиента или ядра"
        )));
    }
    if taken.iter().any(|other| other == name) {
        return Err(AppError::invalid(format!("Имя «{name}» уже занято")));
    }
    Ok(name.to_string())
}

/// Документ групп с новым именем группы и ссылками на неё из других групп.
pub fn in_groups(text: &str, from: &str, to: &str) -> Result<String> {
    let mut groups = GroupsCodec::parse(text)?;
    if !groups.iter().any(|group| group.name == from) {
        return Err(AppError::invalid(format!("Группы «{from}» нет")));
    }
    for group in &mut groups {
        if group.name == from {
            group.name = to.to_string();
        }
        for name in &mut group.proxies {
            if name == from {
                *name = to.to_string();
            }
        }
    }
    GroupsCodec::render(text, &groups)
}

/// Документ маршрута с новой целью там, где была старая: правила, rule sets, готовые наборы
/// и `MATCH`. `None` — группа в нём не упоминается, переписывать незачем.
pub fn in_route(text: &str, from: &str, to: &str) -> Result<Option<String>> {
    let mut routing = RulesCodec::parse(text)?;
    let mut touched = false;
    let mut swap = |target: &mut String| {
        if target == from {
            *target = to.to_string();
            touched = true;
        }
    };
    for rule in &mut routing.rules {
        swap(&mut rule.target);
    }
    for set in &mut routing.rule_sets {
        swap(&mut set.target);
    }
    for ready in &mut routing.ready {
        if let Some(target) = &mut ready.target {
            swap(target);
        }
    }
    swap(&mut routing.fallback);
    if !touched {
        return Ok(None);
    }
    RulesCodec::render(text, &routing).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// Имя, которое сломало бы конфиг или спорило бы с чужим, отвергается словами.
    #[test]
    fn a_name_that_would_break_the_config_is_refused() {
        let others = taken(&["POLAND", "Poland 1"]);
        assert_eq!(check("  Европа ", &others).unwrap(), "Европа");
        for bad in [
            "",
            "  ",
            "A,B",
            "a\nb",
            "AUTO",
            "direct",
            "umiray",
            "umiray-geo-pl",
            "POLAND",
            "Poland 1",
        ] {
            assert!(check(bad, &others).is_err(), "{bad:?}");
        }
    }

    /// Ссылки из других групп едут за именем; незнакомое поле группы остаётся на месте.
    #[test]
    fn other_groups_follow_the_new_name() {
        let text = "proxy-groups:
  - name: POLAND
    type: url-test
    use: [s1]
    icon: x
  - name: ALL
    type: select
    proxies: [POLAND, DIRECT]
";
        let out = GroupsCodec::parse(&in_groups(text, "POLAND", "Польша").unwrap()).unwrap();
        assert_eq!(out[0].name, "Польша");
        assert_eq!(out[0].extra, ["icon"]);
        assert_eq!(out[1].proxies, ["Польша", "DIRECT"]);
        assert!(in_groups(text, "нет такой", "x").is_err());
    }

    /// Правила, списки, готовые наборы и `MATCH` — всё, что целилось в группу, целится
    /// в неё и после; документ без неё не трогаем.
    #[test]
    fn route_targets_follow_the_new_name() {
        let text = "rules:
  - DOMAIN-SUFFIX,pl,POLAND
  - DOMAIN-SUFFIX,ru,DIRECT
  - MATCH,POLAND
rule-sets:
  - id: antizapret
    target: POLAND
ready:
  - id: direct-ru
    target: POLAND
";
        let out = in_route(text, "POLAND", "Польша").unwrap().unwrap();
        let routing = RulesCodec::parse(&out).unwrap();
        assert_eq!(routing.rules[0].target, "Польша");
        assert_eq!(routing.rules[1].target, "DIRECT");
        assert_eq!(routing.fallback, "Польша");
        assert_eq!(routing.rule_sets[0].target, "Польша");
        assert_eq!(routing.ready[0].target.as_deref(), Some("Польша"));
        assert!(in_route(text, "ДРУГАЯ", "x").unwrap().is_none());
    }
}
