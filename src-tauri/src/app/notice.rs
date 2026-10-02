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

/// Подписка не обновилась: панель не ответила или прислала ответ без узлов. Узлы остались
/// прежними, но человек должен знать, что они стареют (D-038, B-039).
pub const SOURCES: &str = "sources";

/// Кого показываем первым. Впереди то, о чём человек не узнает сам: уведённый маршрут —
/// это трафик мимо туннеля при зелёном окне. Дальше причина вперёд симптома — уехавшие
/// часы и протухшая подписка **и есть** причины того, на что пожалуется сторож, — и защита,
/// которой нет.
const ORDER: [&str; 5] = [ROUTE, CLOCK, FIREWALL, SOURCES, GUARD];

/// Одна жалоба.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    /// Отметка запуска ядра, если жалоба про **это** ядро: поднятое заново — чистый лист
    /// (D-107). Пусто — жалоба живёт сама по себе и снимается тем, кто её поставил.
    pub since: Option<u64>,
    /// Лечит ли её перезапуск VPN: только тогда окно предлагает кнопку. Подписке, которая
    /// не обновилась, перезапуск не поможет, и кнопка рядом с ней была бы враньём.
    pub restart: bool,
}

impl Notice {
    pub fn about(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            since: None,
            restart: true,
        }
    }

    pub fn about_core(text: impl Into<String>, started: u64) -> Self {
        Self {
            text: text.into(),
            since: Some(started),
            restart: true,
        }
    }

    /// Жалоба, которую перезапуск не лечит: окно показывает её без кнопки.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            since: None,
            restart: false,
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
    pub fn top(&self, started: Option<u64>) -> Option<Notice> {
        let all = self.0.lock().unwrap();
        ORDER
            .iter()
            .filter_map(|id| all.get(id))
            .find(|notice| notice.since.is_none() || notice.since == started)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(notices: &Notices, started: Option<u64>) -> Option<String> {
        notices.top(started).map(|notice| notice.text)
    }

    #[test]
    fn the_order_is_the_priority_and_a_cleared_notice_lets_the_next_one_through() {
        let notices = Notices::default();
        notices.set(GUARD, Some(Notice::about_core("трафик не идёт", 7)));
        notices.set(CLOCK, Some(Notice::about("часы спешат")));

        assert_eq!(text(&notices, Some(7)).as_deref(), Some("часы спешат"));
        notices.set(CLOCK, None);
        assert_eq!(text(&notices, Some(7)).as_deref(), Some("трафик не идёт"));
    }

    /// Ради этого у жалобы есть отметка запуска: поднятое заново ядро — чистый лист,
    /// и висящая с прошлой жизни строка не переживёт переподключения (D-107).
    #[test]
    fn a_complaint_about_another_core_does_not_count() {
        let notices = Notices::default();
        notices.set(GUARD, Some(Notice::about_core("трафик не идёт", 7)));

        assert_eq!(text(&notices, Some(8)), None, "жалоба прошлого ядра");
        assert_eq!(text(&notices, None), None, "ядра нет — и жалобы нет");
        assert_eq!(text(&notices, Some(7)).as_deref(), Some("трафик не идёт"));
    }

    /// А жалоба не про ядро живёт сама по себе: часы уехали и при остановленном ядре.
    #[test]
    fn a_complaint_of_its_own_outlives_the_core() {
        let notices = Notices::default();
        notices.set(CLOCK, Some(Notice::about("часы спешат")));
        assert_eq!(text(&notices, None).as_deref(), Some("часы спешат"));
    }

    /// Подписка, которая не обновилась, — причина раньше симптома, и кнопки перезапуска
    /// у неё нет: перезапуск её не лечит.
    #[test]
    fn a_stale_subscription_comes_before_the_guard_and_offers_no_restart() {
        let notices = Notices::default();
        notices.set(GUARD, Some(Notice::about_core("трафик не идёт", 7)));
        notices.set(SOURCES, Some(Notice::plain("подписка не обновилась")));
        let top = notices.top(Some(7)).unwrap();
        assert_eq!(top.text, "подписка не обновилась");
        assert!(!top.restart);
        notices.set(SOURCES, None);
        assert!(
            notices.top(Some(7)).unwrap().restart,
            "сторожу перезапуск помогает"
        );
    }

    #[test]
    fn every_known_notice_has_its_place_in_the_order() {
        for id in [GUARD, CLOCK, FIREWALL, ROUTE, SOURCES] {
            assert!(ORDER.contains(&id), "{id} некуда показывать");
        }
    }
}
