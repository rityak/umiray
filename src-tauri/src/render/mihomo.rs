//! Пользовательские файлы плюс источники — в конфиг mihomo.
//!
//! Здесь и только здесь живут имена полей ядра: `proxy-providers`, `tun.enable`, `mixed-port`
//! (D-027). Ссылки разбирает само ядро (D-031) — кроме схем, которых его конвертер не знает:
//! их описание приходит из `nodes::outbound`, а полями ядра оно становится здесь (D-063).

use serde_yaml::{Mapping, Value};

use crate::config::auto::Exclude;
use crate::config::direction::{AUTO, DIRECT, PROBE, SELECTOR, UDP};
use crate::config::mode::Mode;
use crate::config::rules::RulesCodec;
use crate::error::{AppError, Result};
use crate::nodes::health::{Check, EXPECTED};
use crate::render::mihomo_groups::{auto_pick, own_groups, AutoPick};
use crate::render::plan::{Client, NodeSource};
use crate::yaml::Yaml;

/// Имя служебного входа. Отдельное от группы: в логе ядра видно, что это вход, а не выход.
const PROBE_INBOUND: &str = "probe-in";

/// Готовый конфиг, режим, который из него получился, и порт, по которому видно, что ядро
/// действительно поднялось. В TUN слушающего порта нет — там `port` пустой.
pub struct Effective {
    pub yaml: String,
    pub mode: Mode,
    pub port: Option<u16>,
    /// Порт служебного входа под замер (D-072). Пусто — вход не заводили: он нужен только
    /// живому ядру, и в показанный конфиг его не подмешивают.
    pub probe: Option<u16>,
    /// Адаптер TUN, если режим — TUN: kill switch разрешает выход именно через него (D-073).
    pub device: Option<String>,
}

pub struct MihomoRenderer;

impl MihomoRenderer {
    /// Тот же конфиг для ядра без прав на TUN — пробного прогона (D-106). Метку исходящих
    /// ставит сокету только процесс с `CAP_NET_ADMIN`: без прав каждое соединение ядра,
    /// даже скачивание GeoSite, падает с «operation not permitted», и исправный конфиг
    /// выглядел бы сломанным.
    pub fn unprivileged(yaml: &str) -> Result<String> {
        let mut map = Yaml::top_mapping(yaml)?;
        map.remove(Value::from("routing-mark"));
        serde_yaml::to_string(&Value::Mapping(map)).map_err(|e| AppError::invalid(e.to_string()))
    }

    /// Файлы пользователя плюс источники — в тот самый файл, который запускает ядро.
    ///
    /// Порядок: сначала документы пользователя, потом наше. Что клиент ставит жёстко —
    /// то, без чего его работа теряет смысл: пути провайдеров и состав автогруппы. Что
    /// заполняет только при отсутствии — умолчания, которые пользователь вправе перебить
    /// (D-029).
    pub fn config(
        user: &[String],
        builtin: &[String],
        sources: &[NodeSource],
        probe: Option<u16>,
        client: &Client,
    ) -> Result<Effective> {
        let mut map = assemble(user, sources, client)?;
        builtin_rules(&mut map, builtin);
        // После встроенных: их строки тоже вправе ссылаться на скачанный список.
        crate::render::mihomo_lists::MihomoLists::apply(&mut map, &client.lists);
        providers(&mut map, sources, &client.health);
        let mode = crate::config::mode::Mode::of(&map);
        baseline(&mut map, mode);
        // После `baseline`: `MATCH` дописывает он, а правило обязано встать **перед** ним.
        // И только если группа правда собралась — правило в несуществующую цель ядро
        // не примет, и VPN не поднимется вовсе.
        if client.udp && has_group(&map, UDP) {
            udp_rule(&mut map);
        }
        if let Some(port) = probe {
            probe_seam(&mut map, sources, port);
        }
        bare_flags(&mut map);
        // Последним: к этому месту заведены все группы, в которые правило может целиться.
        if let Some(route) = &client.volt {
            crate::render::mihomo_volt::apply(&mut map, route)?;
        }
        missing_targets(&mut map);

        let port = match mode {
            Mode::Local => map
                .get(Value::from("mixed-port"))
                .and_then(Value::as_u64)
                .map(|port| port as u16),
            Mode::Tun => None,
        };
        let device = (mode == Mode::Tun).then(|| crate::config::mode::Mode::tun_device(&map));
        let yaml = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Ok(Effective {
            yaml,
            mode,
            port,
            probe,
            device,
        })
    }
}

/// Служебный вход и группа под него (D-072, замерено в S-016).
///
/// Вход привязан к группе полем `proxy:` — весь его трафик идёт мимо правил, прямо
/// в выбранный узел. Клиент наводит группу через API и меряет своим соединением;
/// маршрут пользователя при этом не трогается.
///
/// **`listen` обязателен**: без него ядро поднимает вход на всех интерфейсах, то есть
/// открытый прокси наружу (S-016, поймано измерением).
///
/// Своё авторитетнее нашего: группу и вход с такими же именами не подменяем.
fn probe_seam(map: &mut Mapping, sources: &[NodeSource], port: u16) {
    if sources.is_empty() {
        return;
    }
    // Узлы, которые клиент выписал сам (D-063), в `use:` не попадают — провайдера у них
    // нет. Без них группа у источника из одних `wireguard://` выходила **пустой**, и ядро
    // отвергало весь конфиг: «probe: `use` or `proxies` missing» (B-016). Берём имена
    // из собранного `proxies:` — там и наши узлы, и записи пользователя, и мерить через
    // группу можно любой из них.
    let named: Vec<Value> = map
        .get(Value::from("proxies"))
        .and_then(Value::as_sequence)
        .map(|list| list.iter().filter_map(name_of).map(Value::from).collect())
        .unwrap_or_default();
    if provider_ids(sources).is_empty() && named.is_empty() {
        return;
    }
    let groups = map
        .get(Value::from("proxy-groups"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    if !groups.iter().filter_map(name_of).any(|name| name == PROBE) {
        let mut group = Mapping::new();
        Yaml::set(&mut group, "name", Value::from(PROBE));
        Yaml::set(&mut group, "type", Value::from("select"));
        set_use(&mut group, sources);
        if !named.is_empty() {
            Yaml::set(&mut group, "proxies", Value::Sequence(named));
        }
        // Ни `url`, ни `interval`: фоновой проверки этой группе не нужно, она существует
        // ради замеров по нажатию.
        let mut all = groups;
        all.push(Value::Mapping(group));
        Yaml::set(map, "proxy-groups", Value::Sequence(all));
    }

    let mut inbounds = map
        .get(Value::from("listeners"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    if inbounds
        .iter()
        .filter_map(name_of)
        .any(|name| name == PROBE_INBOUND)
    {
        return;
    }
    let mut inbound = Mapping::new();
    Yaml::set(&mut inbound, "name", Value::from(PROBE_INBOUND));
    Yaml::set(&mut inbound, "type", Value::from("mixed"));
    Yaml::set(&mut inbound, "listen", Value::from("127.0.0.1"));
    Yaml::set(&mut inbound, "port", Value::from(port));
    Yaml::set(&mut inbound, "proxy", Value::from(PROBE));
    inbounds.push(Value::Mapping(inbound));
    Yaml::set(map, "listeners", Value::Sequence(inbounds));
}

/// Строки встроенных наборов — между правилами пользователя и `MATCH` (D-083).
///
/// **Правила пользователя выше**: побеждает первое совпавшее, и своё правило обязано
/// перебивать наше. `MATCH` остаётся последним — всё, что не совпало, по-прежнему решает он.
///
/// Строку, которая в документе уже есть, не повторяем: вторая такая же всё равно мертва,
/// а в собранном конфиге читалась бы как ошибка сборки.
fn builtin_rules(map: &mut Mapping, builtin: &[String]) {
    if builtin.is_empty() {
        return;
    }
    let mut rules: Vec<Value> = map
        .get(Value::from("rules"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let at = rules
        .iter()
        .position(|line| line.as_str().is_some_and(is_match))
        .unwrap_or(rules.len());
    let mut added = 0;
    for line in builtin {
        let value = Value::from(line.clone());
        if rules.contains(&value) {
            continue;
        }
        rules.insert(at + added, value);
        added += 1;
    }
    Yaml::set(map, "rules", Value::Sequence(rules));
}

fn is_match(line: &str) -> bool {
    line.split(',')
        .next()
        .is_some_and(|kind| kind.trim() == "MATCH")
}

/// Общая часть конфига и списка выбора: документы пользователя, узлы, которые ядро
/// из ссылки не прочитает, и наши группы.
fn assemble(user: &[String], sources: &[NodeSource], client: &Client) -> Result<Mapping> {
    let mut map = Yaml::top_mapping("")?;
    for document in user {
        Yaml::merge(&mut map, Yaml::top_mapping(document)?);
    }
    proxies(&mut map);
    groups(&mut map, sources, &[], client)?;
    node_targets(&mut map, sources);
    Ok(map)
}

/// Узлы из «Настроек» — то, что человек написал в `proxies:` сам.
///
/// Узлы источников сюда больше не врезаются: с D-122 они лежат записями в файле своего
/// провайдера, и клиенту нечего к ним дописывать. Функция осталась одна — проследить,
/// что чужой список не потерялся при сборке.
fn proxies(map: &mut Mapping) {
    let theirs: Vec<Value> = map
        .get(Value::from("proxies"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    if !theirs.is_empty() {
        Yaml::set(map, "proxies", Value::Sequence(theirs));
    }
}

/// То, без чего конфиг не работает, но чего человек писать не должен.
///
/// Всё заполняется только при отсутствии: шаблоны файлов уже содержат эти поля явно,
/// а подстраховка нужна на случай, когда пользователь их оттуда убрал.
fn baseline(map: &mut Mapping, mode: Mode) {
    Yaml::fill(map, "mode", Value::from("rule"));
    // Без порта local-режим не работает вовсе, а в TUN лишний слушатель ни к чему.
    if mode == Mode::Local {
        Yaml::fill(
            map,
            "mixed-port",
            Value::from(crate::config::files::LOCAL_PROXY_PORT),
        );
    }
    // Метка своих исходящих — по ней запрет выхода выпускает ядро там, где брандмауэр
    // не умеет пускать по пути к бинарю (Linux, S-035). Жёстко: без неё kill switch
    // запер бы и само ядро. Ставить её ядру позволяет только право на TUN.
    if let (Mode::Tun, Some(mark)) = (mode, crate::system::killswitch::CORE_MARK) {
        Yaml::set(map, "routing-mark", Value::from(mark));
    }
    Yaml::fill(map, "log-level", Value::from("info"));
    Yaml::fill(map, "allow-lan", Value::from(false));
    Yaml::fill(map, "ipv6", Value::from(false));

    // Выбранный узел помним сами (D-039), и `store-selected` у ядра его через перезапуск
    // не удержал (S-012). Пишем выключенным, чтобы не было двух хозяев у одного выбора.
    Yaml::fill(
        Yaml::sub(map, "profile"),
        "store-selected",
        Value::from(false),
    );

    // А вот карту подменных адресов помнить просим (D-103, S-021): без неё после подъёма
    // `198.18.0.4` достаётся тому, кто спросил первым, и приложение с запомненным адресом
    // уезжает по чужому правилу молча. Работает это только с мягкой остановкой — она
    // у нас теперь есть (`system::console`).
    Yaml::fill(
        Yaml::sub(map, "profile"),
        "store-fake-ip",
        Value::from(true),
    );

    // Именно «завести раздел целиком, если его нет», а не заполнение внутри: `sub` создал
    // бы раздел и дописал бы `enable: false` в **пользовательский** `dns`, где его не было,
    // то есть выключил бы то, что человек только что настроил.
    if !map.contains_key(Value::from("dns")) {
        let mut section = Mapping::new();
        Yaml::set(&mut section, "enable", Value::from(false));
        map.insert(Value::from("dns"), Value::Mapping(section));
    }

    // Последнее правило решает, куда идёт всё остальное. Псевдоним есть всегда — даже без
    // источников, где он указывает на прямое соединение, — поэтому целиться можно безусловно.
    //
    // Именно «дописать, если его нет», а не «завести ключ, если его нет»: правила в списке
    // могут быть и без `MATCH` — свои, написанные руками, или строки встроенного набора
    // (D-083). Без него судьбу остального решал бы не этот документ.
    let mut rules: Vec<Value> = map
        .get(Value::from("rules"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    if !rules.iter().any(|line| line.as_str().is_some_and(is_match)) {
        rules.push(Value::from(format!("MATCH,{SELECTOR}")));
        Yaml::set(map, "rules", Value::Sequence(rules));
    }
}

/// Источники узлов — провайдеры ядра. Ссылки разбирает оно само (D-031), поэтому здесь
/// только путь к файлу и проверка живости, по которой автогруппа обходит мёртвые узлы.
fn providers(map: &mut Mapping, sources: &[NodeSource], check: &Check) {
    let mine: Vec<&NodeSource> = sources.iter().filter(|source| provides(source)).collect();
    if mine.is_empty() {
        return;
    }
    let providers = Yaml::sub(map, "proxy-providers");
    for source in mine {
        let entry = Yaml::sub(providers, &source.id);
        Yaml::set(entry, "type", Value::from("file"));
        Yaml::set(
            entry,
            "path",
            Value::from(source.path.display().to_string()),
        );

        let health = Yaml::sub(entry, "health-check");
        Yaml::set(health, "enable", Value::from(true));
        Yaml::set(health, "url", Value::from(check.url.as_str()));
        Yaml::set(health, "interval", Value::from(check.interval));
        // Без него ответом считается любой: заглушка провайдера и страница captive
        // portal отвечают двухсотым, и мёртвый выход остаётся в группе (D-108).
        Yaml::set(health, "expected-status", Value::from(EXPECTED.to_string()));
        // Ленивая: узлы проверяются, когда группой пользуются. Числа в таблице от неё
        // больше не зависят — их меряет клиент сам (D-062), а фоновый обход всех узлов
        // каждые пять минут стоил трафика и не давал взамен ничего.
        Yaml::set(health, "lazy", Value::from(true));
    }
}

/// Две группы клиента поверх групп пользователя (D-053).
///
/// `AUTO` — автовыбор по всем источникам сразу; `umiray` — псевдоним, который переключает
/// список серверов и на который смотрит `MATCH`. Обе собираются заново при каждом запуске,
/// поэтому узлы новой подписки попадают в них сами. Группы пользователя не трогаются вовсе
/// и становятся отдельными пунктами внутри псевдонима. Служебные имена зарезервированы,
/// чтобы одно имя не получило два разных смысла (D-135).
fn groups(
    map: &mut Mapping,
    sources: &[NodeSource],
    ours: &[String],
    client: &Client,
) -> Result<()> {
    let theirs: Vec<Value> = map
        .get(Value::from("proxy-groups"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let names: Vec<String> = theirs.iter().filter_map(name_of).collect();
    if let Some(name) = names
        .iter()
        .find(|name| [AUTO, SELECTOR, UDP, PROBE].contains(&name.as_str()))
    {
        return Err(AppError::invalid(format!(
            "Группа «{name}» служебная. Переименуйте её в groups.yaml"
        )));
    }
    if let Some(name) = names
        .iter()
        .enumerate()
        .find_map(|(at, name)| names[..at].contains(name).then_some(name))
    {
        return Err(AppError::invalid(format!(
            "Две группы называются «{name}». Переименуйте одну в groups.yaml"
        )));
    }

    let own = own_groups(sources, client.grouping, &names, &client.health);
    let own_names: Vec<String> = own
        .iter()
        .filter_map(|group| name_of(&Value::Mapping(group.clone())))
        .collect();
    let mut all = Vec::new();
    // Без источников автогруппе не из чего выбирать, а пустой список ядро отвергнет.
    if !sources.is_empty() {
        all.push(Value::Mapping(auto(
            sources,
            ours,
            &client.health,
            &client.exclude,
        )));
    }
    all.push(Value::Mapping(selector(&names, &own_names, sources, ours)));
    // Группа с нативным UDP — только если такие узлы правда есть: пустую группу ядро
    // не принимает и не стартует вовсе (D-113).
    if client.grouping.udp {
        if let Some(group) = udp_group(sources, ours, &client.health) {
            all.push(Value::Mapping(group));
        }
    }
    all.extend(own.into_iter().map(Value::Mapping));
    all.extend(theirs);
    Yaml::set(map, "proxy-groups", Value::Sequence(all));
    Ok(())
}

/// Узлы, которые клиент записал сам, попадают в группу по имени: в отличие от узлов
/// провайдера (S-012) они адресуемы напрямую — этого ради шов и затевался (D-063).
fn named(ours: &[String]) -> Vec<Value> {
    ours.iter().map(|name| Value::from(name.clone())).collect()
}

/// `AUTO` без того, что человек из него вынул (D-172): источник — из `use`, узел —
/// `exclude-filter`, потому что поимённо узел провайдера не адресуется (S-012).
/// Вынуто всё — исключения не действуют: пустую группу ядро не примет и не стартует.
fn auto(sources: &[NodeSource], ours: &[String], health: &Check, exclude: &Exclude) -> Mapping {
    let AutoPick { kept, out, mine } = auto_pick(sources, ours, exclude);
    let mut group = Mapping::new();
    Yaml::set(&mut group, "name", Value::from(AUTO));
    // consistent-hashing, а не round-robin: один и тот же адрес обязан уходить через один
    // и тот же сервер, иначе сессия рвётся на каждом запросе.
    Yaml::set(&mut group, "type", Value::from("load-balance"));
    Yaml::set(&mut group, "strategy", Value::from("consistent-hashing"));
    set_use(&mut group, &kept);
    if !mine.is_empty() {
        Yaml::set(&mut group, "proxies", Value::Sequence(named(&mine)));
    }
    if !out.is_empty() {
        Yaml::set(&mut group, "exclude-filter", Value::from(any(&out)));
    }
    checked(&mut group, health);
    group
}

/// Проверка живости группы — та же цель и тот же ожидаемый код, что у остальных (D-108).
pub(super) fn checked(group: &mut Mapping, health: &Check) {
    Yaml::set(group, "url", Value::from(health.url.as_str()));
    Yaml::set(group, "interval", Value::from(health.interval));
    Yaml::set(group, "expected-status", Value::from(EXPECTED.to_string()));
}

/// Псевдоним. Прямое соединение в списке всегда: оно же делает конфиг рабочим до первой
/// подписки — `MATCH,umiray` без единого узла иначе некуда было бы направить.
fn selector(
    user_groups: &[String],
    own: &[String],
    sources: &[NodeSource],
    ours: &[String],
) -> Mapping {
    let mut options: Vec<Value> = Vec::new();
    if !sources.is_empty() {
        options.push(Value::from(AUTO));
    }
    options.extend(named(ours));
    // Пропускаем повторы между рассчитанными целями и DIRECT; служебные имена пользователя
    // уже отклонены выше понятной ошибкой (D-135).
    for name in user_groups
        .iter()
        .chain(own)
        .map(|name| Value::from(name.clone()))
        .chain(std::iter::once(Value::from(DIRECT)))
    {
        if !options.contains(&name) {
            options.push(name);
        }
    }

    let mut group = Mapping::new();
    Yaml::set(&mut group, "name", Value::from(SELECTOR));
    Yaml::set(&mut group, "type", Value::from("select"));
    Yaml::set(&mut group, "proxies", Value::Sequence(options));
    // Узлы провайдера попадают в группу только через `use` (проверено, S-012).
    // Провайдера нет ни у одного источника — поля не будет: пустой список ядро
    // читает как «поля нет» и спотыкается о него (B-016).
    set_use(&mut group, sources);
    group
}

/// Группа узлов, несущих UDP датаграммой (D-113). `None` — таких узлов нет, и группы
/// быть не должно: пустую ядро не принимает и не стартует вовсе.
///
/// Узлы провайдера адресуются только фильтром по именам (S-012). `url-test`, а не
/// `load-balance`: у UDP-узлов проверка живости всё равно меряет TCP, и раскладывать
/// датаграммы по узлам, о живости которых мы знаем только это, было бы хуже, чем
/// держаться одного.
fn udp_group(sources: &[NodeSource], ours: &[String], health: &Check) -> Option<Mapping> {
    let from: Vec<Value> = sources
        .iter()
        .filter(|source| provides(source) && !source.udp.is_empty())
        .map(|source| Value::from(source.id.clone()))
        .collect();
    let mine: Vec<String> = ours
        .iter()
        .filter(|name| udp_by_name(sources, name))
        .cloned()
        .collect();
    if from.is_empty() && mine.is_empty() {
        return None;
    }

    let mut group = Mapping::new();
    Yaml::set(&mut group, "name", Value::from(UDP));
    Yaml::set(&mut group, "type", Value::from("url-test"));
    if !from.is_empty() {
        let names: Vec<String> = sources
            .iter()
            .filter(|source| provides(source))
            .flat_map(|source| source.udp.iter().cloned())
            .collect();
        Yaml::set(&mut group, "use", Value::Sequence(from));
        Yaml::set(&mut group, "filter", Value::from(any(&names)));
    }
    if !mine.is_empty() {
        Yaml::set(&mut group, "proxies", Value::Sequence(named(&mine)));
    }
    // Та же цель и тот же ожидаемый код, что у остальных проверок живости (D-108):
    // без `url` группа осталась бы без обхода вовсе. Что этот обход меряет **TCP** —
    // ограничение, названное в D-113: глухой по UDP узел из группы не выпадет.
    checked(&mut group, health);
    Some(group)
}

/// Узел, который клиент записал сам (D-063), — из тех, что несут UDP датаграммой.
/// Имя ищем в тех же источниках: протокол знает каталог, а не рендер.
fn udp_by_name(sources: &[NodeSource], name: &str) -> bool {
    sources
        .iter()
        .any(|source| source.udp.iter().any(|udp| udp == name))
}

/// Есть ли в собранном группа с таким именем.
fn has_group(map: &Mapping, wanted: &str) -> bool {
    map.get(Value::from("proxy-groups"))
        .and_then(Value::as_sequence)
        .is_some_and(|groups| groups.iter().filter_map(name_of).any(|name| name == wanted))
}

/// Правило «весь UDP — в группу с нативным UDP», перед `MATCH` (D-113).
///
/// Именно перед: `MATCH` ловит всё, и строка после него недостижима.
fn udp_rule(map: &mut Mapping) {
    let mut rules: Vec<Value> = map
        .get(Value::from("rules"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let line = format!("NETWORK,udp,{UDP}");
    if rules
        .iter()
        .any(|rule| rule.as_str() == Some(line.as_str()))
    {
        return;
    }
    let at = rules
        .iter()
        .position(|rule| rule.as_str().is_some_and(is_match))
        .unwrap_or(rules.len());
    rules.insert(at, Value::from(line));
    Yaml::set(map, "rules", Value::Sequence(rules));
}

/// Группы под узлы, названные целью правила (D-082).
///
/// Ядро не умеет отправить правило прямо в узел провайдера: поимённо тот не адресуется
/// (S-012). Значит цель, которая называет узел, получает группу из одного этого узла —
/// с его же именем, чтобы правило в документе пользователя осталось читаемым.
///
/// Своё авторитетнее нашего: имя, за которым уже стоит группа или запись `proxies:`,
/// не трогаем — цель адресуема и без нас.
fn node_targets(map: &mut Mapping, sources: &[NodeSource]) {
    let mut groups: Vec<Value> = map
        .get(Value::from("proxy-groups"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let mut taken: Vec<String> = groups.iter().filter_map(name_of).collect();
    taken.extend(names(map, "proxies"));

    let before = groups.len();
    for target in rule_targets(map) {
        if taken.contains(&target) {
            continue;
        }
        // Источники, в которых такой узел правда есть. Пусто — цель не узел (или опечатка):
        // группа с фильтром в никуда пустая, а пустую ядро не принимает и не стартует вовсе.
        let from: Vec<Value> = sources
            .iter()
            .filter(|source| provides(source) && source.names.contains(&target))
            .map(|source| Value::from(source.id.clone()))
            .collect();
        if from.is_empty() {
            continue;
        }
        let mut group = Mapping::new();
        Yaml::set(&mut group, "name", Value::from(target.clone()));
        Yaml::set(&mut group, "type", Value::from("select"));
        Yaml::set(&mut group, "use", Value::Sequence(from));
        Yaml::set(&mut group, "filter", Value::from(exact(&target)));
        groups.push(Value::Mapping(group));
        taken.push(target);
    }
    if groups.len() != before {
        Yaml::set(map, "proxy-groups", Value::Sequence(groups));
    }
}

/// Куда правила отправляют, без повторов.
fn rule_targets(map: &Mapping) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    let lines = map
        .get(Value::from("rules"))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    for line in lines.iter().filter_map(Value::as_str) {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(at) = RulesCodec::exit_at(&parts) else {
            continue;
        };
        if !targets.iter().any(|seen| seen == parts[at]) {
            targets.push(parts[at].to_string());
        }
    }
    targets
}

/// Выходы, которые у ядра есть всегда.
const EXITS: [&str; 6] = [
    DIRECT,
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "COMPATIBLE",
    "GLOBAL",
];

/// `no-resolve` в хвосте regex и составных правил (B-035). Цель у них — последняя часть
/// строки, и ядро прочло бы флаг целью; у доменного правила он ничего не значит. Документ
/// человека не трогаем — снимаем хвост только с того, что уходит ядру.
fn bare_flags(map: &mut Mapping) {
    let Some(rules) = map
        .get_mut(Value::from("rules"))
        .and_then(Value::as_sequence_mut)
    else {
        return;
    };
    for line in rules.iter_mut() {
        let Some(text) = line.as_str() else {
            continue;
        };
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        if !RulesCodec::flag_after_exit(&parts) {
            continue;
        }
        // Снимаем только хвост: середина — регулярка, и пробел после запятой в ней — её
        // часть (B-045).
        let bare = text
            .rsplit_once(',')
            .map_or(text, |(head, _)| head)
            .trim_end()
            .to_string();
        *line = Value::from(bare);
    }
}

/// Цель, которой нет, — на псевдоним (D-156).
///
/// Узел пропал из подписки — правило в него ядро отвергло бы вместе со всем конфигом, и VPN
/// не поднялся бы из-за одной строки. Выкинуть правило нельзя: непойманное уйдёт по `MATCH`,
/// а тот бывает `DIRECT` — мимо туннеля. Документ пользователя не трогаем: вернётся узел —
/// правило снова ведёт в него.
fn missing_targets(map: &mut Mapping) {
    let mut known = names(map, "proxy-groups");
    known.extend(names(map, "proxies"));
    let Some(rules) = map
        .get_mut(Value::from("rules"))
        .and_then(Value::as_sequence_mut)
    else {
        return;
    };
    for line in rules.iter_mut() {
        let Some(text) = line.as_str() else {
            continue;
        };
        let mut parts: Vec<&str> = text.split(',').map(str::trim).collect();
        let Some(at) = RulesCodec::exit_at(&parts) else {
            continue;
        };
        if EXITS.contains(&parts[at]) || known.iter().any(|name| name == parts[at]) {
            continue;
        }
        parts[at] = SELECTOR;
        let rewritten = parts.join(",");
        *line = Value::from(rewritten);
    }
}

/// Имена записей списка `key`: групп или узлов.
fn names(map: &Mapping, key: &str) -> Vec<String> {
    map.get(Value::from(key))
        .and_then(Value::as_sequence)
        .map(|list| list.iter().filter_map(name_of).collect())
        .unwrap_or_default()
}

/// Несколько имён одним фильтром: `^(одно|другое)$`. Каждое экранируется тем же
/// способом, что и одиночное, — иначе `Node (1)` стал бы скобочной группой.
pub(super) fn any(names: &[String]) -> String {
    let inner: Vec<String> = names
        .iter()
        .map(|name| {
            let escaped = exact(name);
            escaped[1..escaped.len() - 1].to_string()
        })
        .collect();
    format!("^({})$", inner.join("|"))
}

/// Имя узла фильтром «ровно оно»: метасимволы экранируются, иначе `Node (1)` стал бы
/// скобочной группой. Тот же приём, что и в окне при выборе узлов в группу.
fn exact(name: &str) -> String {
    let mut out = String::from("^");
    for symbol in name.chars() {
        if r"\.+*?()|[]{}^$".contains(symbol) {
            out.push('\\');
        }
        out.push(symbol);
    }
    out.push('$');
    out
}

/// Идентификаторы источников, у которых правда есть провайдер.
fn provider_ids(sources: &[NodeSource]) -> Vec<Value> {
    sources
        .iter()
        .filter(|source| provides(source))
        .map(|source| Value::from(source.id.clone()))
        .collect()
}

/// Поставить группе `use:` — и **только если есть что ставить**. Пустой список ядро
/// читает как «поля нет» и на группе без `proxies:` отказывается запускаться вовсе
/// (B-016), а писать пустоту туда, где её можно не писать, незачем и без этого.
fn set_use(group: &mut Mapping, sources: &[NodeSource]) {
    let list = provider_ids(sources);
    if !list.is_empty() {
        Yaml::set(group, "use", Value::Sequence(list));
    }
}

/// Заводить ли источнику провайдера.
///
/// Не заводить, если ни одной ссылки, которую ядро читает само, в нём нет: такой файл его
/// конвертер отвергает целиком (`format invalid`), и ядро не поднимается вовсе. Узлы такого
/// источника доезжают записями `proxies:` (D-063). Источник в формате clash-YAML ссылок
/// не содержит по определению — его провайдер нужен всегда.
pub(super) fn provides(_source: &NodeSource) -> bool {
    true
}

fn name_of(group: &Value) -> Option<String> {
    group
        .as_mapping()?
        .get(Value::from("name"))?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::auto::Grouping;
    use crate::config::files::LOCAL_PROXY_PORT;
    use crate::render::mihomo_groups::built;
    use crate::render::plan::NodeFact;

    /// Цель проверки живости приходит из документа клиента, и почти ни одному тесту здесь
    /// не интересна: заслоняем её умолчанием, чтобы не повторять пятым аргументом везде.
    /// Тому, кому она интересна, остаётся `super::config`.
    fn config(
        user: &[String],
        builtin: &[String],
        sources: &[NodeSource],
        probe: Option<u16>,
    ) -> Result<Effective> {
        super::MihomoRenderer::config(user, builtin, sources, probe, &plain())
    }

    /// Настройки клиента, при которых конфиг собирается «как обычно»: цель проверки
    /// живости умолчательная, UDP-группа выключена, маскировка рукопожатия — как
    /// у нового клиента (D-118).
    fn plain() -> Client {
        Client {
            volt: None,
            health: Check::default(),
            udp: false,
            mask: crate::config::awg::Mask::default(),
            lists: Vec::new(),
            exclude: Exclude::default(),
            grouping: Grouping::default(),
        }
    }

    /// Сборка без диска: документы пользователя подаём строками, источники — списком имён.
    fn assembled(documents: &[&str], sources: &[&str]) -> Effective {
        let user: Vec<String> = documents.iter().map(|d| (*d).to_string()).collect();
        config(&user, &[], &nodes(sources), None).unwrap()
    }

    fn nodes(sources: &[&str]) -> Vec<NodeSource> {
        sources.iter().map(|id| source(id, &[])).collect()
    }

    /// Источник со ссылками: они нужны там, где проверяется, что ядро прочитает само,
    /// а что клиент обязан записать за него (D-063).
    /// Источник для сборки: с D-122 у него нет ссылок — только имена узлов, которые
    /// ядро увидит в его провайдере.
    fn source(id: &str, names: &[&str]) -> NodeSource {
        NodeSource {
            id: id.to_string(),
            path: std::path::PathBuf::from(format!("/tmp/{id}.txt")),
            names: names.iter().map(|name| (*name).to_string()).collect(),
            udp: Vec::new(),
            facts: Vec::new(),
        }
    }

    /// Источник с узлами провайдера: ссылок в нём может не быть вовсе (clash-YAML),
    /// а имена ядро всё равно увидит.
    fn source_of(id: &str, names: &[&str]) -> NodeSource {
        NodeSource {
            names: names.iter().map(|name| (*name).to_string()).collect(),
            ..source(id, &[])
        }
    }

    /// D-083: встроенное встаёт между правилами пользователя и MATCH — его правило выше.
    #[test]
    fn a_builtin_set_lands_under_the_user_rules_and_above_match() {
        let mine = "rules:
  - DOMAIN-SUFFIX,ru,umiray
  - MATCH,umiray
";
        let builtin = [
            "DOMAIN-SUFFIX,ru,DIRECT".to_string(),
            "GEOSITE,category-ads-all,REJECT".to_string(),
            // Та же строка, что у пользователя: вторая такая всё равно мертва.
            "DOMAIN-SUFFIX,ru,umiray".to_string(),
        ];
        let out = parsed(&config(&[mine.to_string()], &builtin, &[], None).unwrap());
        let lines: Vec<&str> = out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            lines,
            [
                "DOMAIN-SUFFIX,ru,umiray",
                "DOMAIN-SUFFIX,ru,DIRECT",
                "GEOSITE,category-ads-all,REJECT",
                "MATCH,umiray",
            ],
            "своё правило перебивает наше, MATCH остаётся последним, повтор не дублируется"
        );
    }

    /// Выключенный набор не оставляет ни строки, а без документа пользователя встроенное
    /// всё равно доезжает: направление на это не влияет (D-083).
    #[test]
    fn builtin_rules_reach_the_core_in_any_direction() {
        let empty = parsed(&config(&[], &[], &[], None).unwrap());
        assert_eq!(
            empty["rules"].as_sequence().unwrap().len(),
            1,
            "выключено — только MATCH, который клиент ставит сам"
        );
        let with = parsed(
            &config(
                &[],
                &["GEOSITE,category-ads-all,REJECT".to_string()],
                &[],
                None,
            )
            .unwrap(),
        );
        let lines: Vec<&str> = with["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(lines, ["GEOSITE,category-ads-all,REJECT", "MATCH,umiray"]);
    }

    /// D-082: правило целится в узел — группу под него дописывает сборка, потому что
    /// поимённо узел провайдера не адресуется (S-012).
    #[test]
    fn a_rule_aimed_at_a_node_gets_a_group_of_that_one_node() {
        let rules = "rules:
  - DOMAIN-SUFFIX,github.com,Poland 1
  - MATCH,Sweden (0)
";
        let built = config(
            &[rules.to_string()],
            &[],
            &[source_of("aaaa1111bbbb2222", &["Poland 1", "Sweden (0)"])],
            None,
        )
        .unwrap();
        let out = parsed(&built);
        let groups = out["proxy-groups"].as_sequence().unwrap();
        let made: Vec<&Value> = groups
            .iter()
            .filter(|group| {
                let name = group["name"].as_str().unwrap_or_default();
                name == "Poland 1" || name == "Sweden (0)"
            })
            .collect();
        assert_eq!(made.len(), 2, "и правило, и MATCH умеют целиться в узел");
        assert_eq!(made[0]["type"], Value::from("select"));
        assert_eq!(
            made[0]["use"],
            Value::Sequence(vec![Value::from("aaaa1111bbbb2222")])
        );
        assert_eq!(made[0]["filter"], Value::from("^Poland 1$"));
        assert_eq!(
            made[1]["filter"],
            Value::from(r"^Sweden \(0\)$"),
            "скобки в имени — не скобочная группа"
        );
    }

    /// Своё авторитетнее нашего, и цель, адресуемая без нас, группы не получает.
    #[test]
    fn a_target_that_is_already_addressable_gets_nothing_extra() {
        let groups = "proxy-groups:
  - name: Poland 1
    type: select
    use: [aaaa1111bbbb2222]
";
        let rules = "rules:
  - DOMAIN,a.ru,Poland 1
  - DOMAIN,b.ru,DIRECT
  - DOMAIN,c.ru,опечатка
  - MATCH,umiray
";
        let built = config(
            &[groups.to_string(), rules.to_string()],
            &[],
            &[source_of("aaaa1111bbbb2222", &["Poland 1"])],
            None,
        )
        .unwrap();
        let out = parsed(&built);
        let names: Vec<&str> = out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|group| group["name"].as_str())
            .collect();
        assert_eq!(
            names.iter().filter(|name| **name == "Poland 1").count(),
            1,
            "группа пользователя остаётся одна"
        );
        assert!(
            !names.contains(&"опечатка"),
            "под несуществующий узел группы быть не должно: пустую ядро не примет"
        );
        assert!(!names.contains(&"DIRECT"), "концы маршрута — не узлы");
    }

    /// `no-resolve` в хвосте regex-правила человек пишет сам (B-035). Ядро у regex берёт целью
    /// последнюю часть строки — флаг отдаём без хвоста, иначе правило ушло бы в `umiray`
    /// с `,DIRECT` внутри регулярки и не совпало бы никогда. У доменных правил он ничего
    /// не значит, у адресных — остаётся.
    #[test]
    fn no_resolve_after_a_regex_rule_does_not_become_its_target() {
        let rules = "rules:
  - DOMAIN-REGEX,^(.+[.])?ozon[.](by|kz)$,DIRECT,no-resolve
  - GEOIP,RU,DIRECT,no-resolve
  - MATCH,DIRECT
";
        let out = parsed(&config(&[rules.to_string()], &[], &[], None).unwrap());
        let lines: Vec<&str> = out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            lines,
            [
                "DOMAIN-REGEX,^(.+[.])?ozon[.](by|kz)$,DIRECT",
                "GEOIP,RU,DIRECT,no-resolve",
                "MATCH,DIRECT",
            ]
        );
    }

    /// B-045: снятие хвоста резало строку по запятым, обрезало части и склеивало обратно —
    /// пробел после запятой внутри регулярки пропадал, и она переставала совпадать.
    #[test]
    fn a_space_after_a_comma_inside_a_regex_reaches_the_core() {
        let rules = "rules:
  - DOMAIN-REGEX,^a, b$,DIRECT,no-resolve
  - MATCH,DIRECT
";
        let out = parsed(&config(&[rules.to_string()], &[], &[], None).unwrap());
        assert_eq!(out["rules"][0].as_str(), Some("DOMAIN-REGEX,^a, b$,DIRECT"));
    }

    /// B-018, D-156: узел пропал из подписки — правило в него уходит в псевдоним, а не валит
    /// конфиг целиком. Цель составного правила — последняя часть, и узел там живой.
    #[test]
    fn a_rule_aimed_at_a_vanished_node_goes_through_the_alias() {
        let rules = "rules:
  - DOMAIN-SUFFIX,aeza.net,vless-reality-gone
  - AND,((DOMAIN,a.ru),(NETWORK,UDP)),Poland 1
  - SUB-RULE,(NETWORK,tcp),inner
  - DOMAIN,b.ru,REJECT
  - MATCH,vless-reality-gone
";
        let built = config(
            &[rules.to_string()],
            &[],
            &[source_of("aaaa1111bbbb2222", &["Poland 1"])],
            None,
        )
        .unwrap();
        let out = parsed(&built);
        let lines: Vec<&str> = out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            lines,
            [
                "DOMAIN-SUFFIX,aeza.net,umiray",
                "AND,((DOMAIN,a.ru),(NETWORK,UDP)),Poland 1",
                "SUB-RULE,(NETWORK,tcp),inner",
                "DOMAIN,b.ru,REJECT",
                "MATCH,umiray",
            ]
        );
        assert!(
            out["proxy-groups"]
                .as_sequence()
                .unwrap()
                .iter()
                .any(|group| group["name"] == "Poland 1"),
            "составное правило в живой узел получает его группу"
        );
    }

    const WG: &str =
        "wireguard://a2V5@10.9.8.7:51820?address=10.0.0.5/32&publickey=cHVi&mtu=1420#wg";

    fn parsed(effective: &Effective) -> Value {
        serde_yaml::from_str(&effective.yaml).unwrap()
    }

    /// Что предлагает псевдоним. Раньше это была отдельная команда для окна; с приходом
    /// направлений (D-056) окно спрашивает не список, а одно значение, и функция осталась
    /// нужна только здесь — чтобы видеть состав группы, не разбирая конфиг в каждом тесте.
    fn alias_options(documents: &[&str], sources: &[&str]) -> Vec<String> {
        alias_options_of(&parsed(&assembled(documents, sources)))
    }

    /// То же по уже собранному документу: там, где сборка нестандартная, помощник выше
    /// не подходит — он строит источники сам.
    fn alias_options_of(out: &Value) -> Vec<String> {
        out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|group| group["name"] == SELECTOR)
            .and_then(|group| group["proxies"].as_sequence())
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Пользовательские файлы накладываются по очереди, и последний выигрывает (D-044).
    /// Проверяем обе половины: и что предыдущие вообще доезжают, и что спор решается
    /// в пользу последнего.
    #[test]
    fn user_documents_layer_in_order_and_the_last_one_wins() {
        let out = parsed(&assembled(
            &["log-level: debug\nmixed-port: 1111\n", "mixed-port: 2222\n"],
            &[],
        ));
        assert_eq!(
            out["mixed-port"],
            Value::from(2222),
            "спор решает последний"
        );
        assert_eq!(
            out["log-level"],
            Value::from("debug"),
            "непротиворечивое из первого файла должно доехать"
        );
    }

    /// Выключенное должно быть написано, а не опущено: опущенное поле означает
    /// «решает ядро», и конфиг перестаёт отвечать на вопрос «а включено ли это».
    #[test]
    fn what_is_off_is_written_down_explicitly() {
        let out = parsed(&assembled(&[""], &[]));
        assert_eq!(out["allow-lan"], Value::from(false));
        assert_eq!(out["ipv6"], Value::from(false));
        assert_eq!(out["dns"]["enable"], Value::from(false));
        // Выбор узла помним мы (D-039) — у ядра эта память должна быть выключена.
        assert_eq!(out["profile"]["store-selected"], Value::from(false));
    }

    /// Метка исходящих ядра (kill switch на Linux, S-035) — только в TUN и только там,
    /// где её просит ОС; пробному прогону без прав её не достаётся.
    #[test]
    fn the_core_mark_goes_to_tun_but_not_to_the_dry_run() {
        let tun = assembled(&["tun:\n  enable: true\n"], &[]).yaml;
        let mark = crate::system::killswitch::CORE_MARK.map(Value::from);
        assert_eq!(parsed_text(&tun).get("routing-mark").cloned(), mark);
        let local = assembled(&["tun:\n  enable: false\n"], &[]).yaml;
        assert!(parsed_text(&local).get("routing-mark").is_none());
        let dry = MihomoRenderer::unprivileged(&tun).unwrap();
        assert!(parsed_text(&dry).get("routing-mark").is_none());
        assert_eq!(
            parsed_text(&dry)["tun"],
            parsed_text(&tun)["tun"],
            "остальное — как было"
        );
    }

    fn parsed_text(yaml: &str) -> Mapping {
        Yaml::top_mapping(yaml).unwrap()
    }

    /// Режим приходит из самого конфига, а не из настроек рядом с ним (D-052).
    #[test]
    fn the_mode_is_read_back_out_of_the_config() {
        let local = assembled(&["tun:\n  enable: false\n"], &[]);
        assert_eq!(local.mode, Mode::Local);
        assert_eq!(
            local.port,
            Some(LOCAL_PROXY_PORT),
            "в local готовность проверяем по порту"
        );

        let tun = assembled(&["tun:\n  enable: true\n"], &[]);
        assert_eq!(tun.mode, Mode::Tun);
        assert_eq!(
            tun.port, None,
            "в TUN слушающего порта нет — готовность так не проверить"
        );
    }

    #[test]
    fn the_ready_port_follows_the_user() {
        assert_eq!(assembled(&["mixed-port: 7777\n"], &[]).port, Some(7777));
    }

    /// Умолчания ставятся только при отсутствии, поэтому пользователь их перебивает.
    #[test]
    fn explicit_defaults_never_win_over_the_user() {
        let out = parsed(&assembled(&["allow-lan: true\nipv6: true\n"], &[]));
        assert_eq!(out["allow-lan"], Value::from(true));
        assert_eq!(out["ipv6"], Value::from(true));
    }

    /// Пользовательский `dns` без `enable` трогать нельзя: дописав туда `false`,
    /// мы выключили бы ровно то, что человек настраивал.
    #[test]
    fn a_user_dns_section_is_left_alone() {
        let out = parsed(&assembled(&["dns:\n  nameserver:\n    - 1.1.1.1\n"], &[]));
        assert_eq!(out["dns"]["nameserver"][0], Value::from("1.1.1.1"));
        assert!(
            out["dns"].get("enable").is_none(),
            "в чужой раздел `enable` не дописываем"
        );
    }

    /// Каждый источник — провайдер ядра, и все они подтягиваются и в автогруппу,
    /// и в псевдоним.
    #[test]
    fn every_source_becomes_a_provider_inside_both_client_groups() {
        let out = parsed(&assembled(&[""], &["a1", "b2"]));
        assert_eq!(out["proxy-providers"]["a1"]["type"], Value::from("file"));
        assert_eq!(
            out["proxy-providers"]["a1"]["health-check"]["enable"],
            Value::from(true),
            "без проверки живости в таблице не будет задержек"
        );

        let both = Value::Sequence(vec![Value::from("a1"), Value::from("b2")]);
        assert_eq!(out["proxy-groups"][0]["name"], Value::from(AUTO));
        assert_eq!(out["proxy-groups"][0]["type"], Value::from("load-balance"));
        assert_eq!(
            out["proxy-groups"][0]["strategy"],
            Value::from("consistent-hashing")
        );
        assert_eq!(out["proxy-groups"][0]["use"], both);
        assert_eq!(out["proxy-groups"][1]["name"], Value::from(SELECTOR));
        assert_eq!(
            out["proxy-groups"][1]["use"], both,
            "узлы провайдера попадают в группу только через use"
        );
    }

    /// Источник с UDP-узлами: имена и их протокол приходят из каталога, здесь только
    /// сырьё для сборки.
    fn udp_source(id: &str, all: &[&str], udp: &[&str]) -> NodeSource {
        NodeSource {
            id: id.to_string(),
            path: std::path::PathBuf::from(format!("/tmp/{id}.txt")),
            names: all.iter().map(|name| (*name).to_string()).collect(),
            udp: udp.iter().map(|name| (*name).to_string()).collect(),
            facts: Vec::new(),
        }
    }

    fn with_udp(sources: &[NodeSource]) -> Value {
        udp_render(sources, true)
    }

    fn udp_render(sources: &[NodeSource], rule: bool) -> Value {
        parsed(
            &super::MihomoRenderer::config(
                &[],
                &[],
                sources,
                None,
                &Client {
                    volt: None,
                    health: Check::default(),
                    udp: rule,
                    mask: crate::config::awg::Mask::default(),
                    lists: Vec::new(),
                    exclude: Exclude::default(),
                    grouping: Grouping {
                        udp: true,
                        ..Grouping::default()
                    },
                },
            )
            .unwrap(),
        )
    }

    /// Критерий D-113: с источником, где есть hysteria2, галка даёт группу из этих узлов
    /// и строку `NETWORK,udp` **перед** `MATCH` — после него она была бы мертва.
    #[test]
    fn the_udp_switch_gives_a_group_and_a_rule_before_match() {
        let out = with_udp(&[udp_source(
            "s1",
            &["vless-1", "hy2-1", "hy2-2"],
            &["hy2-1", "hy2-2"],
        )]);
        let group = out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|group| name_of(group).as_deref() == Some(UDP))
            .expect("группы с нативным UDP нет");
        assert_eq!(group["type"], Value::from("url-test"));
        assert_eq!(group["use"], Value::Sequence(vec![Value::from("s1")]));
        assert_eq!(group["filter"], Value::from("^(hy2-1|hy2-2)$"));

        let rules: Vec<&str> = out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let at = rules
            .iter()
            .position(|line| *line == "NETWORK,udp,umiray-udp");
        let matched = rules.iter().position(|line| is_match(line));
        assert!(at.is_some(), "правила про UDP нет: {rules:?}");
        assert!(at < matched, "правило после MATCH мертво: {rules:?}");
    }

    /// Вторая половина критерия: узлов с нативным UDP нет — нет ни группы, ни правила.
    /// Пустую группу ядро не принимает и не стартует вовсе.
    #[test]
    fn without_such_nodes_there_is_no_empty_group() {
        let out = with_udp(&[udp_source("s1", &["vless-1", "trojan-1"], &[])]);
        assert!(
            !out["proxy-groups"]
                .as_sequence()
                .unwrap()
                .iter()
                .any(|group| name_of(group).as_deref() == Some(UDP)),
            "пустая группа не должна появляться"
        );
        assert!(
            !out["rules"]
                .as_sequence()
                .unwrap()
                .iter()
                .any(|line| line.as_str() == Some("NETWORK,udp,umiray-udp")),
            "правило без группы ядро не примет"
        );
    }

    /// Галка группы без правила (D-113): группа есть, весь UDP по-прежнему идёт в выход.
    #[test]
    fn the_udp_group_alone_routes_nothing() {
        let out = udp_render(
            &[udp_source("s1", &["vless-1", "hy2-1"], &["hy2-1"])],
            false,
        );
        assert!(out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|group| name_of(group).as_deref() == Some(UDP)));
        assert!(!out["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|line| line.as_str() == Some("NETWORK,udp,umiray-udp")));
    }

    /// Выключенная галка не оставляет следов: ни группы, ни строки.
    #[test]
    fn the_switch_off_changes_nothing() {
        let out =
            parsed(&config(&[], &[], &[udp_source("s1", &["hy2-1"], &["hy2-1"])], None).unwrap());
        assert!(!out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|group| name_of(group).as_deref() == Some(UDP)));
    }

    /// Имя со скобками и точкой не должно стать регулярным выражением: `Node (1)`
    /// в фильтре — это скобочная группа, а не имя.
    #[test]
    fn names_with_metacharacters_stay_names() {
        assert_eq!(any(&["Node (1)".to_string()]), r"^(Node \(1\))$");
        assert_eq!(
            any(&["a.b".to_string(), "c+d".to_string()]),
            r"^(a\.b|c\+d)$"
        );
    }

    /// Критерий D-108: цель меняется в одном месте и доезжает **до обеих** проверок
    /// живости — и до провайдера, и до автогруппы. Раньше их было две, и один узел
    /// получал два вердикта.
    #[test]
    fn one_target_reaches_both_health_checks() {
        let mine = "http://example.org/generate_204";
        let out = parsed(
            &super::MihomoRenderer::config(
                &[],
                &[],
                &nodes(&["a1"]),
                None,
                &Client {
                    volt: None,
                    health: Check {
                        url: mine.to_string(),
                        interval: 600,
                    },
                    udp: false,
                    mask: crate::config::awg::Mask::default(),
                    lists: Vec::new(),
                    exclude: Exclude::default(),
                    grouping: Grouping::default(),
                },
            )
            .unwrap(),
        );
        assert_eq!(
            out["proxy-providers"]["a1"]["health-check"]["url"],
            Value::from(mine)
        );
        assert_eq!(out["proxy-groups"][0]["name"], Value::from(AUTO));
        assert_eq!(out["proxy-groups"][0]["url"], Value::from(mine));
        // И частота — одна на обе проверки (поле «Перепроверка групп» в настройках).
        assert_eq!(
            out["proxy-providers"]["a1"]["health-check"]["interval"],
            Value::from(600)
        );
        assert_eq!(out["proxy-groups"][0]["interval"], Value::from(600));
    }

    /// Без `expected-status` заглушка провайдера и страница captive portal отвечают
    /// двухсотым, и мёртвый выход остаётся в группе живым.
    #[test]
    fn a_node_behind_a_captive_portal_is_not_alive() {
        let out = parsed(&assembled(&[""], &["a1"]));
        let expected = Value::from(EXPECTED.to_string());
        assert_eq!(
            out["proxy-providers"]["a1"]["health-check"]["expected-status"],
            expected
        );
        assert_eq!(out["proxy-groups"][0]["expected-status"], expected);
    }

    /// Главное свойство раздела «Группы»: своя группа остаётся на месте, а узлы новой
    /// подписки попадают в автовыбор сами.
    #[test]
    fn adding_a_source_refreshes_auto_and_leaves_the_users_group_alone() {
        let mine = "proxy-groups:\n  - name: Польша\n    type: fallback\n    use: [a1]\n";

        let before = parsed(&assembled(&[mine], &["a1"]));
        assert_eq!(
            before["proxy-groups"][0]["use"],
            Value::Sequence(vec![Value::from("a1")])
        );

        let after = parsed(&assembled(&[mine], &["a1", "b2"]));
        assert_eq!(
            after["proxy-groups"][0]["use"],
            Value::Sequence(vec![Value::from("a1"), Value::from("b2")]),
            "новая подписка обязана доехать в автовыбор"
        );
        let theirs = after["proxy-groups"].as_sequence().unwrap().last().unwrap();
        assert_eq!(theirs["name"], Value::from("Польша"));
        assert_eq!(
            theirs["use"],
            Value::Sequence(vec![Value::from("a1")]),
            "чужая группа не должна шевелиться от новой подписки"
        );
    }

    /// Своя группа становится пунктом псевдонима: именно так на неё и целится MATCH.
    #[test]
    fn the_users_group_shows_up_as_a_choice() {
        let mine = "proxy-groups:\n  - name: Польша\n    type: fallback\n    use: [a1]\n";
        assert_eq!(
            alias_options(&[mine], &["a1"]),
            vec![AUTO, "Польша", DIRECT]
        );
    }

    /// Служебное имя не имеет двух смыслов: конфликт объясняется до запуска ядра.
    #[test]
    fn a_reserved_group_name_is_refused_before_start() {
        let mine = "proxy-groups:\n  - name: umiray\n    type: select\n    proxies: [DIRECT]\n";
        let error = config(&[mine.to_string()], &[], &nodes(&["a1"]), None)
            .err()
            .expect("конфликт должен быть ошибкой");
        assert!(error.to_string().contains("umiray"));
    }

    /// Две группы с одним именем ядро не примет; говорим это своими словами и до запуска.
    #[test]
    fn two_groups_with_one_name_are_refused_before_start() {
        let mine = "proxy-groups:\n  - name: Европа\n    type: select\n    proxies: [DIRECT]\n  - name: Европа\n    type: select\n    proxies: [DIRECT]\n";
        let error = config(&[mine.to_string()], &[], &nodes(&["a1"]), None)
            .err()
            .expect("повтор имени должен быть ошибкой");
        assert!(error.to_string().contains("Европа"));
    }

    /// Служебный вход под замер: он есть, только когда порт попросили, и слушает **петлю**.
    ///
    /// Без `listen` ядро поднимает его на всех интерфейсах — это открытый прокси наружу,
    /// и поймано это было измерением, а не чтением (S-016). Правило стережёт тест, потому
    /// что цена ошибки здесь не «неудобно», а «чужой ходит через ваш VPN».
    #[test]
    fn the_probe_seam_appears_only_on_demand_and_listens_on_the_loopback() {
        let plain = parsed(&assembled(&[""], &["s1"]));
        assert!(
            plain.get("listeners").is_none(),
            "без просьбы служебного входа быть не должно"
        );

        let user: Vec<String> = vec![String::new()];
        let with_probe = parsed(&config(&user, &[], &nodes(&["s1"]), Some(31_337)).unwrap());
        let inbound = &with_probe["listeners"][0];
        assert_eq!(inbound["listen"], Value::from("127.0.0.1"));
        assert_eq!(inbound["port"], Value::from(31_337));
        assert_eq!(inbound["proxy"], Value::from(PROBE));

        let groups = with_probe["proxy-groups"].as_sequence().unwrap();
        let probe = groups
            .iter()
            .find(|group| group["name"] == PROBE)
            .expect("группы probe нет");
        assert_eq!(probe["type"], Value::from("select"));
        assert!(
            probe.get("url").is_none(),
            "фоновая проверка этой группе не нужна: она для замеров по нажатию"
        );
    }

    /// Без источников мерить нечего, и служебная пара не заводится: пустой `use` ядро
    /// отвергнет целиком.
    #[test]
    fn the_probe_seam_is_skipped_without_sources() {
        let out = parsed(&config(&[String::new()], &[], &[], Some(31_337)).unwrap());
        assert!(out.get("listeners").is_none());
        assert!(!out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|group| group["name"] == PROBE));
    }

    /// До первой подписки конфиг обязан быть рабочим: MATCH целится в псевдоним всегда.
    #[test]
    fn without_sources_the_alias_still_exists_and_points_at_direct() {
        let out = parsed(&assembled(&[""], &[]));
        assert!(
            out.get("proxy-providers").is_none(),
            "провайдеров нет — и раздела быть не должно"
        );
        let groups = out["proxy-groups"].as_sequence().unwrap();
        assert_eq!(groups.len(), 1, "автогруппе не из чего выбирать");
        assert_eq!(groups[0]["name"], Value::from(SELECTOR));
        assert_eq!(
            groups[0]["proxies"],
            Value::Sequence(vec![Value::from(DIRECT)])
        );
        assert_eq!(
            out["rules"][0],
            Value::from(format!("MATCH,{SELECTOR}")),
            "правило целится в псевдоним и без источников"
        );
    }

    /// Написанный человеком узел с тем же именем остаётся его: своё авторитетнее нашего,
    /// как и с группами (D-053).
    #[test]
    fn a_proxy_the_user_wrote_himself_is_not_replaced() {
        let mine =
            "proxies:\n  - name: wg\n    type: socks5\n    server: 127.0.0.1\n    port: 1080\n";
        let out = parsed(&config(&[mine.to_string()], &[], &[source("s1", &[WG])], None).unwrap());
        let proxies = out["proxies"].as_sequence().unwrap();
        assert_eq!(
            proxies.len(),
            1,
            "второй записи с тем же именем быть не должно"
        );
        assert_eq!(
            proxies[0]["type"],
            Value::from("socks5"),
            "осталась запись человека"
        );
    }

    /// Живая проверка: собранный конфиг принимает само ядро, а не только наши тесты.
    ///
    /// Помечен `ignore`, потому что требует скачанного `mihomo.exe`. Запуск:
    /// `cargo test -- --ignored core_accepts_the_assembled_config --nocapture`
    ///
    /// Рядом идёт контрольный прогон на заведомо битом файле: без него зелёный ложный —
    /// `mihomo -t` на несуществующем пути молча создаёт дефолтный конфиг и рапортует
    /// об успехе (GOTCHAS).
    #[test]
    #[ignore]
    fn core_accepts_the_assembled_config() {
        let core = crate::paths::Paths::core();
        assert!(core.exists(), "ядро не скачано, проверять нечем");
        let dir = std::env::temp_dir().join("umiray-render-check");
        std::fs::create_dir_all(&dir).unwrap();

        let check = |name: &str, body: &str| -> bool {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            std::process::Command::new(&core)
                .arg("-t")
                .arg("-d")
                .arg(&dir)
                .arg("-f")
                .arg(&path)
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).contains("test is successful"))
                .unwrap_or(false)
        };

        for tun in ["false", "true"] {
            let yaml = assembled(&[&format!("tun:\n  enable: {tun}\n")], &[]).yaml;
            assert!(check("ok.yaml", &yaml), "ядро отвергло конфиг:\n{yaml}");
        }

        // С источником: только так проверяются AUTO и umiray — без провайдера они
        // не заводятся, и самая новая часть конфига осталась бы непроверенной (D-053).
        // Файл провайдера кладём в рабочий каталог: чужие пути ядро не читает (GOTCHAS).
        let links = dir.join("s1.txt");
        std::fs::write(&links, "vless://11111111-1111-1111-1111-111111111111@example.com:443?type=tcp&security=tls&sni=example.com#Test 0
").unwrap();
        let with_source = config(
            &[String::new()],
            &[],
            &[NodeSource {
                id: "s1".into(),
                path: links,
                names: vec!["Test 0".into()],
                udp: Vec::new(),
                facts: Vec::new(),
            }],
            None,
        )
        .unwrap()
        .yaml;
        assert!(
            check("sources.yaml", &with_source),
            "ядро отвергло конфиг с автогруппой:
{with_source}"
        );
        // Правило, целящееся в узел провайдера (D-082): группу под него дописывает сборка,
        // и вопрос «примет ли такой конфиг ядро» решается только ядром. Поимённо такой узел
        // не адресуется (S-012) — без группы здесь был бы отказ стартовать.
        let to_node = config(
            &["rules:
  - DOMAIN-SUFFIX,example.com,Test 0
  - MATCH,umiray
"
            .to_string()],
            &[],
            &[NodeSource {
                id: "s1".into(),
                path: dir.join("s1.txt"),
                names: vec!["Test 0".into()],
                udp: Vec::new(),
                facts: Vec::new(),
            }],
            None,
        )
        .unwrap()
        .yaml;
        assert!(
            check("node-target.yaml", &to_node),
            "ядро отвергло правило, целящееся в узел:
{to_node}"
        );

        // То, что собирает **форма** (D-074), — отдельный случай: у неё свой порядок полей
        // и свой способ записать выбор узлов (`use` плюс `filter`), и «наши тесты его
        // разбирают» ещё не значит «ядро его примет».
        let group = crate::config::groups::Group {
            name: "Европа".into(),
            kind: "url-test".into(),
            sources: vec!["s1".into()],
            proxies: Vec::new(),
            filter: Some("^(Test 0)$".into()),
            url: Some("http://www.google.com/generate_204".into()),
            interval: Some(300),
            tolerance: Some(150),
            strategy: None,
            extra: Vec::new(),
            origin: None,
        };
        let groups = crate::config::groups::GroupsCodec::render("", &[group]).unwrap();
        let rules = crate::config::rules::RulesCodec::render(
            "",
            &crate::config::rules::Routing {
                rules: vec![crate::config::rules::Rule {
                    kind: "DOMAIN-SUFFIX".into(),
                    values: vec!["github.com".into(), "gitlab.com".into()],
                    target: "Европа".into(),
                    options: Vec::new(),
                }],
                fallback: SELECTOR.into(),
                rule_sets: Vec::new(),
                ready: Vec::new(),
            },
        )
        .unwrap();
        let by_form = config(
            &[groups, rules],
            &[],
            &[NodeSource {
                id: "s1".into(),
                path: dir.join("s1.txt"),
                names: Vec::new(),
                udp: Vec::new(),
                facts: Vec::new(),
            }],
            None,
        )
        .unwrap()
        .yaml;
        assert!(
            check("form.yaml", &by_form),
            "ядро отвергло то, что собрала форма:\n{by_form}"
        );

        assert!(
            !check("broken.yaml", "proxy-groups:\n  - name: [\n"),
            "контрольный прогон обязан падать, иначе проверка ничего не значит"
        );
    }

    /// Источник, где у каждого узла известны протокол и страна (D-172).
    fn facts_source(id: &str, nodes: &[(&str, &str, &str)]) -> NodeSource {
        NodeSource {
            names: nodes
                .iter()
                .map(|(name, _, _)| (*name).to_string())
                .collect(),
            facts: nodes
                .iter()
                .map(|(name, kind, country)| NodeFact {
                    name: (*name).to_string(),
                    kind: (*kind).to_string(),
                    country: Some((*country).to_string()),
                })
                .collect(),
            ..source(id, &[])
        }
    }

    fn group<'a>(out: &'a Value, name: &str) -> Option<&'a Value> {
        out["proxy-groups"]
            .as_sequence()
            .unwrap()
            .iter()
            .find(|group| group["name"] == name)
    }

    fn built_with(sources: &[NodeSource], client: &Client, user: &[&str]) -> Value {
        let user: Vec<String> = user.iter().map(|d| (*d).to_string()).collect();
        parsed(&super::MihomoRenderer::config(&user, &[], sources, None, client).unwrap())
    }

    /// D-172: вынутый источник уходит из `use`, вынутый узел — в `exclude-filter`; окно
    /// видит тот же состав, что ядро.
    #[test]
    fn auto_leaves_out_what_was_taken_out() {
        let sources = [source_of("a", &["A1", "A2"]), source_of("b", &["B1"])];
        let client = Client {
            exclude: Exclude {
                sources: vec!["b".into()],
                nodes: vec!["A2".into()],
            },
            ..plain()
        };
        let out = built_with(&sources, &client, &[]);
        let auto = group(&out, AUTO).unwrap();
        assert_eq!(auto["use"], Value::Sequence(vec![Value::from("a")]));
        assert_eq!(auto["exclude-filter"], Value::from("^(A2)$"));
        assert_eq!(built(&sources, &client, &[])[0].members, ["A1"]);
    }

    /// Вынуто всё — исключения не действуют: пустую группу ядро не примет.
    #[test]
    fn taking_everything_out_of_auto_takes_nothing() {
        let sources = [source_of("a", &["A1"])];
        let client = Client {
            exclude: Exclude {
                sources: Vec::new(),
                nodes: vec!["A1".into()],
            },
            ..plain()
        };
        let out = built_with(&sources, &client, &[]);
        let auto = group(&out, AUTO).unwrap();
        assert_eq!(auto["use"], Value::Sequence(vec![Value::from("a")]));
        assert!(auto.get("exclude-filter").is_none());
    }

    /// D-172: группа по стране и по протоколу — только где узлов два и больше; они
    /// в псевдониме, а своя группа с тем же именем главнее.
    #[test]
    fn own_groups_come_only_where_there_is_a_choice() {
        let sources = [
            facts_source("a", &[("P1", "Vless", "PL"), ("R1", "Vless", "RU")]),
            facts_source("b", &[("P2", "TUIC", "pl")]),
        ];
        let client = Client {
            grouping: Grouping {
                location: true,
                protocol: true,
                udp: false,
            },
            ..plain()
        };
        let out = built_with(&sources, &client, &[]);
        let pl = group(&out, "umiray-geo-pl").expect("две Польши — группа");
        assert_eq!(pl["type"], Value::from("url-test"));
        assert_eq!(pl["filter"], Value::from("^(P1|P2)$"));
        assert_eq!(
            pl["use"],
            Value::Sequence(vec![Value::from("a"), Value::from("b")])
        );
        assert!(
            group(&out, "umiray-geo-ru").is_none(),
            "одна Россия — не группа"
        );
        assert!(group(&out, "umiray-proto-vless").is_some());
        assert!(group(&out, "umiray-proto-tuic").is_none());
        let options = alias_options_of(&out);
        assert!(options.contains(&"umiray-geo-pl".to_string()));

        let mine =
            "proxy-groups:\n  - name: umiray-geo-pl\n    type: select\n    proxies: [DIRECT]\n";
        let out = built_with(&sources, &client, &[mine]);
        assert_eq!(
            group(&out, "umiray-geo-pl").unwrap()["type"],
            Value::from("select"),
            "своя группа не подменяется"
        );
        let names: Vec<String> = built(&sources, &client, &["umiray-geo-pl".into()])
            .into_iter()
            .map(|group| group.name)
            .collect();
        assert!(!names.contains(&"umiray-geo-pl".to_string()));
    }
}
