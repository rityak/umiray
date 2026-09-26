//! Сборка пользовательских документов и данных клиента в итоговый конфиг ядра (D-135,
//! D-140).

use serde_yaml::{Mapping, Value};

use crate::config::{awg, files, rulesets, udp};
use crate::error::{AppError, Result};
use crate::nodes::sources;
use crate::paths;

use super::mihomo::Effective;
use super::plan::{Client, NodeSource};

const RULES_HEADER: &str = concat!(
    "# Маршрутизация набора: что куда идёт. Правила читаются сверху вниз, побеждает\n",
    "# первое совпавшее, а MATCH решает судьбу всего остального.\n",
    "#\n",
    "# `umiray` — псевдоним выбранного: куда он указывает, решает направление в «Соединении».\n",
    "#\n",
    "# Набор действует, только когда применён.\n\n",
);

/// Всё в один файл — тот самый, который запускает ядро.
pub fn effective(rules: Option<&str>, probe: Option<u16>) -> Result<Effective> {
    let mut layers = Vec::new();
    layers.push(owned(
        &files::read(files::GROUPS)?,
        &["proxy-groups"],
        false,
    )?);
    if let Some(rules) = rules {
        layers.push(owned(rules, &["rules"], false)?);
    }
    layers.push(owned(
        &files::read(files::ADVANCED)?,
        &["proxy-groups", "rules"],
        true,
    )?);
    super::mihomo::config(
        &layers,
        &rulesets::enabled_rules(),
        &node_sources(),
        probe,
        &Client {
            health: crate::nodes::health::url()?,
            udp: udp::on(),
            mask: awg::get(),
        },
    )
}

fn owned(text: &str, keys: &[&str], inverse: bool) -> Result<String> {
    let source = crate::yaml::top_mapping(text)?;
    let mut map = Mapping::new();
    for (key, value) in source {
        let named = key.as_str().is_some_and(|name| keys.contains(&name));
        if named != inverse {
            map.insert(key, value);
        }
    }
    serde_yaml::to_string(&Value::Mapping(map)).map_err(|why| AppError::invalid(why.to_string()))
}

pub fn generated_rules() -> Result<String> {
    let built = crate::yaml::top_mapping(
        &super::mihomo::config(
            &[],
            &[],
            &node_sources(),
            None,
            &Client {
                health: crate::nodes::health::DEFAULT.to_string(),
                udp: false,
                mask: awg::Mask::default(),
            },
        )?
        .yaml,
    )?;
    document(RULES_HEADER, "rules", &built)
}

pub fn reset(id: &str) -> Result<String> {
    if files::part_and_preset(id).is_none() {
        return files::reset(id);
    }
    let text = generated_rules()?;
    files::write(id, &text)?;
    Ok(text)
}

pub fn groups_seed(groups: &str) -> Result<String> {
    let template = files::template(files::GROUPS)?;
    if groups.trim().is_empty() {
        return Ok(template.to_string());
    }
    Ok(format!("{template}{}\n{groups}", legend()))
}

fn legend() -> String {
    let sources = sources::list();
    if sources.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = sources
        .iter()
        .map(|source| {
            format!(
                "#   {} — «{}», узлов {}\n",
                source.id, source.name, source.nodes
            )
        })
        .collect();
    format!(
        "# Что за идентификаторы в use: это ваши источники.\n{}\n",
        lines.concat()
    )
}

fn document(header: &str, key: &str, built: &Mapping) -> Result<String> {
    let mut map = Mapping::new();
    if let Some(value) = built.get(Value::from(key)) {
        map.insert(Value::from(key), value.clone());
    }
    let body = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|why| AppError::invalid(why.to_string()))?;
    Ok(format!("{header}{body}"))
}

pub fn udp_nodes() -> usize {
    node_sources().iter().map(|source| source.udp.len()).sum()
}

fn node_sources() -> Vec<NodeSource> {
    let nodes = crate::nodes::source_catalog::nodes();
    sources::list()
        .into_iter()
        .map(|source| NodeSource {
            path: paths::source_links(&source.id),
            names: nodes
                .iter()
                .filter(|node| node.source == source.id && node.supported)
                .map(|node| node.name.clone())
                .collect(),
            udp: nodes
                .iter()
                .filter(|node| {
                    node.source == source.id
                        && node.supported
                        && crate::nodes::ping::udp_only(&node.kind)
                })
                .map(|node| node.name.clone())
                .collect(),
            id: source.id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misplaced_top_level_keys_do_not_cross_document_boundaries() {
        let groups = owned(
            "proxy-groups: [one]\nrules: [wrong]\nmode: global\n",
            &["proxy-groups"],
            false,
        )
        .unwrap();
        let rules = owned(
            "proxy-groups: [wrong]\nrules: [right]\nmode: global\n",
            &["rules"],
            false,
        )
        .unwrap();
        let advanced = owned(
            "proxy-groups: [wrong]\nrules: [wrong]\nmode: rule\n",
            &["proxy-groups", "rules"],
            true,
        )
        .unwrap();
        assert_eq!(crate::yaml::top_mapping(&groups).unwrap().keys().count(), 1);
        assert_eq!(crate::yaml::top_mapping(&rules).unwrap().keys().count(), 1);
        let advanced = crate::yaml::top_mapping(&advanced).unwrap();
        assert_eq!(advanced.len(), 1);
        assert_eq!(advanced[Value::from("mode")], Value::from("rule"));
    }
}
