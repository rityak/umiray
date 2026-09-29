//! Что вокруг клиента может врать: брандмауэр, чужой прокси, маршруты, соседи.
//!
//! Эти пробы не ходят в сеть вовсе — они спрашивают саму Windows, и потому идут
//! в «быструю» проверку целиком. Каждая отвечает на вопрос, который иначе выясняется
//! часом переписки: «почему защита стоит, а трафик идёт», «почему браузер мимо VPN».

use std::time::Instant;

use crate::diag::report::{Report, Row, Tone, Verdict};
use crate::error::Result;
use crate::system::killswitch::Firewall;
use crate::system::net;
use crate::system::net::NetInfo;
use crate::system::sysproxy::WinProxy;

pub struct SystemProbe;

impl SystemProbe {
    /// `firewall`: включён ли брандмауэр вообще.
    ///
    /// Первый же живой прогон kill switch упёрся ровно в это: правила стояли, а брандмауэр
    /// был выключен целиком — и не запрещал ничего (B-012). Проба на две строки, цена
    /// ошибки — открытый трафик при зелёной галке.
    pub fn firewall() -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("firewall");
        report.say(Tone::Info, "firewall: три профиля Windows");
        report.columns = ["Профиль", "Брандмауэр", "Исходящие по умолчанию"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let profiles = Firewall::profiles()?;
        let mut off = 0;
        for profile in &profiles {
            // `True`/`False` приходят от командлета: значения перечисления, не перевод.
            let enabled = profile.enabled.eq_ignore_ascii_case("true");
            if !enabled {
                off += 1;
            }
            report.say(
                if enabled { Tone::Ok } else { Tone::Bad },
                format!(
                    "{:<10} {:<8} {}",
                    profile.name,
                    if enabled {
                        "включён"
                    } else {
                        "выключен"
                    },
                    profile.action
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    profile.name.clone(),
                    if enabled {
                        "включён"
                    } else {
                        "выключен"
                    }
                    .to_string(),
                    profile.action.clone(),
                ],
                verdict: if enabled { Verdict::Ok } else { Verdict::Bad },
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match (off, profiles.len()) {
            (0, _) => (Verdict::Ok, "все включены".to_string()),
            (off, all) if off == all => (
                Verdict::Bad,
                "выключен целиком — kill switch не запрёт".to_string(),
            ),
            (off, _) => (Verdict::Warn, format!("выключен в {off} профилях")),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// `sysproxy`: кто записан системным прокси.
    ///
    /// Чужая запись — это не поломка сама по себе, но она объясняет «трафик идёт не туда»
    /// быстрее, чем что-либо ещё: браузер слушается её, а не нас.
    pub fn proxy() -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("sysproxy");
        let current = WinProxy::read()?;
        let ours = !current.ours.is_empty() && current.server == current.ours;

        report.columns = ["Что", "Значение"].iter().map(|s| s.to_string()).collect();
        let rows = [
            (
                "Включён",
                if current.enabled { "да" } else { "нет" }.to_string(),
            ),
            (
                "Адрес",
                if current.server.is_empty() {
                    "—".to_string()
                } else {
                    current.server.clone()
                },
            ),
            ("Чей", if ours { "наш" } else { "не наш" }.to_string()),
            (
                "Исключения",
                current.bypass.clone().unwrap_or_else(|| "—".to_string()),
            ),
        ];
        for (name, value) in &rows {
            report.say(Tone::Dim, format!("{name:<12} {value}"));
            report.rows.push(Row {
                cells: vec![name.to_string(), value.clone()],
                verdict: Verdict::Ok,
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match (current.enabled, ours) {
            (false, _) => (Verdict::Ok, "никто не держит".to_string()),
            (true, true) => (Verdict::Ok, "наш".to_string()),
            (true, false) => (Verdict::Warn, format!("держит чужой: {}", current.server)),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// `routes`: куда система шлёт всё остальное и кто ещё стоит рядом.
    ///
    /// Отвечает сразу на два вопроса, потому что они один: «идёт ли трафик мимо туннеля»
    /// решается сравнением метрик, а «почему он идёт мимо» — списком соседей, среди которых
    /// чужой VPN встречается чаще всего.
    ///
    /// В TUN проба ещё и **судит** (D-109): туннель добавляет свой маршрут, но выигрывает
    /// наименьшая метрика, и чужой адаптер — виртуалка, вторая VPN, Hyper-V — уводит трафик
    /// мимо при полностью зелёном окне. Режим приходит из состояния: без него видно только
    /// список, а не беду.
    pub fn routes(mode: Option<&str>) -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("routes");
        report.columns = ["Адаптер", "Шлюз", "Метрика", "Кто это"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let adapters = NetInfo::adapters()?;
        let routes = NetInfo::default_routes()?;
        let driver_of = |name: &str| -> String {
            adapters
                .iter()
                .find(|adapter| adapter.name == name)
                .map(|adapter| adapter.driver.clone())
                .unwrap_or_else(|| "—".to_string())
        };

        for (place, route) in routes.iter().enumerate() {
            let driver = driver_of(&route.adapter);
            let first = place == 0;
            report.say(
                if first { Tone::Ok } else { Tone::Dim },
                format!(
                    "{:<24} {:<16} {:>4}  {}",
                    route.adapter, route.gateway, route.metric, driver
                ),
            );
            report.rows.push(Row {
                cells: vec![
                    route.adapter.clone(),
                    route.gateway.clone(),
                    route.metric.to_string(),
                    driver,
                ],
                verdict: if first { Verdict::Ok } else { Verdict::Idle },
                // Отмечен тот, чей маршрут выигрывает: именно через него всё и уходит.
                mark: first,
            });
        }

        // Соседи: чужие туннели и виртуальные коммутаторы. Не беда, но объяснение.
        let neighbours: Vec<&net::Adapter> = adapters
            .iter()
            .filter(|adapter| foreign(&adapter.driver) && adapter.status.eq_ignore_ascii_case("up"))
            .collect();
        for adapter in &neighbours {
            report.say(
                Tone::Warn,
                format!("сосед: {} — {}", adapter.name, adapter.driver),
            );
        }

        let ms = started.elapsed().as_millis() as u64;
        if let Some(stolen) = stolen(mode, &routes) {
            report.say(Tone::Bad, format!("{stolen} — {}", driver_of(&stolen)));
            return Ok(report.finish(
                Verdict::Bad,
                format!("трафик уходит мимо туннеля, через {stolen}"),
                ms,
            ));
        }
        let (verdict, headline) = match (routes.first(), neighbours.len()) {
            (None, _) => (Verdict::Bad, "маршрута по умолчанию нет".to_string()),
            (Some(route), 0) => (Verdict::Ok, format!("через {}", route.adapter)),
            (Some(route), soseday) => (
                Verdict::Warn,
                format!("через {}, рядом ещё {soseday}", route.adapter),
            ),
        };
        Ok(report.finish(verdict, headline, ms))
    }

    /// Кто увёл трафик у туннеля. `None` — уводить нечего (не TUN) или не увели.
    ///
    /// Имя адаптера читаем из `tun.device` тем же кодом, что и kill switch (D-073): угадывать
    /// его нельзя — пользователь вправе написать своё, и промах здесь означал бы жалобу
    /// на исправный туннель.
    /// Кто увёл маршрут по умолчанию у нашего туннеля — типизированный слой (D-097).
    /// Пусто: режим не TUN, маршрут наш или спрашивать не у чего.
    pub fn thief_of_the_route() -> Option<String> {
        stolen(Some("tun"), &NetInfo::default_routes().ok()?)
    }

    /// `resolvers`: какие DNS прописаны в самой системе.
    ///
    /// Именно их спрашивает всё остальное на машине, и именно их подменяют чаще всего —
    /// поэтому первый из них уходит в сверку `dns-spoof`.
    pub fn resolvers() -> Result<Report> {
        let started = Instant::now();
        let mut report = Report::new("resolvers");
        report.columns = ["Адаптер", "Серверы"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let list = NetInfo::resolvers()?;
        for entry in &list {
            report.say(
                Tone::Dim,
                format!("{:<24} {}", entry.adapter, entry.servers.join(", ")),
            );
            report.rows.push(Row {
                cells: vec![entry.adapter.clone(), entry.servers.join(", ")],
                verdict: Verdict::Ok,
                mark: false,
            });
        }

        let ms = started.elapsed().as_millis() as u64;
        let (verdict, headline) = match NetInfo::first_resolver() {
            None if list.is_empty() => (Verdict::Bad, "система не назвала ни одного".to_string()),
            None => (Verdict::Warn, "только локальные заглушки".to_string()),
            Some(first) => (Verdict::Ok, first),
        };
        Ok(report.finish(verdict, headline, ms))
    }
}

fn stolen(mode: Option<&str>, routes: &[net::Route]) -> Option<String> {
    if mode != Some("tun") {
        return None;
    }
    let text = crate::config::files::Documents::read(crate::config::files::ADVANCED).ok()?;
    let map = crate::yaml::Yaml::top_mapping(&text).ok()?;
    thief(&crate::config::mode::Mode::tun_device(&map), routes)
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

/// Драйверы, которые сами уводят трафик. Список по подстрокам, а не точный: имена
/// у драйверов длинные и с номерами, а слово в них стабильное.
fn foreign(driver: &str) -> bool {
    const MARKS: [&str; 6] = [
        "WireGuard",
        "TAP-Windows",
        "OpenVPN",
        "Hyper-V",
        "VirtualBox",
        "VMware",
    ];
    let lower = driver.to_lowercase();
    MARKS
        .iter()
        .any(|mark| lower.contains(&mark.to_lowercase()))
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

    /// Не TUN — судить нечего: в local маршрут по умолчанию и должен быть чужим.
    #[test]
    fn outside_tun_nobody_is_judged() {
        assert_eq!(stolen(None, &[route("Ethernet", 5)]), None);
        assert_eq!(stolen(Some("local"), &[route("Ethernet", 5)]), None);
    }

    /// Чужой туннель узнаётся по слову в описании драйвера, а не по точному имени:
    /// имена там с номерами и скобками.
    #[test]
    fn a_foreign_tunnel_is_recognised_by_its_driver() {
        assert!(foreign("WireGuard Tunnel #2"));
        assert!(foreign("TAP-Windows Adapter V9"));
        assert!(foreign("Hyper-V Virtual Ethernet Adapter"));
        assert!(!foreign("Realtek PCIe GbE Family Controller"));
        assert!(
            !foreign("Meta"),
            "наш собственный адаптер соседом не считается"
        );
    }
}
