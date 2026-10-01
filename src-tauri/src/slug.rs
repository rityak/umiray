//! Имя файла из человеческого имени, и свободное рядом с занятым.
//!
//! Отдельный модуль, потому что файлы по имени заводят двое — встроенные наборы правил
//! и скачанные списки (D-157), — и правило имени у них должно быть одно.

pub struct Slug;

impl Slug {
    /// Всё, что не буква, не цифра и не дефис, становится дефисом: идентификатор уходит
    /// и в путь, и в конфиг, и оставлять там точки и косые черты нельзя. Буквы любые —
    /// кириллица в имени файла законна, а переводить её в латиницу значило бы тащить
    /// таблицу транслитерации ради имени, которое видит один человек.
    ///
    /// `fallback` — имя, когда от исходного не осталось ничего.
    pub fn of(title: &str, fallback: &str) -> String {
        let cut: String = title
            .to_lowercase()
            .chars()
            .map(|letter| {
                if letter.is_alphanumeric() {
                    letter
                } else {
                    '-'
                }
            })
            .collect();
        // Подряд идущие дефисы схлопываем: «C:\Windows» иначе даёт `c--windows`.
        let mut trimmed = String::with_capacity(cut.len());
        for letter in cut.chars() {
            if letter != '-' || !trimmed.ends_with('-') {
                trimmed.push(letter);
            }
        }
        let trimmed = trimmed.trim_matches('-').to_string();
        // Длинное имя файла — это длинный путь, а он на Windows кончается отказом записи.
        let short: String = trimmed.chars().take(40).collect();
        let short = short.trim_matches('-').to_string();
        if short.is_empty() {
            fallback.to_string()
        } else {
            short
        }
    }

    /// Свободное имя рядом с занятым: `свой`, `свой-2`, `свой-3`. Второй с тем же именем —
    /// обычное дело, а молча перезаписать чужой файл нельзя.
    pub fn free(wanted: &str, taken: &[String]) -> String {
        if !taken.iter().any(|id| id == wanted) {
            return wanted.to_string();
        }
        (2..)
            .map(|number| format!("{wanted}-{number}"))
            .find(|candidate| !taken.iter().any(|id| id == candidate))
            .unwrap_or_else(|| wanted.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Идентификатор уходит и в путь, и в конфиг: всё, что могло бы вывести из папки,
    /// обязано превратиться в дефис ещё здесь.
    #[test]
    fn a_name_becomes_a_file_name_that_cannot_leave_the_folder() {
        assert_eq!(Slug::of("Мой набор", "nabor"), "мой-набор");
        assert_eq!(Slug::of("Block Ads!", "nabor"), "block-ads");
        assert_eq!(Slug::of("../../etc/passwd", "nabor"), "etc-passwd");
        assert_eq!(Slug::of(r"C:\Windows", "nabor"), "c-windows");
        assert_eq!(
            Slug::of("...", "nabor"),
            "nabor",
            "пустое имя — тоже имя файла"
        );
        assert_eq!(Slug::of("---", "list"), "list");
        assert!(Slug::of(&"я".repeat(80), "nabor").chars().count() <= 40);
    }

    #[test]
    fn a_taken_name_gets_the_next_free_number() {
        let taken = ["свой".to_string(), "свой-2".to_string()];
        assert_eq!(Slug::free("чужой", &taken), "чужой");
        assert_eq!(Slug::free("свой", &taken), "свой-3");
    }
}
