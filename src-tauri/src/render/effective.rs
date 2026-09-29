//! Сборка пользовательских документов и данных клиента в итоговый конфиг ядра (D-135,
//! D-140).

use serde_yaml::{Mapping, Value};

use crate::config::{awg, files};
use crate::error::{AppError, Result};

use super::mihomo::Effective;
use super::plan::{Client, NodeSource};
use crate::config::awg::Mask;
use crate::config::files::Documents;
use crate::config::rulesets::RulesetStore;
use crate::config::udp::UdpGroup;
use crate::nodes::sources::SourceStore;

const RULES_HEADER: &str = concat!(
    "# Маршрутизация набора: что куда идёт. Правила читаются сверху вниз, побеждает\n",
    "# первое совпавшее, а MATCH решает судьбу всего остального.\n",
    "#\n",
    "# `umiray` — псевдоним выбранного: куда он указывает, решает направление в «Соединении».\n",
    "#\n",
    "# Набор действует, только когда применён.\n\n",
);

pub struct ConfigRenderer;

impl ConfigRenderer {
    /// Всё в один файл — тот самый, который запускает ядро.
    pub fn effective(rules: Option<&str>, probe: Option<u16>) -> Result<Effective> {
        let mut layers = Vec::new();
        layers.push(owned(
            &Documents::read(files::GROUPS)?,
            &["proxy-groups"],
            false,
        )?);
        if let Some(rules) = rules {
            layers.push(owned(rules, &["rules"], false)?);
        }
        layers.push(owned(
            &Documents::read(files::ADVANCED)?,
            &["proxy-groups", "rules"],
            true,
        )?);
        super::mihomo::MihomoRenderer::config(
            &layers,
            &RulesetStore::enabled_rules(),
            &node_sources(),
            probe,
            &Client {
                health: crate::nodes::health::HealthCheck::url()?,
                udp: UdpGroup::on(),
                mask: Mask::get(),
            },
        )
    }

    pub fn generated_rules() -> Result<String> {
        let built = crate::yaml::Yaml::top_mapping(
            &super::mihomo::MihomoRenderer::config(
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
        if Documents::part_and_preset(id).is_none() {
            return Documents::reset(id);
        }
        let text = ConfigRenderer::generated_rules()?;
        Documents::write(id, &text)?;
        Ok(text)
    }

    pub fn groups_seed(groups: &str) -> Result<String> {
        let template = Documents::template(files::GROUPS)?;
        if groups.trim().is_empty() {
            return Ok(template.to_string());
        }
        Ok(format!("{template}{}\n{groups}", legend()))
    }

    pub fn udp_nodes() -> usize {
        node_sources().iter().map(|source| source.udp.len()).sum()
    }
}

fn owned(text: &str, keys: &[&str], inverse: bool) -> Result<String> {
    let source = crate::yaml::Yaml::top_mapping(text)?;
    let mut map = Mapping::new();
    for (key, value) in source {
        let named = key.as_str().is_some_and(|name| keys.contains(&name));
        if named != inverse {
            map.insert(key, value);
        }
    }
    serde_yaml::to_string(&Value::Mapping(map)).map_err(|why| AppError::invalid(why.to_string()))
}

fn legend() -> String {
    let sources = SourceStore::list();
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

fn node_sources() -> Vec<NodeSource> {
    let nodes = crate::nodes::source_catalog::SourceCatalog::nodes();
    SourceStore::list()
        .into_iter()
        .map(|source| NodeSource {
            path: crate::nodes::sources::SourceStore::provider(&source.id),
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
                        && crate::nodes::ping::Pinger::udp_only(&node.kind)
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
        assert_eq!(
            crate::yaml::Yaml::top_mapping(&groups)
                .unwrap()
                .keys()
                .count(),
            1
        );
        assert_eq!(
            crate::yaml::Yaml::top_mapping(&rules)
                .unwrap()
                .keys()
                .count(),
            1
        );
        let advanced = crate::yaml::Yaml::top_mapping(&advanced).unwrap();
        assert_eq!(advanced.len(), 1);
        assert_eq!(advanced[Value::from("mode")], Value::from("rule"));
    }
}
