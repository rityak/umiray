//! Запись документа набора маршрутизации с проверкой по смыслу (D-158).
//!
//! Форма проверяет код тем, что не показывает того, чего не соберёт обратно (D-074). Код
//! проверяется здесь, при записи: разделы по форме (`route`) и названное в них существует.
//! Свои правила по смыслу не проверяются — ядро знает о них больше нас, и ради них код
//! и нужен.

use crate::collections::Collections;
use crate::config::files::Documents;
use crate::config::route::{Sections, READY, RULE_SETS};
use crate::config::rulesets::RulesetStore;
use crate::error::{AppError, Result};

/// Часть набора, в которой живёт маршрут.
const RULES_PART: &str = "rules";

pub struct RouteDocument;

impl RouteDocument {
    /// Записать документ; маршрут набора — только если он проходит проверку.
    pub fn write(id: &str, text: &str) -> Result<()> {
        if Documents::part_and_preset(id).is_some_and(|(part, _)| part == RULES_PART) {
            RouteDocument::check(text)?;
        }
        Documents::write(id, text)
    }

    /// Названное в `rule-sets` и `ready` существует. Список без `url` должен быть в каталоге:
    /// иначе клиенту неоткуда его взять, и строка молча выпала бы из маршрута.
    pub fn check(text: &str) -> Result<()> {
        let sections = Sections::read(text)?;
        let catalog = Collections::lists()?.lists;
        for set in &sections.rule_sets {
            if set.url.is_none() && !catalog.iter().any(|entry| entry.id == set.id) {
                return Err(AppError::invalid(format!(
                    "{RULE_SETS}: «{}» нет в каталоге — добавьте `url:`, откуда его качать",
                    set.id
                )));
            }
        }
        let ready = RulesetStore::list();
        for set in &sections.ready {
            if !ready.iter().any(|known| known.id == set.id) {
                return Err(AppError::invalid(format!(
                    "{READY}: готового набора «{}» нет — уберите его или заведите заново",
                    set.id
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каталог и готовые наборы читаются с диска, а при его отсутствии — из образцов,
    /// вшитых в бинарь: проверка видит то же, что увидит пользователь на чистой машине.
    #[test]
    fn a_name_nobody_knows_is_refused_and_a_known_one_passes() {
        assert!(RouteDocument::check("rule-sets:\n  - {id: antifilter, target: umiray}\n").is_ok());
        assert!(RouteDocument::check(
            "rule-sets:\n  - {id: mine, url: 'https://x.org/l.txt', target: DIRECT}\n"
        )
        .is_ok());
        let unknown = RouteDocument::check("rule-sets:\n  - {id: mine, target: DIRECT}\n");
        assert!(unknown.unwrap_err().to_string().contains("url"));
    }
}
