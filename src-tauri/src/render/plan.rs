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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub health: String,
    pub udp: bool,
    pub mask: crate::config::awg::Mask,
}
