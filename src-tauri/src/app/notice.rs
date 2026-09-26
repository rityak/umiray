//! Что сейчас не так — строкой для окна (D-115).
//!
//! Жалоба это **состояние**, а не событие: висит, пока есть повод, и пропадает сама.
//! Событий у нас нет вовсе — окно опрашивает статус (D-007), — и заводить их ради
//! «всплывашки» значило бы завести вторую дорогу для того же самого.
//!
//! Поводов стало больше одного, поэтому здесь реестр, а не поле: у каждой жалобы есть
//! хозяин — тот шаг, который её поставил, он же и снимает. Порядок `ORDER` — это
//! приоритет: окно показывает первую, остальные ждут своей очереди. Так же, как список
//! шагов у фаз: новая жалоба — одна запись, а не ветка в функции (D-101).

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Сторож соединения: ядро работает, а наружу ничего не идёт (D-107).
pub const GUARD: &str = "guard";
/// Часы машины уехали — рукопожатие VMess и VLESS перестанет вставать (D-115).
pub const CLOCK: &str = "clock";
/// Запрет выхода мимо туннеля стоит, а исполнять его некому: брандмауэр выключили
/// после того, как защита встала (B-012, D-115).
pub const FIREWALL: &str = "firewall";

/// Маршрут по умолчанию у чужого адаптера, хотя работает TUN: трафик уходит мимо
/// туннеля и молча (D-109, D-115).
pub const ROUTE: &str = "routes";

/// Кого показываем первым. Впереди то, о чём человек не узнает сам: уведённый маршрут —
/// это трафик мимо туннеля при зелёном окне. Дальше причина вперёд симптома — уехавшие
/// часы **и есть** причина того, на что пожалуется сторож, — и защита, которой нет.
const ORDER: [&str; 4] = [ROUTE, CLOCK, FIREWALL, GUARD];

/// Одна жалоба.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    /// Отметка запуска ядра, если жалоба про **это** ядро: поднятое заново — чистый лист
    /// (D-107). Пусто — жалоба живёт сама по себе и снимается тем, кто её поставил.
    pub since: Option<u64>,
}

impl Notice {
    pub fn about(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            since: None,
        }
    }

    pub fn about_core(text: impl Into<String>, started: u64) -> Self {
        Self {
            text: text.into(),
            since: Some(started),
        }
    }
}

#[derive(Default)]
pub struct Notices(Mutex<BTreeMap<&'static str, Notice>>);

impl Notices {
    /// Поставить или снять жалобу. Снимает тот же, кто поставил: чужую не трогаем,
    /// иначе шаги начали бы затирать друг друга.
    pub fn set(&self, id: &'static str, notice: Option<Notice>) {
        let mut all = self.0.lock().unwrap();
        match notice {
            Some(notice) => all.insert(id, notice),
            None => all.remove(id),
        };
    }

    /// Что показать окну. `started` — нынешний запуск ядра: жалоба прошлой жизни
    /// не считается.
    pub fn top(&self, started: Option<u64>) -> Option<String> {
        let all = self.0.lock().unwrap();
        ORDER
            .iter()
            .filter_map(|id| all.get(id))
            .find(|notice| notice.since.is_none() || notice.since == started)
            .map(|notice| notice.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_is_the_priority_and_a_cleared_notice_lets_the_next_one_through() {
        let notices = Notices::default();
        notices.set(GUARD, Some(Notice::about_core("трафик не идёт", 7)));
        notices.set(CLOCK, Some(Notice::about("часы спешат")));

        assert_eq!(notices.top(Some(7)).as_deref(), Some("часы спешат"));
        notices.set(CLOCK, None);
        assert_eq!(notices.top(Some(7)).as_deref(), Some("трафик не идёт"));
    }

    /// Ради этого у жалобы есть отметка запуска: поднятое заново ядро — чистый лист,
    /// и висящая с прошлой жизни строка не переживёт переподключения (D-107).
    #[test]
    fn a_complaint_about_another_core_does_not_count() {
        let notices = Notices::default();
        notices.set(GUARD, Some(Notice::about_core("трафик не идёт", 7)));

        assert_eq!(notices.top(Some(8)), None, "жалоба прошлого ядра");
        assert_eq!(notices.top(None), None, "ядра нет — и жалобы нет");
        assert_eq!(notices.top(Some(7)).as_deref(), Some("трафик не идёт"));
    }

    /// А жалоба не про ядро живёт сама по себе: часы уехали и при остановленном ядре.
    #[test]
    fn a_complaint_of_its_own_outlives_the_core() {
        let notices = Notices::default();
        notices.set(CLOCK, Some(Notice::about("часы спешат")));
        assert_eq!(notices.top(None).as_deref(), Some("часы спешат"));
    }

    #[test]
    fn every_known_notice_has_its_place_in_the_order() {
        for id in [GUARD, CLOCK, FIREWALL, ROUTE] {
            assert!(ORDER.contains(&id), "{id} некуда показывать");
        }
    }
}
