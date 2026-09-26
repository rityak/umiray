//! Пользовательские документы и точечные настройки клиента.
//!
//! Сборка итогового конфига ядра живёт в `render::effective`: этот модуль не знает ни
//! каталог источников, ни формат результата.

pub mod advanced;
pub mod awg;
pub mod direction;
pub mod files;
pub mod groups;
pub mod mode;
pub mod presets;
pub mod rules;
pub mod rulesets;
pub mod udp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_section_gets_its_own_part_and_advanced_gets_the_rest() {
        let full =
            "proxy-groups:\n  - name: AUTO\nrules:\n  - MATCH,umiray\ntun:\n  enable: false\n";
        let map = crate::yaml::top_mapping(full).unwrap();

        let only = |mine: &[&str]| -> Vec<String> {
            if mine.is_empty() {
                let others = files::keys_of_others(files::ADVANCED);
                map.keys()
                    .filter(|key| !key.as_str().is_some_and(|key| others.contains(&key)))
                    .filter_map(|key| key.as_str().map(String::from))
                    .collect()
            } else {
                mine.iter().map(|key| (*key).to_string()).collect()
            }
        };

        assert_eq!(only(&["proxy-groups"]), vec!["proxy-groups"]);
        assert_eq!(only(&["rules"]), vec!["rules"]);
        assert_eq!(only(&[]), vec!["tun"]);
    }
}
