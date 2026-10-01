//! Сборка пользовательских документов и данных клиента в итоговый конфиг ядра (D-135,
//! D-140).

use serde_yaml::{Mapping, Value};

use crate::config::{awg, files};
use crate::error::{AppError, Result};

use super::mihomo::Effective;
use super::mihomo_lists::MihomoLists;
use super::plan::{Client, NodeSource, Route, RuleSet};
use crate::config::awg::Mask;
use crate::config::files::Documents;
use crate::config::route::{Priority, Sections};
use crate::config::rulesets::RulesetStore;
use crate::config::udp::UdpGroup;
use crate::lists::store::{ListStore, Part};
use crate::nodes::sources::SourceStore;

const RULES_HEADER: &str = concat!(
    "# Маршрутизация набора: что куда идёт. Сверху вниз, побеждает первое совпавшее:\n",
    "#   rules      — свои правила;\n",
    "#   rule-sets  — скачанные списки: `- id: antizapret` и `target: umiray`;\n",
    "#   ready      — готовые наборы: `- id: direct-ru`, target — чтобы сменить выход;\n",
    "#   MATCH      — последняя строка rules, судьба всего остального.\n",
    "# `priority: high|low` двигает запись выше или ниже, на одном уровне rule sets выше\n",
    "# ready (D-158). Выключенная маршрутизация не берёт отсюда ничего (D-166).\n",
    "#\n",
    "# `umiray` — псевдоним выбранного: куда он указывает, решает выбор в «Соединении».\n\n",
);

pub struct ConfigRenderer;

impl ConfigRenderer {
    /// Всё в один файл — тот самый, который запускает ядро.
    pub fn effective(route: &Route, probe: Option<u16>) -> Result<Effective> {
        let mut layers = Vec::new();
        layers.push(owned(
            &Documents::read(files::GROUPS)?,
            &["proxy-groups"],
            false,
        )?);
        if let Some(text) = route.text.as_deref() {
            layers.push(owned(text, &["rules"], false)?);
        }
        layers.push(owned(
            &Documents::read(files::ADVANCED)?,
            &["proxy-groups", "rules"],
            true,
        )?);
        super::mihomo::MihomoRenderer::config(
            &layers,
            &sets(route.text.as_deref())?,
            &node_sources(),
            probe,
            &Client {
                health: crate::nodes::health::HealthCheck::url()?,
                // `NETWORK,udp` — тоже маршрут: выключенная маршрутизация шлёт всё
                // в выбранный выход, UDP тоже (D-166).
                udp: UdpGroup::on() && route.text.is_some(),
                mask: Mask::get(),
                lists: rule_sets(),
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
                    lists: Vec::new(),
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

fn document(header: &str, key: &str, built: &Mapping) -> Result<String> {
    let mut map = Mapping::new();
    if let Some(value) = built.get(Value::from(key)) {
        map.insert(Value::from(key), value.clone());
    }
    let body = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|why| AppError::invalid(why.to_string()))?;
    Ok(format!("{header}{body}"))
}

/// Строки маршрута под своими правилами (D-158): по уровням приоритета, внутри уровня
/// rule sets, затем готовые наборы, дальше — порядок документа. Набор, которого нет,
/// пропускается: документ мог сослаться на удалённый мимо нас, и VPN из-за этого без
/// маршрута не остаётся.
fn sets(text: Option<&str>) -> Result<Vec<String>> {
    let Some(text) = text else {
        return Ok(Vec::new());
    };
    Ok(ordered(&Sections::read(text)?, RulesetStore::lines))
}

/// Уровень за уровнем; внутри уровня rule sets выше готовых наборов: заблокированный `.ru`
/// из списка обходит «Россию — напрямую» и без настройки (D-158). Строки готового набора
/// даёт `ready` — отдельно, чтобы порядок проверялся без диска.
fn ordered(
    sections: &Sections,
    ready: impl Fn(&str, Option<&str>) -> Option<Vec<String>>,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for level in Priority::ORDER {
        for set in sections
            .rule_sets
            .iter()
            .filter(|set| set.priority == level)
        {
            lines.push(format!("RULE-SET,{},{}", set.id, set.target));
        }
        for set in sections.ready.iter().filter(|set| set.priority == level) {
            lines.extend(ready(&set.id, set.target.as_deref()).unwrap_or_default());
        }
    }
    lines
}

/// Списки, которые mihomo уже собрал в свой формат (D-157). Несобранная часть не в счёт:
/// ссылка на отсутствующий файл уронила бы ядро.
fn rule_sets() -> Vec<RuleSet> {
    ListStore::list()
        .into_iter()
        .map(|list| {
            let built =
                |part| Some(MihomoLists::artifact(&list.id, part)).filter(|path| path.exists());
            RuleSet {
                domains: built(Part::Domains),
                cidrs: built(Part::Cidrs),
                id: list.id,
            }
        })
        .filter(|set| set.domains.is_some() || set.cidrs.is_some())
        .collect()
}

/// Несёт ли узел UDP датаграммой — кандидат в UDP-группу (D-113).
///
/// Hysteria v1 живёт на UDP, но в ядре v1.19.30 его UDP сломан: `DialUDP` шлёт запрос
/// с `UDP: false`, и сервер видит TCP к `:0` (S-029). Ссылку клиент поэтому не разбирает,
/// но узел из YAML-подписки ядро поднимет — и группа, выбравшая его по TCP-замеру, уронила
/// бы звонки. ponytail: исключение по имени; в ветке Meta уже `UDP: true` — снять вместе
/// с возвратом протокола, когда стенд покажет UDP на новом ядре.
pub(crate) fn datagram(kind: &str) -> bool {
    crate::nodes::ping::Pinger::udp_only(kind) && !kind.eq_ignore_ascii_case("hysteria")
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
                .filter(|node| node.source == source.id && node.supported && datagram(&node.kind))
                .map(|node| node.name.clone())
                .collect(),
            id: source.id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hysteria v1 без TCP-порта, но и без живого UDP в ядре: в UDP-группу он не идёт.
    #[test]
    fn hysteria_v1_is_not_a_datagram_carrier() {
        assert!(datagram("Hysteria2"));
        assert!(datagram("Tuic"));
        assert!(!datagram("Hysteria"));
        assert!(!datagram("Vless"));
    }

    /// D-158: rule sets идут в порядке документа и раньше готовых наборов; набор, которого
    /// нет, пропускается, а не роняет сборку.
    #[test]
    fn route_lines_follow_the_document_and_skip_what_is_gone() {
        let doc = "rule-sets:
  - id: antizapret
    target: umiray
  - id: telegram
    target: AUTO
ready:
  - id: definitely-not-a-set
rules:
  - MATCH,DIRECT
";
        assert_eq!(
            sets(Some(doc)).unwrap(),
            ["RULE-SET,antizapret,umiray", "RULE-SET,telegram,AUTO"]
        );
        assert!(sets(None).unwrap().is_empty());
        assert!(
            sets(Some("rule-sets:\n  - id: x\n")).is_err(),
            "запись без target — отказ, а не молча выпавший список"
        );
    }

    /// D-158: уровни сверху вниз, внутри уровня rule sets выше готовых, дальше — порядок
    /// документа. Случай из жизни: antizapret выше «России — напрямую» при равных уровнях,
    /// а поднятый набор встаёт над списком.
    #[test]
    fn priority_levels_decide_the_order_and_rule_sets_win_a_tie() {
        let lookup = |id: &str, target: Option<&str>| {
            Some(vec![format!(
                "DOMAIN-SUFFIX,{id}.example,{}",
                target.unwrap_or("DIRECT")
            )])
        };
        let doc = |ready_priority: &str| {
            format!(
                "rule-sets:
  - id: antizapret
    target: AUTO
  - id: late
    target: umiray
    priority: low
ready:
  - id: direct-ru
{ready_priority}rules:
  - MATCH,umiray
"
            )
        };
        let tie = Sections::read(&doc("")).unwrap();
        assert_eq!(
            ordered(&tie, lookup),
            [
                "RULE-SET,antizapret,AUTO",
                "DOMAIN-SUFFIX,direct-ru.example,DIRECT",
                "RULE-SET,late,umiray"
            ]
        );
        let raised = Sections::read(&doc("    priority: high\n")).unwrap();
        assert_eq!(
            ordered(&raised, lookup)[0],
            "DOMAIN-SUFFIX,direct-ru.example,DIRECT",
            "поднятый набор встаёт над списком"
        );
    }

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
