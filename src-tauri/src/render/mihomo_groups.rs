//! Группы, которые клиент собирает сам из узлов (D-172): кто остаётся в `AUTO` и автогруппы
//! по стране и протоколу. Состав считается здесь один раз — его пишет конфиг (`mihomo.rs`)
//! и показывает окно (`built`), и они не расходятся.

use serde_yaml::{Mapping, Value};

use super::mihomo::{any, checked, provides};
use super::plan::{Client, NodeFact, NodeSource};
use crate::config::auto::{Exclude, Grouping};
use crate::config::direction::{AUTO, GEO_PREFIX, PROTO_PREFIX, UDP};
use crate::yaml::Yaml;

/// Кто остаётся в `AUTO`: источники целиком, узлы под `exclude-filter` и свои записи.
pub(super) struct AutoPick {
    pub(super) kept: Vec<NodeSource>,
    pub(super) out: Vec<String>,
    pub(super) mine: Vec<String>,
}

impl AutoPick {
    fn members(&self) -> Vec<String> {
        self.kept
            .iter()
            .flat_map(|source| source.names.iter())
            .filter(|name| !self.out.contains(name))
            .chain(&self.mine)
            .cloned()
            .collect()
    }
}

pub(super) fn auto_pick(sources: &[NodeSource], ours: &[String], exclude: &Exclude) -> AutoPick {
    let kept: Vec<NodeSource> = sources
        .iter()
        .filter(|source| !exclude.sources.contains(&source.id))
        .cloned()
        .collect();
    let out: Vec<String> = kept
        .iter()
        .flat_map(|source| source.names.iter())
        .chain(ours)
        .filter(|name| exclude.nodes.contains(name))
        .cloned()
        .collect();
    let mine: Vec<String> = ours
        .iter()
        .filter(|name| !out.contains(name))
        .cloned()
        .collect();
    let left = kept.iter().map(|source| source.names.len()).sum::<usize>() + ours.len();
    if left > out.len() {
        AutoPick { kept, out, mine }
    } else {
        AutoPick {
            kept: sources.to_vec(),
            out: Vec::new(),
            mine: ours.to_vec(),
        }
    }
}

/// Группы, которые клиент собирает сам (D-172): по стране узла и по протоколу. Группа —
/// только где узлов два и больше: из одного выбирать нечего. Своя группа с тем же именем
/// главнее — её не подменяем.
pub(super) fn own_groups(
    sources: &[NodeSource],
    grouping: Grouping,
    taken: &[String],
    health: &crate::nodes::health::Check,
) -> Vec<Mapping> {
    own_buckets(sources, grouping, taken)
        .into_iter()
        .map(|bucket| {
            let mut from: Vec<Value> = Vec::new();
            for id in &bucket.sources {
                let id = Value::from(id.clone());
                if !from.contains(&id) {
                    from.push(id);
                }
            }
            let mut group = Mapping::new();
            Yaml::set(&mut group, "name", Value::from(bucket.name));
            Yaml::set(&mut group, "type", Value::from("url-test"));
            Yaml::set(&mut group, "use", Value::Sequence(from));
            Yaml::set(&mut group, "filter", Value::from(any(&bucket.names)));
            checked(&mut group, health);
            group
        })
        .collect()
}

/// Своя группа клиента: имя, откуда узлы и какие.
struct Bucket {
    name: String,
    sources: Vec<String>,
    names: Vec<String>,
}

fn own_buckets(sources: &[NodeSource], grouping: Grouping, taken: &[String]) -> Vec<Bucket> {
    let mut out = Vec::new();
    if grouping.location {
        out.extend(buckets(sources, GEO_PREFIX, taken, |fact| {
            fact.country.as_deref().map(str::to_lowercase)
        }));
    }
    if grouping.protocol {
        out.extend(buckets(sources, PROTO_PREFIX, taken, |fact| {
            Some(fact.kind.to_lowercase())
        }));
    }
    out
}

/// Узлы всех источников, разложенные по ключу; ключ из одного узла группы не даёт.
fn buckets(
    sources: &[NodeSource],
    prefix: &str,
    taken: &[String],
    key: impl Fn(&NodeFact) -> Option<String>,
) -> Vec<Bucket> {
    let mut found: std::collections::BTreeMap<String, Bucket> = Default::default();
    for source in sources.iter().filter(|source| provides(source)) {
        for fact in &source.facts {
            let Some(value) = key(fact) else { continue };
            let slug: String = value.chars().filter(char::is_ascii_alphanumeric).collect();
            if slug.is_empty() {
                continue;
            }
            let bucket = found.entry(slug.clone()).or_insert_with(|| Bucket {
                name: format!("{prefix}{slug}"),
                sources: Vec::new(),
                names: Vec::new(),
            });
            bucket.sources.push(source.id.clone());
            bucket.names.push(fact.name.clone());
        }
    }
    found
        .into_values()
        .filter(|bucket| bucket.names.len() >= 2 && !taken.contains(&bucket.name))
        .collect()
}

/// Группа клиента с составом — то, что окно показывает во вкладке «Группы» (D-172).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Built {
    pub name: String,
    pub kind: String,
    pub members: Vec<String>,
}

/// Группы, которые клиент кладёт в сборку, с составом: `AUTO`, UDP-группа, автогруппы.
/// Состав считают те же функции, что пишут конфиг, — окно и ядро не разойдутся.
pub fn built(sources: &[NodeSource], client: &Client, taken: &[String]) -> Vec<Built> {
    let mut out = Vec::new();
    if !sources.is_empty() {
        out.push(Built {
            name: AUTO.into(),
            kind: "load-balance".into(),
            members: auto_pick(sources, &[], &client.exclude).members(),
        });
    }
    let udp: Vec<String> = sources
        .iter()
        .flat_map(|source| source.udp.iter().cloned())
        .collect();
    if client.grouping.udp && !udp.is_empty() {
        out.push(Built {
            name: UDP.into(),
            kind: "url-test".into(),
            members: udp,
        });
    }
    out.extend(
        own_buckets(sources, client.grouping, taken)
            .into_iter()
            .map(|bucket| Built {
                name: bucket.name,
                kind: "url-test".into(),
                members: bucket.names,
            }),
    );
    out
}
