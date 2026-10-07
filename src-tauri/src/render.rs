//! Перевод нейтрального описания в конфиг конкретного ядра.
//!
//! Единственное место, которому позволено знать, как поля называются в ядре (D-027).
//! Второе ядро — ещё один модуль рядом.

pub mod effective;
pub mod mihomo;
pub mod mihomo_groups;
pub mod mihomo_lists;
pub mod plan;
