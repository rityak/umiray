//! Слои крейта (D-155) — проверкой, а не пересказом. Подсистема зависит только от тех,
//! что ниже неё; соседи внутри одной подсистемы — её дело. Новая подсистема без строки
//! в `RANK` — провал теста: у блока обязано быть место на картинке.

/// Сверху вниз: граница, сервисы, инструменты, ядра, сборка конфига, хранилища,
/// платформа, база, примитивы. Больше — выше.
const RANK: &[(&str, u8)] = &[
    ("commands", 90),
    ("app", 80),
    ("diag", 70),
    ("core", 60),
    ("render", 50),
    ("nodes", 40),
    ("lists", 35),
    ("config", 30),
    ("collections", 25),
    ("system", 20),
    // База знает, где она лежит (`paths`), и ничего о доменах (D-170).
    ("db", 15),
    ("paths", 10),
    ("yaml", 10),
    ("http", 10),
    ("stamp", 10),
    ("slug", 10),
    ("atomic", 10),
    ("error", 0),
];

fn rank(subsystem: &str) -> Option<u8> {
    RANK.iter()
        .find(|(name, _)| *name == subsystem)
        .map(|(_, rank)| *rank)
}

/// Рабочий код файла: до модуля тестов и без комментариев — ссылка в тексте не зависимость.
fn code(text: &str) -> String {
    text.split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or_default()
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_subsystem_depends_only_on_the_ones_below() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src.clone()];
    let mut wrong = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if !name.ends_with(".rs")
                || ["main.rs", "live.rs", "layers.rs"].contains(&name.as_str())
            {
                continue;
            }
            let me = name.split('/').next().unwrap().trim_end_matches(".rs");
            let Some(mine) = rank(me) else {
                wrong.push(format!("{name}: подсистема {me} без места в слоях"));
                continue;
            };
            let text = std::fs::read_to_string(&path).unwrap();
            for used in code(&text).split("crate::").skip(1) {
                let target: String = used
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if target == me {
                    continue;
                }
                match rank(&target) {
                    Some(theirs) if theirs < mine => {}
                    Some(_) => wrong.push(format!("{name}: {me} → {target} — вверх или вбок")),
                    None => wrong.push(format!("{name}: {me} → {target} — нет места в слоях")),
                }
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
