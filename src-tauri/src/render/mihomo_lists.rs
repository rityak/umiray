//! Rule sets на языке mihomo (D-157): `rule-providers` и строки `RULE-SET`.
//!
//! У провайдера mihomo поведение одно — `domain` или `ipcidr`, — а список бывает смешанным.
//! Поэтому список с обеими частями становится двумя провайдерами и двумя строками:
//! `<id>` для доменов и `<id>@ip` для подсетей. `@` в имя списка не попадает никогда
//! (`ListStore::valid`), и столкнуться не с чем.

use std::path::PathBuf;

use serde_yaml::{Mapping, Value};

use crate::lists::store::{ListStore, Part};
use crate::render::plan::RuleSet;
use crate::yaml::Yaml;

/// Имя ядра в `lists/<ядро>/` — там лежит собранное им.
pub const ENGINE: &str = "mihomo";

pub struct MihomoLists;

impl MihomoLists {
    /// Где лежит часть списка, собранная в `.mrs`. Одно место и для сборки файла
    /// (`core::mihomo`), и для ссылки на него в конфиге.
    pub fn artifact(id: &str, part: Part) -> PathBuf {
        ListStore::artifact(ENGINE, &format!("{id}.{}.mrs", part.name()))
    }

    /// Дописать провайдеров под строки `RULE-SET`, которые называют наш список.
    ///
    /// Строка на список, которого нет или который ещё не собран, в конфиг не идёт: ядро
    /// отвергло бы его целиком, и VPN не поднялся бы из-за одной строки (как D-156).
    /// Провайдер с тем же именем, написанный человеком, авторитетнее нашего (D-029).
    pub fn apply(map: &mut Mapping, lists: &[RuleSet]) {
        let mut providers = map
            .get(Value::from("rule-providers"))
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        let theirs: Vec<String> = providers
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let Some(rules) = map.get(Value::from("rules")).and_then(Value::as_sequence) else {
            return;
        };
        let mut out = Vec::with_capacity(rules.len());
        for line in rules {
            let Some(text) = line.as_str() else {
                out.push(line.clone());
                continue;
            };
            let parts: Vec<&str> = text.split(',').map(str::trim).collect();
            if parts[0] != "RULE-SET"
                || parts.len() < 3
                || theirs.iter().any(|name| name == parts[1])
            {
                out.push(line.clone());
                continue;
            }
            let Some(list) = lists.iter().find(|list| list.id == parts[1]) else {
                continue;
            };
            for (name, behavior, path) in halves(list) {
                providers.insert(Value::from(name.clone()), provider(behavior, path));
                let mut named = parts.clone();
                named[1] = &name;
                out.push(Value::from(named.join(",")));
            }
        }
        Yaml::set(map, "rules", Value::Sequence(out));
        if !providers.is_empty() {
            Yaml::set(map, "rule-providers", Value::Mapping(providers));
        }
    }
}

/// Провайдеры одного списка: имя, поведение, файл. Подсети получают `@ip` только рядом
/// с доменами — список из одних подсетей зовётся своим именем, как и написано в правиле.
fn halves(list: &RuleSet) -> Vec<(String, &'static str, &PathBuf)> {
    let mut out = Vec::new();
    if let Some(path) = &list.domains {
        out.push((list.id.clone(), "domain", path));
    }
    if let Some(path) = &list.cidrs {
        let name = if out.is_empty() {
            list.id.clone()
        } else {
            format!("{}@ip", list.id)
        };
        out.push((name, "ipcidr", path));
    }
    out
}

fn provider(behavior: &str, path: &std::path::Path) -> Value {
    let mut entry = Mapping::new();
    Yaml::set(&mut entry, "type", Value::from("file"));
    Yaml::set(&mut entry, "behavior", Value::from(behavior));
    Yaml::set(&mut entry, "format", Value::from("mrs"));
    Yaml::set(
        &mut entry,
        "path",
        Value::from(path.to_string_lossy().into_owned()),
    );
    Value::Mapping(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(id: &str, domains: bool, cidrs: bool) -> RuleSet {
        RuleSet {
            id: id.into(),
            domains: domains.then(|| PathBuf::from(format!("/l/{id}.domains.mrs"))),
            cidrs: cidrs.then(|| PathBuf::from(format!("/l/{id}.cidrs.mrs"))),
        }
    }

    fn applied(text: &str, lists: &[RuleSet]) -> Mapping {
        let mut map = Yaml::top_mapping(text).unwrap();
        MihomoLists::apply(&mut map, lists);
        map
    }

    fn lines(map: &Mapping) -> Vec<&str> {
        map[&Value::from("rules")]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    /// Смешанный список — два провайдера и две строки; хвост правила едет в обе.
    #[test]
    fn a_mixed_list_becomes_two_providers_and_two_lines() {
        let map = applied(
            "rules:\n  - RULE-SET,antizapret,umiray,no-resolve\n  - MATCH,DIRECT\n",
            &[set("antizapret", true, true)],
        );
        assert_eq!(
            lines(&map),
            [
                "RULE-SET,antizapret,umiray,no-resolve",
                "RULE-SET,antizapret@ip,umiray,no-resolve",
                "MATCH,DIRECT"
            ]
        );
        let providers = map[&Value::from("rule-providers")].as_mapping().unwrap();
        assert_eq!(providers[&Value::from("antizapret")]["behavior"], "domain");
        assert_eq!(
            providers[&Value::from("antizapret@ip")]["behavior"],
            "ipcidr"
        );
        assert_eq!(providers[&Value::from("antizapret")]["format"], "mrs");
    }

    /// Список из одних подсетей зовётся своим именем — строка остаётся как написана.
    #[test]
    fn a_list_of_subnets_keeps_its_own_name() {
        let map = applied(
            "rules:\n  - RULE-SET,geoip-ru,DIRECT\n",
            &[set("geoip-ru", false, true)],
        );
        assert_eq!(lines(&map), ["RULE-SET,geoip-ru,DIRECT"]);
        assert_eq!(
            map[&Value::from("rule-providers")][&Value::from("geoip-ru")]["behavior"],
            "ipcidr"
        );
    }

    /// Нет списка или он не собран — строки нет: иначе ядро отвергло бы конфиг целиком.
    /// Провайдер, написанный человеком, не подменяется.
    #[test]
    fn a_missing_list_is_left_out_and_the_users_provider_is_kept() {
        let map = applied(
            "rule-providers:\n  mine: {type: http, behavior: domain, url: 'https://x'}\nrules:\n  - RULE-SET,gone,umiray\n  - RULE-SET,mine,DIRECT\n  - DOMAIN,a.ru,DIRECT\n",
            &[set("mine", true, false)],
        );
        assert_eq!(lines(&map), ["RULE-SET,mine,DIRECT", "DOMAIN,a.ru,DIRECT"]);
        let providers = map[&Value::from("rule-providers")].as_mapping().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[&Value::from("mine")]["type"], "http");
    }
}
