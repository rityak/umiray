//! Преобразование сырья источника в записи ядра.

use crate::error::{AppError, Result};
use crate::nodes::entries::EntryPatch;

/// Ссылки источника — в документ `proxies:` (D-122). Второй список — то, чего разбор
/// не осилил: такие ссылки в конфиг не идут, но в окне видны.
///
/// Правка ложится **поверх** разобранного, а не в него: свежий ключ с сервера должен
/// доезжать и через неделю после правки.
pub(super) fn converted(
    id: &str,
    lines: Vec<String>,
    mask: &crate::config::awg::Mask,
) -> Result<(String, Vec<String>)> {
    let written = EntryPatch::load(id);
    let mask = mask.option();
    let mut proxies = Vec::new();
    let mut skipped = Vec::new();
    for line in lines {
        match crate::nodes::convert::Converter::to_entry(&line) {
            Some(mut entry) => {
                // Маскировка рукопожатия — настройка клиента, а живёт она полем узла
                // (D-118). Кладём её здесь, где узел и рождается, и только узлу
                // WireGuard: у остальных такого поля нет.
                if entry
                    .get(serde_yaml::Value::from("type"))
                    .and_then(serde_yaml::Value::as_str)
                    == Some("wireguard")
                {
                    if let Some(option) = mask.clone() {
                        crate::yaml::Yaml::set(&mut entry, "amnezia-wg-option", option);
                    }
                }
                if let Some(patch) =
                    SourceBuilder::identity_of(&entry).and_then(|key| written.get(&key))
                {
                    EntryPatch::apply(patch, &mut entry);
                }
                proxies.push(serde_yaml::Value::Mapping(entry));
            }
            None => skipped.push(line),
        }
    }
    let mut document = serde_yaml::Mapping::new();
    crate::yaml::Yaml::set(
        &mut document,
        "proxies",
        serde_yaml::Value::Sequence(proxies),
    );
    let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
        .map_err(|e| AppError::invalid(e.to_string()))?;
    Ok((text, skipped))
}

pub struct SourceBuilder;

impl SourceBuilder {
    /// Тождество узла — по **записи**, а не по строке: строки больше нет (D-122).
    ///
    /// Имя в ключ не входит: панели переименовывают узлы, да и мы сами их чистим. `sni` тоже
    /// не входит — его ротируют, и узел «переезжал» бы на ровном месте. Путь и имя
    /// gRPC-сервиса входят: за одним адресом бывает несколько разных узлов.
    pub fn identity_of(entry: &serde_yaml::Mapping) -> Option<String> {
        let text = |key: &str| {
            entry
                .get(serde_yaml::Value::from(key))
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let kind = text("type");
        if kind.is_empty() {
            return None;
        }
        let port = entry
            .get(serde_yaml::Value::from("port"))
            .and_then(serde_yaml::Value::as_u64)
            .unwrap_or_default();
        let path = ["ws-opts", "h2-opts", "xhttp-opts"]
            .iter()
            .find_map(|opts| {
                entry
                    .get(serde_yaml::Value::from(*opts))?
                    .get("path")?
                    .as_str()
                    .map(str::to_string)
            })
            .or_else(|| {
                entry
                    .get(serde_yaml::Value::from("grpc-opts"))?
                    .get("grpc-service-name")?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        Some(format!("{kind}|{}:{port}|{path}", text("server")))
    }
}
