//! Живые проверки: настоящий каталог пользователя, настоящее ядро, настоящая сеть.
//!
//! Отдельный модуль, а не тесты внутри своих: каждая проверка проходит через несколько
//! модулей сразу — переезд, сборку, супервизор и контроллер, — и ничьей в отдельности
//! не является.
//!
//! В обычный прогон не входят: все помечены `#[ignore]`. Они подменяют `LOCALAPPDATA`
//! всему процессу, поэтому идут в один поток.
//!
//! ```text
//! cargo test live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Настоящий каталог только **читается**: работа идёт на копии в temp. Ядро не копируется —
//! пятьдесят мегабайт, вместо этого жёсткая ссылка.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::app::migrate::Migration;
use crate::config::advanced::Advanced;
use crate::config::direction::Direction;
use crate::config::files;
use crate::config::files::Documents;
use crate::config::mode::Mode;
use crate::config::rulesets::RulesetStore;
use crate::core::mihomo::Mihomo;
use crate::db::{Db, Table};
use crate::nodes::sources::SourceStore;
use crate::paths::Paths;
use crate::system::autostart::Autostart;
use crate::system::killswitch::Firewall;
use crate::system::registry::Registry;
use crate::system::sysproxy::WinProxy;
use crate::system::task::SchedulerTask;
use crate::yaml::Yaml;

/// Настоящий каталог пользователя. Запоминается один раз — после первой песочницы
/// `LOCALAPPDATA` уже подменён, и второй раз спрашивать поздно.
fn real() -> &'static Path {
    static REAL: OnceLock<PathBuf> = OnceLock::new();
    // Каталог этой сборки, а не имя строкой: у отладочной он свой (D-116), и живые
    // проверки обязаны копировать тот, в котором сами же и живут.
    REAL.get_or_init(crate::paths::Paths::root)
}

/// Копия настоящего каталога в temp, переехавшая в базу так же, как при запуске (D-170);
/// `LOCALAPPDATA` подменяется процессу целиком.
fn sandbox(name: &str) -> PathBuf {
    let app = copy_real(name);
    crate::app::import::FileImport::run().expect("переезд в базу не прошёл");
    app
}

/// Копия настоящего каталога как есть — для проверки самого переезда.
fn copy_real(name: &str) -> PathBuf {
    let source = real().to_path_buf();
    assert!(
        source.exists(),
        "нет настоящего каталога {} — проверять нечего",
        source.display()
    );
    let root = std::env::temp_dir().join(format!("umiray-live-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join(crate::paths::Paths::root().file_name().unwrap());
    std::fs::create_dir_all(&app).unwrap();
    copy_tree(&source, &app);

    std::env::set_var("LOCALAPPDATA", &root);
    assert_eq!(Paths::root(), app, "песочница не подхватилась");
    app
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else if entry.file_name() == crate::paths::CORE_NAME {
            // Жёсткая ссылка вместо копии: тот же том, полсекунды против пятидесяти мегабайт.
            std::fs::hard_link(entry.path(), &target)
                .or_else(|_| std::fs::copy(entry.path(), &target).map(|_| ()))
                .unwrap();
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Принимает ли **настоящее ядро** то, что мы собрали. Аргументы те же, что у супервизора.
fn core_accepts(yaml: &str) -> (bool, String) {
    Paths::ensure_run_dir().unwrap();
    let path = Paths::effective_config();
    std::fs::write(&path, yaml).unwrap();
    let out = std::process::Command::new(Paths::core())
        .arg("-t")
        .arg("-d")
        .arg(Paths::run_dir())
        .arg("-f")
        .arg(&path)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text.contains("test is successful"), text)
}

/// Выбрать готовый набор в маршруте выбранного набора так, как это делает окно (D-158):
/// правкой раздела `ready` и записью с проверкой. Отдаёт, был ли он выбран до этого.
fn set_ready(state: &crate::app::state::AppState, id: &str, on: bool) -> bool {
    use crate::config::presets::PresetStore;
    use crate::config::route::ReadyUse;
    use crate::config::rules::RulesCodec;
    let preset = state
        .routing
        .applied_preset(state)
        .expect("набор есть всегда");
    let text = PresetStore::read(&preset, "rules").unwrap();
    let mut routing = RulesCodec::parse(&text).unwrap();
    let was = routing.ready.iter().any(|set| set.id == id);
    routing.ready.retain(|set| set.id != id);
    if on {
        routing.ready.push(ReadyUse {
            id: id.into(),
            target: None,
            priority: Default::default(),
        });
    }
    let text = RulesCodec::render(&text, &routing).unwrap();
    crate::config::route_doc::RouteDocument::check(&text).unwrap();
    PresetStore::write(&preset, "rules", &text).unwrap();
    was
}

/// Гасит ядро, что бы ни случилось с тестом. Без этого паника посреди TUN-проверки
/// оставила бы поднятый адаптер и переписанную таблицу маршрутов.
struct Running<'a>(&'a Mihomo);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Выход через первый отвечающий узел. Мёртвый узел подписки — не повод валить проверку,
/// которая не про него: перебираем, пока кто-нибудь не выведет наружу.
async fn alive_exit(mihomo: &Mihomo, port: u16) -> String {
    for node in crate::nodes::source_catalog::SourceCatalog::nodes()
        .iter()
        .filter(|node| node.supported)
        .take(10)
    {
        mihomo.select(&node.name).await.expect("узел не выбрался");
        if let Ok(ip) = try_external_ip(Some(port)).await {
            println!("через «{}»: {ip}", node.name);
            return ip;
        }
    }
    panic!("ни один узел не ответил — выходить не через что");
}

/// Внешний адрес, когда выход решает группа ядра: `fallback` и `url-test` переключаются
/// на живой узел только после первой проверки живости, а до неё ведут в первый по списку.
async fn eventual_external_ip(port: u16) -> String {
    let started = std::time::Instant::now();
    loop {
        match try_external_ip(Some(port)).await {
            Ok(ip) => return ip,
            Err(why) if started.elapsed() > std::time::Duration::from_secs(30) => {
                panic!("запрос наружу не прошёл и за 30 с: {why}")
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
        }
    }
}

/// Внешний адрес: через прокси или напрямую. Единственная проверка, которой мало
/// «ядро отвечает» — она показывает, что трафик действительно идёт наружу через VPN.
async fn external_ip(through: Option<u16>) -> String {
    try_external_ip(through)
        .await
        .unwrap_or_else(|why| panic!("запрос наружу не прошёл: {why}"))
}

/// То же, но без паники: узел подписки может быть мёртв, и это не повод валить проверку —
/// повод взять следующий.
async fn try_external_ip(through: Option<u16>) -> std::result::Result<String, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(25));
    if let Some(port) = through {
        builder = builder.proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap());
    }
    let response = builder
        .build()
        .map_err(|e| e.to_string())?
        .get("https://api.ipify.org")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response
        .text()
        .await
        .map_err(|e| e.to_string())?
        .trim()
        .to_string())
}

/// Переезд настоящего каталога в базу (D-170) на его копии: документы и настройки
/// доехали байт в байт, источников и наборов столько же, файлов не осталось, ядро
/// принимает собранное.
///
/// Живая, потому что смотрит на настоящую раскладку человека, а не на придуманную.
#[test]
#[ignore]
fn live_the_real_directory_moves_into_the_database() {
    let app = copy_real("import");
    let documents: Vec<(&str, Table, &str, String)> = [
        ("advanced.yaml", Table::Documents, "advanced"),
        ("client.yaml", Table::Documents, "client"),
        ("groups.yaml", Table::Documents, "groups"),
        ("settings.json", Table::State, "settings"),
        ("hwid.txt", Table::State, "hwid"),
    ]
    .into_iter()
    .filter_map(|(file, table, id)| {
        Some((
            file,
            table,
            id,
            std::fs::read_to_string(app.join(file)).ok()?,
        ))
    })
    .collect();
    let count = |dir: &str, suffix: &str, not: &str| {
        std::fs::read_dir(app.join(dir))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .filter(|name| name.ends_with(suffix) && !name.ends_with(not))
                    .count()
            })
            .unwrap_or(0)
    };
    let sources = count("sources", ".json", ".patch.json");
    let presets = count("presets", ".json", ".patch.json");
    if documents.is_empty() && sources == 0 {
        println!("настоящий каталог уже в базе — переносить нечего");
    }

    crate::app::import::FileImport::run().expect("переезд не прошёл");

    // Сверка — сразу после переезда: дальше `Migration` вправе освежить пустой шаблон.
    for (file, table, id, text) in &documents {
        assert!(!app.join(file).exists(), "{file} остался на диске");
        assert_eq!(
            Db::get(*table, id, "").unwrap().as_deref(),
            Some(text.as_str()),
            "{file} доехал не байт в байт"
        );
    }
    if sources > 0 {
        assert_eq!(
            SourceStore::list().len(),
            sources,
            "источников не столько же"
        );
    }
    assert!(
        crate::config::presets::PresetStore::list().len() >= presets.max(1),
        "наборы потерялись"
    );
    for dir in ["presets", "collections", "lists"] {
        assert!(!app.join(dir).exists(), "{dir}/ остался на диске");
    }
    println!(
        "переехало: документов {}, источников {sources}, наборов {presets}",
        documents.len()
    );
    Migration::run().expect("шаги после переезда не прошли");

    let (ok, log) = core_accepts(
        &crate::render::effective::ConfigRenderer::effective(
            &crate::render::plan::Route::default(),
            None,
        )
        .unwrap()
        .yaml,
    );
    assert!(ok, "ядро отвергло конфиг после переезда:\n{log}");
    println!("ядро приняло собранный конфиг после переезда");
}

/// Переключение режимов подряд: `advanced.yaml` остаётся полным и валидным, а написанное
/// пользователем переживает все четыре шага.
#[test]
#[ignore]
fn live_mode_switch_keeps_the_config_whole() {
    let _app = sandbox("mode");
    Migration::run().unwrap();

    // Как будто пользователь поправил своё руками: сменил порт и дописал поле, которого
    // клиент не знает вовсе. Порт ставим разбором, а не заменой строки: песочница —
    // копия настоящего каталога, и порт в ней какой угодно (у debug — 3091, D-150).
    let mut own = Yaml::top_mapping(&Documents::read(files::ADVANCED).unwrap()).unwrap();
    Yaml::set(&mut own, "mixed-port", serde_yaml::Value::from(7777));
    let mine = serde_yaml::to_string(&serde_yaml::Value::Mapping(own)).unwrap()
        + "
experimental:
  quic-go-disable-gso: true
";
    Documents::write(files::ADVANCED, &mine).unwrap();

    for step in [Mode::Tun, Mode::Local, Mode::Tun, Mode::Local] {
        Mode::write(step).expect("режим не записался");

        let text = Documents::read(files::ADVANCED).unwrap();
        let map = Yaml::top_mapping(&text).unwrap();
        let out = serde_yaml::Value::Mapping(map);

        assert_eq!(
            out["tun"]["enable"],
            serde_yaml::Value::from(step == Mode::Tun),
            "режим не записался: {step:?}"
        );
        assert_eq!(
            out["mixed-port"],
            serde_yaml::Value::from(7777),
            "свой порт обязан пережить переключение"
        );
        assert_eq!(
            out["experimental"]["quic-go-disable-gso"],
            serde_yaml::Value::from(true),
            "незнакомое клиенту поле обязано пережить переключение"
        );
        assert_eq!(
            out["profile"]["store-selected"],
            serde_yaml::Value::from(false),
            "конфиг обязан остаться полным"
        );

        let effective = crate::render::effective::ConfigRenderer::effective(
            &crate::render::plan::Route::default(),
            None,
        )
        .unwrap();
        assert_eq!(effective.mode, step, "режим читается из собранного конфига");
        let (ok, log) = core_accepts(&effective.yaml);
        assert!(ok, "ядро отвергло конфиг в режиме {step:?}:\n{log}");
        println!("{step:?}: ядро приняло конфиг, чужие поля на месте");
    }
}

/// Живое ядро в local-режиме: поднялось, знает `AUTO`, переключается на него и на узел,
/// и через него действительно идёт трафик.
#[tokio::test]
#[ignore]
async fn live_core_routes_through_the_alias() {
    sandbox("core");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);

    let status = mihomo.status();
    assert!(status.running);
    assert_eq!(status.mode, Some(Mode::Local));
    let port = status.port.expect("в local-режиме порт обязан быть");
    println!("ядро поднялось на 127.0.0.1:{port}");

    let nodes = crate::nodes::source_catalog::SourceCatalog::nodes();
    assert!(!nodes.is_empty(), "источники есть, а узлов нет");
    println!("узлов в источниках: {}", nodes.len());

    // Главная проверка D-053: ядро знает нашу автогруппу. На несуществующее имя оно
    // отвечает 400, поэтому успех здесь и означает, что группа собралась.
    mihomo
        .select(crate::config::direction::AUTO)
        .await
        .expect("ядро не знает AUTO — автогруппа не собралась");
    assert_eq!(
        mihomo.selected().await.as_deref(),
        Some(crate::config::direction::AUTO),
        "псевдоним не переключился на автовыбор"
    );

    let auto_ip = external_ip(Some(port)).await;
    println!("через AUTO внешний адрес: {auto_ip}");
    assert!(auto_ip.contains('.') || auto_ip.contains(':'), "{auto_ip}");

    // И тот же путь на конкретный узел. Перебираем несколько: узел подписки может быть
    // мёртв, и падать из-за чужого сервера проверка не должна — она про переключение.
    let mut through_node = None;
    for node in nodes.iter().take(4) {
        mihomo.select(&node.name).await.expect("узел не выбрался");
        assert_eq!(
            mihomo.selected().await.as_deref(),
            Some(node.name.as_str()),
            "псевдоним не переключился на узел"
        );
        match try_external_ip(Some(port)).await {
            Ok(ip) => {
                println!("через «{}» внешний адрес: {ip}", node.name);
                through_node = Some(ip);
                break;
            }
            Err(why) => println!("узел «{}» не ответил ({why}) — берём следующий", node.name),
        }
    }
    assert!(
        through_node.is_some(),
        "ни один из четырёх узлов не ответил — проверять переключение не на чем"
    );

    let traffic = mihomo
        .traffic()
        .await
        .unwrap()
        .expect("трафик у работающего ядра обязан читаться");
    println!("передано вверх {} вниз {}", traffic.up, traffic.down);
    assert!(
        traffic.up > 0 && traffic.down > 0,
        "счётчики пустые — значит трафик шёл мимо ядра"
    );
}

/// TUN. Нужны права администратора, поэтому запускать этот тест отдельно и из
/// поднятой консоли:
///
/// ```text
/// cargo test live_tun -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_tun_captures_everything() {
    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        "TUN без прав администратора не поднимется — запустите тест из поднятой консоли"
    );

    // До запуска, чтобы было с чем сравнивать. Не печатаем: это домашний адрес.
    let before = external_ip(None).await;

    sandbox("tun");
    Migration::run().unwrap();
    Mode::write(Mode::Tun).unwrap();

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);

    let status = mihomo.status();
    assert_eq!(status.mode, Some(Mode::Tun));
    assert_eq!(
        status.port, None,
        "в TUN слушающего порта нет — готовность проверяется ответом ядра"
    );

    // Напрямую, без прокси: в TUN перехватывается весь трафик машины, и в этом весь смысл.
    let after = external_ip(None).await;
    println!("в TUN внешний адрес: {after}");
    assert_ne!(
        before, after,
        "адрес не изменился — трафик идёт мимо адаптера"
    );
}

/// Утечка имён под настоящим TUN (D-099).
///
/// Единственная проба, которую нельзя проверить в режиме Proxy: там перехвата нет
/// по устройству режима. Здесь адаптер поднимается по-настоящему, и `dns-hijack` обязан
/// поймать запрос к заведомо мёртвому резолверу.
///
/// ```text
/// cargo test live_dns_leak -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_dns_leak_under_tun() {
    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        "TUN без прав администратора не поднимется — запустите тест из поднятой консоли"
    );
    sandbox("dns-leak");
    Migration::run().unwrap();
    let physical = crate::system::net::NetInfo::physical_resolvers().unwrap();
    assert!(
        !physical.is_empty(),
        "Windows не назвала DNS поднятого физического адаптера — проверять утечку не на чем"
    );
    Mode::write(Mode::Tun).unwrap();

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&mihomo);

    // Физический резолвер и чужой публичный обязаны вернуть подменный адрес ядра
    // (`198.18.0.0/16`) или промолчать: настоящий ответ значит, что запрос ушёл в обход.
    for addr in physical.iter().map(String::as_str).chain(["9.9.9.9"]) {
        let candidate = crate::diag::dns::Candidate {
            provider: addr.into(),
            variant: addr.into(),
            filter: String::new(),
            proto: "udp".into(),
            addr: addr.into(),
        };
        let shot = crate::diag::dns::DnsProbe::shoot(
            &candidate,
            crate::diag::dns::DEFAULT_DOMAIN,
            Duration::from_millis(1500),
        )
        .await;
        println!("{addr}: {:?} {:?}", shot.ips, shot.error);
        let leaked = shot.ok()
            && shot.ips.iter().any(|ip| match ip {
                std::net::IpAddr::V4(v4) => v4.octets()[..2] != [198, 18],
                std::net::IpAddr::V6(_) => true,
            });
        assert!(
            !leaked,
            "{addr} ответил настоящим адресом — имена уходят в обход туннеля"
        );
    }
}

/// Свой набор при включённой маршрутизации: группа и правило, которые написал человек,
/// доезжают до ядра и правда уводят трафик.
///
/// Это и есть сценарий «MATCH на Польшу» из постановки — только правило пишет пользователь,
/// а не клиент (D-166).
#[tokio::test]
#[ignore]
async fn live_a_user_set_routes_through_its_own_group() {
    sandbox("group");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let sources = crate::nodes::sources::SourceStore::list();
    assert!(
        !sources.is_empty(),
        "нет источников — группе не из чего брать"
    );

    let state = crate::app::state::AppState::new();
    let preset = state.presets.create().unwrap();

    let groups = format!(
        "proxy-groups:
  - name: Своя
    type: fallback
    use: [{}]
    url: http://www.gstatic.com/generate_204
    interval: 300
",
        sources[0].id
    );
    Documents::write(files::GROUPS, &groups).unwrap();
    Documents::write(
        &format!("rules/{}", preset.id),
        "rules:
  - MATCH,Своя
",
    )
    .unwrap();
    // Применить — отдельное действие (D-071): правка набора его не включает.
    state.presets.choose(&state, &preset.id).unwrap();
    assert!(
        state.settings.get().routing,
        "применить набор — включить маршрутизацию"
    );

    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        effective.yaml.contains("MATCH,Своя"),
        "правило пользователя обязано доехать до сборки"
    );
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(
        ok,
        "ядро отвергло конфиг со своей группой:
{log}"
    );

    let mihomo = Mihomo::new();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&mihomo);

    let port = mihomo.status().port.unwrap();
    println!(
        "через свою группу внешний адрес: {}",
        eventual_external_ip(port).await
    );
}

/// Тумблер маршрутизации решает, участвует ли набор в сборке, — и ничего не возит
/// (D-071, D-166).
///
/// Главное здесь — что текст набора не зависит от тумблера вовсе: он лежит на своём
/// месте всегда, а выключенная маршрутизация просто перестаёт его подмешивать.
#[tokio::test]
#[ignore]
async fn live_routing_switch_decides_whether_the_set_is_used() {
    sandbox("sets");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let mine = "rules:\n  - MATCH,DIRECT\n";

    // Набор завёл переезд: без единого набора разделу нечего показывать.
    let preset = crate::config::presets::PresetStore::list()
        .first()
        .expect("переезд обязан был завести первый набор")
        .id
        .clone();
    crate::config::presets::PresetStore::write(&preset, "rules", mine).unwrap();

    // Маршрутизация выключена — набор не участвует: всё в выбранный выход.
    state.routing.set_routing(&state, false).unwrap();
    let yaml = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap()
    .yaml;
    assert!(
        yaml.contains("MATCH,umiray"),
        "без маршрутизации дно ставит клиент:\n{yaml}"
    );
    assert!(
        !yaml.contains("MATCH,DIRECT"),
        "набор не должен участвовать при выключенной маршрутизации"
    );

    // Применили — участвует, и текст остался ровно тем же.
    state.presets.choose(&state, &preset).unwrap();
    assert_eq!(
        crate::config::presets::PresetStore::read(&preset, "rules").unwrap(),
        mine,
        "набор обязан лежать слово в слово, что бы ни делал тумблер"
    );
    let yaml = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap()
    .yaml;
    assert!(
        yaml.contains("MATCH,DIRECT"),
        "применённый набор обязан доехать до сборки:\n{yaml}"
    );

    // И ядро принимает конфиг в каждом направлении, включая наш набор.
    for direction in [Direction::Direct, Direction::Auto, Direction::Manual] {
        state
            .routing
            .set_direction(&state, direction, None)
            .unwrap();
        let (ok, log) = core_accepts(
            &crate::render::effective::ConfigRenderer::effective(
                &state.routing.document(&state).unwrap(),
                None,
            )
            .unwrap()
            .yaml,
        );
        assert!(
            ok,
            "ядро отвергло конфиг в направлении {direction:?}:\n{log}"
        );
        println!("{direction:?}: ядро приняло конфиг");
    }
}

/// Направление правда меняет выход, а не только подпись в окне (D-056).
///
/// Домашний адрес не печатаем: он тут нужен только для сравнения.
#[tokio::test]
#[ignore]
async fn live_directions_change_the_exit() {
    sandbox("exit");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    // Ядро поднимает **тот же** супервизор, что держит состояние: направление наводит
    // псевдоним через него, и отдельно созданный второй просто ничего бы не сделал.
    let state = crate::app::state::AppState::new();
    state
        .routing
        .set_direction(&state, Direction::Auto, None)
        .unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    if let Err(why) = state.mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.mihomo);
    let port = state
        .mihomo
        .status()
        .port
        .expect("в local-режиме порт обязан быть");

    state.routing.point_alias(&state).await.unwrap();
    let through_auto = external_ip(Some(port)).await;
    println!("AUTO: {through_auto}");

    // Перебираем несколько узлов: мёртвый сервер подписки — не повод валить проверку,
    // она про переключение направления, а не про чужой аптайм.
    let mut through_node = None;
    for node in crate::nodes::source_catalog::SourceCatalog::nodes()
        .into_iter()
        .take(4)
    {
        state
            .routing
            .set_direction(&state, Direction::Manual, Some(node.name.clone()))
            .unwrap();
        // Настройку записали — теперь скажите об этом ядру. Без этого проверка утверждает
        // про действие, которого не совершала: псевдоним остаётся там, куда его навели
        // в прошлый раз, и «MANUAL» с «DIRECT» отвечают адресом от `AUTO`.
        // Руками — потому что здесь нет `AppHandle`. В настоящей жизни эту строку делает
        // `connect::apply`, и то, что она там есть, сторожит ui-check, а не эта проверка.
        state.routing.point_alias(&state).await.unwrap();
        match try_external_ip(Some(port)).await {
            Ok(ip) => {
                println!("MANUAL «{}»: {ip}", node.name);
                through_node = Some(ip);
                break;
            }
            Err(why) => println!("узел «{}» не ответил ({why}) — берём следующий", node.name),
        }
    }
    let through_node = through_node.expect("ни один узел не ответил");

    state
        .routing
        .set_direction(&state, Direction::Direct, None)
        .unwrap();
    state.routing.point_alias(&state).await.unwrap();
    let direct = external_ip(Some(port)).await;
    assert_ne!(
        direct, through_auto,
        "DIRECT обязан идти мимо VPN — адрес не должен совпадать с адресом через AUTO"
    );
    assert_ne!(direct, through_node, "то же и для выбранного вручную узла");
    println!("DIRECT: адрес отличается от обоих — трафик пошёл мимо VPN");
}

/// Состав источников менялся мимо собранного набора, и это ломало ядро (B-005).
///
/// Поломка была структурной: собранные группы лежали в файле, файл жил своей жизнью,
/// и любое изменение состава оставляло его вчерашним. После D-071 хранить их негде —
/// собранное собирается в момент сборки, — и проверка стережёт именно это свойство:
/// удалили источник, и его больше нет в конфиге **без единого пересобирающего вызова**.
#[tokio::test]
#[ignore]
async fn live_source_set_changes_reach_the_generated_groups() {
    let app = sandbox("sources-reach-auto");
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();
    println!("песочница: {}", app.display());

    Migration::run().unwrap();
    let before: Vec<String> = crate::nodes::sources::SourceStore::list()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(
        before.len() >= 2,
        "нужны хотя бы два источника, есть {}",
        before.len()
    );

    let used = |yaml: &str| -> Vec<String> {
        before
            .iter()
            .filter(|id| yaml.contains(id.as_str()))
            .cloned()
            .collect()
    };
    let groups = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap()
    .yaml;
    assert_eq!(used(&groups).len(), before.len(), "сначала все на месте");

    // Удаляем один источник — и **ничего не пересобираем**: пересобирать нечего.
    let gone = before[0].clone();
    crate::nodes::sources::SourceStore::delete(&gone).unwrap();

    let groups = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap()
    .yaml;
    assert!(
        !groups.contains(&gone),
        "удалённый источник остался в собранных группах — ядро откажется стартовать:
{groups}"
    );

    let yaml = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap()
    .yaml;
    let (ok, log) = core_accepts(&yaml);
    assert!(
        ok,
        "ядро не приняло конфиг после удаления источника:
{log}"
    );
    println!("после удаления «{gone}» ядро приняло конфиг");
}

/// Замер «через прокси» правда идёт через ядро, а `healthcheck` правда дожидается
/// результата (D-069).
///
/// Открытый вопрос был ровно один и решающий: отвечает ли
/// `GET /providers/proxies/<id>/healthcheck` **после** проверки или сразу. Во втором случае
/// историю мы читаем раньше, чем ядро её дописало, и «лучшее из двух» оказалось бы лучшим
/// из ничего — то есть колонка молча осталась бы пустой.
///
/// Проверка отвечает на него измерением: если после двух проходов ни один узел не получил
/// числа с пометкой «через прокси», значит ответ приходит раньше замера.
#[tokio::test]
#[ignore]
async fn live_proxy_ping_goes_through_the_core() {
    use crate::nodes::ping::Method;

    sandbox("ping-proxy");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    crate::app::client::ClientConfig::set_ping(Method::Proxy).unwrap();
    assert!(
        !crate::nodes::sources::SourceStore::list().is_empty(),
        "нет источников — мерить нечего"
    );

    // На остановленном ядре замер обязан отказать словами, а не оставить прочерки молча.
    let refused = state.catalog.measure(&state).await;
    assert!(refused.is_err(), "через прокси без ядра мерить нечем");
    println!("без ядра: {}", refused.unwrap_err());

    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    if let Err(why) = state.mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.mihomo);

    let started = std::time::Instant::now();
    state
        .catalog
        .measure(&state)
        .await
        .expect("замер через прокси не прошёл");
    let spent = started.elapsed();

    let nodes = state.catalog.nodes();
    let measured: Vec<_> = nodes.iter().filter(|node| node.delay.is_some()).collect();
    let through: Vec<_> = measured
        .iter()
        .filter(|node| node.method == Some(Method::Proxy) && !node.fallback)
        .collect();
    let fallback = measured.iter().filter(|node| node.fallback).count();

    for node in measured.iter().take(8) {
        println!(
            "{}: {} мс, {:?}, фолбэк {}",
            node.name,
            node.delay.unwrap(),
            node.method.unwrap(),
            node.fallback
        );
    }
    println!(
        "замер занял {spent:?}: через прокси {} из {} узлов, запасным способом {fallback}",
        through.len(),
        nodes.len()
    );

    assert!(
        !through.is_empty(),
        "ни один узел не померился через прокси: либо healthcheck отвечает раньше, чем меряет,          либо наружу не идёт ни один узел подписки"
    );
}

/// S-016: можно ли померить **выбранный** узел своим транспортом — и с keep-alive.
///
/// Тезис: mihomo умеет отдельный вход (`listeners:`) с полем `proxy:`, привязывающим его
/// трафик к конкретной группе мимо правил. Тогда клиент заводит служебную группу `probe`,
/// наводит её на нужный узел через API (мгновенно, без перезапуска), открывает **свой**
/// CONNECT-туннель к этому входу и шлёт в нём два запроса: второй и есть «чистое время
/// ответа по уже поднятому соединению» — то, что просили как `proxy-keepalive` (D-069).
///
/// Проверка отвечает на три вопроса сразу: принимает ли ядро такой конфиг, меняет ли
/// `PUT /proxies/probe` выход для новых соединений без перезапуска, и правда ли второй
/// запрос в туннеле заметно быстрее первого.
#[tokio::test]
#[ignore]
async fn live_probe_listener_measures_a_chosen_node() {
    use serde_yaml::Value;
    use std::io::{Read, Write};

    const LISTENER: u16 = 39_299;
    const API: u16 = 39_298;
    const SECRET: &str = "spike";

    sandbox("probe");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    // Конфиг: обычный собранный плюс служебный вход и группа под него.
    let mut map = Yaml::top_mapping(
        &crate::render::effective::ConfigRenderer::effective(
            &crate::render::plan::Route::default(),
            None,
        )
        .unwrap()
        .yaml,
    )
    .unwrap();
    let sources: Vec<Value> = crate::nodes::sources::SourceStore::list()
        .into_iter()
        .map(|source| Value::from(source.id))
        .collect();
    assert!(!sources.is_empty(), "нет источников — выбирать не из чего");

    let mut probe = serde_yaml::Mapping::new();
    probe.insert(Value::from("name"), Value::from("probe"));
    probe.insert(Value::from("type"), Value::from("select"));
    probe.insert(Value::from("use"), Value::Sequence(sources));
    map.get_mut(Value::from("proxy-groups"))
        .and_then(Value::as_sequence_mut)
        .expect("в собранном конфиге нет групп")
        .push(Value::Mapping(probe));

    let mut inbound = serde_yaml::Mapping::new();
    inbound.insert(Value::from("name"), Value::from("probe-in"));
    inbound.insert(Value::from("type"), Value::from("mixed"));
    inbound.insert(Value::from("port"), Value::from(LISTENER));
    inbound.insert(Value::from("proxy"), Value::from("probe"));
    map.insert(
        Value::from("listeners"),
        Value::Sequence(vec![Value::Mapping(inbound)]),
    );

    let yaml = serde_yaml::to_string(&Value::Mapping(map)).unwrap();
    let (ok, log) = core_accepts(&yaml);
    assert!(
        ok,
        "ядро не приняло конфиг со служебным входом:
{log}"
    );
    println!("1/3: конфиг со служебным входом принят");

    // Ядро поднимаем сами: супервизор про служебные входы ничего не знает, а спайку
    // и не нужно, чтобы знал.
    Paths::ensure_run_dir().unwrap();
    std::fs::write(Paths::effective_config(), &yaml).unwrap();
    let core = std::process::Command::new(Paths::core())
        .arg("-d")
        .arg(Paths::run_dir())
        .arg("-f")
        .arg(Paths::effective_config())
        .arg("-ext-ctl")
        .arg(format!("127.0.0.1:{API}"))
        .arg("-secret")
        .arg(SECRET)
        .spawn()
        .expect("ядро не запустилось");
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _guard = Kill(core);

    let http = reqwest::Client::new();
    let api = format!("http://127.0.0.1:{API}");
    for _ in 0..40 {
        if http
            .get(format!("{api}/version"))
            .bearer_auth(SECRET)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Из чего выбирать — спрашиваем у самой группы: имена узлов провайдера знает ядро.
    let body: serde_json::Value = http
        .get(format!("{api}/proxies/probe"))
        .bearer_auth(SECRET)
        .send()
        .await
        .expect("ядро не ответило про группу probe")
        .json()
        .await
        .unwrap();
    let all: Vec<String> = body["all"]
        .as_array()
        .expect("у probe нет списка узлов")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    println!("2/3: в probe {} узлов", all.len());
    assert!(!all.is_empty());

    // Меряем несколько узлов подряд: мёртвый сервер подписки — не повод валить спайк.
    let mut measured = 0;
    for name in all.iter().take(5) {
        let put = http
            .put(format!("{api}/proxies/probe"))
            .bearer_auth(SECRET)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await;
        if !put.is_ok_and(|r| r.status().is_success()) {
            println!("«{name}»: переключить не удалось");
            continue;
        }

        let probe = tokio::task::spawn_blocking(move || -> std::io::Result<(u128, u128)> {
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", LISTENER))?;
            socket.set_read_timeout(Some(Duration::from_secs(8)))?;
            socket.write_all(
                b"CONNECT cp.cloudflare.com:80 HTTP/1.1\r\nHost: cp.cloudflare.com:80\r\n\r\n",
            )?;
            let mut head = [0u8; 128];
            let read = socket.read(&mut head)?;
            let answer = String::from_utf8_lossy(&head[..read]).to_string();
            if !answer.contains(" 200 ") {
                return Err(std::io::Error::other(answer.trim().to_string()));
            }

            // Два запроса в одном туннеле. Второй и есть «чистое время ответа»: рукопожатия
            // в нём уже нет.
            //
            // Ответ дочитываем **до конца заголовков**, а не «сколько дали»: у 204 тела нет,
            // и пустая строка — это ровно один ответ. Одиночный `read` оставлял хвост первого
            // ответа в буфере, и второй замер показывал ноль — не скорость, а остатки.
            let mut once = || -> std::io::Result<u128> {
                let started = std::time::Instant::now();
                socket.write_all(
                    b"GET /generate_204 HTTP/1.1\r\nHost: cp.cloudflare.com\r\nConnection: keep-alive\r\n\r\n",
                )?;
                let mut answer: Vec<u8> = Vec::new();
                let mut buffer = [0u8; 256];
                while !answer.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer)?;
                    if read == 0 {
                        return Err(std::io::Error::other("туннель закрылся"));
                    }
                    answer.extend_from_slice(&buffer[..read]);
                }
                Ok(started.elapsed().as_millis())
            };
            Ok((once()?, once()?))
        })
        .await
        .unwrap();

        match probe {
            Ok((first, second)) => {
                println!("«{name}»: первый {first} мс, второй {second} мс");
                measured += 1;
            }
            Err(why) => println!("«{name}»: {why}"),
        }
    }
    println!("3/3: померено узлов {measured} из 5");
    assert!(
        measured > 0,
        "ни через один узел туннель не поднялся — способ не годится"
    );
}

/// Замер «через прокси, keep-alive» целиком по пути клиента (D-072).
///
/// Спайк S-016 проверял механизм на конфиге, собранном руками. Здесь то же самое, но
/// конфиг собирает сам клиент, вход поднимает его запуск, а меряет `state::measure` —
/// то есть проверяется ровно тот путь, которым это работает у пользователя.
#[tokio::test]
#[ignore]
async fn live_keepalive_ping_measures_through_our_own_tunnel() {
    use crate::nodes::ping::Method;

    sandbox("ping-keepalive");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    crate::app::client::ClientConfig::set_ping(Method::ProxyKeepalive).unwrap();

    // Порт служебного входа выбирает запуск — повторяем то же, что делает `connect::start`.
    let probe = crate::core::Ports::free_port().unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        Some(probe),
    )
    .unwrap();
    assert_eq!(
        effective.probe,
        Some(probe),
        "порт входа не доехал до сборки"
    );
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(
        ok,
        "ядро не приняло конфиг со служебным входом:
{log}"
    );

    if let Err(why) = state.mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.mihomo);
    assert_eq!(
        state.mihomo.probe_port(),
        Some(probe),
        "супервизор обязан помнить порт входа: без него мерить некуда"
    );

    let started = std::time::Instant::now();
    state
        .catalog
        .measure(&state)
        .await
        .expect("замер не прошёл");
    let spent = started.elapsed();

    let nodes = state.catalog.nodes();
    let tunnelled: Vec<_> = nodes
        .iter()
        .filter(|node| node.method == Some(Method::ProxyKeepalive) && !node.fallback)
        .collect();
    for node in nodes.iter().filter(|node| node.delay.is_some()).take(8) {
        println!(
            "{}: {} мс, {:?}, фолбэк {}",
            node.name,
            node.delay.unwrap(),
            node.method.unwrap(),
            node.fallback
        );
    }
    println!(
        "замер занял {spent:?}: своим туннелем померено {} из {} узлов",
        tunnelled.len(),
        nodes.len()
    );
    assert!(
        !tunnelled.is_empty(),
        "ни один узел не померился своим туннелем — служебный вход или группа не работают"
    );
}

// ---------------------------------------------------------------------------
// Состояние операционной системы: реестр
//
// Эти две проверки — единственные здесь, которые **нельзя** запесочить: реестр общий,
// `LOCALAPPDATA` его не подменяет. Поэтому каждая снимает сырое состояние ключа до работы
// и возвращает его через `Drop` — то есть и после провала, и после паники.
//
// Проверять их надо было давно: `System` — одна из трёх кнопок в шапке, автозапуск —
// тумблер в настройках, и оба меняют состояние машины кодом, который ни разу
// не наблюдался работающим.
// ---------------------------------------------------------------------------

/// Сырой снимок ключа: что лежало там до нас, дословно. Возвращается сам, что бы
/// ни случилось с проверкой, — иначе неудачный прогон оставит машину с чужим прокси.
struct RegistryGuard {
    subkey: &'static str,
    values: Vec<(&'static str, Option<RegistryValue>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegistryValue {
    Text(String),
    Number(u32),
}

impl RegistryGuard {
    fn text(subkey: &'static str, names: &[&'static str]) -> Self {
        let values = names
            .iter()
            .map(|name| {
                let read = crate::system::registry::Registry::read_string(subkey, name).unwrap();
                (*name, read.map(RegistryValue::Text))
            })
            .collect();
        Self { subkey, values }
    }

    fn with_number(mut self, name: &'static str) -> Self {
        let read = crate::system::registry::Registry::read_dword(self.subkey, name).unwrap();
        self.values.push((name, read.map(RegistryValue::Number)));
        self
    }
}

impl Drop for RegistryGuard {
    fn drop(&mut self) {
        for (name, value) in &self.values {
            let restored = match value {
                Some(RegistryValue::Text(text)) => Registry::write_string(self.subkey, name, text),
                Some(RegistryValue::Number(number)) => {
                    Registry::write_dword(self.subkey, name, *number)
                }
                None => Registry::delete_value(self.subkey, name),
            };
            restored.expect("сырое состояние реестра обязано вернуться");
        }
    }
}

fn proxy_key() -> &'static str {
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
}

fn proxy_value(name: &str) -> Option<String> {
    crate::system::registry::Registry::read_string(proxy_key(), name).unwrap()
}

fn proxy_enabled() -> u32 {
    crate::system::registry::Registry::read_dword(proxy_key(), "ProxyEnable")
        .unwrap()
        .unwrap_or(0)
}

/// `System`-режим целиком: запись доезжает до реестра, снимок возвращает **всё**, что мы
/// трогали, и чужую свежую настройку мы не затираем (три правила D-047).
#[test]
#[ignore]
fn live_system_proxy_reaches_the_registry_and_gives_it_back() {
    let _guard = RegistryGuard::text(proxy_key(), &["ProxyServer", "ProxyOverride"])
        .with_number("ProxyEnable");

    // Исходное состояние выдумываем сами: у пользователя может стоять что угодно, а нам
    // нужен известный «чужой» прокси со своим списком исключений — иначе возврат
    // не с чем сверять.
    let foreign = "127.0.0.1:2080";
    let foreign_bypass = "*.corp.example;<local>";
    crate::system::registry::Registry::write_string(proxy_key(), "ProxyServer", foreign).unwrap();
    crate::system::registry::Registry::write_string(proxy_key(), "ProxyOverride", foreign_bypass)
        .unwrap();
    crate::system::registry::Registry::write_dword(proxy_key(), "ProxyEnable", 1).unwrap();

    let ours = "127.0.0.1:3090";
    let backup = WinProxy::enable(ours).unwrap();

    assert_eq!(proxy_enabled(), 1, "ProxyEnable не встал");
    assert_eq!(
        proxy_value("ProxyServer").as_deref(),
        Some(ours),
        "в реестре не наш адрес — режим System не делает ничего"
    );
    assert!(
        proxy_value("ProxyOverride")
            .unwrap_or_default()
            .contains("127.*"),
        "петля не в исключениях: окно не достучится до external-controller"
    );
    assert!(WinProxy::is_ours(ours), "свою запись обязаны узнавать");

    WinProxy::restore(&backup).unwrap();

    assert_eq!(
        proxy_enabled(),
        1,
        "чужой прокси был включён — включён и остался"
    );
    assert_eq!(
        proxy_value("ProxyServer").as_deref(),
        Some(foreign),
        "чужой адрес не вернулся: «выключить наш» не значит «выключить любой»"
    );
    assert_eq!(
        proxy_value("ProxyOverride").as_deref(),
        Some(foreign_bypass),
        "список исключений пользователя затёрт нашим и не вернулся"
    );

    // Второе правило D-047: пока мы работали, прокси сменил кто-то третий.
    let backup = WinProxy::enable(ours).unwrap();
    let third_party = "127.0.0.1:9999";
    crate::system::registry::Registry::write_string(proxy_key(), "ProxyServer", third_party)
        .unwrap();
    WinProxy::restore(&backup).unwrap();
    assert_eq!(
        proxy_value("ProxyServer").as_deref(),
        Some(third_party),
        "затёрли свежую чужую настройку своей несвежей"
    );
}

/// Автозапуск: запись появляется и исчезает, путь в кавычках, повторное выключение
/// не ошибка. Состояние читается **из реестра** — значит убранная мимо окна запись
/// видна окном (D-049).
#[test]
#[ignore]
fn live_autostart_writes_and_removes_its_run_entry() {
    use crate::system::autostart;

    let run = r"Software\Microsoft\Windows\CurrentVersion\Run";
    // Имя записи — то, которым пишет эта сборка: у debug это `umiray-dev` (D-150). С голым
    // `"umiray"` проверка читала запись установленного клиента, а свою оставляла в `Run`.
    let _guard = RegistryGuard::text(run, &[crate::paths::APP_NAME]);
    // При своей задаче автозапуском заведует она, а не `Run` (D-087), и пишется он с правами.
    assert!(
        !crate::system::task::SchedulerTask::usable(),
        "у этой сборки заведена задача «{}»: автозапуском заведует она, а проверка — про Run",
        crate::system::task::NAME
    );

    Autostart::set(true).unwrap();
    let written = crate::system::registry::Registry::read_string(run, crate::paths::APP_NAME)
        .unwrap()
        .expect("записи автозапуска нет — тумблер не работает");
    assert!(
        written.starts_with('"') && written.ends_with(&format!("\" {}", autostart::AT_LOGON)),
        "путь без кавычек или без флага автозапуска (D-129) — {written}"
    );
    assert!(
        written.to_lowercase().contains(".exe"),
        "в автозапуск попал не бинарь: {written}"
    );
    assert!(Autostart::enabled(), "запись есть, а окно её не видит");

    Autostart::set(false).unwrap();
    assert!(
        crate::system::registry::Registry::read_string(run, crate::paths::APP_NAME)
            .unwrap()
            .is_none(),
        "запись осталась после выключения"
    );
    assert!(!Autostart::enabled());

    Autostart::set(false).expect("повторное выключение — не ошибка, значения и так нет");
}

/// Запрос мимо любого прокси — базовая линия «как машина ходит сама».
async fn direct_ip() -> String {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap()
        .get("https://api.ipify.org")
        .send()
        .await
        .expect("прямой запрос не прошёл")
        .text()
        .await
        .unwrap()
        .trim()
        .to_string()
}

/// То же, что `direct_ip`, но без паники: под kill switch выход обязан **не** получиться,
/// и это ожидаемый исход, а не провал проверки.
async fn try_direct_ip() -> std::result::Result<String, String> {
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?
        .get("https://api.ipify.org")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response
        .text()
        .await
        .map_err(|e| e.to_string())?
        .trim()
        .to_string())
}

/// Запрос обычного приложения: прокси **не задан явно**, клиент читает системный
/// из реестра — ровно то, ради чего D-047 туда и пишет.
async fn ip_via_system_proxy() -> std::result::Result<String, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?
        .get("https://api.ipify.org")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response
        .text()
        .await
        .map_err(|e| e.to_string())?
        .trim()
        .to_string())
}

/// Главное обещание режима `System`: запись в реестр **правда уводит чужой трафик**
/// в наш прокси, а не просто ложится в реестр.
///
/// Проверять это отдельно от `live_system_proxy_reaches_the_registry_and_gives_it_back`
/// приходится потому, что там нет ядра: там доказано, что значения записаны и возвращены,
/// здесь — что приложение, ничего про нас не знающее, из-за них меняет выход.
#[tokio::test]
#[ignore]
async fn live_system_proxy_actually_redirects_a_foreign_client() {
    sandbox("sysproxy");
    Migration::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _running = Running(&mihomo);
    let port = mihomo
        .status()
        .port
        .expect("в local-режиме порт обязан быть");

    // Проверка про системный прокси, а не про AUTO: сразу после старта его проверка
    // живости ещё не прошла, и хэш мог увести в мёртвый узел подписки.
    alive_exit(&mihomo, port).await;

    let _guard = RegistryGuard::text(proxy_key(), &["ProxyServer", "ProxyOverride"])
        .with_number("ProxyEnable");

    let address = format!("127.0.0.1:{port}");
    let backup = WinProxy::enable(&address).unwrap();
    println!("системный прокси включён на {address}");

    let through = ip_via_system_proxy()
        .await
        .expect("клиент, читающий системный прокси, не смог выйти наружу");
    println!("адрес приложения, ничего про нас не знающего: {through}");

    // Возврат делаем до утверждений: провалившаяся проверка не должна оставлять
    // машину с чужим прокси даже на время печати сообщения.
    WinProxy::restore(&backup).unwrap();

    assert_ne!(
        through, home,
        "запись в реестр есть, а трафик как шёл домой, так и идёт — режим System не работает"
    );

    let after = ip_via_system_proxy()
        .await
        .expect("после возврата наружу выйти не удалось");
    assert_eq!(
        after, home,
        "выключили наш прокси, а трафик всё ещё идёт через него"
    );
}

/// Kill switch целиком (D-073): защита правда запирает выход, ядро из-под неё выведено,
/// а возврат отпирает обратно.
///
/// Проверка **по-настоящему** запирает сеть этой машины на несколько секунд, поэтому:
/// возврат стоит в `Drop` и срабатывает даже после паники, а утверждения идут **после**
/// него — упавшее утверждение не должно оставить машину без интернета.
///
/// Требует прав администратора (правила брандмауэра) и поднятого TUN.
#[tokio::test]
#[ignore]
async fn live_kill_switch_locks_the_way_out_and_gives_it_back() {
    use crate::system::killswitch;

    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        "нужны права администратора: правила брандмауэра иначе не поставить"
    );

    sandbox("killswitch");
    Migration::run().unwrap();
    Mode::write(Mode::Tun).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _running = Running(&mihomo);
    assert_eq!(mihomo.status().mode, Some(Mode::Tun));
    mihomo
        .select(crate::config::direction::AUTO)
        .await
        .expect("автогруппа не собралась");

    let device = crate::config::mode::Mode::tun_device(
        &Yaml::top_mapping(&Documents::read(files::ADVANCED).unwrap()).unwrap(),
    );
    println!("адаптер ядра: {device}");

    // Возврат обязан случиться, что бы дальше ни произошло.
    struct Unlock(killswitch::Backup);
    impl Drop for Unlock {
        fn drop(&mut self) {
            Firewall::release(&self.0).expect("сеть обязана вернуться");
        }
    }

    let через_туннель = {
        let guard = Unlock(Firewall::engage(&Paths::core(), &device).unwrap());
        // Прогон, убитый снаружи (Ctrl+C), до `Drop` не доходит и оставит машину запертой.
        // Печатаем лекарство **по снятому снимку**, а не общими словами: искать его
        // в панике и без интернета будет неоткуда.
        println!(
            "защита поставлена. Если прогон прервать, сеть вернуть так:
{}
             Get-NetFirewallRule -DisplayName 'umiray killswitch*' | Remove-NetFirewallRule",
            killswitch::restore_script(&guard.0)
        );
        // При живом ядре запрет ничего не ломает: трафик идёт через его адаптер.
        let ip = try_direct_ip().await;
        drop(guard);
        ip
    };
    println!("под защитой при живом ядре: {через_туннель:?}");

    // А теперь то, ради чего всё: ядро умирает, адаптер исчезает — и выхода быть не должно.
    let backup = Firewall::engage(&Paths::core(), &device).unwrap();
    let guard = Unlock(backup);
    mihomo.stop();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after_death = try_direct_ip().await;
    drop(guard);
    println!("после смерти ядра под защитой: {after_death:?}");

    let restored = direct_ip().await;

    assert!(
        через_туннель.is_ok(),
        "защита отняла сеть у живого туннеля: {через_туннель:?}"
    );
    assert_ne!(
        через_туннель.as_deref().unwrap_or_default(),
        home,
        "под защитой трафик шёл мимо туннеля"
    );
    assert!(
        after_death.is_err(),
        "ядро умерло, а выход наружу остался — kill switch не работает: {after_death:?}"
    );
    assert_eq!(restored, home, "после снятия защиты сеть не вернулась");
}

/// Самолечение kill switch (D-073): клиент умер, не прибравшись, — следующий запуск
/// возвращает машине сеть, ничего не нажимая.
///
/// Ради этой проверки вариант A вообще был принят: без неё обещание «починить значит
/// открыть umiray ещё раз» держится на чтении кода, а цена ошибки — машина без интернета.
///
/// Смерть клиента изображаем честно: ядро гасим, а `release_kill_switch` **не зовём** —
/// с точки зрения брандмауэра это ровно то же, что `taskkill /F` по клиенту. Дальше
/// заводим **новое** состояние приложения, то есть перечитываем `settings.json` с диска,
/// как это делает `setup` при запуске.
///
/// Требует прав администратора и на несколько секунд запирает сеть этой машины.
#[tokio::test]
#[ignore]
async fn live_kill_switch_heals_itself_after_the_client_dies() {
    use crate::app::state::AppState;
    use crate::system::killswitch;

    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        "нужны права администратора: правила брандмауэра иначе не поставить"
    );

    sandbox("killswitch-heal");
    Migration::run().unwrap();
    Mode::write(Mode::Tun).unwrap();

    let home = direct_ip().await;

    // Первая жизнь клиента: тумблер включён, TUN поднят, защита встала.
    let doomed = AppState::new();
    doomed
        .settings
        .patch(crate::app::settings::Patch {
            kill_switch: Some(true),
            ..Default::default()
        })
        .unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = doomed.mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            doomed.mihomo.log().lines().join("\n")
        );
    }
    assert_eq!(doomed.mihomo.status().mode, Some(Mode::Tun));
    doomed
        .kill_switch
        .follow(&doomed, &doomed.mihomo)
        .expect("защита должна встать");

    // Страховка на случай, если проверка развалится посередине: сеть обязана вернуться.
    struct Unlock(killswitch::Backup);
    impl Drop for Unlock {
        fn drop(&mut self) {
            let _ = Firewall::release(&self.0);
        }
    }
    let snapshot = doomed
        .settings
        .get()
        .kill_switch_backup
        .expect("защита встала, но снимок на диск не лёг — лечить будет нечем");
    let _safety = Unlock(snapshot);
    println!("защита встала, снимок лежит в settings.json");

    // Клиент умирает, не прибравшись.
    doomed.mihomo.stop();
    drop(doomed);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let while_dead = try_direct_ip().await;
    println!("клиент мёртв, ядра нет: {while_dead:?}");

    // Следующий запуск: то же, что делает `setup` в `main.rs`.
    let reborn = AppState::new();
    let carried = reborn.settings.get().kill_switch_backup.is_some();
    if carried {
        reborn
            .kill_switch
            .release(&reborn)
            .expect("защита должна сняться");
    }
    let after_restart = try_direct_ip().await;
    println!("после нового запуска: {after_restart:?}");

    assert!(
        while_dead.is_err(),
        "клиент умер, а выход наружу остался: защиты не было вовсе"
    );
    assert!(
        carried,
        "снимок не пережил смерть клиента — следующий запуск не узнает, что запирал он"
    );
    assert_eq!(
        after_restart.as_deref().ok(),
        Some(home.as_str()),
        "запуск клиента сеть не вернул: обещание «починить = открыть umiray» не выполняется"
    );
    assert!(
        reborn.settings.get().kill_switch_backup.is_none(),
        "снимок остался на диске — следующий запуск будет лечить уже здоровое"
    );
}

/// Правило на **конкретный узел** живьём (D-082).
///
/// Ядро такой конфиг принимает — это проверено им самим; здесь про другое: что трафик
/// правда уходит через названный узел, а не через соседний по группе. Доказательство —
/// внешний адрес: правило ведёт `ipify` на узел, а всё остальное — напрямую, и адрес
/// обязан совпасть с адресом этого узла и отличаться от домашнего.
#[tokio::test]
#[ignore]
async fn live_a_rule_sends_traffic_through_the_named_node() {
    sandbox("rule-node");
    Migration::run().unwrap();
    // Правило проверяется в local: прав тут не нужно, а маршрут от режима не зависит.
    Mode::write(Mode::Local).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    // Сначала узнаём, какой адрес даёт узел сам по себе: сравнивать правило не с чем,
    // пока неизвестно, куда узел выходит.
    let (name, expected) = {
        let mihomo = Mihomo::new();
        let effective = crate::render::effective::ConfigRenderer::effective(
            &crate::render::plan::Route::default(),
            None,
        )
        .unwrap();
        mihomo.start(&effective).await.expect("ядро не встало");
        let _guard = Running(&mihomo);
        let port = mihomo.status().port.unwrap();
        let mut found = None;
        for node in crate::nodes::source_catalog::SourceCatalog::nodes()
            .iter()
            .take(6)
        {
            if mihomo.select(&node.name).await.is_err() {
                continue;
            }
            match try_external_ip(Some(port)).await {
                Ok(ip) if ip != home => {
                    println!("узел «{}» выходит через {ip}", node.name);
                    found = Some((node.name.clone(), ip));
                    break;
                }
                Ok(ip) => println!("узел «{}» дал домашний адрес {ip} — не годится", node.name),
                Err(why) => println!("узел «{}» не ответил ({why})", node.name),
            }
        }
        found.expect("ни один узел не ответил — правило проверять не на чем")
    };

    let state = crate::app::state::AppState::new();
    let preset = state.presets.create().unwrap();
    Documents::write(
        &format!("rules/{}", preset.id),
        &format!(
            "rules:
  - DOMAIN-SUFFIX,ipify.org,{name}
  - MATCH,DIRECT
"
        ),
    )
    .unwrap();
    state.presets.choose(&state, &preset.id).unwrap();

    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло правило на узел:\n{log}");

    let mihomo = Mihomo::new();
    mihomo
        .start(&effective)
        .await
        .expect("ядро не встало с правилом на узел");
    let _guard = Running(&mihomo);
    let port = mihomo.status().port.unwrap();

    let through_rule = external_ip(Some(port)).await;
    println!("по правилу на «{name}» внешний адрес: {through_rule}");
    assert_ne!(
        through_rule, home,
        "правило не сработало: трафик ушёл мимо узла, напрямую"
    );
    assert_eq!(
        through_rule, expected,
        "трафик ушёл не через названный узел, а через кого-то ещё"
    );
}

/// Форма «Ядра» против живого ядра (D-086).
///
/// Все одиннадцать полей читаются и пишутся обычными тестами, но ядро с ними не
/// запускалось: разбор `tun.stack`, `mixed-port` и `dns.nameserver` до сих пор проверял
/// только наш собственный сериализатор.
#[tokio::test]
#[ignore]
async fn live_the_core_starts_on_what_the_form_wrote() {
    use crate::config::advanced::{Enhanced, LogLevel, Stack};

    sandbox("form");
    Migration::run().unwrap();
    // В local: форма пишет и поля TUN, но поднимать туннель ради разбора конфига незачем.
    Mode::write(Mode::Local).unwrap();

    let mut options = Advanced::read().unwrap();
    // Порт берём свободный, а не круглый: занятый номер уронил бы запуск по чужой причине.
    let port = crate::core::Ports::free_port().unwrap();
    options.mixed_port = port;
    options.stack = Stack::Gvisor;
    options.log_level = LogLevel::Warning;
    options.enhanced_mode = Enhanced::RedirHost;
    options.nameserver = vec!["1.1.1.1".into(), "tls://8.8.8.8:853".into()];
    options.dns_hijack = vec!["any:53".into()];
    options.mtu = 1400;
    options.device = "umiray-live".into();
    options.strict_route = true;
    options.sniffer = true;
    options.dns_enable = true;
    Advanced::write(&options).unwrap();

    let back = Advanced::read().unwrap();
    assert_eq!(back, options, "форма прочитала не то, что записала");

    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло то, что написала форма:\n{log}");

    let mihomo = Mihomo::new();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось на конфиге формы: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);
    assert_eq!(
        mihomo.status().port,
        Some(port),
        "ядро слушает не тот порт, который написала форма"
    );
    println!("ядро поднялось на порту формы {port}, стек gvisor, свои серверы имён");
}

/// Источник, в котором **нет ни одной** ссылки, понятной конвертеру ядра (D-063).
///
/// Провайдера такому источнику заводить нельзя: его конвертер отвергает файл целиком
/// (`format invalid`), и ядро не поднимается вовсе. Узлы обязаны доехать записями
/// `proxies:`. Настоящих таких подписок нет, поэтому источник заводим сами.
#[tokio::test]
#[ignore]
async fn live_a_source_the_converter_cannot_read_still_reaches_the_core() {
    sandbox("unreadable");
    Migration::run().unwrap();
    // В local: проверка про сборку источника, а не про режим перехвата.
    Mode::write(Mode::Local).unwrap();

    // Ключи — настоящие 32 байта в base64: ядро их разбирает, а до туннеля дело
    // не дойдёт — проверка про сборку и запуск, а не про чужой сервер.
    let key = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU%3D";
    let name = "живой-шов";
    let source = crate::nodes::source_import::SourceImporter::add_link(&format!(
        "wireguard://{key}@1.2.3.4:51820?address=10.0.0.2/32&publickey={key}#{name}"
    ))
    .unwrap();
    assert_eq!(source.nodes, 1, "ссылка не завела узел");

    // С D-122 «непонятная конвертеру ядра схема» перестала быть особым случаем: такую
    // ссылку разбирает клиент, и узел приезжает обычной записью.
    assert!(
        crate::nodes::sources::SourceStore::content(&source.id).contains("type: wireguard"),
        "узел лёг записью, а не ссылкой"
    );

    // С D-122 такой источник — обычный провайдер с документом `proxies:`: ядро читает
    // запись, а не ссылку, и его конвертер в деле уже не участвует.
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    assert!(
        effective.yaml.contains(&source.id),
        "источник не доехал до сборки провайдером"
    );
    assert!(
        crate::nodes::sources::SourceStore::content(&source.id).contains(name),
        "узел не лёг в файл провайдера"
    );

    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло конфиг с узлом из шва:\n{log}");

    let mihomo = Mihomo::new();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);
    assert!(mihomo.status().running, "ядро не работает");
    println!("ядро поднялось с источником, который его конвертер не читает");
}

/// Внешний адрес **по UDP**: `whoami.cloudflare TXT CH` у 1.1.1.1 отвечает адресом, с которого
/// пришёл запрос. Через SOCKS5 UDP ASSOCIATE входа `mixed` — так UDP идёт через узел,
/// а не мимо. Отдаёт ответ целиком: адрес в нём строкой TXT.
async fn udp_exit(port: u16) -> std::result::Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fail = |e: std::io::Error| e.to_string();
    let mut control = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(fail)?;
    control.write_all(&[5, 1, 0]).await.map_err(fail)?;
    let mut hello = [0u8; 2];
    control.read_exact(&mut hello).await.map_err(fail)?;
    control
        .write_all(&[5, 3, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(fail)?;
    let mut reply = [0u8; 10];
    control.read_exact(&mut reply).await.map_err(fail)?;
    if reply[1] != 0 || reply[3] != 1 {
        return Err(format!("UDP ASSOCIATE отвергнут: {reply:?}"));
    }
    let relay = u16::from_be_bytes([reply[8], reply[9]]);

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .map_err(fail)?;
    // Заголовок SOCKS5 UDP до 1.1.1.1:53, затем запрос DNS: id, флаги, один вопрос.
    let mut packet = vec![0, 0, 0, 1, 1, 1, 1, 1, 0, 53];
    packet.extend_from_slice(&[0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in ["whoami", "cloudflare"] {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.extend_from_slice(&[0, 0, 16, 0, 3]);
    socket
        .send_to(&packet, ("127.0.0.1", relay))
        .await
        .map_err(fail)?;
    let mut buf = [0u8; 1500];
    let got = tokio::time::timeout(Duration::from_secs(8), socket.recv(&mut buf))
        .await
        .map_err(|_| "ответа по UDP нет".to_string())?
        .map_err(fail)?;
    Ok(String::from_utf8_lossy(&buf[..got]).into_owned())
}

/// Гасит ядро стенда, что бы ни случилось с проверкой.
struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Стенд протоколов (S-029): каждая ссылка идёт в ядро **дважды** — нашей записью
/// и разбором самого mihomo, — и оба выхода обязаны прийти наружу с адреса сервера.
///
/// Это и есть критерий схемы из TODO: запись, которую ядро примет молча и поведёт
/// не туда, видна только так. Строка стенда с `{` — готовая запись (flow-YAML):
/// у протокола нет ссылки, узел собирает форма, и сверять не с чем.
///
/// Строка `file:<путь>` — конфиг файлом (`.ovpn`, `.conf`), тем же разбором, что «Из файла».
/// Поле `stand-exit: any` в записи — выход не с адреса сервера (WARP выходит адресом
/// Cloudflare): тогда достаточно, что он не домашний.
///
/// Стенд — файл вне репозитория: в нём секреты серверов. `UMIRAY_STAND_CORE` — путь к другому
/// бинарю ядра, `UMIRAY_STAND_LOG` — уровень его лога.
///
/// ```text
/// set UMIRAY_STAND=<файл ссылок>
/// cargo test live_stand -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore]
async fn live_stand_links_leave_where_the_core_would() {
    use crate::nodes::convert::Converter;
    use crate::nodes::link::LinkParser;
    use serde_yaml::{Mapping, Value};

    let Ok(file) = std::env::var("UMIRAY_STAND") else {
        panic!("UMIRAY_STAND не задан: стенд — файл ссылок вне репозитория");
    };
    let lines: Vec<String> = std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect();
    // Тот же шаг, что у источника: ссылка mieru с несколькими портами — несколько узлов.
    let lines = LinkParser::split(&lines);

    let dir = std::env::temp_dir().join("umiray-live-stand");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut proxies = Vec::new();
    let mut links = Vec::new();
    let mut groups = Vec::new();
    let mut listeners = Vec::new();
    // (строка, адрес сервера — пусто, если выход где угодно, обещан ли UDP, порт нашего
    // входа, порт входа ядра)
    let mut rows = Vec::new();
    let listener = |name: String, proxy: String| {
        // Свободный и для UDP: Windows держит зарезервированные диапазоны, и вход `mixed`
        // на таком порту молча остаётся без UDP (`bind: forbidden` в логе ядра).
        let port = std::iter::repeat_with(|| crate::core::Ports::free_port().unwrap())
            .find(|port| std::net::UdpSocket::bind(("127.0.0.1", *port)).is_ok())
            .unwrap();
        let mut map = Mapping::new();
        Yaml::set(&mut map, "name", Value::from(name));
        Yaml::set(&mut map, "type", Value::from("mixed"));
        Yaml::set(&mut map, "listen", Value::from("127.0.0.1"));
        Yaml::set(&mut map, "port", Value::from(port));
        Yaml::set(&mut map, "proxy", Value::from(proxy));
        (port, Value::Mapping(map))
    };
    for (at, line) in lines.iter().enumerate() {
        let ours = if let Some(rest) = line.strip_prefix("file:") {
            // `file:<путь> {dialer-proxy: ours-0}` — поля поверх разобранного: OpenVPN
            // с этой сети режет DPI, и проверить разбор можно только цепочкой (S-029).
            let (path, extra) = rest.split_once(" {").map_or((rest, None), |(path, extra)| {
                (
                    path,
                    serde_yaml::from_str::<Mapping>(&format!("{{{extra}")).ok(),
                )
            });
            let text = std::fs::read_to_string(path.trim()).unwrap();
            let mut entry = crate::nodes::source_import::SourceImporter::file_proxy(&text, "file")
                .unwrap_or_else(|e| panic!("файл стенда не разобран: {e}"));
            entry.extend(extra.unwrap_or_default());
            Some(entry)
        } else if line.starts_with('{') {
            serde_yaml::from_str::<Mapping>(line).ok()
        } else {
            Converter::to_entry(line)
        };
        let Some(mut ours) = ours else {
            panic!("наш разбор не осилил строку стенда: {line}");
        };
        let server = ours
            .get(Value::from("server"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let anywhere = ours.remove(Value::from("stand-exit")).is_some();
        // UDP проверяем там, где на него рассчитывает клиент: запись обещает `udp: true`
        // (забытый TCP не заметит) или узел идёт в UDP-группу.
        let udp = ours.get(Value::from("udp")).and_then(Value::as_bool) == Some(true)
            || ours
                .get(Value::from("type"))
                .and_then(Value::as_str)
                .is_some_and(crate::render::effective::datagram);
        let name = format!("ours-{at}");
        Yaml::set(&mut ours, "name", Value::from(name.clone()));
        proxies.push(Value::Mapping(ours));
        let (mine, entry) = listener(format!("l-{name}"), name);
        listeners.push(entry);

        // Схемы, которые читает конвертер самого ядра (`common/convert/converter.go`).
        // Чужую ему не даём: провайдер без узлов оставил бы группу пустой, а пустая
        // группа молча выходит напрямую — и «сверка» сравнила бы с домашним адресом.
        const CORE_READS: [&str; 16] = [
            "hysteria",
            "hysteria2",
            "hy2",
            "tuic",
            "trojan",
            "vless",
            "vmess",
            "ss",
            "ssr",
            "socks",
            "socks5",
            "socks5h",
            "http",
            "https",
            "anytls",
            "mierus",
        ];
        let scheme = line.split("://").next().unwrap_or_default().to_lowercase();
        let mut core = None;
        if !line.starts_with('{') && CORE_READS.contains(&scheme.as_str()) {
            // Имя ядро берёт из фрагмента, у mieru дописывает `:порт/транспорт`.
            let name = format!("core-{at}");
            links.push(LinkParser::set_name(line, &name));
            let mut group = Mapping::new();
            Yaml::set(&mut group, "name", Value::from(format!("g-{name}")));
            Yaml::set(&mut group, "type", Value::from("select"));
            Yaml::set(
                &mut group,
                "use",
                Value::Sequence(vec![Value::from("core")]),
            );
            Yaml::set(&mut group, "filter", Value::from(format!("^{name}(:|$)")));
            groups.push(Value::Mapping(group));
            let (port, entry) = listener(format!("l-{name}"), format!("g-{name}"));
            listeners.push(entry);
            core = Some(port);
        }
        let server = if anywhere { String::new() } else { server };
        rows.push((line.clone(), server, udp, mine, core));
    }

    let mut config = Mapping::new();
    let level = std::env::var("UMIRAY_STAND_LOG").unwrap_or_else(|_| "warning".into());
    Yaml::set(&mut config, "log-level", Value::from(level));
    Yaml::set(&mut config, "proxies", Value::Sequence(proxies));
    if !links.is_empty() {
        std::fs::write(dir.join("links.txt"), links.join("\n")).unwrap();
        let mut provider = Mapping::new();
        Yaml::set(&mut provider, "type", Value::from("file"));
        Yaml::set(&mut provider, "path", Value::from("./links.txt"));
        // Сертификат стенда самоподписанный: наши ссылки несут `allow_insecure=1`, как
        // у панелей, а разбор ядра половину таких флагов не читает — и сверка сравнивала бы
        // отказ с отказом. Доверие к сертификату пусть проверяют обычные тесты записи.
        // (`tls.custom-certifactes` не помогает: пул узел берёт при разборе, раньше него.)
        let mut trust = Mapping::new();
        Yaml::set(&mut trust, "skip-cert-verify", Value::from(true));
        Yaml::set(&mut provider, "override", Value::Mapping(trust));
        let mut providers = Mapping::new();
        Yaml::set(&mut providers, "core", Value::Mapping(provider));
        Yaml::set(&mut config, "proxy-providers", Value::Mapping(providers));
        Yaml::set(&mut config, "proxy-groups", Value::Sequence(groups));
    }
    Yaml::set(&mut config, "listeners", Value::Sequence(listeners));
    let path = dir.join("config.yaml");
    std::fs::write(
        &path,
        serde_yaml::to_string(&Value::Mapping(config)).unwrap(),
    )
    .unwrap();

    let log = std::fs::File::create(dir.join("core.log")).unwrap();
    // Другое ядро — ответ на «встанет ли после обновления» (hysteria v1, openvpn).
    let binary = std::env::var("UMIRAY_STAND_CORE").map_or_else(|_| Paths::core(), PathBuf::from);
    let _core = Child(
        std::process::Command::new(binary)
            .arg("-d")
            .arg(&dir)
            .arg("-f")
            .arg(&path)
            .env("SAFE_PATHS", &dir)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let last = rows.last().map(|row| row.4.unwrap_or(row.3)).unwrap();
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", last)).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let home = try_direct_ip().await.unwrap_or_default();
    let mut failed = Vec::new();
    for (line, server, udp, mine, core) in &rows {
        // Сетевым узлам (tailscale, zerotier) первый вход в сеть честно занимает секунды:
        // первый запрос у них падает и у живого узла — так пишет и документация ядра.
        let mut ours = try_external_ip(Some(*mine)).await;
        for _ in 0..2 {
            if ours.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            ours = try_external_ip(Some(*mine)).await;
        }
        let theirs = match core {
            Some(port) => Some(try_external_ip(Some(*port)).await),
            None => None,
        };
        let datagram = match udp {
            true => Some(
                udp_exit(*mine)
                    .await
                    .map(|text| text.contains(server.as_str())),
            ),
            false => None,
        };
        let kind = line.split_once("://").map_or_else(
            || line.chars().take(24).collect(),
            |(kind, _)| kind.to_string(),
        );
        println!("{kind:<12} {server:<16} наш: {ours:?}  UDP: {datagram:?}  ядро: {theirs:?}");
        let expected = server.parse::<std::net::IpAddr>().is_ok();
        // Разбор ядра — для сравнения, не условие: там, где он не встаёт (hysteria с
        // обфускацией, tuic без `allow_insecure`), наш обязан встать.
        let good = match &ours {
            Ok(ip) => (!expected || ip == server) && *ip != home,
            Err(_) => false,
        } && datagram.as_ref().is_none_or(|udp| matches!(udp, Ok(true)));
        if !good {
            failed.push(line.clone());
        }
    }
    let log = std::fs::read_to_string(dir.join("core.log")).unwrap_or_default();
    assert!(
        failed.is_empty(),
        "не вышли наружу через сервер: {failed:#?}\n{log}"
    );
}

/// Снимок задачи планировщика, дословный. Возвращается сам, что бы ни случилось
/// с проверкой: задача — это то, чем система поднимает клиента, и оставить её чужой
/// или потерять вовсе значит сломать пользователю запуск.
///
/// Хранится сам XML, а не «была/не была»: пересоздать задачу нашим кодом мало —
/// он подставит **свой** путь, а у настоящей задачи он мог быть любым.
struct TaskGuard {
    xml: Option<String>,
}

impl TaskGuard {
    const NAME: &'static str = crate::system::task::NAME;

    fn snapshot() -> Self {
        let out = std::process::Command::new("schtasks.exe")
            .args(["/Query", "/TN", Self::NAME, "/XML"])
            .output()
            .expect("планировщик не отозвался");
        let xml = out.status.success().then(|| decode(&out.stdout));
        Self { xml }
    }
}

/// Вывод `schtasks` бывает и UTF-16, и однобайтным — смотрим на метку порядка байт.
fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).to_string()
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("schtasks.exe")
            .args(["/Delete", "/TN", Self::NAME, "/F"])
            .output();
        if let Some(xml) = &self.xml {
            // Планировщик читает только UTF-16 с меткой — тот же формат, которым пишем
            // задачу сами (`system::task`).
            let mut bytes = vec![0xFF, 0xFE];
            for unit in xml.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            let path = std::env::temp_dir().join("umiray-task-restore.xml");
            std::fs::write(&path, &bytes).unwrap();
            let out = std::process::Command::new("schtasks.exe")
                .args([
                    "/Create",
                    "/TN",
                    Self::NAME,
                    "/XML",
                    &path.to_string_lossy(),
                    "/F",
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "задача пользователя не вернулась: {}",
                decode(&out.stderr)
            );
            let _ = std::fs::remove_file(&path);
        }
        crate::system::task::forget();
    }
}

/// Задача, чей файл исчез, больше не забирает запуск себе (B-015).
///
/// Проверка настоящая, а не на строках: задача заводится в планировщике, потом её
/// описание переписывается на несуществующий путь — ровно то состояние, в котором клиент
/// молча не поднимался. `schtasks /Run` на такой задаче докладывает об успехе, поэтому
/// единственный способ не уйти в никуда — прочитать её `<Command>` заранее.
///
/// **Нужны права администратора:** завести задачу с `HighestAvailable` без них нельзя.
#[test]
#[ignore]
fn live_a_stale_task_does_not_take_the_launch() {
    use crate::system::task;

    sandbox("stale");
    let _registry = RegistryGuard::text(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        &[crate::paths::APP_NAME],
    );
    let _task = TaskGuard::snapshot();

    Autostart::set_always_admin(true)
        .expect("нужны права администратора — запускать эту проверку из поднятой консоли");
    assert!(
        SchedulerTask::usable(),
        "живая задача должна считаться рабочей"
    );
    assert!(Autostart::always_admin(), "окно не видит заведённую задачу");

    // Тот самый случай: каталог со сборкой исчез, задача осталась. Портим **свою** задачу —
    // ту, что сторожит `TaskGuard`: с голым `"umiray"` под `cargo test` перезаписывалась
    // задача установленного клиента, и вернуть её было некому.
    let gone = std::env::temp_dir()
        .join("umiray-которого-нет")
        .join("umiray.exe");
    let xml = crate::system::task::document(&gone, false);
    let path = std::env::temp_dir().join("umiray-stale-task.xml");
    std::fs::write(&path, crate::system::task::utf16(&xml)).unwrap();
    let out = std::process::Command::new("schtasks.exe")
        .args([
            "/Create",
            "/TN",
            crate::paths::APP_NAME,
            "/XML",
            &path.to_string_lossy(),
            "/F",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "задача не переписалась: {}",
        decode(&out.stderr)
    );
    let _ = std::fs::remove_file(&path);
    task::forget();

    assert!(SchedulerTask::exists(), "задача в планировщике осталась");
    assert!(
        !SchedulerTask::usable(),
        "задача с исчезнувшим файлом считается рабочей — клиент снова уйдёт в никуда"
    );
    assert!(
        !Autostart::always_admin(),
        "окно показывает «всегда от администратора» по задаче, которая ничего не поднимает"
    );

    // И «схема» лечится тем же тумблером: включили — задача перезаведена на живой путь.
    Autostart::set_always_admin(true).unwrap();
    assert!(
        SchedulerTask::usable(),
        "тумблер не починил протухшую задачу"
    );
    println!("протухшая задача опознана и перезаведена");
}

/// Обратный путь тумблера «всегда от администратора» (D-087).
///
/// Заведение задачи и подъём с правами проверены живьём; здесь — снятие: задача уходит,
/// а автозапуск не теряется по дороге, потому что за него снова отвечает реестр.
/// Чего эта проверка не видит — что следующий вход в систему правда поднимет клиента
/// по вернувшейся записи: для этого нужен вход в систему, а не тест.
///
/// **Нужны права администратора:** завести задачу с `HighestAvailable` без них нельзя.
#[test]
#[ignore]
fn live_always_admin_gives_the_launch_back_to_the_registry() {
    sandbox("admin");
    let run = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let _registry = RegistryGuard::text(run, &[crate::paths::APP_NAME]);
    let _task = TaskGuard::snapshot();

    // Исходное состояние: задачи нет, автозапуск включён обычным способом.
    SchedulerTask::remove()
        .expect("нужны права администратора — запускать эту проверку из поднятой консоли");
    Autostart::set(true).unwrap();
    assert!(Autostart::enabled(), "автозапуск не включился");

    Autostart::set_always_admin(true).unwrap();
    assert!(SchedulerTask::exists(), "задача не завелась");
    assert!(
        Autostart::enabled(),
        "автозапуск потерялся при переезде в задачу"
    );
    assert!(
        crate::system::registry::Registry::read_string(run, crate::paths::APP_NAME)
            .unwrap()
            .is_none(),
        "запись в Run осталась вместе с задачей — вход в систему поднял бы вторую копию, и без прав"
    );

    Autostart::set_always_admin(false).unwrap();
    assert!(
        !SchedulerTask::exists(),
        "задача осталась после снятия тумблера"
    );
    let written = crate::system::registry::Registry::read_string(run, crate::paths::APP_NAME)
        .unwrap()
        .expect("запись в Run не вернулась — автозапуск пропал вместе с задачей");
    assert!(
        written.to_lowercase().contains(".exe"),
        "в автозапуск вернулся не бинарь: {written}"
    );
    assert!(Autostart::enabled(), "окно не видит вернувшийся автозапуск");
    println!("задача ушла, автозапуск вернулся в реестр: {written}");

    // И то же самое при выключенном автозапуске: задача есть, но триггера у неё нет,
    // а после снятия тумблера в реестре не появляется ничего.
    Autostart::set(false).unwrap();
    Autostart::set_always_admin(true).unwrap();
    assert!(SchedulerTask::exists());
    assert!(
        !Autostart::enabled(),
        "выключенный автозапуск включился сам от смены способа"
    );
    Autostart::set_always_admin(false).unwrap();
    assert!(!SchedulerTask::exists());
    assert!(
        crate::system::registry::Registry::read_string(run, crate::paths::APP_NAME)
            .unwrap()
            .is_none(),
        "выключенный автозапуск вернулся записью в реестр"
    );
}

// ---------------------------------------------------------------------------
// S-020: что доезжает перезагрузкой конфига, а что требует подъёма заново.
//
// S-019 доказал, что `PUT /configs` вообще работает: PID не меняется, открытые соединения
// целы. Здесь — поштучно, ключ за ключом: применяется ли изменение живому ядру.
// ---------------------------------------------------------------------------

/// Конфиг замера: маленький, без провайдеров и без geosite — иначе стенд не встанет
/// за секунду, а мерить надо не сборку, а ядро.
fn probe_config(port: u16, tweak: &str) -> String {
    format!(
        "mixed-port: {port}
mode: rule
log-level: info
ipv6: false
dns:
  enable: true
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver: [1.1.1.1]
tun:
  enable: false
  stack: mixed
  device: umiray-probe
{tweak}
rules:
  - MATCH,DIRECT
"
    )
}

/// Ответило ли что-то на этом порту. Единственный способ узнать, доехал ли `mixed-port`
/// до самого слушателя, а не только до ответа `/configs`.
fn listens(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// Что даёт резолвер ядра на `example.com` — одной строкой для таблицы.
async fn asked(bench: &crate::diag::Bench) -> String {
    match bench.resolve("example.com").await {
        Ok(addresses) => addresses.join(", "),
        Err(why) => format!("не ответил ({why})"),
    }
}

/// Поштучно: какой ключ доезжает до живого ядра перезагрузкой конфига, а какой требует
/// подъёма заново (S-020). Печатает таблицу и **проверяет** выводы, которые из неё
/// записаны в `core::apply` — иначе следующая версия ядра сменит поведение молча.
#[tokio::test]
#[ignore]
async fn live_which_keys_a_reload_carries() {
    use crate::diag::Bench;

    sandbox("reload");
    let port = crate::core::Ports::free_port().unwrap();
    let second = crate::core::Ports::free_port().unwrap();
    let base = probe_config(port, "");
    let bench = Bench::with(&base).await.unwrap();
    let pid = bench.pid();
    assert!(listens(port), "стенд не слушает свой mixed-port");

    let put = async |config: &str| {
        std::fs::write(bench.config_path(), config).unwrap();
        let done = bench.controller().apply(&bench.config_path()).await;
        // Полсекунды на применение: слушатели и адаптеры поднимаются не в ответе на запрос.
        tokio::time::sleep(Duration::from_millis(500)).await;
        done
    };

    println!("\n| ключ | чем смотрели | что вышло |");
    println!("|---|---|---|");

    // --- порт локального прокси -------------------------------------------
    put(&probe_config(second, "")).await.unwrap();
    let json = bench.controller().config().await.unwrap();
    let moved = listens(second);
    let stayed = listens(port);
    println!(
        "| `mixed-port` | слушатель + `/configs` | `/configs` показывает {} (просили {second}), новый порт слушает {moved}, старый {stayed} |",
        json["mixed-port"]
    );
    assert_eq!(
        json["mixed-port"].as_u64(),
        Some(u64::from(port)),
        "ядро всё-таки приняло новый номер — таблицу в core::apply пора менять"
    );
    assert!(
        !moved && stayed,
        "порт переехал перезагрузкой — таблицу в core::apply пора менять"
    );
    put(&base).await.unwrap();

    // --- подробность лога ---------------------------------------------------
    put(&base.replace("log-level: info", "log-level: warning"))
        .await
        .unwrap();
    let level = bench.controller().config().await.unwrap()["log-level"].clone();
    println!("| `log-level` | `/configs` | {level} |");
    assert_eq!(level, "warning", "уровень лога не доехал перезагрузкой");
    put(&base).await.unwrap();

    // --- разбор SNI ---------------------------------------------------------
    // В `/configs` его нет вовсе, зато ядро объявляет о нём в собственном логе:
    // «Sniffer is loaded and working» приходит ровно на PUT.
    put(&probe_config(port, "sniffer:\n  enable: true\n"))
        .await
        .unwrap();
    println!("| `sniffer` | лог ядра | «Sniffer is loaded and working» на каждый PUT |");
    put(&base).await.unwrap();

    // --- имена --------------------------------------------------------------
    // `dns.*` в `/configs` не приходит вовсе, поэтому спрашиваем сам резолвер. Подмену
    // (`enhanced-mode`) так не увидеть: `/dns/query` отвечает честным адресом в любом
    // режиме — подставные раздаёт путь трафика, а не эта ручка. Меряем то, что видно:
    // кому ядро задаёт вопрос и задаёт ли вообще.
    let honest = asked(&bench).await;
    println!("| `dns` (база, `1.1.1.1`) | `/dns/query` | {honest} |");
    assert!(
        honest.contains('.'),
        "база обязана отвечать адресом, а ответила {honest}"
    );

    put(&base.replace("nameserver: [1.1.1.1]", "nameserver: [240.0.0.1]"))
        .await
        .unwrap();
    let blackhole = asked(&bench).await;
    println!("| `dns.nameserver` | `/dns/query` | {blackhole} |");
    assert!(
        blackhole.starts_with("не ответил"),
        "вопрос ушёл в чёрную дыру, а ответ пришёл: значит резолвер остался прежним ({blackhole})"
    );
    put(&base).await.unwrap();

    put(&base.replace("  enable: true", "  enable: false"))
        .await
        .unwrap();
    let off = asked(&bench).await;
    println!("| `dns.enable` | `/dns/query` | {off} |");
    assert!(
        off.starts_with("не ответил"),
        "резолвер выключен, а отвечает: {off}"
    );
    put(&base).await.unwrap();

    // --- маршрут ------------------------------------------------------------
    // Главный вопрос всей таблицы: тумблер набора и смена направления правят именно
    // `rules`, и если они доезжают перезагрузкой — соединения рвать больше незачем.
    // Смотрим не в `/configs` (правил там нет), а на сам трафик через прокси.
    let allowed = try_external_ip(Some(port)).await;
    println!("| `rules` (база, `MATCH,DIRECT`) | запрос через прокси | {allowed:?} |");
    assert!(allowed.is_ok(), "база не пускает трафик — мерить нечем");

    put(&base.replace("MATCH,DIRECT", "MATCH,REJECT"))
        .await
        .unwrap();
    let rejected = try_external_ip(Some(port)).await;
    println!("| `rules` | запрос через прокси | {rejected:?} |");
    assert!(
        rejected.is_err(),
        "правило `MATCH,REJECT` не доехало перезагрузкой: трафик всё ещё идёт"
    );
    put(&base).await.unwrap();
    assert!(
        try_external_ip(Some(port)).await.is_ok(),
        "возврат базы тоже обязан доезжать"
    );

    // --- туннель ------------------------------------------------------------
    put(&base.replace("  enable: false", "  enable: true"))
        .await
        .unwrap();
    let tun = bench.controller().config().await.unwrap()["tun"]["enable"].clone();
    let elevated = crate::system::elevation::Elevation::is_elevated();
    println!("| `tun.enable` | `/configs` | {tun} (права: {elevated}) |");
    if elevated {
        assert_eq!(
            tun.as_bool(),
            Some(true),
            "с правами туннель обязан подниматься перезагрузкой"
        );
    } else {
        assert_eq!(
            tun.as_bool(),
            Some(false),
            "без прав ядро пишет «Access is denied» и остаётся без туннеля"
        );
    }
    put(&base).await.unwrap();

    assert_eq!(
        pid,
        bench.pid(),
        "перезагрузка сменила процесс — это не перезагрузка"
    );
}

/// Тумблер встроенного набора при живом ядре — перезагрузкой, а не перезапуском (D-102).
///
/// Доказательство того, что ядро **то же самое**, — его собственные счётчики трафика:
/// они считаются с начала работы процесса, и перезапуск обнулил бы их. Что перезагрузка
/// при этом не рвёт открытые соединения, измерено отдельно (S-019).
#[tokio::test]
#[ignore]
async fn live_a_ruleset_toggle_reloads_instead_of_restarting() {
    sandbox("toggle");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let probe = crate::core::Ports::free_port().unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        Some(probe),
    )
    .unwrap();
    if let Err(why) = state.mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            state.mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&state.mihomo);
    let port = state.mihomo.status().port.unwrap();

    alive_exit(&state.mihomo, port).await;
    let before = state
        .mihomo
        .traffic()
        .await
        .unwrap()
        .expect("у работающего ядра счётчики обязаны читаться");
    assert!(before.up > 0, "трафик не пошёл — сравнивать будет нечего");

    let ruleset = crate::config::rulesets::RulesetStore::list()
        .into_iter()
        .next()
        .expect("встроенных наборов нет — тумблер проверять не на чем");
    let was = set_ready(&state, &ruleset.id, true);
    set_ready(&state, &ruleset.id, !was);

    let changed = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        Some(probe),
    )
    .unwrap();
    let launched = state.mihomo.launched().unwrap();
    assert_eq!(
        crate::core::mihomo::apply::Apply::needed(&launched, &changed.yaml).unwrap(),
        Some(crate::core::mihomo::apply::Apply::Reload),
        "правка набора вдруг требует перезапуска — таблица разошлась с реальностью"
    );
    state.mihomo.apply(&changed).await.unwrap();

    let after = state
        .mihomo
        .traffic()
        .await
        .unwrap()
        .expect("после перезагрузки ядро обязано остаться тем же");
    assert!(
        after.up >= before.up && after.down >= before.down,
        "счётчики обнулились — значит ядро перезапустилось, а не перечитало конфиг: \
         было {}/{}, стало {}/{}",
        before.up,
        before.down,
        after.up,
        after.down
    );
    println!(
        "тумблер набора применён на ходу: счётчики {}/{} → {}/{}",
        before.up, before.down, after.up, after.down
    );

    // И собранное теперь совпадает с работающим — окно не будет предлагать перезапуск.
    assert_eq!(
        crate::core::mihomo::apply::Apply::needed(&state.mihomo.launched().unwrap(), &changed.yaml)
            .unwrap(),
        None,
        "после применения ядро и файл обязаны сойтись"
    );
    // Перезагрузка вернула выход к выбору из настроек — он мог оказаться мёртвым узлом.
    alive_exit(&state.mihomo, port).await;

    set_ready(&state, &ruleset.id, was);
}

// ---------------------------------------------------------------------------
// S-021: чем гасить ядро мягко и переживает ли это карта подменных адресов.
//
// S-019 показал, что `store-fake-ip` работает только при мягкой остановке, а гасим мы
// жёстко — и кнопкой, и клеткой при выходе. Вопрос первый и главный: есть ли у нас
// вообще способ погасить mihomo мягко на Windows.
// ---------------------------------------------------------------------------

/// Конфиг с подменными адресами и своим слушателем DNS: у `/dns/query` подмены не видно
/// (S-020), её раздаёт именно слушатель.
fn fakeip_config(port: u16, dns: u16) -> String {
    format!(
        "mixed-port: {port}
mode: rule
log-level: warning
ipv6: false
dns:
  enable: true
  listen: 127.0.0.1:{dns}
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver: [1.1.1.1]
profile:
  store-fake-ip: true
rules:
  - MATCH,DIRECT
"
    )
}

/// Спросить имя у слушателя ядра — своим пакетом, как в `diag::wire`.
async fn fake_address(dns: u16, name: &str) -> String {
    let packet =
        crate::diag::wire::DnsWire::query(name, crate::diag::wire::TYPE_A, 0x4242).unwrap();
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&packet, format!("127.0.0.1:{dns}"))
        .await
        .unwrap();
    let mut buffer = vec![0u8; 512];
    let read = tokio::time::timeout(Duration::from_secs(3), socket.recv(&mut buffer))
        .await
        .expect("слушатель DNS ядра не ответил")
        .unwrap();
    crate::diag::wire::DnsWire::answer(&buffer[..read], 0x4242)
        .unwrap()
        .ips
        .first()
        .map(std::string::ToString::to_string)
        .unwrap_or_default()
}

/// Поднять ядро **ровно так, как это делает супервизор**: своя невидимая консоль плюс
/// своя группа процессов. Иначе проверялась бы не та мягкая остановка, что в клиенте.
async fn fakeip_core(dir: &Path, config: &str) -> std::process::Child {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    std::fs::write(dir.join("config.yaml"), config).unwrap();
    let child = std::process::Command::new(Paths::core())
        .arg("-d")
        .arg(dir)
        .arg("-f")
        .arg(dir.join("config.yaml"))
        .creation_flags(CREATE_NO_WINDOW | crate::system::console::NEW_PROCESS_GROUP)
        .spawn()
        .expect("ядро не запустилось");
    tokio::time::sleep(Duration::from_millis(900)).await;
    child
}

#[tokio::test]
#[ignore]
async fn live_a_soft_stop_keeps_the_fake_ip_map() {
    sandbox("fakeip");
    let port = crate::core::Ports::free_port().unwrap();
    let dns = crate::core::Ports::free_port().unwrap();
    let dir = Paths::run_dir().join("fakeip");
    std::fs::create_dir_all(&dir).unwrap();
    let config = fakeip_config(port, dns);
    let names = ["example.com", "github.com", "wikipedia.org"];

    let mut child = fakeip_core(&dir, &config).await;
    let mut before = Vec::new();
    for name in names {
        before.push(fake_address(dns, name).await);
    }
    println!("до остановки: {before:?}");
    assert!(
        before.iter().all(|address| address.starts_with("198.18.")),
        "слушатель не раздаёт подменные адреса — мерить нечего: {before:?}"
    );

    // --- мягко ---------------------------------------------------------------
    let sent = crate::system::console::Console::interrupt(child.id());
    let stopped = std::time::Instant::now();
    let quiet = tokio::task::spawn_blocking(move || {
        for _ in 0..50 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        false
    })
    .await
    .unwrap();
    println!(
        "Ctrl+Break отправлен: {sent}, ядро вышло само: {quiet} за {:?}",
        stopped.elapsed()
    );

    let mut child = fakeip_core(&dir, &config).await;
    // Спрашиваем в обратном порядке: пул раздаётся по порядку запросов, и совпадение
    // при том же порядке доказывало бы меньше, чем кажется (S-019).
    let mut after = Vec::new();
    for name in names.iter().rev() {
        after.push(fake_address(dns, name).await);
    }
    after.reverse();
    println!("после подъёма: {after:?}");
    let _ = child.kill();
    let _ = child.wait();

    assert!(sent, "Ctrl+Break не ушёл вовсе");
    assert!(quiet, "ядро не вышло само — значит сигнал оно не поняло");
    assert_eq!(
        before, after,
        "карта подменных адресов не пережила мягкую остановку"
    );

    // --- отрицательный контроль: жёстко ------------------------------------
    // Без него «карта цела» проходило бы и в мире, где мягкая остановка ни на что
    // не влияет. Каталог новый: старый уже хранит карту, и жёсткое убийство её
    // не испортит — потеряется только то, что не успело сохраниться.
    let rough = Paths::run_dir().join("fakeip-rough");
    std::fs::create_dir_all(&rough).unwrap();
    let mut child = fakeip_core(&rough, &config).await;
    let mut first = Vec::new();
    for name in names {
        first.push(fake_address(dns, name).await);
    }
    let _ = child.kill();
    let _ = child.wait();

    let mut child = fakeip_core(&rough, &config).await;
    let mut second = Vec::new();
    for name in names.iter().rev() {
        second.push(fake_address(dns, name).await);
    }
    second.reverse();
    let _ = child.kill();
    let _ = child.wait();
    println!("жёстко: было {first:?}, стало {second:?}");
    assert_ne!(
        first, second,
        "жёсткое убийство карту не испортило — тогда и мягкая остановка ни к чему"
    );
}

/// Что окно узнаёт о нужном перезапуске (D-102).
///
/// Правило проверено обычными тестами по двум строкам YAML; здесь — весь путь целиком:
/// настоящий конфиг, работающее ядро и `Status`, каким его увидит окно.
#[tokio::test]
#[ignore]
async fn live_the_window_is_told_which_change_needs_a_restart() {
    sandbox("restart-reason");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let probe = crate::core::Ports::free_port().unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        Some(probe),
    )
    .unwrap();
    state.mihomo.start(&effective).await.unwrap();
    let _guard = Running(&state.mihomo);
    let port = state.mihomo.status().port.unwrap();

    let quiet = crate::app::status::Status::gather(&state);
    assert_eq!(
        quiet.restart_reason(),
        None,
        "только что поднятому ядру перезапуск не нужен"
    );

    // Правка, которая доезжает: подробность лога.
    let mut options = crate::config::advanced::Advanced::read().unwrap();
    options.log_level = crate::config::advanced::LogLevel::Warning;
    crate::config::advanced::Advanced::write(&options).unwrap();
    assert_eq!(
        crate::app::status::Status::gather(&state).restart_reason(),
        None,
        "уровень лога перезапуска не требует — он перечитывается (S-020)"
    );

    // Правка, которая не доезжает: порт локального прокси.
    options.mixed_port = crate::core::Ports::free_port().unwrap();
    crate::config::advanced::Advanced::write(&options).unwrap();
    let asked = crate::app::status::Status::gather(&state);
    let why = asked
        .restart_reason()
        .expect("смена порта обязана попросить перезапуск");
    assert!(why.contains("Порт"), "причина не про порт: {why}");
    assert_eq!(
        state.mihomo.status().port,
        Some(port),
        "ядро перезапустилось само — а не должно было"
    );
    println!("окно скажет: {why}");

    // Вернули как было — предложение обязано уйти само.
    options.mixed_port = port;
    crate::config::advanced::Advanced::write(&options).unwrap();
    assert_eq!(
        crate::app::status::Status::gather(&state).restart_reason(),
        None,
        "расхождения нет, а окно всё ещё предлагает перезапуск"
    );
}

/// Правка встроенного набора из окна доезжает до собранного конфига (D-104).
#[tokio::test]
#[ignore]
async fn live_an_edited_ruleset_reaches_the_assembled_config() {
    sandbox("ruleset-edit");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let set = RulesetStore::list()
        .into_iter()
        .next()
        .expect("встроенных наборов нет — править нечего");
    let before = RulesetStore::read(&set.id).unwrap();
    assert!(
        before.contains("rules"),
        "набор читается не файлом: {before:.40}"
    );

    // Битое не принимаем: набор, выпавший из сборки при включённом тумблере, — это
    // тихо неработающее правило.
    assert!(
        RulesetStore::write(&set.id, "не yaml: [и не набор").is_err(),
        "битый текст записался"
    );
    assert!(
        RulesetStore::write(&set.id, "title: пусто\n").is_err(),
        "набор без единого правила записался"
    );
    assert_eq!(
        RulesetStore::read(&set.id).unwrap(),
        before,
        "файл всё же тронут"
    );

    let mark = "DOMAIN-SUFFIX,umiray-live-check.example,DIRECT";
    // Сразу под `rules:`, а не в конец файла: переезд D-151 дописывает `title_en` в конец,
    // и строка после него стала бы продолжением заголовка, а не правилом.
    RulesetStore::write(
        &set.id,
        &before.replacen("rules:\n", &format!("rules:\n  - {mark}\n"), 1),
    )
    .unwrap();
    let state = crate::app::state::AppState::new();
    let was = set_ready(&state, &set.id, true);

    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        effective.yaml.contains(mark),
        "правка набора не доехала до собранного конфига\nдокумент:\n{}\nнабор:\n{}\nсобрано:\n{}",
        state
            .routing
            .document(&state)
            .unwrap()
            .text
            .unwrap_or_default(),
        RulesetStore::read(&set.id).unwrap(),
        effective
            .yaml
            .lines()
            .skip_while(|line| !line.starts_with("rules:"))
            .take(12)
            .collect::<Vec<_>>()
            .join("\n")
    );
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло конфиг с правленым набором:\n{log}");

    RulesetStore::write(&set.id, &before).unwrap();
    set_ready(&state, &set.id, was);
    println!("правка набора «{}» доехала и вернулась", set.title);
}

/// «Умный DNS»: замер плюс одна запись (D-105).
///
/// Проверяется весь путь — от гонки резолверов до `dns.nameserver` в документе
/// пользователя и до собранного конфига, который принимает ядро.
#[tokio::test]
#[ignore]
async fn live_smart_dns_writes_the_fastest_resolvers() {
    sandbox("smart-dns");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let before = crate::config::advanced::Advanced::read()
        .unwrap()
        .nameserver;
    println!("было: {before:?}");

    let report = crate::diag::smart::Smart::apply("dns-race", Default::default())
        .await
        .unwrap();
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "гонка резолверов ничего не выбрала: {}",
        report.headline
    );

    let after = crate::config::advanced::Advanced::read()
        .unwrap()
        .nameserver;
    println!("стало: {after:?} · {}", report.headline);
    assert!(
        !after.is_empty() && after.len() <= crate::diag::dns::BEST,
        "в конфиг попало не то количество: {after:?}"
    );
    assert_ne!(after, before, "форма не изменилась");

    // И собранный конфиг с ними ядро принимает — иначе «умный» выбор ломал бы запуск.
    let state = crate::app::state::AppState::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    for server in &after {
        assert!(
            effective.yaml.contains(server),
            "резолвер {server} не доехал до собранного конфига"
        );
    }
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(
        ok,
        "ядро отвергло конфиг с подобранными резолверами:\n{log}"
    );

    // Незнакомый подбор — отказ, а не тишина.
    assert!(
        crate::diag::smart::Smart::apply("нет-такого", Default::default())
            .await
            .is_err()
    );
}

/// Проверка перед подключением (D-106): битый конфиг объясняется **до** запуска.
#[tokio::test]
#[ignore]
async fn live_a_broken_config_is_explained_before_the_core_starts() {
    sandbox("preflight");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    // Сколько стоит сухой прогон: он теперь на пути каждого подключения.
    let good = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    let said = crate::diag::config::DryRun::accepts(&good.yaml).unwrap();
    assert!(said.ok, "здоровый конфиг не принят: {}", said.complaint());

    // А теперь заведомо битый — правило, которого у ядра нет.
    let broken = good
        .yaml
        .replace("rules:", "rules:\n  - НЕПРАВИЛО,куда-то,DIRECT");
    let refused = crate::diag::config::DryRun::accepts(&broken).unwrap();
    assert!(!refused.ok, "ядро приняло несуществующее правило");
    println!("ядро сказало: {}", refused.complaint());
    assert!(
        refused.complaint().to_lowercase().contains("error"),
        "жалоба ядра не похожа на объяснение: {}",
        refused.complaint()
    );

    // И то же самое целиком: фаза `core` обязана отмениться «до»-шагом, не запустив ядро.
    // Правила сборка берёт из применённого набора маршрута (D-158), а не из advanced.yaml:
    // туда битое правило и кладём, при включённой маршрутизации.
    let state = crate::app::state::AppState::new();
    let preset = state
        .routing
        .applied_preset(&state)
        .expect("набор маршрута есть всегда");
    let mut document =
        Yaml::top_mapping(&crate::config::presets::PresetStore::content(&preset).unwrap()).unwrap();
    let mut rules = document
        .get(serde_yaml::Value::from("rules"))
        .and_then(serde_yaml::Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    rules.insert(0, serde_yaml::Value::from("НЕПРАВИЛО,куда-то,DIRECT"));
    Yaml::set(&mut document, "rules", serde_yaml::Value::Sequence(rules));
    crate::config::presets::PresetStore::write(
        &preset,
        crate::config::presets::RULES,
        &serde_yaml::to_string(&serde_yaml::Value::Mapping(document)).unwrap(),
    )
    .unwrap();
    state.routing.set_routing(&state, true).unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    let checked = crate::diag::config::DryRun::accepts(&effective.yaml).unwrap();
    assert!(!checked.ok, "битое правило не доехало до сборки");
    assert!(
        !state.mihomo.status().running,
        "ядро не должно быть поднято этой проверкой"
    );
}

/// Автоподбор MTU: замер плюс одна запись (D-105).
#[tokio::test]
#[ignore]
async fn live_the_measured_mtu_reaches_the_core_form() {
    sandbox("smart-mtu");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let path = crate::diag::pmtu::PmtuProbe::path("1.1.1.1")
        .unwrap()
        .expect("узел молчит по ICMP — мерить нечем");
    println!(
        "путь держит {path}, туннелю остаётся {}",
        path - crate::diag::pmtu::TUNNEL
    );

    let report = crate::diag::smart::Smart::apply("pmtu", Default::default())
        .await
        .unwrap();
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "{}",
        report.headline
    );

    let written = crate::config::advanced::Advanced::read().unwrap().mtu;
    assert_eq!(
        written,
        path - crate::diag::pmtu::TUNNEL,
        "в форму попал не подобранный MTU"
    );
    println!("в «Настройках mihomo»: mtu {written} · {}", report.headline);

    // И ядро такой конфиг принимает: подобранное число не должно ломать запуск.
    let state = crate::app::state::AppState::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    let said = crate::diag::config::DryRun::accepts(&effective.yaml).unwrap();
    assert!(
        said.ok,
        "ядро отвергло подобранный MTU: {}",
        said.complaint()
    );
}

/// Сторож соединения (D-107): обрыв при живом ядре виден в статусе без нажатий.
#[tokio::test]
#[ignore]
async fn live_the_guard_notices_a_tunnel_that_carries_nothing() {
    sandbox("guard");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    // Ядра нет — жалоб не бывает: сторож про туннель, а не про его отсутствие.
    crate::app::guard::Guard::look(&state).await;
    assert_eq!(crate::app::status::Status::gather(&state).trouble(), None);

    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    state.mihomo.start(&effective).await.unwrap();
    let _guard = Running(&state.mihomo);
    let _ = state.routing.point_alias(&state).await;

    crate::app::guard::Guard::look(&state).await;
    assert_eq!(
        crate::app::status::Status::gather(&state).trouble(),
        None,
        "рабочий туннель не должен вызывать жалоб"
    );

    // А теперь рвём: наводим псевдоним на заведомо мёртвый узел, добавленный сюда же.
    let dead = crate::nodes::source_import::SourceImporter::add_link(
        "vless://00000000-0000-0000-0000-000000000000@203.0.113.1:443?type=tcp&security=none#мёртвый",
    )
    .unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    state.mihomo.apply(&effective).await.unwrap();
    state.mihomo.select("мёртвый").await.unwrap();

    crate::app::guard::Guard::look(&state).await;
    let shown = crate::app::status::Status::gather(&state);
    let complaint = shown.trouble();
    println!("сторож сказал: {complaint:?}");
    assert!(
        complaint.is_some(),
        "трафик через мёртвый узел не идёт, а сторож молчит"
    );

    crate::nodes::sources::SourceStore::delete(&dead.id).unwrap();
}

/// Часы машины против настоящего заголовка `Date` (D-097).
///
/// Разбор проверяется обычным тестом на строке из RFC, а вот **что живой сервер вообще
/// присылает разбираемое** — только этим: заголовок мог бы прийти в устаревшей форме
/// или не прийти вовсе, и утилита молча отвечала бы «сверять не с чем».
#[tokio::test]
#[ignore]
async fn live_the_clock_is_checked_against_a_real_date_header() {
    let skew = crate::diag::clock::ClockProbe::skew()
        .await
        .expect("эталон не ответил или заголовок не разобрался — проверять нечего");
    println!("расхождение: {skew} с");
    assert_eq!(
        crate::diag::clock::ClockProbe::complaint(skew),
        None,
        "часы этой машины разошлись с эталоном на {skew} с"
    );
}

/// Форсированная перепроверка узлов доезжает до ядра (D-112).
///
/// Проверяется не наблюдатель, а действие: имена провайдеров приходят от самого ядра,
/// и каждому уходит `healthcheck`. Наблюдатель за сменой сети сюда не входит — его
/// повод (сон, смена интерфейса) из теста не создать, для него есть `tools/wake-check.cmd`.
#[tokio::test]
#[ignore]
async fn live_a_forced_recheck_reaches_every_provider() {
    sandbox("recheck");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let mihomo = Mihomo::new();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);

    let asked = mihomo.recheck().await.unwrap();
    println!("перепроверено провайдеров: {asked}");
    assert!(
        asked > 0,
        "ни одного провайдера не перепроверили — ядро их не загрузило?"
    );
}

/// Погашенному ядру перепроверять нечего, и это не отказ: сеть могла смениться,
/// когда VPN просто не работал.
#[tokio::test]
#[ignore]
async fn live_a_recheck_without_a_core_is_not_a_failure() {
    sandbox("recheck-idle");
    let mihomo = Mihomo::new();
    assert_eq!(mihomo.recheck().await.unwrap(), 0);
}

/// Критерий D-113 на живом ядре: с галкой конфиг с группой `umiray-udp` и правилом
/// `NETWORK,udp` ядро **принимает**, а `url-test` с фильтром по именам собирается
/// в непустую группу.
///
/// Обычными тестами проверяется, что мы написали; здесь — что написанное ядро понимает:
/// фильтр-регулярка, тип группы и порядок правил — всё это его правила, не наши.
#[tokio::test]
#[ignore]
async fn live_the_udp_group_is_accepted_by_the_core() {
    sandbox("udp");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let with_udp = crate::render::effective::ConfigRenderer::udp_nodes();
    println!("узлов с нативным UDP в источниках: {with_udp}");
    crate::config::udp::UdpGroup::write(true).unwrap();

    // Группу и правило ставит только маршрутизация: выключенная шлёт всё, UDP тоже,
    // в выбранный выход (D-113, D-166).
    let state = crate::app::state::AppState::new();
    state.routing.set_routing(&state, true).unwrap();
    let effective = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(&state).unwrap(),
        None,
    )
    .unwrap();
    let map = Yaml::top_mapping(&effective.yaml).unwrap();
    let group = map
        .get(serde_yaml::Value::from("proxy-groups"))
        .and_then(serde_yaml::Value::as_sequence)
        .unwrap()
        .iter()
        .find(|group| group.get("name").and_then(serde_yaml::Value::as_str) == Some("umiray-udp"))
        .cloned();
    if with_udp == 0 {
        assert!(group.is_none(), "узлов нет, а группа собралась");
        println!("узлов с нативным UDP нет — проверено, что пустая группа не собирается");
        return;
    }
    let group = group.expect("узлы есть, а группы нет");
    println!("группа: {}", serde_yaml::to_string(&group).unwrap().trim());

    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло конфиг с UDP-группой:\n{log}");

    // И оно правда её подняло: с такой группой ядро не просто приняло конфиг,
    // а стартовало и ответило.
    let mihomo = Mihomo::new();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось с UDP-группой: {why:?}
{}",
            mihomo.log().lines().join(
                "
"
            )
        );
    }
    let _guard = Running(&mihomo);
    assert!(mihomo.status().running, "ядро с UDP-группой не работает");
}

/// Разбор ссылок на **живых подписках** пользователя (D-122): каждая ли ссылка стала
/// записью и примет ли ядро то, что вышло. Читает копию настоящего каталога, в него
/// ничего не пишет — результат кладёт рядом файлом, который проверяется `mihomo -t`.
#[tokio::test]
#[ignore]
async fn live_every_real_link_becomes_an_entry() {
    let _app = sandbox("links");
    let mut lines: Vec<String> = Vec::new();
    // Ссылки — в сырье: собранное с D-122 уже документ `proxies:`.
    for source in SourceStore::list() {
        lines.extend(
            SourceStore::raw(&source.id)
                .lines()
                .filter(|line| line.contains("://"))
                .map(str::to_string),
        );
    }
    assert!(!lines.is_empty(), "в профиле нет ни одной ссылки");

    let mut proxies = Vec::new();
    let mut missed = Vec::new();
    for line in &lines {
        match crate::nodes::convert::Converter::to_entry(line) {
            Some(entry) => proxies.push(serde_yaml::Value::Mapping(entry)),
            None => missed.push(line.split("://").next().unwrap_or("?").to_string()),
        }
    }
    println!("ссылок {}, разобрано {}", lines.len(), proxies.len());
    for (at, proxy) in proxies.iter().enumerate() {
        let map = proxy.as_mapping().unwrap();
        let kind = map.get(serde_yaml::Value::from("type")).unwrap();
        let keys: Vec<&str> = map.keys().filter_map(|k| k.as_str()).collect();
        println!("  {at}: {kind:?} · {}", keys.join(", "));
    }
    assert!(missed.is_empty(), "не разобрали схемы: {missed:?}");

    let mut document = serde_yaml::Mapping::new();
    crate::yaml::Yaml::set(
        &mut document,
        "proxies",
        serde_yaml::Value::Sequence(proxies),
    );
    let path = std::env::temp_dir().join("umiray-converted.yaml");
    std::fs::write(
        &path,
        serde_yaml::to_string(&serde_yaml::Value::Mapping(document)).unwrap(),
    )
    .unwrap();
    println!("записано: {}", path.display());
}

/// D-157: каждый список каталога скачивается, собирается в `.mrs` настоящим ядром,
/// и конфиг с `RULE-SET` на все сразу ядро принимает. Удалённый список ядро не роняет.
///
/// Качает весь каталог — сотню мегабайт, минуты.
#[tokio::test]
#[ignore]
async fn live_every_catalog_list_is_downloaded_built_and_accepted() {
    use crate::lists::import::ListImporter;
    use crate::lists::store::{ListStore, Part};
    use crate::render::mihomo_lists::MihomoLists;

    sandbox("lists");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();

    let catalog = crate::collections::Collections::lists().unwrap().lists;
    let mut failed = Vec::new();
    for entry in &catalog {
        let started = std::time::Instant::now();
        let list = if ListStore::get(&entry.id).is_ok() {
            ListImporter::refresh(&entry.id).await
        } else {
            ListImporter::ensure(&entry.id, None).await
        };
        match list {
            Ok(list) => {
                println!(
                    "{:22} доменов {:>8} · подсетей {:>7} · пропущено {:>5} · опубликован {} · {} мс",
                    list.id,
                    list.domains,
                    list.cidrs,
                    list.skipped,
                    list.published
                        .map(|at| format!(
                            "{} сут. назад",
                            (crate::stamp::Stamp::now().unwrap() - at) / 86_400
                        ))
                        .unwrap_or_else(|| "—".into()),
                    started.elapsed().as_millis()
                );
                // Потеря заметной доли — разбор не понял формат, а не мусор в списке:
                // так однажды пропали все суффиксы antizapret (`.x` у sing-box).
                if list.skipped * 10 > list.domains + list.cidrs {
                    failed.push(format!("{}: пропущено {}", list.id, list.skipped));
                }
            }
            Err(why) => {
                println!("{:22} ОТКАЗ: {why}", entry.id);
                failed.push(entry.id.clone());
            }
        }
    }
    assert!(failed.is_empty(), "не скачались: {failed:?}");

    let started = std::time::Instant::now();
    let built = crate::core::mihomo::lists::ListBuild::prepare();
    println!(
        "собрано в .mrs: {} списков за {} мс",
        built.changed.len(),
        started.elapsed().as_millis()
    );
    assert!(built.failed.is_empty(), "{:?}", built.failed);
    for list in ListStore::list() {
        for part in Part::ALL {
            assert_eq!(
                ListStore::part(&list.id, part).is_some(),
                MihomoLists::artifact(&list.id, part).exists(),
                "{}: часть {} и её .mrs расходятся",
                list.id,
                part.name()
            );
        }
    }

    // Весь каталог разделом `rule-sets` (D-158): списки маршрута обязаны доехать до ядра.
    let rules = |ids: &[String]| {
        let mut text = String::from("rule-sets:\n");
        for id in ids {
            text.push_str(&format!("  - id: {id}\n    target: umiray\n"));
        }
        crate::render::plan::Route {
            text: Some(text + "rules:\n  - MATCH,DIRECT\n"),
        }
    };
    let ids: Vec<String> = catalog.iter().map(|entry| entry.id.clone()).collect();
    let effective =
        crate::render::effective::ConfigRenderer::effective(&rules(&ids), None).unwrap();
    let providers = effective.yaml.matches("format: mrs").count();
    println!("провайдеров в конфиге: {providers}");
    assert!(providers >= ids.len(), "не все списки дошли до конфига");
    let said = crate::diag::config::DryRun::accepts(&effective.yaml).unwrap();
    assert!(said.ok, "ядро не приняло конфиг: {}", said.lines.join("\n"));

    ListStore::delete(&ids[0]).unwrap();
    let effective =
        crate::render::effective::ConfigRenderer::effective(&rules(&ids), None).unwrap();
    assert!(
        !effective.yaml.contains(&format!("RULE-SET,{},", ids[0])),
        "строка на удалённый список осталась"
    );
    let said = crate::diag::config::DryRun::accepts(&effective.yaml).unwrap();
    assert!(said.ok, "без списка ядро упало: {}", said.lines.join("\n"));
}

/// D-157, D-158: rule set с выходом `AUTO` ведёт трафик через VPN **даже в направлении
/// Direct**, а всё мимо списка идёт напрямую. Нужны узлы — гоняется релизной сборкой,
/// на копии настоящего каталога:
///
/// ```text
/// cargo test --release live_a_rule_set -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_a_rule_set_sends_its_domains_through_the_vpn() {
    use crate::lists::parse::Payload;
    use crate::lists::store::{ListStore, RuleList};

    sandbox("rule-set");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();
    own_ports();

    // Список без сети: одна точная запись, чтобы соседний домен того же сервиса
    // в него не попал.
    let list = RuleList {
        id: "probe-set".into(),
        title: "probe".into(),
        urls: vec!["https://example.invalid".into()],
        domains: 1,
        ..RuleList::default()
    };
    ListStore::save(
        &list,
        &Payload {
            domains: vec!["api.ipify.org".into()],
            ..Payload::default()
        },
    )
    .unwrap();
    let built = crate::core::mihomo::lists::ListBuild::prepare();
    assert!(built.failed.is_empty(), "{:?}", built.failed);

    let mihomo = Mihomo::new();
    // Выход Direct: `MATCH` набора — в псевдоним, а тот наводится на DIRECT ниже.
    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route {
            text: Some(
                "rule-sets:\n  - id: probe-set\n    target: AUTO\nrules:\n  - MATCH,umiray\n"
                    .into(),
            ),
        },
        None,
    )
    .unwrap();
    assert!(effective.yaml.contains("probe-set"), "{}", effective.yaml);
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);
    let port = mihomo.status().port.expect("порт local-режима");
    // Псевдоним — как в направлении Direct: всё непойманное мимо VPN.
    mihomo
        .select(crate::config::direction::DIRECT)
        .await
        .expect("DIRECT");

    let get = |url: &'static str, through: Option<u16>| async move {
        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(25));
        if let Some(port) = through {
            builder =
                builder.proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap());
        } else {
            builder = builder.no_proxy();
        }
        builder
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap_or_else(|why| panic!("{url}: {why}"))
            .text()
            .await
            .unwrap()
            .trim()
            .to_string()
    };
    let direct = get("https://ipv4.icanhazip.com", None).await;
    let listed = get("https://api.ipify.org", Some(port)).await;
    let unlisted = get("https://ipv4.icanhazip.com", Some(port)).await;
    println!("напрямую {direct} · домен из списка {listed} · мимо списка {unlisted}");
    println!("umiray ведёт: {:?}", mihomo.route("umiray").await);
    for line in mihomo.log().lines() {
        if line.contains("match") || line.contains("probe-set") || line.contains("error") {
            println!("  {line}");
        }
    }
    assert_ne!(listed, direct, "домен из списка ушёл мимо VPN");
    assert_eq!(unlisted, direct, "домен мимо списка ушёл в VPN");
}

/// Своё ядро в песочнице — на своих портах, с логом правил.
///
/// На машине разработчика рядом может работать настоящий клиент: запрос на его 2080 ушёл
/// бы в его ядро с его правилами, и проверка соврала бы (D-133). Строки «match … using …»
/// ядро пишет с уровня info — по ним видно, какое правило сработало.
fn own_ports() {
    let advanced = Documents::read(files::ADVANCED).unwrap();
    let mut map = Yaml::top_mapping(&advanced).unwrap();
    Yaml::set(&mut map, "log-level", serde_yaml::Value::from("info"));
    let free = || crate::core::Ports::free_port().unwrap();
    Yaml::set(&mut map, "mixed-port", serde_yaml::Value::from(free()));
    let mut dns = map
        .get(serde_yaml::Value::from("dns"))
        .and_then(serde_yaml::Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    Yaml::set(
        &mut dns,
        "listen",
        serde_yaml::Value::from(format!("127.0.0.1:{}", free())),
    );
    Yaml::set(&mut map, "dns", serde_yaml::Value::Mapping(dns));
    Documents::write(
        files::ADVANCED,
        &serde_yaml::to_string(&serde_yaml::Value::Mapping(map)).unwrap(),
    )
    .unwrap();
}

/// Песочница, оставленная прошлой проверкой, — не копировать заново: вторая половина
/// проверки идёт в другом процессе (с правами) и продолжает то, что сделала первая.
fn resume(name: &str) -> Option<PathBuf> {
    let root = std::env::temp_dir().join(format!("umiray-live-{name}"));
    let app = root.join(crate::paths::Paths::root().file_name().unwrap());
    if !app.is_dir() {
        return None;
    }
    std::env::set_var("LOCALAPPDATA", &root);
    Some(app)
}

/// D-161, первая половина — без прав. Тумблер «qd» выключен: бинаря нет, база qd цела.
/// Ссылка `qd://` из поля добавления качает qd, а принять её некому: qd встаёт только
/// с правами — ссылка ждёт в базе (D-170). Вторая половина — следующая проверка.
///
/// Ссылка — своя, из `UMIRAY_QD_LINK`: выдумать её нельзя, а в файлах ей не место.
#[tokio::test]
#[ignore]
async fn live_qd_link_without_rights_waits_for_qd() {
    let Ok(link) = std::env::var("UMIRAY_QD_LINK") else {
        println!("пропуск: нет UMIRAY_QD_LINK");
        return;
    };
    let app = sandbox("qd-link");
    let state = crate::app::state::AppState::new();
    let elevated = crate::system::elevation::Elevation::is_elevated();

    state.qd_panel.remove(&state).await.unwrap();
    assert!(!state.qd.present(), "бинарь qd остался");
    assert!(app.join("qd").is_dir(), "удаление унесло базу qd");
    assert_eq!(
        state.settings.get().engine,
        crate::core::EngineId::Mihomo,
        "вид не вернулся к mihomo"
    );

    let adopted = state.qd_panel.adopt(&state, &link).await.unwrap();
    let adopted = serde_json::to_value(adopted).unwrap();
    println!("ссылка: {adopted}");
    assert!(state.qd.present(), "qd не скачался ради ссылки");
    assert!(
        adopted["downloaded"].is_string(),
        "не сказано, что qd скачан"
    );
    let pending = || Db::get(Table::State, "qd-pending", "").unwrap();
    if elevated {
        assert_eq!(
            adopted["pending"], false,
            "с правами ссылку принимают сразу"
        );
        state.qd.shutdown().await;
    } else {
        assert_eq!(adopted["pending"], true, "без прав ссылка ждёт");
        assert_eq!(
            pending().as_deref().map(str::trim),
            Some(link.trim()),
            "ждёт не та ссылка"
        );
    }
    // Песочница остаётся: её продолжает `live_qd_takes_the_waiting_link_when_it_comes_up`.
}

/// D-161, вторая половина — с правами (`tools\as-admin.cmd`). qd поднимается ради своих
/// разделов и забирает ждущую ссылку сам; потом тумблер выключают при живом процессе —
/// бинарь уходит, база остаётся.
#[tokio::test]
#[ignore]
async fn live_qd_takes_the_waiting_link_when_it_comes_up() {
    let Some(app) = resume("qd-link") else {
        println!("пропуск: сначала live_qd_link_without_rights_waits_for_qd");
        return;
    };
    let pending = || Db::get(Table::State, "qd-pending", "").unwrap();
    if pending().is_none() {
        println!("пропуск: ссылка не ждёт — первая половина шла с правами");
        return;
    }
    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        "нужны права: tools\\as-admin.cmd"
    );
    let state = crate::app::state::AppState::new();

    let status = serde_json::to_value(state.qd_panel.status(&state).await).unwrap();
    println!("qd: {status}");
    assert_eq!(status["problem"], serde_json::Value::Null, "qd не встал");
    assert_eq!(
        status["state"]["imported"], true,
        "qd не принял ждущую ссылку"
    );
    assert!(pending().is_none(), "принятая ссылка осталась ждать");

    state.qd_panel.remove(&state).await.unwrap();
    assert!(!state.qd.running(), "процесс qd пережил удаление");
    assert!(!state.qd.present(), "бинарь qd остался");
    assert!(
        app.join("qd").join("client.db").exists(),
        "удаление унесло базу qd"
    );
    let _ = std::fs::remove_dir_all(app.parent().unwrap());
}

/// D-163 на настоящей базе: в копии документы, наборы, коллекции, источники и настройки
/// окна, а HWID, кэши и архив — нет. Окно сохранения — не здесь: оно системное.
#[test]
#[ignore]
fn live_the_export_of_the_real_directory_carries_settings_only() {
    let app = sandbox("export");
    let target = app.parent().unwrap().join("umiray-settings.db");
    crate::app::data::DataDir::export(&target).unwrap();
    let copy = rusqlite::Connection::open(&target).unwrap();
    let ids = |table: Table| -> Vec<String> {
        let mut statement = copy
            .prepare(&format!("SELECT DISTINCT id FROM {}", table.name()))
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap()
    };
    let documents = ids(Table::Documents);
    println!(
        "документы {documents:?}, источников {}",
        ids(Table::Sources).len()
    );
    for must in ["advanced", "client"] {
        assert!(documents.iter().any(|id| id == must), "в копии нет {must}");
    }
    assert!(!ids(Table::Sources).is_empty(), "нет источников");
    assert!(!ids(Table::Presets).is_empty(), "нет наборов");
    assert_eq!(ids(Table::State), ["settings"], "HWID и кэш стран не едут");
    assert!(ids(Table::Lists).is_empty(), "rule sets — кэш");
    assert!(
        ids(Table::Archive).is_empty(),
        "архив старых файлов не едет"
    );
}

/// D-157: кнопка geo-баз на работающем ядре обновляет файлы в `run/`, на остановленном —
/// отказывает словами, а не молчит.
#[tokio::test]
#[ignore]
async fn live_the_core_updates_its_geo_databases() {
    sandbox("geo");
    Migration::run().unwrap();
    Mode::write(Mode::Local).unwrap();
    own_ports();

    let mihomo = Mihomo::new();
    let refused = mihomo.update_geo().await;
    assert!(refused.is_err(), "без ядра обновлять нечем");
    println!("без ядра: {}", refused.unwrap_err());

    let effective = crate::render::effective::ConfigRenderer::effective(
        &crate::render::plan::Route::default(),
        None,
    )
    .unwrap();
    if let Err(why) = mihomo.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            mihomo.log().lines().join("\n")
        );
    }
    let _guard = Running(&mihomo);
    // Ядро качает базы через свои же правила — то есть через узел.
    let port = mihomo.status().port.expect("порт local-режима");
    alive_exit(&mihomo, port).await;
    let before = Mihomo::geo_files();
    println!("до: {before:?}");
    assert!(!before.is_empty(), "geo-баз в run/ нет — сравнивать нечего");

    let started = std::time::Instant::now();
    let after = mihomo
        .update_geo()
        .await
        .unwrap_or_else(|why| panic!("{why}\n{}", mihomo.log().lines().join("\n")));
    println!("после ({} мс): {after:?}", started.elapsed().as_millis());
    for line in mihomo.log().lines() {
        let lower = line.to_lowercase();
        if lower.contains("geo") || lower.contains("mmdb") || lower.contains("updat") {
            println!("  {line}");
        }
    }
    // Ядро сверяет хэш скачанного с тем, что лежит, и неизменившуюся базу не переписывает:
    // дата сдвигается только у той, что у источника правда новая.
    for file in &before {
        let fresh = after.iter().find(|seen| seen.name == file.name).unwrap();
        assert!(
            fresh.modified >= file.modified,
            "{}: дата отъехала назад",
            file.name
        );
    }
    assert!(
        mihomo
            .log()
            .lines()
            .iter()
            .any(|line| line.contains("Updating GEO database")),
        "ядро не начинало обновление"
    );
}

/// S-030: какой DNS-набор быстрее отвечает из этой сети — замер под план рекомендованного
/// конфига (RECOMMENDED.md). Стенд — одноразовое ядро на каждый набор и прогон: первый
/// вопрос по имени идёт к резолверам, повтор — из кэша ядра. Сеть машины, где запущен.
///
/// ```text
/// cargo test live_spike_dns_sets -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_spike_dns_sets() {
    sandbox("dns-spike");
    Migration::run().unwrap();

    const NAMES: [&str; 16] = [
        "ya.ru",
        "vk.com",
        "ozon.ru",
        "gosuslugi.ru",
        "wildberries.ru",
        "mail.ru",
        "avito.ru",
        "sberbank.ru",
        "google.com",
        "youtube.com",
        "github.com",
        "discord.com",
        "telegram.org",
        "wikipedia.org",
        "microsoft.com",
        "instagram.com",
    ];
    let shots = crate::diag::dns::DnsProbe::race(
        crate::diag::dns::DEFAULT_DOMAIN,
        Duration::from_millis(1500),
        crate::diag::dns::DnsFilter::Any,
    )
    .await
    .unwrap();
    let picks: Vec<String> = crate::diag::dns::DnsProbe::fastest(&shots, crate::diag::dns::BEST)
        .into_iter()
        .map(|index| shots[index].candidate.addr.clone())
        .collect();
    println!("мастер выбрал: {picks:?}");

    let list = |servers: &[&str]| -> String {
        servers
            .iter()
            .map(|server| format!("    - \"{server}\"\n"))
            .collect()
    };
    let picked: Vec<&str> = picks.iter().map(String::as_str).collect();
    let sets: Vec<(&str, String)> = vec![
        (
            "шаблон: DoH 1.1.1.1",
            format!("  nameserver:\n{}", list(&["https://1.1.1.1/dns-query"])),
        ),
        (
            "мастер: три шифрованных",
            format!("  nameserver:\n{}", list(&picked)),
        ),
        (
            "релиз: пять смешанных, .ru — Яндекс",
            format!(
                "  cache-algorithm: arc\n  default-nameserver:\n{}  nameserver:\n{}  nameserver-policy:\n    \"+.ru,+.xn--p1ai,+.yandex.net,+.vk.com,+.userapi.com\":\n{}",
                list(&["8.8.8.8", "77.88.8.8", "1.1.1.1"]),
                list(&[
                    "quic://dns.alidns.com:853",
                    "https://cloudflare-dns.com/dns-query",
                    "quic://dns.comss.one",
                    "https://dns.google/dns-query",
                    "tls://one.one.one.one",
                ]),
                list(&["77.88.8.8", "77.88.8.1"]).replace("    -", "      -"),
            ),
        ),
        (
            "Яндекс без шифрования",
            format!("  nameserver:\n{}", list(&["77.88.8.8"])),
        ),
    ];

    for (title, dns) in &sets {
        let config = format!(
            "mixed-port: 0\nmode: rule\nlog-level: silent\nexternal-ui: \"\"\n\
             dns:\n  enable: true\n  listen: \"\"\n  ipv6: false\n  enhanced-mode: fake-ip\n{dns}\
             rules:\n  - MATCH,DIRECT\n"
        );
        // Российские имена — первая половина списка, зарубежные — вторая: у набора
        // с политикой для `.ru` они идут к разным резолверам.
        let mut ru: Vec<u64> = Vec::new();
        let mut abroad: Vec<u64> = Vec::new();
        let mut warm: Vec<u64> = Vec::new();
        let mut fresh: Vec<u64> = Vec::new();
        let mut failed = 0;
        for round in 0..2 {
            let bench = match crate::diag::Bench::with(&config).await {
                Ok(bench) => bench,
                Err(why) => panic!("{title}: стенд не встал: {why:?}\n{config}"),
            };
            for (index, name) in NAMES.iter().enumerate() {
                let cold = if index < NAMES.len() / 2 {
                    &mut ru
                } else {
                    &mut abroad
                };
                for (attempt, into) in [(0, cold), (1, &mut warm)] {
                    let at = std::time::Instant::now();
                    let answer =
                        tokio::time::timeout(Duration::from_secs(5), bench.resolve(name)).await;
                    let ms = at.elapsed().as_millis() as u64;
                    match answer {
                        Ok(Ok(ips)) if !ips.is_empty() => into.push(ms),
                        _ if attempt == 0 => failed += 1,
                        _ => {}
                    }
                }
            }
            // Имя, которого нет ни в одном кэше на пути: ответ на него — честное время
            // до настоящего рекурсивного резолвера, а не до ближайшего кэша.
            for probe in 0..3 {
                let name = format!(
                    "umiray-{}-{round}-{probe}.1.2.3.4.nip.io",
                    std::process::id()
                );
                let at = std::time::Instant::now();
                let answer =
                    tokio::time::timeout(Duration::from_secs(5), bench.resolve(&name)).await;
                if matches!(answer, Ok(Ok(ref ips)) if !ips.is_empty()) {
                    fresh.push(at.elapsed().as_millis() as u64);
                }
            }
        }
        let stat = |values: &mut Vec<u64>| -> String {
            if values.is_empty() {
                return "—".into();
            }
            values.sort_unstable();
            let pick = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
            format!(
                "медиана {} мс, p90 {} мс, макс {} мс",
                pick(0.5),
                pick(0.9),
                pick(1.0)
            )
        };
        println!(
            "{title}\n  .ru: {}\n  зарубежные: {}\n  свежее имя: {}\n  повтор (кэш): {} · не ответил: {failed} из {}",
            stat(&mut ru),
            stat(&mut abroad),
            stat(&mut fresh),
            stat(&mut warm),
            NAMES.len() * 2,
        );
    }
}

/// S-032: `tcp-concurrent` на прямых соединениях — замер под план рекомендованного конфига
/// (RECOMMENDED.md). Стенд со своим `mixed-port`, запрос `http://<имя>/` через него, время
/// до заголовков ответа: в нём соединение ядра с сайтом, а резолв вынесен заранее — имя
/// спрошено до запроса и лежит в кэше. Вкл и выкл чередуются, у каждого прогона свой стенд.
///
/// ```text
/// cargo test live_spike_tcp_concurrent -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_spike_tcp_concurrent() {
    sandbox("tcp-concurrent");
    Migration::run().unwrap();

    // Имена с несколькими A-записями — на них `tcp-concurrent` и может что-то решать;
    // сколько адресов у каждого сейчас, печатается в начале.
    const NAMES: [&str; 12] = [
        "ya.ru",
        "vk.com",
        "mail.ru",
        "ozon.ru",
        "avito.ru",
        "rambler.ru",
        "microsoft.com",
        "amazon.com",
        "yahoo.com",
        "apple.com",
        "cloudflare.com",
        "reddit.com",
    ];
    const ROUNDS: usize = 5;

    let mut times: [Vec<u64>; 2] = [Vec::new(), Vec::new()];
    let mut failed = [0usize; 2];
    for round in 0..ROUNDS * 2 {
        let concurrent = round % 2 == 1;
        let port = crate::core::Ports::free_port().unwrap();
        let config = format!(
            "mixed-port: {port}\nmode: rule\nlog-level: silent\nexternal-ui: \"\"\n\
             tcp-concurrent: {concurrent}\nipv6: false\n\
             dns:\n  enable: true\n  listen: \"\"\n  ipv6: false\n  enhanced-mode: fake-ip\n\
             \x20 nameserver:\n    - \"https://1.1.1.1/dns-query\"\n    - \"https://8.8.8.8/dns-query\"\n\
             rules:\n  - MATCH,DIRECT\n"
        );
        let bench = crate::diag::Bench::with(&config).await.unwrap();
        // Без пула: каждый запрос — новое соединение ядра с сайтом, иначе мерили бы
        // переиспользованный сокет.
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://127.0.0.1:{port}")).unwrap())
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        for name in NAMES {
            let ips = bench.resolve(name).await.unwrap_or_default();
            if round == 0 {
                println!("{name}: {} адр.", ips.len());
            }
            let at = std::time::Instant::now();
            match client.get(format!("http://{name}/")).send().await {
                Ok(_) => times[concurrent as usize].push(at.elapsed().as_millis() as u64),
                Err(why) => {
                    failed[concurrent as usize] += 1;
                    println!("  {name} (concurrent {concurrent}): {why}");
                }
            }
        }
    }
    for (index, title) in ["tcp-concurrent: false", "tcp-concurrent: true"]
        .iter()
        .enumerate()
    {
        let values = &mut times[index];
        values.sort_unstable();
        let pick = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
        println!(
            "{title}: медиана {} мс, p90 {} мс, макс {} мс · не ответил {} из {}",
            pick(0.5),
            pick(0.9),
            pick(1.0),
            failed[index],
            NAMES.len() * ROUNDS
        );
    }
}

/// S-031: MTU адаптера TUN — подбор мастера или умолчание ядра (RECOMMENDED.md). TUN через
/// `AUTO`, `tun.mtu` по очереди: не задан (решает ядро), 9000, 1500, 1440; на каждый —
/// время до первого байта и скорость скачивания 50 МБ, три круга вперемешку. Печатает и MTU,
/// который Windows показывает у адаптера, — им проверяется умолчание ядра.
///
/// Нужны права: `tools\spike-mtu.cmd` поднимает себя сам, вывод — `%TEMP%\umiray-spike-mtu.txt`.
#[tokio::test]
#[ignore]
async fn live_spike_tun_mtu() {
    assert!(
        crate::system::elevation::Elevation::is_elevated(),
        r"TUN без прав администратора не поднимется — запустите tools\spike-mtu.cmd"
    );
    sandbox("tun-mtu");
    Migration::run().unwrap();
    Mode::write(Mode::Tun).unwrap();

    const URL: &str = "https://speed.cloudflare.com/__down?bytes=50000000";
    const VARIANTS: [u32; 4] = [0, 9000, 1500, 1440];
    let mut results: Vec<(u32, u64, f64)> = Vec::new();
    for round in 0..3 {
        for mtu in VARIANTS {
            let mut options = Advanced::read().unwrap();
            options.mtu = mtu;
            Advanced::write(&options).unwrap();
            let mihomo = Mihomo::new();
            let effective = crate::render::effective::ConfigRenderer::effective(
                &crate::render::plan::Route::default(),
                None,
            )
            .unwrap();
            if let Err(why) = mihomo.start(&effective).await {
                panic!(
                    "ядро не поднялось: {why:?}\n{}",
                    mihomo.log().lines().join("\n")
                );
            }
            let _running = Running(&mihomo);
            mihomo
                .select(crate::config::direction::AUTO)
                .await
                .expect("автогруппа не собралась");
            if round == 0 {
                let device = Mode::tun_device(
                    &Yaml::top_mapping(&Documents::read(files::ADVANCED).unwrap()).unwrap(),
                );
                let shown = std::process::Command::new("netsh")
                    .args(["interface", "ipv4", "show", "subinterfaces"])
                    .output()
                    .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                    .unwrap_or_default();
                let line = shown
                    .lines()
                    .find(|line| line.contains(&device))
                    .unwrap_or("адаптер не найден");
                println!("mtu {mtu}: Windows видит «{}»", line.trim());
            }
            // Прогрев: первое соединение платит за подъём узла и резолв, а мерим адаптер.
            let client = reqwest::Client::builder()
                .no_proxy()
                .pool_max_idle_per_host(0)
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap();
            let _ = client
                .get("https://speed.cloudflare.com/__down?bytes=1000")
                .send()
                .await;

            let at = std::time::Instant::now();
            let mut response = match client.get(URL).send().await {
                Ok(response) => response,
                Err(why) => {
                    println!("mtu {mtu}, круг {round}: {why}");
                    continue;
                }
            };
            let first = at.elapsed().as_millis() as u64;
            let mut bytes = 0usize;
            while let Ok(Some(chunk)) = response.chunk().await {
                bytes += chunk.len();
            }
            let mbit = bytes as f64 * 8.0 / at.elapsed().as_secs_f64() / 1e6;
            println!(
                "mtu {mtu}, круг {round}: первый байт {first} мс, {mbit:.1} Мбит/с, {bytes} байт"
            );
            results.push((mtu, first, mbit));
        }
    }
    for mtu in VARIANTS {
        let mut speeds: Vec<f64> = results.iter().filter(|r| r.0 == mtu).map(|r| r.2).collect();
        let mut firsts: Vec<u64> = results.iter().filter(|r| r.0 == mtu).map(|r| r.1).collect();
        speeds.sort_by(f64::total_cmp);
        firsts.sort_unstable();
        println!(
            "итог mtu {}: скорость {:?} Мбит/с, первый байт {:?} мс",
            if mtu == 0 {
                "не задан".into()
            } else {
                mtu.to_string()
            },
            speeds.iter().map(|s| format!("{s:.1}")).collect::<Vec<_>>(),
            firsts
        );
    }
}

/// S-033: хватает ли шифрованных резолверов на каждую категорию фильтра, и правда ли
/// категория режет то, что обещает. Каждая шифрованная точка каждого варианта коллекции
/// плюс кандидаты на добавление; три вопроса: `example.com` (жив и за сколько),
/// `doubleclick.net` (режет рекламу), `malware.testcategory.com` (режет опасное — тестовое
/// имя Cloudflare). Прав не нужно.
///
/// ```text
/// cargo test live_spike_dns_categories -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_spike_dns_categories() {
    use crate::diag::dns::{Candidate, DnsProbe};
    sandbox("dns-categories");
    Migration::run().unwrap();

    let mut list: Vec<Candidate> = Vec::new();
    for provider in crate::collections::Collections::dns().unwrap().providers {
        for variant in &provider.variants {
            for server in variant
                .servers
                .iter()
                .filter(|s| s.proto != "udp" && !s.ipv6)
            {
                list.push(Candidate {
                    provider: provider.id.clone(),
                    variant: variant.id.clone(),
                    filter: variant.filter.clone(),
                    proto: server.proto.clone(),
                    addr: server.addr.clone(),
                });
            }
        }
    }
    // Кандидаты на добавление — их ещё нет в коллекции.
    for (provider, variant, filter, proto, addr) in [
        (
            "mullvad",
            "default",
            "none",
            "doh",
            "https://dns.mullvad.net/dns-query",
        ),
        (
            "mullvad",
            "adblock",
            "ads",
            "doh",
            "https://adblock.dns.mullvad.net/dns-query",
        ),
        (
            "mullvad",
            "adblock",
            "ads",
            "dot",
            "tls://adblock.dns.mullvad.net",
        ),
        (
            "mullvad",
            "base",
            "ads",
            "doh",
            "https://base.dns.mullvad.net/dns-query",
        ),
        (
            "controld",
            "p0",
            "none",
            "doh",
            "https://freedns.controld.com/p0",
        ),
        (
            "controld",
            "p1",
            "security",
            "doh",
            "https://freedns.controld.com/p1",
        ),
        (
            "controld",
            "p2",
            "ads",
            "doh",
            "https://freedns.controld.com/p2",
        ),
        (
            "controld",
            "p2",
            "ads",
            "dot",
            "tls://p2.freedns.controld.com",
        ),
        (
            "dnsforge",
            "default",
            "ads",
            "doh",
            "https://dnsforge.de/dns-query",
        ),
        ("dnsforge", "default", "ads", "dot", "tls://dnsforge.de"),
        (
            "libredns",
            "ads",
            "ads",
            "doh",
            "https://doh.libredns.gr/ads",
        ),
        (
            "dnssb",
            "default",
            "none",
            "doh",
            "https://doh.dns.sb/dns-query",
        ),
    ] {
        list.push(Candidate {
            provider: format!("+{provider}"),
            variant: variant.into(),
            filter: filter.into(),
            proto: proto.into(),
            addr: addr.into(),
        });
    }

    let timeout = Duration::from_millis(2500);
    let ask = |candidate: Candidate, name: &'static str| async move {
        if matches!(candidate.proto.as_str(), "doh") {
            DnsProbe::shoot(&candidate, name, timeout).await
        } else {
            DnsProbe::shoot_via_core(&candidate, name, timeout).await
        }
    };
    // Подменный ответ блокировщика — пусто, NXDOMAIN, 0.0.0.0 или свой адрес-заглушка;
    // настоящий у doubleclick.net и тестового имени Cloudflare — публичный и не нулевой.
    let blocked = |shot: &crate::diag::dns::Shot| {
        !shot.ok()
            || shot
                .ips
                .iter()
                .all(|ip| ip.is_unspecified() || ip.is_loopback())
    };
    println!("| провайдер | вариант | filter | proto | example.com, мс (3) | реклама | опасное |");
    for candidate in list {
        let mut times = Vec::new();
        for _ in 0..3 {
            let shot = ask(candidate.clone(), "example.com").await;
            times.push(shot.ms.filter(|_| shot.ok()));
        }
        let ads = ask(candidate.clone(), "doubleclick.net").await;
        let bad = ask(candidate.clone(), "malware.testcategory.com").await;
        let times: Vec<String> = times
            .iter()
            .map(|ms| ms.map_or("—".into(), |ms| ms.to_string()))
            .collect();
        println!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            candidate.provider,
            candidate.variant,
            candidate.filter,
            candidate.proto,
            times.join(" "),
            if blocked(&ads) {
                format!("режет {:?}", ads.ips)
            } else {
                "пускает".into()
            },
            if blocked(&bad) {
                format!("режет {:?}", bad.ips)
            } else {
                "пускает".into()
            },
        );
    }
}

/// S-034: какой протокол шифрованного DNS быстрее у одного и того же провайдера — DoH,
/// DoH поверх HTTP/3 (`#h3` и `prefer-h3`), DoT, DoQ, — и чей ответ ближе для прямого
/// трафика. Стенд на каждую точку; первое имя — с рукопожатием, следующие пять — по уже
/// открытому соединению; все имена свежие (`nip.io`), кэша на пути нет. Для прямого —
/// российские имена и TCP-соединение до первого адреса ответа: близкий CDN важнее быстрого
/// ответа. Прав не нужно.
///
/// ```text
/// cargo test live_spike_dns_protocols -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_spike_dns_protocols() {
    sandbox("dns-protocols");
    Migration::run().unwrap();

    const POINTS: [(&str, &str, bool); 22] = [
        ("cloudflare", "https://1.1.1.1/dns-query", false),
        ("cloudflare", "https://1.1.1.1/dns-query#h3", false),
        ("cloudflare", "https://1.1.1.1/dns-query", true),
        ("cloudflare", "tls://1.1.1.1", false),
        ("google", "https://8.8.8.8/dns-query", false),
        ("google", "https://8.8.8.8/dns-query#h3", false),
        ("google", "tls://8.8.8.8", false),
        (
            "adguard",
            "https://unfiltered.adguard-dns.com/dns-query",
            false,
        ),
        ("adguard", "https://dns.adguard-dns.com/dns-query", false),
        ("adguard", "https://dns.adguard-dns.com/dns-query#h3", false),
        ("adguard", "tls://dns.adguard-dns.com", false),
        ("adguard", "quic://dns.adguard-dns.com", false),
        ("alidns", "https://223.5.5.5/dns-query", false),
        ("alidns", "tls://223.5.5.5", false),
        ("alidns", "quic://223.5.5.5", false),
        ("controld", "https://freedns.controld.com/p0", false),
        ("controld", "https://freedns.controld.com/p0#h3", false),
        ("controld", "quic://p0.freedns.controld.com", false),
        ("surfshark", "quic://dns.surfsharkdns.com", false),
        (
            "yandex",
            "https://common.dot.dns.yandex.net/dns-query",
            false,
        ),
        ("yandex", "tls://common.dot.dns.yandex.net", false),
        ("yandex", "77.88.8.8", false),
    ];
    let config = |server: &str, h3: bool| {
        format!(
            "mixed-port: 0\nmode: rule\nlog-level: silent\nexternal-ui: \"\"\n\
             dns:\n  enable: true\n  listen: \"\"\n  ipv6: false\n  enhanced-mode: fake-ip\n\
             \x20 prefer-h3: {h3}\n  default-nameserver: [77.88.8.8, 1.1.1.1]\n\
             \x20 nameserver:\n    - \"{server}\"\nrules:\n  - MATCH,DIRECT\n"
        )
    };
    let median = |values: &mut Vec<u64>| -> String {
        if values.is_empty() {
            return "—".into();
        }
        values.sort_unstable();
        format!("{}", values[values.len() / 2])
    };

    println!("| провайдер | точка | prefer-h3 | первое имя, мс | дальше, медиана мс | ответили |");
    for (provider, server, h3) in POINTS {
        let mut cold = Vec::new();
        let mut warm = Vec::new();
        let mut ok = 0;
        for round in 0..2 {
            let Ok(bench) = crate::diag::Bench::with(&config(server, h3)).await else {
                continue;
            };
            for n in 0..6 {
                let name = format!("umiray-{}-{round}-{n}.1.2.3.4.nip.io", std::process::id());
                let at = std::time::Instant::now();
                let answer =
                    tokio::time::timeout(Duration::from_secs(5), bench.resolve(&name)).await;
                let ms = at.elapsed().as_millis() as u64;
                if matches!(answer, Ok(Ok(ref ips)) if !ips.is_empty()) {
                    ok += 1;
                    if n == 0 {
                        cold.push(ms)
                    } else {
                        warm.push(ms)
                    }
                }
            }
        }
        println!(
            "| {provider} | `{server}` | {h3} | {} | {} | {ok} из 12 |",
            median(&mut cold),
            median(&mut warm)
        );
    }

    // Прямой трафик: чей ответ ведёт к ближнему серверу.
    const DIRECT: [&str; 6] = [
        "77.88.8.8",
        "https://common.dot.dns.yandex.net/dns-query",
        "8.8.8.8",
        "https://8.8.8.8/dns-query",
        "https://1.1.1.1/dns-query",
        "https://dns.adguard-dns.com/dns-query",
    ];
    const RU: [&str; 10] = [
        "ya.ru",
        "yandex.ru",
        "vk.com",
        "ozon.ru",
        "wildberries.ru",
        "gosuslugi.ru",
        "avito.ru",
        "rutube.ru",
        "mail.ru",
        "kinopoisk.ru",
    ];
    println!("\n| резолвер | ответ, медиана мс | TCP до адреса ответа, медиана мс | p90 |");
    for server in DIRECT {
        let Ok(bench) = crate::diag::Bench::with(&config(server, false)).await else {
            println!("| `{server}` | стенд не встал | | |");
            continue;
        };
        let mut answers = Vec::new();
        let mut connects = Vec::new();
        for name in RU {
            let at = std::time::Instant::now();
            let Ok(Ok(ips)) =
                tokio::time::timeout(Duration::from_secs(5), bench.resolve(name)).await
            else {
                continue;
            };
            answers.push(at.elapsed().as_millis() as u64);
            let Some(ip) = ips
                .first()
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
            else {
                continue;
            };
            // Лучшее из трёх: одна попытка ловит шум, а мерим расстояние, не шум.
            let mut best = u64::MAX;
            for _ in 0..3 {
                let at = std::time::Instant::now();
                let tcp = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::net::TcpStream::connect((ip, 443)),
                )
                .await;
                if matches!(tcp, Ok(Ok(_))) {
                    best = best.min(at.elapsed().as_millis() as u64);
                }
            }
            if best != u64::MAX {
                connects.push(best);
            }
        }
        let p90 = {
            let mut sorted = connects.clone();
            sorted.sort_unstable();
            sorted
                .get(((sorted.len().max(1) - 1) as f64 * 0.9).round() as usize)
                .map_or("—".to_string(), |v| v.to_string())
        };
        println!(
            "| `{server}` | {} | {} | {p90} |",
            median(&mut answers),
            median(&mut connects)
        );
    }
}

/// MASQUE (D-165) — живой выход через выпущенный WARP: узел `masque` из источника этой
/// машины, стенд со своим `mixed-port` и `MATCH` на этот узел; `cdn-cgi/trace` Cloudflare
/// обязан сказать `warp=on`. Ключи из записи не печатаются.
///
/// ```text
/// cargo test live_masque_leaves_through_warp -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_masque_leaves_through_warp() {
    let root = sandbox("masque");
    Migration::run().unwrap();
    let node = std::fs::read_dir(root.join("sources"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "raw"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_yaml::from_str::<serde_yaml::Value>(&text).ok())
        .filter_map(|doc| doc.get("proxies").and_then(|p| p.as_sequence()).cloned())
        .flatten()
        .find(|node| node.get("type").and_then(|t| t.as_str()) == Some("masque"))
        .expect("в источниках нет узла masque — сначала «Добавить» → «Cloudflare WARP»");
    let name = node
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap()
        .to_string();
    let port = crate::core::Ports::free_port().unwrap();
    let mut proxy = serde_yaml::Mapping::new();
    proxy.insert("proxies".into(), serde_yaml::Value::Sequence(vec![node]));
    let config = format!(
        "mixed-port: {port}\nmode: rule\nlog-level: silent\nexternal-ui: \"\"\n{}rules:\n  - MATCH,{name}\n",
        serde_yaml::to_string(&proxy).unwrap()
    );
    let bench = crate::diag::Bench::with(&config).await.unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).unwrap())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let mut seen = Err(String::new());
    for attempt in 0..3 {
        match client
            .get("https://www.cloudflare.com/cdn-cgi/trace")
            .send()
            .await
        {
            Ok(response) => {
                seen = Ok(response.text().await.unwrap_or_default());
                break;
            }
            Err(why) => {
                println!("попытка {attempt}: {why}");
                seen = Err(why.to_string());
            }
        }
    }
    drop(bench);
    let trace = seen.unwrap_or_else(|why| panic!("через MASQUE наружу не вышло: {why}"));
    let warp = trace
        .lines()
        .find(|line| line.starts_with("warp="))
        .unwrap_or("warp=?");
    println!("{warp}");
    assert_eq!(warp, "warp=on", "ответил Cloudflare, но не через WARP");
}
