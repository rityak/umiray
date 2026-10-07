//! Нейтральное описание того, что клиент добавляет в итоговый конфиг ядра.
//!
//! Типы живут рядом со сборкой: пользовательские документы о них не знают, а конкретное
//! написание верхнеуровневых полей mihomo остаётся в `render::mihomo` (D-122, D-140).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSource {
    pub id: String,
    pub path: std::path::PathBuf,
    pub names: Vec<String>,
    pub udp: Vec<String>,
    /// Протокол и страна каждого узла: из них клиент собирает свои группы (D-172).
    pub facts: Vec<NodeFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeFact {
    pub name: String,
    pub kind: String,
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub health: crate::nodes::health::Check,
    pub udp: bool,
    pub mask: crate::config::awg::Mask,
    /// Скачанные списки, уже собранные ядром (D-157).
    pub lists: Vec<RuleSet>,
    /// Кого нет в `AUTO` (D-172).
    pub exclude: crate::config::auto::Exclude,
    /// Какие группы клиент собирает сам (D-172).
    pub grouping: crate::config::auto::Grouping,
}

/// Маршрут выбранного набора (D-158): свои правила, rule sets, готовые наборы и `MATCH`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Route {
    /// Документ набора. Пусто — маршрутизация выключена (D-166) или наборов нет: всё
    /// уходит в `MATCH,umiray`, то есть в выбранный выход.
    pub text: Option<String>,
}

/// Rule set, готовый для ядра: файл на каждую часть, которая у списка есть и собрана.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub id: String,
    pub domains: Option<std::path::PathBuf>,
    pub cidrs: Option<std::path::PathBuf>,
}
