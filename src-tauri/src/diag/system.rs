//! Кто увёл маршрут по умолчанию у туннеля (D-109) — проба такта TUN.
//!
//! В сеть не ходит: спрашивает саму Windows. Отвечает на вопрос, который иначе выясняется
//! часом переписки: «почему TUN стоит, а трафик идёт в обход».

use crate::system::net;
use crate::system::net::NetInfo;

pub struct SystemProbe;

impl SystemProbe {
    /// Чей адаптер держит маршрут по умолчанию вместо нашего. Пусто — маршрут наш
    /// или спрашивать не у чего. Зовётся только в TUN: в local маршрут и должен быть чужим.
    ///
    /// Имя адаптера читаем из `tun.device` тем же кодом, что и kill switch (D-073): угадывать
    /// его нельзя — пользователь вправе написать своё, и промах означал бы жалобу
    /// на исправный туннель.
    pub fn thief_of_the_route() -> Option<String> {
        let text = crate::config::files::Documents::read(crate::config::files::ADVANCED).ok()?;
        let map = crate::yaml::Yaml::top_mapping(&text).ok()?;
        thief(
            &crate::config::mode::Mode::tun_device(&map),
            &NetInfo::default_routes().ok()?,
        )
    }
}

/// Тот же вопрос без диска: чей маршрут выигрывает у адаптера с этим именем.
///
/// Равная метрика — тоже увод: чей маршрут возьмёт система при ничьей, решает порядок
/// в её таблице, а не мы. Пустого списка достаточно, чтобы сказать то же самое —
/// маршрута у туннеля нет вовсе, только назвать вора тогда некем.
fn thief(device: &str, routes: &[net::Route]) -> Option<String> {
    let winner = routes.first()?;
    (winner.adapter != device).then(|| winner.adapter.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(adapter: &str, metric: u32) -> net::Route {
        net::Route {
            adapter: adapter.to_string(),
            gateway: "0.0.0.0".to_string(),
            metric,
        }
    }

    /// Критерий D-109: чужой маршрут с меньшей метрикой называется по имени, а не
    /// прикрывается зелёным «всё хорошо». Список приходит уже отсортированным
    /// (`NetInfo::default_routes`), поэтому вор — это первый.
    #[test]
    fn a_foreign_route_that_wins_is_named() {
        let stolen = [route("Ethernet", 5), route("Meta", 20)];
        assert_eq!(thief("Meta", &stolen).as_deref(), Some("Ethernet"));
        let ours = [route("Meta", 5), route("Ethernet", 20)];
        assert_eq!(thief("Meta", &ours), None);
    }

    /// Своё имя адаптера из настроек ядра — не то же, что умолчание ядра: сравнивать
    /// надо с тем, что написано, иначе жалоба прилетит исправному туннелю.
    #[test]
    fn the_name_comes_from_the_document_not_from_a_guess() {
        let routes = [route("umiray0", 1), route("Ethernet", 20)];
        assert_eq!(thief("umiray0", &routes), None);
        assert_eq!(thief("Meta", &routes).as_deref(), Some("umiray0"));
    }

    /// Ничья — тоже увод: чей маршрут возьмёт система, решает её таблица, а не мы.
    /// И пустой список не даёт зелёного: маршрута у туннеля нет вовсе.
    #[test]
    fn a_tie_and_an_empty_table_are_not_a_green_light() {
        let tie = [route("Ethernet", 5), route("Meta", 5)];
        assert_eq!(thief("Meta", &tie).as_deref(), Some("Ethernet"));
        assert_eq!(
            thief("Meta", &[]),
            None,
            "называть некого, но и хвалить нечего"
        );
    }
}
