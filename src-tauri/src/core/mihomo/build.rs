//! Сборка ядра mihomo (D-194): сам mihomo или его форк miray. Конфиг, API и хуки у них
//! общие — различаются бинарь на диске и релиз, откуда он качается.
//!
//! Выбор помнит этот модуль (строка `core-build` в `state`). Какой сборкой работать на деле,
//! решает `pick`: выбранной, если она на диске, иначе другой — запасной.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::db::{Db, Table};
use crate::error::Result;
use crate::paths::Paths;

const ROW: &str = "core-build";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Build {
    #[default]
    Miray,
    Mihomo,
}

impl Build {
    pub const ALL: [Build; 2] = [Build::Miray, Build::Mihomo];

    pub fn name(self) -> &'static str {
        match self {
            Build::Miray => "miray",
            Build::Mihomo => "mihomo",
        }
    }

    pub fn binary(self) -> PathBuf {
        match self {
            Build::Miray => Paths::miray(),
            Build::Mihomo => Paths::core(),
        }
    }

    pub fn present(self) -> bool {
        self.binary().exists()
    }

    fn other(self) -> Build {
        match self {
            Build::Miray => Build::Mihomo,
            Build::Mihomo => Build::Miray,
        }
    }

    /// Что выбрал человек. На macOS умолчание — mihomo: у miray там нет релизов.
    pub fn chosen() -> Build {
        match Db::get(Table::State, ROW, "").ok().flatten().as_deref() {
            Some("mihomo") => Build::Mihomo,
            Some("miray") => Build::Miray,
            _ if cfg!(target_os = "macos") => Build::Mihomo,
            _ => Build::Miray,
        }
    }

    pub fn choose(self) -> Result<()> {
        if cfg!(target_os = "macos") && self == Build::Miray {
            return Err(crate::error::AppError::invalid(
                "Miray не публикует сборок для macOS — используйте mihomo",
            ));
        }
        Db::put(Table::State, ROW, "", self.name())
    }

    /// Какой сборкой запускать сейчас.
    pub fn current() -> Build {
        let chosen = Build::chosen();
        if cfg!(target_os = "macos") && chosen == Build::Miray && !Build::Miray.present() {
            return Build::Mihomo;
        }
        pick(chosen, Build::present)
    }
}

/// Выбранная, если на диске; нет её, но есть другая — другая. Нет ни одной — выбранная:
/// отказ «ядра нет» назовёт тот файл, который и надо скачать.
fn pick(chosen: Build, present: impl Fn(Build) -> bool) -> Build {
    if present(chosen) || !present(chosen.other()) {
        chosen
    } else {
        chosen.other()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_build_falls_back_to_the_other() {
        let only = |build: Build| move |seen: Build| seen == build;
        assert_eq!(pick(Build::Miray, only(Build::Miray)), Build::Miray);
        assert_eq!(pick(Build::Miray, only(Build::Mihomo)), Build::Mihomo);
        assert_eq!(pick(Build::Mihomo, only(Build::Miray)), Build::Miray);
        assert_eq!(pick(Build::Miray, |_| true), Build::Miray);
        assert_eq!(pick(Build::Mihomo, |_| false), Build::Mihomo);
    }

    #[test]
    fn the_choice_survives_and_the_default_has_a_native_release() {
        let _sandbox = crate::paths::Sandbox::new("core-build");
        let fallback = if cfg!(target_os = "macos") {
            Build::Mihomo
        } else {
            Build::Miray
        };
        assert_eq!(Build::chosen(), fallback);
        Build::Mihomo.choose().unwrap();
        assert_eq!(Build::chosen(), Build::Mihomo);
    }
}
