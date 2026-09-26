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
use crate::error::{AppError, Result};
use crate::yaml::{set, top_mapping};

/// Ключ, за который отвечает раздел «Маршрутизация».
const KEY: &str = "rules";

/// Последнее правило: куда идёт всё, что не совпало.
const MATCH: &str = "MATCH";

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
}

/// Правила документа. Отсутствие ключа `rules` — это ноль правил, а не ошибка.
///
/// Строку, которую форма не собрала бы обратно без потерь, она отвергает: половина
/// документа в окне и запись поверх второй половины хуже, чем честный отказ.
pub fn parse(text: &str) -> Result<Routing> {
    let map = top_mapping(text)?;
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
        let parts: Vec<String> = line
            .split(',')
            .map(|part| part.trim().to_string())
            .collect();
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
        // Match mihomo's payload parser: regex/composite rules use the final part as target.
        let literal_payload = matches!(
            parts[0].as_str(),
            "NOT"
                | "OR"
                | "AND"
                | "SUB-RULE"
                | "DOMAIN-REGEX"
                | "PROCESS-NAME-REGEX"
                | "PROCESS-PATH-REGEX"
        );
        let rule = Rule {
            kind: parts[0].clone(),
            values: vec![if literal_payload {
                parts[1..parts.len() - 1].join(",")
            } else {
                parts[1].clone()
            }],
            target: if literal_payload {
                parts.last().unwrap().clone()
            } else {
                parts[2].clone()
            },
            options: if literal_payload {
                Vec::new()
            } else {
                parts[3..].to_vec()
            },
        };
        // Соседнее правило того же вида и с тем же назначением — это второе значение
        // того же правила окна, а не вторая строка списка.
        match rules.last_mut() {
            Some(previous) if same(previous, &rule) => previous.values.extend(rule.values),
            _ => rules.push(rule),
        }
    }

    Ok(Routing { rules, fallback })
}

fn same(previous: &Rule, next: &Rule) -> bool {
    previous.kind == next.kind && previous.target == next.target && previous.options == next.options
}

/// Собрать документ заново с этими правилами. `MATCH` пишется последним всегда — на него
/// смотрит весь непойманный трафик, и его отсутствие означало бы, что судьбу остального
/// решает не этот документ.
pub fn render(text: &str, routing: &Routing) -> Result<String> {
    let mut map = top_mapping(text)?;
    let mut lines: Vec<Value> = Vec::new();
    for rule in &routing.rules {
        for value in &rule.values {
            let value = value.trim();
            // Правило без значения ядро не примет: пустая строка формы в конфиг не едет.
            if value.is_empty() || rule.kind.trim().is_empty() || rule.target.trim().is_empty() {
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
    set(&mut map, KEY, Value::Sequence(lines));
    serde_yaml::to_string(&Value::Mapping(map)).map_err(|e| AppError::invalid(e.to_string()))
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
        let parsed = parse(text).unwrap();
        assert_eq!(parsed.rules[0].values, vec!["^file{1,3}$"]);
        assert_eq!(parsed.rules[0].target, "DIRECT");
        assert_eq!(parsed.rules[1].target, "REJECT");
        assert_eq!(parse(&render(text, &parsed).unwrap()).unwrap(), parsed);
    }

    #[test]
    fn neighbours_of_one_kind_become_one_rule_with_two_values() {
        let routing = parse(MINE).unwrap();
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
        let routing = parse(MINE).unwrap();
        assert_eq!(routing.rules[2].values, vec!["mos.ru"]);
        assert_eq!(routing.rules[2].target, "DIRECT");
        let out = render(MINE, &routing).unwrap();
        assert_eq!(
            top_mapping(&out).unwrap()["rules"],
            top_mapping(MINE).unwrap()["rules"],
            "круг «разобрали — собрали» обязан сходиться строка в строку"
        );
    }

    #[test]
    fn a_new_value_becomes_a_new_line_next_to_its_own() {
        let mut routing = parse(MINE).unwrap();
        routing.rules[0].values.push("npmjs.org".into());
        let out = render(MINE, &routing).unwrap();
        let rules = top_mapping(&out).unwrap()["rules"].clone();
        let list = rules.as_sequence().unwrap();
        assert_eq!(list.len(), 6);
        assert_eq!(list[2], Value::from("DOMAIN-SUFFIX,npmjs.org,umiray"));
    }

    /// MATCH — дно списка, а не его строка: он всегда последний и всегда есть.
    #[test]
    fn match_is_written_last_even_when_the_file_had_none() {
        let routing = parse("rules:\n  - DOMAIN,a.ru,DIRECT\n").unwrap();
        assert_eq!(
            routing.fallback, SELECTOR,
            "нет MATCH — целимся в псевдоним"
        );
        let mut routing = routing;
        routing.fallback = "DIRECT".into();
        let out = render("rules:\n  - DOMAIN,a.ru,DIRECT\n", &routing).unwrap();
        let rules = top_mapping(&out).unwrap()["rules"].clone();
        let list = rules.as_sequence().unwrap();
        assert_eq!(list.last().unwrap(), &Value::from("MATCH,DIRECT"));
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
        };
        let out = render("", &routing).unwrap();
        let rules = top_mapping(&out).unwrap()["rules"].clone();
        assert_eq!(rules.as_sequence().unwrap().len(), 2, "правило и MATCH");
    }

    #[test]
    fn the_rest_of_the_document_is_none_of_our_business() {
        // Скобки — это поток YAML, а не одна строка: `[MATCH,DIRECT]` разбирается
        // в два элемента. Правило целиком поэтому в кавычках.
        let text = "log-level: debug\nrules: ['MATCH,DIRECT']\n";
        let out = render(text, &parse(text).unwrap()).unwrap();
        assert_eq!(
            top_mapping(&out).unwrap()["log-level"],
            Value::from("debug")
        );
    }

    #[test]
    fn a_document_the_form_cannot_rebuild_is_refused() {
        assert!(parse("rules:\n  - DOMAIN-SUFFIX\n").is_err());
        assert!(
            parse("rules:\n  - MATCH,umiray\n  - DOMAIN,a.ru,DIRECT\n").is_err(),
            "ниже MATCH ничего не работает"
        );
        assert!(parse("rules:\n  - [a, b]\n").is_err());
        assert!(parse("").unwrap().rules.is_empty());
    }
}
