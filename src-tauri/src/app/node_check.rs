//! Проверка записи узла ядром — до того, как она ляжет в источник (D-120, B-021).

use crate::error::{AppError, Result};

pub struct NodeCheck;

impl NodeCheck {
    /// Запись объектом — из формы, файла или WARP.
    pub fn entry(entry: &serde_yaml::Mapping) -> Result<()> {
        NodeCheck::accepted(&[serde_yaml::Value::Mapping(entry.clone())])
    }

    /// Запись кодом — из редактора узла.
    pub fn text(text: &str) -> Result<()> {
        let entry: serde_yaml::Value = serde_yaml::from_str(text)
            .map_err(|e| AppError::invalid(format!("Это не конфиг узла: {e}")))?;
        NodeCheck::accepted(&[entry])
    }

    /// Запись объектом из окна (D-121).
    pub fn object(entry: &serde_json::Value) -> Result<()> {
        let entry = serde_yaml::to_value(entry)
            .map_err(|e| AppError::invalid(format!("Это не запись узла: {e}")))?;
        NodeCheck::accepted(&[entry])
    }

    /// Сырьё источника, поправленное текстом. У источника записей это документ `proxies:` —
    /// его и показываем ядру. Список ссылок проверять незачем: чего разбор не осилил,
    /// в конфиг не идёт (D-122).
    pub fn document(text: &str) -> Result<()> {
        match serde_yaml::from_str::<serde_yaml::Value>(text)
            .ok()
            .and_then(|value| value.get("proxies")?.as_sequence().cloned())
        {
            Some(proxies) => NodeCheck::accepted(&proxies),
            None => Ok(()),
        }
    }

    /// Примет ли ядро эти записи узлов (`mihomo -t` на документе из них одних).
    ///
    /// Одна битая запись — и ядро отвергает **весь** провайдер вместе с рабочими узлами рядом:
    /// `type: vlesss` или запись без `type` — `initial proxy provider … error`, провайдер пуст
    /// (живьём, 01.10.2026). Окно при этом показывало бы узлы как есть. Ядра ещё нет —
    /// проверять нечем, и запись проходит: отказать в узле из-за незагруженного ядра хуже.
    pub fn accepted(proxies: &[serde_yaml::Value]) -> Result<()> {
        if !crate::core::mihomo::Mihomo::binary().exists() {
            return Ok(());
        }
        let mut document = serde_yaml::Mapping::new();
        crate::yaml::Yaml::set(
            &mut document,
            "proxies",
            serde_yaml::Value::Sequence(proxies.to_vec()),
        );
        let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        let said = crate::diag::config::DryRun::accepts(&yaml)?;
        if said.ok {
            return Ok(());
        }
        Err(AppError::invalid(format!(
            "Ядро не примет этот узел: {}",
            said.reason()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(yaml: &str) -> serde_yaml::Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    /// Живая: настоящее ядро из каталога данных. Битая запись — отказ со словами ядра,
    /// здоровая — проходит; без второй половины зелёный ничего бы не значил.
    #[test]
    #[ignore]
    fn live_the_core_refuses_a_node_it_would_drop_the_provider_for() {
        assert!(
            crate::core::mihomo::Mihomo::binary().exists(),
            "ядро не скачано, проверять нечем"
        );
        let good = entry("{name: Good, type: socks5, server: 127.0.0.1, port: 1080}");
        NodeCheck::accepted(std::slice::from_ref(&good)).expect("здоровый узел принят");

        let typo = entry("{name: Typo, type: vlesss, server: a.example.com, port: 443}");
        let refused = NodeCheck::accepted(&[good, typo]).expect_err("опечатка в типе не прошла");
        println!("ядро сказало: {}", refused);
        assert!(refused.to_string().contains("vlesss"), "{refused}");

        let untyped = entry("{name: X, foo: bar}");
        assert!(
            NodeCheck::accepted(&[untyped]).is_err(),
            "запись без типа не прошла"
        );
    }
}
