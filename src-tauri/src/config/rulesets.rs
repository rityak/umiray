//! Готовые наборы правил: коллекция-папка и строки в сборку (D-083, D-100, D-158).
//!
//! Какие наборы участвуют в маршруте и куда ведут, решает документ набора маршрутизации —
//! раздел `ready` (D-158). Здесь остаётся то, что знает только про сами наборы: разбор,
//! правка и строки с подставленным выходом.
//!
//! Сами наборы — коллекция `rules` (D-100): их раздаёт и перечисляет `collections`.
//!
//! **Выход набора** отдельным полем не хранится: это общий выход его строк. У `direct-ru`
//! все строки ведут в `DIRECT` — это и есть его выход по умолчанию. У набора со строками
//! в разные выходы общего нет, и переопределить его нельзя — только поправить строки.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::collections;
use crate::collections::Collections;
use crate::config::rules::RulesCodec;
use crate::error::{AppError, Result};
use crate::slug::Slug;
use crate::yaml::Yaml;

/// Набор так, как его видит окно.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ruleset {
    /// Идентификатор в коллекции. Он же имя в разделе `ready` документа.
    pub id: String,
    /// Человеческие имена набора: перевод выбирает окно (D-151).
    pub title: String,
    #[serde(default)]
    pub title_en: Option<String>,
    /// Общий выход строк набора. Пусто — строки ведут в разные выходы (D-158).
    pub target: Option<String>,
    /// Строки правил как есть — окно показывает их, когда набор раскрывают.
    pub rules: Vec<String>,
}

pub struct RulesetStore;

impl RulesetStore {
    /// Все наборы коллекции, по алфавиту: порядок в окне не должен зависеть от того,
    /// как файлы легли на диск.
    pub fn list() -> Vec<Ruleset> {
        Collections::folder(collections::RULES)
            .into_iter()
            .filter_map(|(id, text)| parse(&id, &text))
            .collect()
    }

    /// Строки набора для сборки, с выходом `target` вместо своего (D-158). `None` — выход
    /// самого набора. Набора нет — `None`: документ мог сослаться на удалённый мимо нас,
    /// и VPN из-за этого не должен остаться без маршрута.
    ///
    /// Выход подставляется только набору с общим выходом: у разнородного подмена стёрла бы
    /// то, что человек развёл по разным выходам намеренно.
    pub fn lines(id: &str, target: Option<&str>) -> Option<Vec<String>> {
        let set = RulesetStore::list().into_iter().find(|set| set.id == id)?;
        let Some(target) = target.filter(|_| set.target.is_some()) else {
            return Some(set.rules);
        };
        Some(set.rules.iter().map(|line| aimed(line, target)).collect())
    }

    /// Текст набора как есть — тот же файл, что правят руками (D-104).
    ///
    /// Идентификатор приходит из вебвью, поэтому путём он не становится: файл ищется среди
    /// тех, что коллекция и так перечисляет.
    pub fn read(id: &str) -> Result<String> {
        Collections::folder(collections::RULES)
            .into_iter()
            .find(|(name, _)| name == id)
            .map(|(_, text)| text)
            .ok_or_else(|| AppError::invalid(format!("Неизвестный набор правил: {id}")))
    }

    /// Завести свой набор (D-110). Возвращает идентификатор — окно раскроет его редактором.
    ///
    /// Файл сразу с правилом-примером: набор без правил не показывается вовсе (`parse`),
    /// и заведённый из окна пропал бы у пользователя на глазах. Правило выбрано заведомо
    /// безобидное — и в маршрут новый набор не входит, пока его не выберут.
    pub fn create(title: &str) -> Result<String> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::invalid("У набора должно быть имя"));
        }
        let taken: Vec<String> = Collections::folder(collections::RULES)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let id = Slug::free(&Slug::of(title, "nabor"), &taken);
        let text = format!(
            "title: {title}
rules:
  - DOMAIN-SUFFIX,example.com,DIRECT
"
        );
        Collections::put(collections::RULES, &id, &text)?;
        Ok(id)
    }

    /// Удалить набор вместе с файлом. Документы, которые его выбрали, не падают: сборка
    /// пропустит набор, которого нет, а при следующей записи документ попросит убрать ссылку.
    ///
    /// Удаляем и поставляемый тоже — коллекция принадлежит пользователю (D-100), и
    /// «этот трогать нельзя» было бы враньём: файл всё равно правится руками.
    pub fn delete(id: &str) -> Result<()> {
        RulesetStore::read(id)?;
        Collections::remove(collections::RULES, id)
    }

    /// Записать правку. Перед записью — разбор: файл, который не читается, означал бы набор,
    /// молча выпавший из сборки.
    ///
    /// Заводить новые файлы этим нельзя: правится то, что уже лежит в папке — для нового
    /// есть `create`.
    pub fn write(id: &str, text: &str) -> Result<()> {
        RulesetStore::read(id)?;
        if parse(id, text).is_none() {
            return Err(AppError::invalid(
                "Набор должен быть YAML со списком `rules:` — и хотя бы одним правилом",
            ));
        }
        Collections::put(collections::RULES, id, text)
    }
}

/// Строка правила с другим выходом. Режем без обрезки пробелов: в regex-значении они
/// могут быть смыслом, и собирать строку надо ровно такой, какой она была.
fn aimed(line: &str, target: &str) -> String {
    let mut parts: Vec<&str> = line.split(',').collect();
    let trimmed: Vec<&str> = parts.iter().map(|part| part.trim()).collect();
    match RulesCodec::exit_at(&trimmed) {
        Some(at) => {
            parts[at] = target;
            parts.join(",")
        }
        None => line.to_string(),
    }
}

/// Общий выход строк. Строка без выхода (`SUB-RULE`) общего не нарушает — её подмена
/// не касается.
fn common_target(rules: &[String]) -> Option<String> {
    let mut targets = rules.iter().filter_map(|line| {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        RulesCodec::exit_at(&parts).map(|at| parts[at].to_string())
    });
    let first = targets.next()?;
    targets.all(|target| target == first).then_some(first)
}

/// Файл набора: наш `title` и правила ядра. Без правил набор бессмыслен — такой файл
/// в списке не показываем вовсе.
fn parse(id: &str, text: &str) -> Option<Ruleset> {
    let map = Yaml::top_mapping(text).ok()?;
    let rules: Vec<String> = map
        .get(Value::from("rules"))?
        .as_sequence()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if rules.is_empty() {
        return None;
    }
    Some(Ruleset {
        title_en: map
            .get(Value::from("title_en"))
            .and_then(Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string),
        title: map
            .get(Value::from("title"))
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        id: id.to_string(),
        target: common_target(&rules),
        rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Поставляемые наборы читаются прямо из репозитория — с D-100 они файлы, а не
    /// строковые константы, и разбирать надо ровно то, что уедет пользователю.
    const DIRECT_RU: &str = include_str!("../../../collections/rules/direct-ru.yaml");
    const BLOCK_ADS: &str = include_str!("../../../collections/rules/block-ads.yaml");

    #[test]
    fn a_file_gives_its_title_and_its_rules() {
        let set = parse("direct-ru", DIRECT_RU).unwrap();
        assert_eq!(set.title, "Россия — напрямую");
        assert_eq!(set.title_en.as_deref(), Some("Russia — direct"));
        assert_eq!(set.id, "direct-ru");
        assert!(set.rules.contains(&"DOMAIN-SUFFIX,ru,DIRECT".to_string()));
        assert!(
            !set.rules.iter().any(|rule| rule.contains("push")),
            "пуши — не про страну, в этом наборе им места нет"
        );
        assert_eq!(parse("block-ads", BLOCK_ADS).unwrap().rules.len(), 1);
    }

    /// Выход набора — общий выход его строк (D-158): у поставляемых он один на весь файл.
    #[test]
    fn a_set_s_exit_is_the_one_its_lines_share() {
        assert_eq!(
            parse("direct-ru", DIRECT_RU).unwrap().target.as_deref(),
            Some("DIRECT")
        );
        assert_eq!(
            parse("block-ads", BLOCK_ADS).unwrap().target.as_deref(),
            Some("REJECT")
        );
        let mixed = "rules: ['DOMAIN,a.ru,DIRECT', 'DOMAIN,b.ru,REJECT']";
        assert_eq!(parse("mixed", mixed).unwrap().target, None);
    }

    /// Подмена ставит выход туда, где его читает ядро, и не трогает хвост и regex.
    #[test]
    fn the_exit_goes_where_the_core_reads_it() {
        assert_eq!(
            aimed("GEOIP,RU,DIRECT,no-resolve", "AUTO"),
            "GEOIP,RU,AUTO,no-resolve"
        );
        assert_eq!(
            aimed(r"DOMAIN-REGEX,^(.+\.)?a\.(ru|by)$,DIRECT", "umiray"),
            r"DOMAIN-REGEX,^(.+\.)?a\.(ru|by)$,umiray"
        );
        assert_eq!(
            aimed("AND,((DOMAIN,x.ru),(NETWORK,UDP)),DIRECT", "REJECT"),
            "AND,((DOMAIN,x.ru),(NETWORK,UDP)),REJECT"
        );
    }

    /// Без имени набор всё равно показывается — под именем файла. Без правил не показывается
    /// вовсе: включать в нём нечего.
    #[test]
    fn a_set_without_a_title_is_shown_and_one_without_rules_is_not() {
        let named = parse("свой", "rules: ['MATCH,DIRECT']").unwrap();
        assert_eq!(named.title, "свой");
        assert_eq!(named.title_en, None);
        assert_eq!(
            parse("blank", "title_en: '  '\nrules: ['MATCH,DIRECT']")
                .unwrap()
                .title_en,
            None
        );
        assert!(parse("пустой", "title: Пусто\nrules: []").is_none());
        assert!(parse("никакой", "title: Пусто\n").is_none());
        assert!(parse("сломанный", "%%%").is_none());
    }
}
