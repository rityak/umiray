//! Маршрутизация формой: разбор `rules` в строки окна и сборка обратно (D-074).
//!
//! Устроено как `config::groups` — две чистые функции над текстом черновика, без диска.
//! Разница в одном: правило ядра — это строка `ВИД,значение,куда[,хвост]`, и **одно
//! правило окна держит несколько значений**. В конфиг оно разворачивается в столько же
//! строк, а на экране занимает одну: ради этого мультизначение и заведено.
//!
//! Схлопываются только **соседние** строки с одинаковыми видом, назначением и хвостом.
//! Не соседние трогать нельзя: побеждает первое совпавшее, и подъём строки к своей
//! родне поменял бы маршрут.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::config::direction::SELECTOR;
use crate::config::route::{ReadyUse, RuleSetUse, Sections};
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Ключ, за который отвечает раздел «Маршрутизация».
const KEY: &str = "rules";

/// Последнее правило: куда идёт всё, что не совпало.
const MATCH: &str = "MATCH";

/// Флаг, который люди пишут в хвост правила, в том числе доменного.
const NO_RESOLVE: &str = "no-resolve";

/// Правило так, как его видит окно.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// `DOMAIN-SUFFIX`, `IP-CIDR`, `GEOIP`… Что именно умеет ядро, форма не проверяет:
    /// список видов у него длинный и растёт, а незнакомый вид — это не повод не показать
    /// строку.
    pub kind: String,
    /// Значения одного вида и одного назначения. Каждое станет отдельной строкой конфига.
    pub values: Vec<String>,
    pub target: String,
    /// Хвост правила: `no-resolve` и подобное. Форма его не трогает, но и не теряет.
    #[serde(default)]
    pub options: Vec<String>,
}

/// Весь документ маршрутизации: правила по порядку и судьба всего остального.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routing {
    pub rules: Vec<Rule>,
    /// Цель `MATCH`. Его в документе может не быть вовсе — тогда здесь псевдоним,
    /// в который целится сборка (D-053), и при первой же записи он появится в файле.
    pub fallback: String,
    /// Скачанные списки маршрута (D-157, D-158) — раздел `rule-sets`.
    #[serde(default)]
    pub rule_sets: Vec<RuleSetUse>,
    /// Готовые наборы маршрута (D-083, D-158) — раздел `ready`.
    #[serde(default)]
    pub ready: Vec<ReadyUse>,
}

pub struct RulesCodec;

impl RulesCodec {
    /// Правила документа. Отсутствие ключа `rules` — это ноль правил, а не ошибка.
    ///
    /// Строку, которую форма не собрала бы обратно без потерь, она отвергает: половина
    /// документа в окне и запись поверх второй половины хуже, чем честный отказ.
    pub fn parse(text: &str) -> Result<Routing> {
        let map = Yaml::top_mapping(text)?;
        let mut rules: Vec<Rule> = Vec::new();
        let mut fallback = SELECTOR.to_string();

        let lines: Vec<String> = match map.get(Value::from(KEY)) {
            None => Vec::new(),
            Some(value) if value.is_null() => Vec::new(),
            Some(value) => {
                let list = value
                    .as_sequence()
                    .ok_or_else(|| AppError::invalid("rules должен быть списком правил"))?;
                list.iter()
                    .map(|item| {
                        item.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| AppError::invalid("Правило должно быть строкой"))
                    })
                    .collect::<Result<Vec<String>>>()?
            }
        };

        let last = lines.len().saturating_sub(1);
        for (at, line) in lines.iter().enumerate() {
            let raw: Vec<&str> = line.split(',').collect();
            let parts: Vec<String> = raw.iter().map(|part| part.trim().to_string()).collect();
            if parts[0] == MATCH {
                if at != last {
                    return Err(AppError::invalid(
                    "MATCH стоит не последним: всё, что ниже него, недостижимо. Поправьте кодом.",
                ));
                }
                fallback = parts
                    .get(1)
                    .cloned()
                    .ok_or_else(|| AppError::invalid("У MATCH не указано, куда отправлять"))?;
                break;
            }
            if parts.len() < 3 {
                return Err(AppError::invalid(format!(
                    "Правило «{line}» не похоже на «вид,значение,куда» — поправьте кодом"
                )));
            }
            // Цель — там, где её ищет сборка; у `SUB-RULE` её нет, и последняя часть —
            // имя набора `sub-rules`.
            let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
            let at = Self::exit_at(&refs).unwrap_or(parts.len() - 1);
            let rule = Rule {
                kind: parts[0].clone(),
                // Значение — как написано: пробел после запятой внутри регулярки — её часть,
                // обрезанные части склеивались в другую регулярку (B-045).
                values: vec![raw[1..at].join(",").trim().to_string()],
                target: parts[at].clone(),
                options: parts[at + 1..].to_vec(),
            };
            // Соседнее правило того же вида и с тем же назначением — это второе значение
            // того же правила окна, а не вторая строка списка.
            match rules.last_mut() {
                Some(previous) if same(previous, &rule) => previous.values.extend(rule.values),
                _ => rules.push(rule),
            }
        }

        let sections = Sections::read(text)?;
        Ok(Routing {
            rules,
            fallback,
            rule_sets: sections.rule_sets,
            ready: sections.ready,
        })
    }

    /// Собрать документ заново с этими правилами. `MATCH` пишется последним всегда — на него
    /// смотрит весь непойманный трафик, и его отсутствие означало бы, что судьбу остального
    /// решает не этот документ.
    pub fn render(text: &str, routing: &Routing) -> Result<String> {
        let mut map = Yaml::top_mapping(text)?;
        // Ядро режет строку правила по запятой, и цель с ней стала бы своим началом —
        // правило молча ушло бы в `umiray` (B-043). Отказ словами лучше тихого увода.
        if let Some(target) = routing
            .rules
            .iter()
            .map(|rule| rule.target.trim())
            .chain(routing.rule_sets.iter().map(|set| set.target.trim()))
            .chain(routing.ready.iter().filter_map(|set| set.target.as_deref()))
            .chain([routing.fallback.trim()])
            .find(|target| target.contains(','))
        {
            return Err(AppError::invalid(format!(
                "Правило не может вести в «{target}»: ядро режет правило по запятой. \
Переименуйте группу или узел без запятой."
            )));
        }
        let mut lines: Vec<Value> = Vec::new();
        for rule in &routing.rules {
            // У обычного правила запятая делит строку ядра: значение с ней — это несколько
            // значений, иначе часть его стала бы целью. У regex и составных запятая своя.
            let literal = literal_payload(rule.kind.trim());
            let values = rule.values.iter().flat_map(|value| {
                if literal {
                    vec![value.as_str()]
                } else {
                    value.split(',').collect()
                }
            });
            for value in values {
                let value = value.trim();
                // Правило без значения ядро не примет: пустая строка формы в конфиг не едет.
                if value.is_empty() || rule.kind.trim().is_empty() || rule.target.trim().is_empty()
                {
                    continue;
                }
                let mut parts = vec![rule.kind.trim(), value, rule.target.trim()];
                parts.extend(rule.options.iter().map(|option| option.trim()));
                lines.push(Value::from(parts.join(",")));
            }
        }
        let fallback = match routing.fallback.trim() {
            "" => SELECTOR,
            target => target,
        };
        lines.push(Value::from(format!("{MATCH},{fallback}")));
        Yaml::set(&mut map, KEY, Value::Sequence(lines));
        Sections::write(
            &mut map,
            &Sections {
                rule_sets: routing.rule_sets.clone(),
                ready: routing.ready.clone(),
            },
        )?;
        serde_yaml::to_string(&Value::Mapping(map)).map_err(|e| AppError::invalid(e.to_string()))
    }

    /// Какая часть строки правила называет выход. `SUB-RULE` выхода не называет — его цель
    /// набор `sub-rules` (D-156).
    pub fn exit_at(parts: &[&str]) -> Option<usize> {
        let kind = *parts.first()?;
        if kind == MATCH {
            return (parts.len() > 1).then_some(1);
        }
        if kind == "SUB-RULE" || parts.len() < 3 {
            return None;
        }
        Some(if literal_payload(kind) {
            parts.len() - 1 - usize::from(Self::flag_after_exit(parts))
        } else {
            2
        })
    }

    /// `no-resolve` в хвосте правила, у которого цель — последняя часть строки (regex,
    /// составные). Человек пишет его флагом и у доменных правил (B-035); ядро прочло бы его
    /// целью — сборка отдаёт такую строку без хвоста.
    pub fn flag_after_exit(parts: &[&str]) -> bool {
        parts.len() > 3 && literal_payload(parts[0]) && parts.last() == Some(&NO_RESOLVE)
    }
}

/// Как читает ядро: у составных и regex-правил запятые живут в значении, и цель — последняя
/// часть строки.
fn literal_payload(kind: &str) -> bool {
    matches!(
        kind,
        "NOT"
            | "OR"
            | "AND"
            | "SUB-RULE"
            | "DOMAIN-REGEX"
            | "PROCESS-NAME-REGEX"
            | "PROCESS-PATH-REGEX"
    )
}

fn same(previous: &Rule, next: &Rule) -> bool {
    previous.kind == next.kind && previous.target == next.target && previous.options == next.options
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: &str = "rules:
  - DOMAIN-SUFFIX,github.com,umiray
  - DOMAIN-SUFFIX,gitlab.com,umiray
  - GEOIP,RU,DIRECT,no-resolve
  - DOMAIN-SUFFIX,mos.ru,DIRECT
  - MATCH,umiray
";

    #[test]
    fn regex_and_composite_payloads_keep_commas() {
        let text = "rules:\n  - DOMAIN-REGEX,^file{1,3}$,DIRECT\n  - AND,((DOMAIN,example.org),(NETWORK,TCP)),REJECT\n  - MATCH,umiray\n";
        let parsed = RulesCodec::parse(text).unwrap();
        assert_eq!(parsed.rules[0].values, vec!["^file{1,3}$"]);
        assert_eq!(parsed.rules[0].target, "DIRECT");
        assert_eq!(parsed.rules[1].target, "REJECT");
        assert_eq!(
            RulesCodec::parse(&RulesCodec::render(text, &parsed).unwrap()).unwrap(),
            parsed
        );
    }

    #[test]
    fn neighbours_of_one_kind_become_one_rule_with_two_values() {
        let routing = RulesCodec::parse(MINE).unwrap();
        assert_eq!(routing.rules.len(), 3, "пять строк — три правила");
        assert_eq!(routing.rules[0].kind, "DOMAIN-SUFFIX");
        assert_eq!(routing.rules[0].values, vec!["github.com", "gitlab.com"]);
        assert_eq!(routing.rules[0].target, "umiray");
        assert_eq!(routing.rules[1].options, vec!["no-resolve"]);
        assert_eq!(routing.fallback, "umiray");
    }

    /// Не соседние строки одного вида схлопывать нельзя: побеждает первое совпавшее,
    /// и подъём `mos.ru` к своей родне увёл бы его в VPN вместо прямого выхода.
    #[test]
    fn a_rule_never_jumps_over_its_neighbour() {
        let routing = RulesCodec::parse(MINE).unwrap();
        assert_eq!(routing.rules[2].values, vec!["mos.ru"]);
        assert_eq!(routing.rules[2].target, "DIRECT");
        let out = RulesCodec::render(MINE, &routing).unwrap();
        assert_eq!(
            Yaml::top_mapping(&out).unwrap()["rules"],
            Yaml::top_mapping(MINE).unwrap()["rules"],
            "круг «разобрали — собрали» обязан сходиться строка в строку"
        );
    }

    /// B-045: разбор резал строку по запятым и обрезал части — пробел после запятой внутри
    /// регулярки пропадал, и после правки формой она переставала совпадать.
    #[test]
    fn a_space_after_a_comma_inside_a_regex_survives_the_form() {
        let text = "rules:
  - DOMAIN-REGEX,^a, b$,DIRECT
  - MATCH,umiray
";
        let routing = RulesCodec::parse(text).unwrap();
        assert_eq!(routing.rules[0].values, vec!["^a, b$"]);
        let out = RulesCodec::render(text, &routing).unwrap();
        assert_eq!(
            Yaml::top_mapping(&out).unwrap()["rules"][0],
            Value::from("DOMAIN-REGEX,^a, b$,DIRECT")
        );
    }

    #[test]
    fn a_new_value_becomes_a_new_line_next_to_its_own() {
        let mut routing = RulesCodec::parse(MINE).unwrap();
        routing.rules[0].values.push("npmjs.org".into());
        let out = RulesCodec::render(MINE, &routing).unwrap();
        let rules = Yaml::top_mapping(&out).unwrap()["rules"].clone();
        let list = rules.as_sequence().unwrap();
        assert_eq!(list.len(), 6);
        assert_eq!(list[2], Value::from("DOMAIN-SUFFIX,npmjs.org,umiray"));
    }

    /// MATCH — дно списка, а не его строка: он всегда последний и всегда есть.
    #[test]
    fn match_is_written_last_even_when_the_file_had_none() {
        let routing = RulesCodec::parse("rules:\n  - DOMAIN,a.ru,DIRECT\n").unwrap();
        assert_eq!(
            routing.fallback, SELECTOR,
            "нет MATCH — целимся в псевдоним"
        );
        let mut routing = routing;
        routing.fallback = "DIRECT".into();
        let out = RulesCodec::render("rules:\n  - DOMAIN,a.ru,DIRECT\n", &routing).unwrap();
        let rules = Yaml::top_mapping(&out).unwrap()["rules"].clone();
        let list = rules.as_sequence().unwrap();
        assert_eq!(list.last().unwrap(), &Value::from("MATCH,DIRECT"));
    }

    /// Граница с окном (D-158): то, что шлёт форма, разбирается и ложится в текст разделами —
    /// приоритет словом, отсутствующие поля не пишутся.
    #[test]
    fn what_the_window_sends_becomes_the_route_sections() {
        let routing: Routing = serde_json::from_str(
            r#"{"rules":[],"fallback":"umiray",
                "ruleSets":[{"id":"antizapret","target":"AUTO","priority":"high"}],
                "ready":[{"id":"direct-ru","priority":"low"},{"id":"block-ads"}]}"#,
        )
        .unwrap();
        let text = RulesCodec::render("rules: ['MATCH,umiray']\n", &routing).unwrap();
        assert!(text.contains("priority: high"), "{text}");
        assert!(text.contains("priority: low"), "{text}");
        assert!(!text.contains("target: null"), "{text}");
        assert_eq!(RulesCodec::parse(&text).unwrap(), routing);
    }

    #[test]
    fn an_empty_value_does_not_reach_the_core() {
        let routing = Routing {
            rules: vec![Rule {
                kind: "DOMAIN".into(),
                values: vec!["   ".into(), "a.ru".into()],
                target: "DIRECT".into(),
                options: Vec::new(),
            }],
            fallback: "umiray".into(),
            rule_sets: Vec::new(),
            ready: Vec::new(),
        };
        let out = RulesCodec::render("", &routing).unwrap();
        let rules = Yaml::top_mapping(&out).unwrap()["rules"].clone();
        assert_eq!(rules.as_sequence().unwrap().len(), 2, "правило и MATCH");
    }

    /// Запятая в значении обычного правила делит строку ядра: `a.ru, b.ru` стало бы
    /// значением `a.ru` с целью `b.ru`. Такое значение — два значения, а не одно.
    #[test]
    fn a_comma_inside_a_plain_value_splits_it_instead_of_shifting_the_target() {
        let routing = Routing {
            rules: vec![
                Rule {
                    kind: "DOMAIN-SUFFIX".into(),
                    values: vec!["a.ru, b.ru".into(), "c.ru,".into()],
                    target: "DIRECT".into(),
                    options: Vec::new(),
                },
                Rule {
                    kind: "DOMAIN-REGEX".into(),
                    values: vec!["^x{1,3}\\.ru$".into()],
                    target: "DIRECT".into(),
                    options: Vec::new(),
                },
            ],
            fallback: "umiray".into(),
            rule_sets: Vec::new(),
            ready: Vec::new(),
        };
        let out = RulesCodec::render("", &routing).unwrap();
        let back = RulesCodec::parse(&out).unwrap();
        assert_eq!(back.rules[0].values, vec!["a.ru", "b.ru", "c.ru"], "{out}");
        assert_eq!(back.rules[0].target, "DIRECT");
        assert_eq!(
            back.rules[1].values,
            vec!["^x{1,3}\\.ru$"],
            "у regex запятая своя"
        );
    }

    /// `no-resolve` пишут и у доменных правил — у regex и составных цель последняя часть
    /// строки, и хвост читался целью. Целью остаётся `DIRECT`, хвост — опцией и переживает
    /// правку формой.
    #[test]
    fn no_resolve_after_a_regex_rule_is_a_flag_not_its_target() {
        let text = "rules:
  - DOMAIN-REGEX,^(.+[.])?ozon[.](by|kz)$,DIRECT,no-resolve
  - MATCH,DIRECT
";
        let routing = RulesCodec::parse(text).unwrap();
        assert_eq!(routing.rules[0].target, "DIRECT");
        assert_eq!(routing.rules[0].values, vec!["^(.+[.])?ozon[.](by|kz)$"]);
        assert_eq!(routing.rules[0].options, vec!["no-resolve"]);
        let back = RulesCodec::parse(&RulesCodec::render(text, &routing).unwrap()).unwrap();
        assert_eq!(back, routing);

        let parts = ["DOMAIN-REGEX", "^a$", "DIRECT", "no-resolve"];
        assert_eq!(RulesCodec::exit_at(&parts), Some(2));
    }

    /// B-043: цель с запятой ядро режет — `Европа, быстрые` стала бы целью `Европа`,
    /// и правило молча ушло бы в `umiray`. Такую запись отвергаем словами.
    #[test]
    fn a_target_with_a_comma_is_refused_instead_of_cut() {
        let rule = |target: &str| Routing {
            rules: vec![Rule {
                kind: "DOMAIN-SUFFIX".into(),
                values: vec!["netflix.com".into()],
                target: target.into(),
                options: Vec::new(),
            }],
            fallback: "umiray".into(),
            rule_sets: Vec::new(),
            ready: Vec::new(),
        };
        assert!(RulesCodec::render("", &rule("Европа, быстрые")).is_err());
        assert!(RulesCodec::render("", &rule("Европа")).is_ok());
        let mut fallback = rule("DIRECT");
        fallback.fallback = "Европа, быстрые".into();
        assert!(RulesCodec::render("", &fallback).is_err(), "и у MATCH");
    }

    #[test]
    fn the_rest_of_the_document_is_none_of_our_business() {
        // Скобки — это поток YAML, а не одна строка: `[MATCH,DIRECT]` разбирается
        // в два элемента. Правило целиком поэтому в кавычках.
        let text = "log-level: debug\nrules: ['MATCH,DIRECT']\n";
        let out = RulesCodec::render(text, &RulesCodec::parse(text).unwrap()).unwrap();
        assert_eq!(
            Yaml::top_mapping(&out).unwrap()["log-level"],
            Value::from("debug")
        );
    }

    #[test]
    fn a_document_the_form_cannot_rebuild_is_refused() {
        assert!(RulesCodec::parse("rules:\n  - DOMAIN-SUFFIX\n").is_err());
        assert!(
            RulesCodec::parse("rules:\n  - MATCH,umiray\n  - DOMAIN,a.ru,DIRECT\n").is_err(),
            "ниже MATCH ничего не работает"
        );
        assert!(RulesCodec::parse("rules:\n  - [a, b]\n").is_err());
        assert!(RulesCodec::parse("").unwrap().rules.is_empty());
    }
}
