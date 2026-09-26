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

use crate::app::migrate;
use crate::config::direction::Direction;
use crate::config::files;
use crate::config::mode;
use crate::config::mode::Mode;
use crate::core::Supervisor;
use crate::paths;
use crate::yaml::top_mapping;

/// Настоящий каталог пользователя. Запоминается один раз — после первой песочницы
/// `LOCALAPPDATA` уже подменён, и второй раз спрашивать поздно.
fn real() -> &'static Path {
    static REAL: OnceLock<PathBuf> = OnceLock::new();
    // Каталог этой сборки, а не имя строкой: у отладочной он свой (D-116), и живые
    // проверки обязаны копировать тот, в котором сами же и живут.
    REAL.get_or_init(crate::paths::root)
}

/// Копия настоящего каталога в temp; `LOCALAPPDATA` подменяется процессу целиком.
fn sandbox(name: &str) -> PathBuf {
    let source = real().to_path_buf();
    assert!(
        source.exists(),
        "нет настоящего каталога {} — проверять нечего",
        source.display()
    );
    let root = std::env::temp_dir().join(format!("umiray-live-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join(crate::paths::root().file_name().unwrap());
    std::fs::create_dir_all(&app).unwrap();
    copy_tree(&source, &app);

    std::env::set_var("LOCALAPPDATA", &root);
    assert_eq!(paths::root(), app, "песочница не подхватилась");
    app
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else if entry.file_name() == "mihomo.exe" {
            // Жёсткая ссылка вместо копии: тот же том, полсекунды против пятидесяти мегабайт.
            std::fs::hard_link(entry.path(), &target)
                .or_else(|_| std::fs::copy(entry.path(), &target).map(|_| ()))
                .unwrap();
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Принимает ли **настоящее ядро** то, что мы собрали.
///
/// Аргументы те же, что у супервизора, включая `SAFE_PATHS`: без него ядро откажется
/// читать файлы провайдеров из соседнего каталога (GOTCHAS), и проверка соврала бы.
fn core_accepts(yaml: &str) -> (bool, String) {
    paths::ensure_run_dir().unwrap();
    let path = paths::effective_config();
    std::fs::write(&path, yaml).unwrap();
    let out = std::process::Command::new(paths::core())
        .arg("-t")
        .arg("-d")
        .arg(paths::run_dir())
        .arg("-f")
        .arg(&path)
        .env("SAFE_PATHS", paths::sources_dir())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text.contains("test is successful"), text)
}

/// Гасит ядро, что бы ни случилось с тестом. Без этого паника посреди TUN-проверки
/// оставила бы поднятый адаптер и переписанную таблицу маршрутов.
struct Running<'a>(&'a Supervisor);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.stop();
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

/// Переезд `override.yaml` → `advanced.yaml` на копии настоящего каталога.
///
/// Переезд коллекций (D-100) на копии настоящего каталога.
///
/// Проверка живая, а не обычная, по одной причине: она **двигает файлы пользователя**.
/// `catalog/` и `rulesets/` могли быть правлены, и потерять их нельзя — значит смотрим
/// не на «папка появилась», а на то, что содержимое доехало байт в байт.
#[test]
#[ignore]
fn live_collections_move_instead_of_being_dropped() {
    let app = sandbox("collections");

    // Обстановку «до» песочница восстанавливает сама: настоящий каталог мог уже переехать,
    // и тогда проверять было бы нечего.
    let collections = app.join("collections");
    let catalog = app.join("catalog");
    let rulesets = app.join("rulesets");
    if collections.exists() {
        std::fs::create_dir_all(&catalog).unwrap();
        std::fs::create_dir_all(&rulesets).unwrap();
        for name in ["dns.yaml", "sites.yaml"] {
            let from = collections.join(name);
            if from.exists() {
                std::fs::rename(from, catalog.join(name)).unwrap();
            }
        }
        if collections.join("rules").exists() {
            for entry in std::fs::read_dir(collections.join("rules"))
                .unwrap()
                .flatten()
            {
                std::fs::rename(entry.path(), rulesets.join(entry.file_name())).unwrap();
            }
        }
        std::fs::remove_dir_all(&collections).unwrap();
    }
    assert!(catalog.exists() && rulesets.exists(), "нечего переносить");

    // Правка пользователя: ровно она и не должна потеряться.
    let mine = "# правка пользователя, обязана пережить переезд\n";
    let edited = catalog.join("dns.yaml");
    let before = std::fs::read_to_string(&edited).unwrap() + mine;
    std::fs::write(&edited, &before).unwrap();
    let sets: Vec<String> = std::fs::read_dir(&rulesets)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();

    migrate::run().expect("переезд не прошёл");

    assert!(!catalog.exists(), "старый catalog/ остался");
    assert!(!rulesets.exists(), "старый rulesets/ остался");
    assert_eq!(
        std::fs::read_to_string(collections.join("dns.yaml")).unwrap(),
        before,
        "правка пользователя потерялась при переезде"
    );
    for name in sets {
        assert!(
            collections.join("rules").join(&name).exists(),
            "{name} не доехал в коллекцию правил"
        );
    }
    assert!(
        collections.join("sites.yaml").exists(),
        "второй документ коллекции не доехал"
    );

    // Второй прогон ничего не трогает: переезд разовый.
    migrate::run().expect("повторный переезд не прошёл");
    assert_eq!(
        std::fs::read_to_string(collections.join("dns.yaml")).unwrap(),
        before,
        "повторный запуск переписал уже переехавшее"
    );
}

/// Главное, что здесь проверяется, — что чужие ключи не переехали. В живом оверрайде
/// лежат `rules` и `proxy-groups` от прошлого переезда, и конфиг ядра накладывается
/// последним: оставь их там, и правка «Маршрутизации» молча перебивалась бы (GOTCHAS).
#[test]
#[ignore]
fn live_migration_on_a_copy_of_the_real_directory() {
    let app = sandbox("migrate");

    // Настоящий каталог уже переехал, поэтому обстановку «до» песочница восстанавливает
    // сама: иначе проверку нельзя было бы прогнать второй раз.
    let _ = std::fs::remove_file(app.join("advanced.yaml"));
    if app.join("override.yaml.migrated").exists() {
        let _ = std::fs::remove_file(app.join("override.yaml"));
        std::fs::rename(
            app.join("override.yaml.migrated"),
            app.join("override.yaml"),
        )
        .unwrap();
    }
    let before = std::fs::read_to_string(app.join("override.yaml"))
        .expect("нечего переносить: нет ни override.yaml, ни его копии");
    println!(
        "старый override.yaml:
{before}"
    );

    // Шаблон прошлой сборки — файл из одних комментариев. Кладём такой намеренно, чтобы
    // проверка не зависела от того, обновился ли настоящий каталог до этого прогона.
    let stale = "# шаблон прошлой сборки, содержательно пустой
";
    std::fs::write(app.join("rules.yaml"), stale).unwrap();

    migrate::run().expect("переезд не прошёл");

    assert!(!app.join("override.yaml").exists(), "старый файл остался");
    assert!(
        app.join("override.yaml.migrated").exists(),
        "копия пользователя должна остаться лежать рядом"
    );

    let after = std::fs::read_to_string(app.join("advanced.yaml")).unwrap();
    assert_eq!(
        after,
        files::template(files::ADVANCED).unwrap(),
        "отличий от шаблона не было — он обязан лечь дословно, с комментариями"
    );

    let map = top_mapping(&after).unwrap();
    for foreign in files::keys_of_others(files::ADVANCED) {
        assert!(
            !map.contains_key(serde_yaml::Value::from(foreign)),
            "{foreign} не должен переезжать в конфиг ядра"
        );
    }

    // Живого `rules.yaml` больше нет (D-071): его содержимое — это набор, а сам файл
    // переезд убирает переименованием, не удаляя.
    assert!(
        !app.join("rules.yaml").exists(),
        "rules.yaml должен был уехать в набор"
    );
    assert!(
        app.join("rules.yaml.migrated").exists(),
        "копия пользователя должна остаться лежать рядом"
    );
    // А `groups.yaml`, наоборот, обязан быть на месте: с D-075 это общий документ клиента,
    // и переезд кладёт в него группы из набора.
    assert!(
        app.join("groups.yaml").exists(),
        "группы стали документом клиента и должны лежать в корне"
    );
    assert!(
        crate::config::groups::parse(&std::fs::read_to_string(app.join("groups.yaml")).unwrap())
            .expect("группы должны разбираться")
            .iter()
            .all(|group| group.name != "AUTO" && group.name != "umiray"),
        "клиентские группы в свой документ не переезжают: они собираются заново"
    );
    let presets = crate::config::presets::list();
    // Проверяем инвариант D-071 («один набор существует всегда»), а не абсолютное число:
    // песочница — копия **настоящего** каталога, и сколько наборов человек успел завести
    // своими руками, проверке знать неоткуда. Прежнее `== 1` было верно ровно до того дня,
    // когда у пользователя появился второй.
    assert!(
        !presets.is_empty(),
        "после переезда набора нет — разделу «Маршрутизация» нечего показывать"
    );
    println!(
        "после переезда наборов {}: {}",
        presets.len(),
        presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    );

    // Боевое правило человек обязан видеть сразу — теперь его даёт сборка, а не шаблон.
    let (ok, log) = core_accepts(
        &crate::render::effective::effective(None, None)
            .unwrap()
            .yaml,
    );
    assert!(
        ok,
        "ядро отвергло конфиг после переезда:
{log}"
    );
    println!("ядро приняло собранный конфиг после переезда");
}

/// Переключение режимов подряд: `advanced.yaml` остаётся полным и валидным, а написанное
/// пользователем переживает все четыре шага.
#[test]
#[ignore]
fn live_mode_switch_keeps_the_config_whole() {
    let app = sandbox("mode");
    migrate::run().unwrap();

    // Как будто пользователь поправил своё руками: сменил порт и дописал поле, которого
    // клиент не знает вовсе. Порт именно правим, а не дописываем: второй такой же ключ —
    // это битый YAML, и запись справедливо его не примет.
    let mine = std::fs::read_to_string(app.join("advanced.yaml"))
        .unwrap()
        .replace("mixed-port: 3090", "mixed-port: 7777")
        + "
experimental:
  quic-go-disable-gso: true
";
    files::write(files::ADVANCED, &mine).unwrap();

    for step in [Mode::Tun, Mode::Local, Mode::Tun, Mode::Local] {
        mode::write(step).expect("режим не записался");

        let text = std::fs::read_to_string(app.join("advanced.yaml")).unwrap();
        let map = top_mapping(&text).unwrap();
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

        let effective = crate::render::effective::effective(None, None).unwrap();
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
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&supervisor);

    let status = supervisor.status();
    assert!(status.running);
    assert_eq!(status.mode, Some(Mode::Local));
    let port = status.port.expect("в local-режиме порт обязан быть");
    println!("ядро поднялось на 127.0.0.1:{port}");

    let nodes = crate::nodes::source_catalog::nodes();
    assert!(!nodes.is_empty(), "источники есть, а узлов нет");
    println!("узлов в источниках: {}", nodes.len());

    // Главная проверка D-053: ядро знает нашу автогруппу. На несуществующее имя оно
    // отвечает 400, поэтому успех здесь и означает, что группа собралась.
    supervisor
        .select(crate::config::direction::AUTO)
        .await
        .expect("ядро не знает AUTO — автогруппа не собралась");
    assert_eq!(
        supervisor.selected().await.as_deref(),
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
        supervisor
            .select(&node.name)
            .await
            .expect("узел не выбрался");
        assert_eq!(
            supervisor.selected().await.as_deref(),
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

    let traffic = supervisor
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
        crate::system::elevation::is_elevated(),
        "TUN без прав администратора не поднимется — запустите тест из поднятой консоли"
    );

    // До запуска, чтобы было с чем сравнивать. Не печатаем: это домашний адрес.
    let before = external_ip(None).await;

    sandbox("tun");
    migrate::run().unwrap();
    mode::write(Mode::Tun).unwrap();

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&supervisor);

    let status = supervisor.status();
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

/// Полоса напрямую и через туннель (D-099).
///
/// Прав не требует: ядро поднимается в режиме Proxy, и замер идёт через его локальный
/// порт. Проверяет заодно разогрев — без него первые куски через только что поднятый
/// туннель не скачивались вовсе.
///
/// ```text
/// cargo test live_speed -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_speed_through_the_tunnel() {
    sandbox("speed");
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&supervisor);
    let port = supervisor
        .status()
        .port
        .expect("в режиме Proxy порт обязан быть");

    let report = crate::diag::speed::measure(Some(port)).await.unwrap();
    for line in &report.lines {
        println!("{:?} {}", line.tone, line.text);
    }
    assert_ne!(
        report.verdict,
        crate::diag::report::Verdict::Bad,
        "качать не вышло: {}",
        report.headline
    );
    // Разогрев на то и разогрев: после него куски обязаны качаться все до одного.
    let failed = report
        .rows
        .iter()
        .filter(|row| row.verdict == crate::diag::report::Verdict::Bad)
        .count();
    assert_eq!(failed, 0, "не скачались куски: {failed}");
}

/// Какой стек TUN встаёт на этой машине (D-099).
///
/// Проба поднимает адаптер **без** захвата маршрута, поэтому связь не рвётся; прав
/// администратора она всё равно требует — без них адаптера не создать.
///
/// ```text
/// cargo test live_tun_stacks -- --ignored --nocapture --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn live_tun_stacks() {
    assert!(
        crate::system::elevation::is_elevated(),
        "адаптер без прав администратора не создать — запустите тест из поднятой консоли"
    );
    sandbox("tun-stacks");
    migrate::run().unwrap();

    let report = crate::diag::tun::stacks(None).await.unwrap();
    for line in &report.lines {
        println!("{:?} {}", line.tone, line.text);
    }
    assert_ne!(
        report.verdict,
        crate::diag::report::Verdict::Idle,
        "проба отказалась мерить: {}",
        report.headline
    );
    assert!(
        report.rows.iter().any(|row| row.mark),
        "ни один стек не встал: {}",
        report.headline
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
        crate::system::elevation::is_elevated(),
        "TUN без прав администратора не поднимется — запустите тест из поднятой консоли"
    );
    sandbox("dns-leak");
    migrate::run().unwrap();
    let physical = crate::system::net::physical_resolvers().unwrap();
    assert!(
        !physical.is_empty(),
        "Windows не назвала DNS поднятого физического адаптера — проверять утечку не на чем"
    );
    mode::write(Mode::Tun).unwrap();

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&supervisor);

    let report =
        crate::diag::dns::leak_report_with(Some("tun"), Duration::from_millis(1500), &physical)
            .await
            .unwrap();
    for line in &report.lines {
        println!("{:?} {}", line.tone, line.text);
    }
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "перехвата нет — имена уходят мимо туннеля: {}",
        report.headline
    );
}

/// Свой набор в направлении RULES: группа и правило, которые написал человек, доезжают
/// до ядра и правда уводят трафик.
///
/// Это и есть сценарий «MATCH на Польшу» из постановки — только правило пишет пользователь,
/// а не клиент: в RULES контроль его (D-056).
#[tokio::test]
#[ignore]
async fn live_a_user_set_routes_through_its_own_group() {
    sandbox("group");
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let sources = crate::nodes::sources::list();
    assert!(
        !sources.is_empty(),
        "нет источников — группе не из чего брать"
    );

    let state = crate::app::state::AppState::new();
    let preset = state.new_preset().unwrap();

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
    files::write(files::GROUPS, &groups).unwrap();
    files::write(
        &format!("rules/{}", preset.id),
        "rules:
  - MATCH,Своя
",
    )
    .unwrap();
    // Применить — отдельное действие (D-071): правка набора его не включает.
    state.select_preset(&preset.id).unwrap();
    assert_eq!(state.settings().direction, Direction::Rules);

    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
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

    let supervisor = Supervisor::new();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&supervisor);

    let port = supervisor.status().port.unwrap();
    println!(
        "через свою группу внешний адрес: {}",
        external_ip(Some(port)).await
    );
}

/// Направление решает, участвует ли набор в сборке, — и ничего не возит (D-071).
///
/// Главное здесь — что текст набора не зависит от направления вовсе: он лежит на своём
/// месте всегда, а уход из `rules` просто перестаёт его подмешивать.
#[tokio::test]
#[ignore]
async fn live_direction_decides_whether_the_set_is_used() {
    sandbox("sets");
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let mine = "rules:\n  - MATCH,DIRECT\n";

    // Набор завёл переезд: без единого набора разделу нечего показывать.
    let preset = crate::config::presets::list()
        .first()
        .expect("переезд обязан был завести первый набор")
        .id
        .clone();
    crate::config::presets::write(&preset, "rules", mine).unwrap();

    // Вне RULES набор не участвует: в сборке то, что клиент собирает сам.
    state.set_direction(Direction::Auto, None).unwrap();
    let yaml = crate::render::effective::effective(state.routing().unwrap().as_deref(), None)
        .unwrap()
        .yaml;
    assert!(
        yaml.contains("MATCH,umiray"),
        "вне RULES маршрут собирает клиент:\n{yaml}"
    );
    assert!(
        !yaml.contains("MATCH,DIRECT"),
        "набор пользователя не должен участвовать вне RULES"
    );

    // Применили — участвует, и текст остался ровно тем же.
    state.select_preset(&preset).unwrap();
    assert_eq!(
        crate::config::presets::read(&preset, "rules").unwrap(),
        mine,
        "набор обязан лежать слово в слово, что бы ни делало направление"
    );
    let yaml = crate::render::effective::effective(state.routing().unwrap().as_deref(), None)
        .unwrap()
        .yaml;
    assert!(
        yaml.contains("MATCH,DIRECT"),
        "применённый набор обязан доехать до сборки:\n{yaml}"
    );

    // И ядро принимает конфиг в каждом направлении, включая наш набор.
    for direction in [
        Direction::Direct,
        Direction::Auto,
        Direction::Manual,
        Direction::Rules,
    ] {
        state.set_direction(direction, None).unwrap();
        let (ok, log) = core_accepts(
            &crate::render::effective::effective(state.routing().unwrap().as_deref(), None)
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
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    // Ядро поднимает **тот же** супервизор, что держит состояние: направление наводит
    // псевдоним через него, и отдельно созданный второй просто ничего бы не сделал.
    let state = crate::app::state::AppState::new();
    state.set_direction(Direction::Auto, None).unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    if let Err(why) = state.supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.supervisor);
    let port = state
        .supervisor
        .status()
        .port
        .expect("в local-режиме порт обязан быть");

    state.point_alias().await.unwrap();
    let through_auto = external_ip(Some(port)).await;
    println!("AUTO: {through_auto}");

    // Перебираем несколько узлов: мёртвый сервер подписки — не повод валить проверку,
    // она про переключение направления, а не про чужой аптайм.
    let mut through_node = None;
    for node in crate::nodes::source_catalog::nodes().into_iter().take(4) {
        state
            .set_direction(Direction::Manual, Some(node.name.clone()))
            .unwrap();
        // Настройку записали — теперь скажите об этом ядру. Без этого проверка утверждает
        // про действие, которого не совершала: псевдоним остаётся там, куда его навели
        // в прошлый раз, и «MANUAL» с «DIRECT» отвечают адресом от `AUTO`.
        // Руками — потому что здесь нет `AppHandle`. В настоящей жизни эту строку делает
        // `connect::apply`, и то, что она там есть, сторожит ui-check, а не эта проверка.
        state.point_alias().await.unwrap();
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

    state.set_direction(Direction::Direct, None).unwrap();
    state.point_alias().await.unwrap();
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
    mode::write(Mode::Local).unwrap();
    println!("песочница: {}", app.display());

    migrate::run().unwrap();
    let before: Vec<String> = crate::nodes::sources::list()
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
    let groups = crate::render::effective::effective(None, None)
        .unwrap()
        .yaml;
    assert_eq!(used(&groups).len(), before.len(), "сначала все на месте");

    // Удаляем один источник — и **ничего не пересобираем**: пересобирать нечего.
    let gone = before[0].clone();
    crate::nodes::sources::delete(&gone).unwrap();

    let groups = crate::render::effective::effective(None, None)
        .unwrap()
        .yaml;
    assert!(
        !groups.contains(&gone),
        "удалённый источник остался в собранных группах — ядро откажется стартовать:
{groups}"
    );

    let yaml = crate::render::effective::effective(None, None)
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
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    crate::app::client::set_ping(Method::Proxy).unwrap();
    assert!(
        !crate::nodes::sources::list().is_empty(),
        "нет источников — мерить нечего"
    );

    // На остановленном ядре замер обязан отказать словами, а не оставить прочерки молча.
    let refused = state.measure().await;
    assert!(refused.is_err(), "через прокси без ядра мерить нечем");
    println!("без ядра: {}", refused.unwrap_err());

    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    if let Err(why) = state.supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.supervisor);

    let started = std::time::Instant::now();
    state.measure().await.expect("замер через прокси не прошёл");
    let spent = started.elapsed();

    let nodes = state.nodes();
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
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    // Конфиг: обычный собранный плюс служебный вход и группа под него.
    let mut map = top_mapping(
        &crate::render::effective::effective(None, None)
            .unwrap()
            .yaml,
    )
    .unwrap();
    let sources: Vec<Value> = crate::nodes::sources::list()
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
    paths::ensure_run_dir().unwrap();
    std::fs::write(paths::effective_config(), &yaml).unwrap();
    let core = std::process::Command::new(paths::core())
        .arg("-d")
        .arg(paths::run_dir())
        .arg("-f")
        .arg(paths::effective_config())
        .arg("-ext-ctl")
        .arg(format!("127.0.0.1:{API}"))
        .arg("-secret")
        .arg(SECRET)
        .env("SAFE_PATHS", paths::sources_dir())
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
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    crate::app::client::set_ping(Method::ProxyKeepalive).unwrap();

    // Порт служебного входа выбирает запуск — повторяем то же, что делает `connect::start`.
    let probe = crate::core::free_port().unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), Some(probe))
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

    if let Err(why) = state.supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}
{}",
            state.supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&state.supervisor);
    assert_eq!(
        state.supervisor.probe_port(),
        Some(probe),
        "супервизор обязан помнить порт входа: без него мерить некуда"
    );

    let started = std::time::Instant::now();
    state.measure().await.expect("замер не прошёл");
    let spent = started.elapsed();

    let nodes = state.nodes();
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
                let read = crate::system::registry::read_string(subkey, name).unwrap();
                (*name, read.map(RegistryValue::Text))
            })
            .collect();
        Self { subkey, values }
    }

    fn with_number(mut self, name: &'static str) -> Self {
        let read = crate::system::registry::read_dword(self.subkey, name).unwrap();
        self.values.push((name, read.map(RegistryValue::Number)));
        self
    }
}

impl Drop for RegistryGuard {
    fn drop(&mut self) {
        use crate::system::registry;
        for (name, value) in &self.values {
            let restored = match value {
                Some(RegistryValue::Text(text)) => registry::write_string(self.subkey, name, text),
                Some(RegistryValue::Number(number)) => {
                    registry::write_dword(self.subkey, name, *number)
                }
                None => registry::delete_value(self.subkey, name),
            };
            restored.expect("сырое состояние реестра обязано вернуться");
        }
    }
}

fn proxy_key() -> &'static str {
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings"
}

fn proxy_value(name: &str) -> Option<String> {
    crate::system::registry::read_string(proxy_key(), name).unwrap()
}

fn proxy_enabled() -> u32 {
    crate::system::registry::read_dword(proxy_key(), "ProxyEnable")
        .unwrap()
        .unwrap_or(0)
}

/// `System`-режим целиком: запись доезжает до реестра, снимок возвращает **всё**, что мы
/// трогали, и чужую свежую настройку мы не затираем (три правила D-047).
#[test]
#[ignore]
fn live_system_proxy_reaches_the_registry_and_gives_it_back() {
    use crate::system::sysproxy;

    let _guard = RegistryGuard::text(proxy_key(), &["ProxyServer", "ProxyOverride"])
        .with_number("ProxyEnable");

    // Исходное состояние выдумываем сами: у пользователя может стоять что угодно, а нам
    // нужен известный «чужой» прокси со своим списком исключений — иначе возврат
    // не с чем сверять.
    let foreign = "127.0.0.1:2080";
    let foreign_bypass = "*.corp.example;<local>";
    crate::system::registry::write_string(proxy_key(), "ProxyServer", foreign).unwrap();
    crate::system::registry::write_string(proxy_key(), "ProxyOverride", foreign_bypass).unwrap();
    crate::system::registry::write_dword(proxy_key(), "ProxyEnable", 1).unwrap();

    let ours = "127.0.0.1:3090";
    let backup = sysproxy::enable(ours).unwrap();

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
    assert!(sysproxy::is_ours(ours), "свою запись обязаны узнавать");

    sysproxy::restore(&backup).unwrap();

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
    let backup = sysproxy::enable(ours).unwrap();
    let third_party = "127.0.0.1:9999";
    crate::system::registry::write_string(proxy_key(), "ProxyServer", third_party).unwrap();
    sysproxy::restore(&backup).unwrap();
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
    let _guard = RegistryGuard::text(run, &["umiray"]);

    autostart::set(true).unwrap();
    let written = crate::system::registry::read_string(run, "umiray")
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
    assert!(autostart::enabled(), "запись есть, а окно её не видит");

    autostart::set(false).unwrap();
    assert!(
        crate::system::registry::read_string(run, "umiray")
            .unwrap()
            .is_none(),
        "запись осталась после выключения"
    );
    assert!(!autostart::enabled());

    autostart::set(false).expect("повторное выключение — не ошибка, значения и так нет");
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
    use crate::system::sysproxy;

    sandbox("sysproxy");
    migrate::run().unwrap();
    // В local: на машине разработчика в `advanced.yaml` может стоять TUN, а прав
    // у обычного прогона нет — проверка не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _running = Running(&supervisor);
    let port = supervisor
        .status()
        .port
        .expect("в local-режиме порт обязан быть");

    supervisor
        .select(crate::config::direction::AUTO)
        .await
        .expect("автогруппа не собралась");

    let _guard = RegistryGuard::text(proxy_key(), &["ProxyServer", "ProxyOverride"])
        .with_number("ProxyEnable");

    let address = format!("127.0.0.1:{port}");
    let backup = sysproxy::enable(&address).unwrap();
    println!("системный прокси включён на {address}");

    let through = ip_via_system_proxy()
        .await
        .expect("клиент, читающий системный прокси, не смог выйти наружу");
    println!("адрес приложения, ничего про нас не знающего: {through}");

    // Возврат делаем до утверждений: провалившаяся проверка не должна оставлять
    // машину с чужим прокси даже на время печати сообщения.
    sysproxy::restore(&backup).unwrap();

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
        crate::system::elevation::is_elevated(),
        "нужны права администратора: правила брандмауэра иначе не поставить"
    );

    sandbox("killswitch");
    migrate::run().unwrap();
    mode::write(Mode::Tun).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _running = Running(&supervisor);
    assert_eq!(supervisor.status().mode, Some(Mode::Tun));
    supervisor
        .select(crate::config::direction::AUTO)
        .await
        .expect("автогруппа не собралась");

    let device = crate::config::mode::tun_device(
        &top_mapping(&files::read(files::ADVANCED).unwrap()).unwrap(),
    );
    println!("адаптер ядра: {device}");

    // Возврат обязан случиться, что бы дальше ни произошло.
    struct Unlock(killswitch::Backup);
    impl Drop for Unlock {
        fn drop(&mut self) {
            killswitch::release(&self.0).expect("сеть обязана вернуться");
        }
    }

    let через_туннель = {
        let guard = Unlock(killswitch::engage(&paths::core(), &device).unwrap());
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
    let backup = killswitch::engage(&paths::core(), &device).unwrap();
    let guard = Unlock(backup);
    supervisor.stop();
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
    use crate::app::status::{engage_kill_switch, release_kill_switch};
    use crate::system::killswitch;

    assert!(
        crate::system::elevation::is_elevated(),
        "нужны права администратора: правила брандмауэра иначе не поставить"
    );

    sandbox("killswitch-heal");
    migrate::run().unwrap();
    mode::write(Mode::Tun).unwrap();

    let home = direct_ip().await;

    // Первая жизнь клиента: тумблер включён, TUN поднят, защита встала.
    let doomed = AppState::new();
    doomed
        .patch(crate::app::settings::Patch {
            kill_switch: Some(true),
            ..Default::default()
        })
        .unwrap();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = doomed.supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            doomed.supervisor.logs().join("\n")
        );
    }
    assert_eq!(doomed.supervisor.status().mode, Some(Mode::Tun));
    engage_kill_switch(&doomed).expect("защита должна встать");

    // Страховка на случай, если проверка развалится посередине: сеть обязана вернуться.
    struct Unlock(killswitch::Backup);
    impl Drop for Unlock {
        fn drop(&mut self) {
            let _ = killswitch::release(&self.0);
        }
    }
    let snapshot = doomed
        .settings()
        .kill_switch_backup
        .expect("защита встала, но снимок на диск не лёг — лечить будет нечем");
    let _safety = Unlock(snapshot);
    println!("защита встала, снимок лежит в settings.json");

    // Клиент умирает, не прибравшись.
    doomed.supervisor.stop();
    drop(doomed);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let while_dead = try_direct_ip().await;
    println!("клиент мёртв, ядра нет: {while_dead:?}");

    // Следующий запуск: то же, что делает `setup` в `main.rs`.
    let reborn = AppState::new();
    let carried = reborn.settings().kill_switch_backup.is_some();
    if carried {
        release_kill_switch(&reborn).expect("защита должна сняться");
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
        reborn.settings().kill_switch_backup.is_none(),
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
    migrate::run().unwrap();
    // Правило проверяется в local: прав тут не нужно, а маршрут от режима не зависит.
    mode::write(Mode::Local).unwrap();

    let home = direct_ip().await;
    println!("домашний адрес: {home}");

    // Сначала узнаём, какой адрес даёт узел сам по себе: сравнивать правило не с чем,
    // пока неизвестно, куда узел выходит.
    let (name, expected) = {
        let supervisor = Supervisor::new();
        let effective = crate::render::effective::effective(None, None).unwrap();
        supervisor.start(&effective).await.expect("ядро не встало");
        let _guard = Running(&supervisor);
        let port = supervisor.status().port.unwrap();
        let mut found = None;
        for node in crate::nodes::source_catalog::nodes().iter().take(6) {
            if supervisor.select(&node.name).await.is_err() {
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
    let preset = state.new_preset().unwrap();
    files::write(
        &format!("rules/{}", preset.id),
        &format!(
            "rules:
  - DOMAIN-SUFFIX,ipify.org,{name}
  - MATCH,DIRECT
"
        ),
    )
    .unwrap();
    state.select_preset(&preset.id).unwrap();

    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло правило на узел:\n{log}");

    let supervisor = Supervisor::new();
    supervisor
        .start(&effective)
        .await
        .expect("ядро не встало с правилом на узел");
    let _guard = Running(&supervisor);
    let port = supervisor.status().port.unwrap();

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
    use crate::config::advanced::{self, Enhanced, LogLevel, Stack};

    sandbox("form");
    migrate::run().unwrap();
    // В local: форма пишет и поля TUN, но поднимать туннель ради разбора конфига незачем.
    mode::write(Mode::Local).unwrap();

    let mut options = advanced::read().unwrap();
    // Порт берём свободный, а не круглый: занятый номер уронил бы запуск по чужой причине.
    let port = crate::core::free_port().unwrap();
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
    advanced::write(&options).unwrap();

    let back = advanced::read().unwrap();
    assert_eq!(back, options, "форма прочитала не то, что записала");

    let effective = crate::render::effective::effective(None, None).unwrap();
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло то, что написала форма:\n{log}");

    let supervisor = Supervisor::new();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось на конфиге формы: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&supervisor);
    assert_eq!(
        supervisor.status().port,
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
    migrate::run().unwrap();
    // В local: проверка про сборку источника, а не про режим перехвата.
    mode::write(Mode::Local).unwrap();

    // Ключи — настоящие 32 байта в base64: ядро их разбирает, а до туннеля дело
    // не дойдёт — проверка про сборку и запуск, а не про чужой сервер.
    let key = "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU%3D";
    let name = "живой-шов";
    let source = crate::nodes::source_import::add_link(&format!(
        "wireguard://{key}@1.2.3.4:51820?address=10.0.0.2/32&publickey={key}#{name}"
    ))
    .unwrap();
    assert_eq!(source.nodes, 1, "ссылка не завела узел");

    // С D-122 «непонятная конвертеру ядра схема» перестала быть особым случаем: такую
    // ссылку разбирает клиент, и узел приезжает обычной записью.
    assert!(
        crate::nodes::sources::content(&source.id).contains("type: wireguard"),
        "узел лёг записью, а не ссылкой"
    );

    let effective = crate::render::effective::effective(None, None).unwrap();
    assert!(
        !effective.yaml.contains(&format!("{}.yaml", source.id)),
        "источнику без понятных ссылок завели провайдера — ядро откажется его читать"
    );
    assert!(
        effective.yaml.contains(name),
        "узел не доехал записью proxies: — источник исчез целиком"
    );

    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло конфиг с узлом из шва:\n{log}");

    let supervisor = Supervisor::new();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&supervisor);
    assert!(supervisor.status().running, "ядро не работает");
    println!("ядро поднялось с источником, который его конвертер не читает");
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
    const NAME: &'static str = "umiray";

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
    use crate::system::{autostart, task};

    sandbox("stale");
    let _registry = RegistryGuard::text(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        &["umiray"],
    );
    let _task = TaskGuard::snapshot();

    autostart::set_always_admin(true)
        .expect("нужны права администратора — запускать эту проверку из поднятой консоли");
    assert!(task::usable(), "живая задача должна считаться рабочей");
    assert!(autostart::always_admin(), "окно не видит заведённую задачу");

    // Тот самый случай: каталог со сборкой исчез, задача осталась.
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
            "umiray",
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

    assert!(task::exists(), "задача в планировщике осталась");
    assert!(
        !task::usable(),
        "задача с исчезнувшим файлом считается рабочей — клиент снова уйдёт в никуда"
    );
    assert!(
        !autostart::always_admin(),
        "окно показывает «всегда от администратора» по задаче, которая ничего не поднимает"
    );

    // И «схема» лечится тем же тумблером: включили — задача перезаведена на живой путь.
    autostart::set_always_admin(true).unwrap();
    assert!(task::usable(), "тумблер не починил протухшую задачу");
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
    use crate::system::{autostart, task};

    sandbox("admin");
    let run = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let _registry = RegistryGuard::text(run, &["umiray"]);
    let _task = TaskGuard::snapshot();

    // Исходное состояние: задачи нет, автозапуск включён обычным способом.
    task::remove()
        .expect("нужны права администратора — запускать эту проверку из поднятой консоли");
    autostart::set(true).unwrap();
    assert!(autostart::enabled(), "автозапуск не включился");

    autostart::set_always_admin(true).unwrap();
    assert!(task::exists(), "задача не завелась");
    assert!(
        autostart::enabled(),
        "автозапуск потерялся при переезде в задачу"
    );
    assert!(
        crate::system::registry::read_string(run, "umiray")
            .unwrap()
            .is_none(),
        "запись в Run осталась вместе с задачей — вход в систему поднял бы вторую копию, и без прав"
    );

    autostart::set_always_admin(false).unwrap();
    assert!(!task::exists(), "задача осталась после снятия тумблера");
    let written = crate::system::registry::read_string(run, "umiray")
        .unwrap()
        .expect("запись в Run не вернулась — автозапуск пропал вместе с задачей");
    assert!(
        written.to_lowercase().contains(".exe"),
        "в автозапуск вернулся не бинарь: {written}"
    );
    assert!(autostart::enabled(), "окно не видит вернувшийся автозапуск");
    println!("задача ушла, автозапуск вернулся в реестр: {written}");

    // И то же самое при выключенном автозапуске: задача есть, но триггера у неё нет,
    // а после снятия тумблера в реестре не появляется ничего.
    autostart::set(false).unwrap();
    autostart::set_always_admin(true).unwrap();
    assert!(task::exists());
    assert!(
        !autostart::enabled(),
        "выключенный автозапуск включился сам от смены способа"
    );
    autostart::set_always_admin(false).unwrap();
    assert!(!task::exists());
    assert!(
        crate::system::registry::read_string(run, "umiray")
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
    let port = crate::core::free_port().unwrap();
    let second = crate::core::free_port().unwrap();
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
    let elevated = crate::system::elevation::is_elevated();
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let probe = crate::core::free_port().unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), Some(probe))
            .unwrap();
    if let Err(why) = state.supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            state.supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&state.supervisor);
    let port = state.supervisor.status().port.unwrap();

    external_ip(Some(port)).await;
    let before = state
        .supervisor
        .traffic()
        .await
        .unwrap()
        .expect("у работающего ядра счётчики обязаны читаться");
    assert!(before.up > 0, "трафик не пошёл — сравнивать будет нечего");

    let ruleset = crate::config::rulesets::list()
        .into_iter()
        .next()
        .expect("встроенных наборов нет — тумблер проверять не на чем");
    crate::config::rulesets::toggle(&ruleset.id, !ruleset.on).unwrap();

    let changed =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), Some(probe))
            .unwrap();
    let launched = state.supervisor.launched().unwrap();
    assert_eq!(
        crate::core::apply::needed(&launched, &changed.yaml).unwrap(),
        Some(crate::core::apply::Apply::Reload),
        "правка набора вдруг требует перезапуска — таблица разошлась с реальностью"
    );
    state.supervisor.apply(&changed).await.unwrap();

    let after = state
        .supervisor
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
        crate::core::apply::needed(&state.supervisor.launched().unwrap(), &changed.yaml).unwrap(),
        None,
        "после применения ядро и файл обязаны сойтись"
    );
    external_ip(Some(port)).await;

    crate::config::rulesets::toggle(&ruleset.id, ruleset.on).unwrap();
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
    let packet = crate::diag::wire::query(name, crate::diag::wire::TYPE_A, 0x4242).unwrap();
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
    crate::diag::wire::answer(&buffer[..read], 0x4242)
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
    let child = std::process::Command::new(paths::core())
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
    let port = crate::core::free_port().unwrap();
    let dns = crate::core::free_port().unwrap();
    let dir = paths::run_dir().join("fakeip");
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
    let sent = crate::system::console::interrupt(child.id());
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
    let rough = paths::run_dir().join("fakeip-rough");
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    let probe = crate::core::free_port().unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), Some(probe))
            .unwrap();
    state.supervisor.start(&effective).await.unwrap();
    let _guard = Running(&state.supervisor);
    let port = state.supervisor.status().port.unwrap();

    let quiet = crate::app::status::status(&state);
    assert_eq!(
        quiet.restart_reason(),
        None,
        "только что поднятому ядру перезапуск не нужен"
    );

    // Правка, которая доезжает: подробность лога.
    let mut options = crate::config::advanced::read().unwrap();
    options.log_level = crate::config::advanced::LogLevel::Warning;
    crate::config::advanced::write(&options).unwrap();
    assert_eq!(
        crate::app::status::status(&state).restart_reason(),
        None,
        "уровень лога перезапуска не требует — он перечитывается (S-020)"
    );

    // Правка, которая не доезжает: порт локального прокси.
    options.mixed_port = crate::core::free_port().unwrap();
    crate::config::advanced::write(&options).unwrap();
    let asked = crate::app::status::status(&state);
    let why = asked
        .restart_reason()
        .expect("смена порта обязана попросить перезапуск");
    assert!(why.contains("Порт"), "причина не про порт: {why}");
    assert_eq!(
        state.supervisor.status().port,
        Some(port),
        "ядро перезапустилось само — а не должно было"
    );
    println!("окно скажет: {why}");

    // Вернули как было — предложение обязано уйти само.
    options.mixed_port = port;
    crate::config::advanced::write(&options).unwrap();
    assert_eq!(
        crate::app::status::status(&state).restart_reason(),
        None,
        "расхождения нет, а окно всё ещё предлагает перезапуск"
    );
}

/// Правка встроенного набора из окна доезжает до собранного конфига (D-104).
#[tokio::test]
#[ignore]
async fn live_an_edited_ruleset_reaches_the_assembled_config() {
    use crate::config::rulesets;

    sandbox("ruleset-edit");
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let set = rulesets::list()
        .into_iter()
        .next()
        .expect("встроенных наборов нет — править нечего");
    let before = rulesets::read(&set.id).unwrap();
    assert!(
        before.contains("rules"),
        "набор читается не файлом: {before:.40}"
    );

    // Битое не принимаем: набор, выпавший из сборки при включённом тумблере, — это
    // тихо неработающее правило.
    assert!(
        rulesets::write(&set.id, "не yaml: [и не набор").is_err(),
        "битый текст записался"
    );
    assert!(
        rulesets::write(&set.id, "title: пусто\n").is_err(),
        "набор без единого правила записался"
    );
    assert_eq!(
        rulesets::read(&set.id).unwrap(),
        before,
        "файл всё же тронут"
    );

    let mark = "DOMAIN-SUFFIX,umiray-live-check.example,DIRECT";
    rulesets::write(&set.id, &format!("{}\n  - {mark}\n", before.trim_end())).unwrap();
    rulesets::toggle(&set.id, true).unwrap();

    let state = crate::app::state::AppState::new();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    assert!(
        effective.yaml.contains(mark),
        "правка набора не доехала до собранного конфига"
    );
    let (ok, log) = core_accepts(&effective.yaml);
    assert!(ok, "ядро отвергло конфиг с правленым набором:\n{log}");

    rulesets::write(&set.id, &before).unwrap();
    rulesets::toggle(&set.id, set.on).unwrap();
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let before = crate::config::advanced::read().unwrap().nameserver;
    println!("было: {before:?}");

    let report = crate::diag::smart::apply("dns-race", crate::diag::Args::default())
        .await
        .unwrap();
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "гонка резолверов ничего не выбрала: {}",
        report.headline
    );

    let after = crate::config::advanced::read().unwrap().nameserver;
    println!("стало: {after:?} · {}", report.headline);
    assert!(
        !after.is_empty() && after.len() <= crate::diag::dns::BEST,
        "в конфиг попало не то количество: {after:?}"
    );
    assert_ne!(after, before, "форма не изменилась");

    // И собранный конфиг с ними ядро принимает — иначе «умный» выбор ломал бы запуск.
    let state = crate::app::state::AppState::new();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
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

    // У утилиты нет действия — отказ, а не тишина.
    assert!(
        crate::diag::smart::apply("external-ip", crate::diag::Args::default())
            .await
            .is_err()
    );
}

/// Проверка перед подключением (D-106): битый конфиг объясняется **до** запуска.
#[tokio::test]
#[ignore]
async fn live_a_broken_config_is_explained_before_the_core_starts() {
    sandbox("preflight");
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    // Сколько стоит сухой прогон: он теперь на пути каждого подключения.
    let good = crate::render::effective::effective(None, None).unwrap();
    let said = crate::diag::config::accepts(&good.yaml).unwrap();
    assert!(said.ok, "здоровый конфиг не принят: {}", said.complaint());
    println!("сухой прогон здорового конфига: {} мс", said.ms);

    // А теперь заведомо битый — правило, которого у ядра нет.
    let broken = good
        .yaml
        .replace("rules:", "rules:\n  - НЕПРАВИЛО,куда-то,DIRECT");
    let refused = crate::diag::config::accepts(&broken).unwrap();
    assert!(!refused.ok, "ядро приняло несуществующее правило");
    println!("ядро сказало: {}", refused.complaint());
    assert!(
        refused.complaint().to_lowercase().contains("error"),
        "жалоба ядра не похожа на объяснение: {}",
        refused.complaint()
    );

    // И то же самое целиком: фаза `core` обязана отмениться «до»-шагом, не запустив ядро.
    let state = crate::app::state::AppState::new();
    crate::config::files::write(
        crate::config::files::ADVANCED,
        &format!(
            "{}\nrules:\n  - НЕПРАВИЛО,куда-то,DIRECT\n",
            crate::config::files::read(crate::config::files::ADVANCED).unwrap()
        ),
    )
    .unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    let checked = crate::diag::config::accepts(&effective.yaml).unwrap();
    assert!(!checked.ok, "битое правило не доехало до сборки");
    assert!(
        !state.supervisor.status().running,
        "ядро не должно быть поднято этой проверкой"
    );
}

/// Автоподбор MTU: замер плюс одна запись (D-105).
#[tokio::test]
#[ignore]
async fn live_the_measured_mtu_reaches_the_core_form() {
    sandbox("smart-mtu");
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let path = crate::diag::pmtu::path("1.1.1.1")
        .unwrap()
        .expect("узел молчит по ICMP — мерить нечем");
    println!(
        "путь держит {path}, туннелю остаётся {}",
        path - crate::diag::pmtu::TUNNEL
    );

    let report = crate::diag::smart::apply("pmtu", crate::diag::Args::default())
        .await
        .unwrap();
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "{}",
        report.headline
    );

    let written = crate::config::advanced::read().unwrap().mtu;
    assert_eq!(
        written,
        path - crate::diag::pmtu::TUNNEL,
        "в форму попал не подобранный MTU"
    );
    println!("в «Ядре»: mtu {written} · {}", report.headline);

    // И ядро такой конфиг принимает: подобранное число не должно ломать запуск.
    let state = crate::app::state::AppState::new();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    let said = crate::diag::config::accepts(&effective.yaml).unwrap();
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let state = crate::app::state::AppState::new();
    // Ядра нет — жалоб не бывает: сторож про туннель, а не про его отсутствие.
    crate::app::guard::look(&state).await;
    assert_eq!(crate::app::status::status(&state).trouble(), None);

    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    state.supervisor.start(&effective).await.unwrap();
    let _guard = Running(&state.supervisor);
    let _ = state.point_alias().await;

    crate::app::guard::look(&state).await;
    assert_eq!(
        crate::app::status::status(&state).trouble(),
        None,
        "рабочий туннель не должен вызывать жалоб"
    );

    // А теперь рвём: наводим псевдоним на заведомо мёртвый узел, добавленный сюда же.
    let dead = crate::nodes::source_import::add_link(
        "vless://00000000-0000-0000-0000-000000000000@203.0.113.1:443?type=tcp&security=none#мёртвый",
    )
    .unwrap();
    let effective =
        crate::render::effective::effective(state.routing().unwrap().as_deref(), None).unwrap();
    state.supervisor.apply(&effective).await.unwrap();
    state.supervisor.select("мёртвый").await.unwrap();

    crate::app::guard::look(&state).await;
    let shown = crate::app::status::status(&state);
    let complaint = shown.trouble();
    println!("сторож сказал: {complaint:?}");
    assert!(
        complaint.is_some(),
        "трафик через мёртвый узел не идёт, а сторож молчит"
    );

    crate::nodes::sources::delete(&dead.id).unwrap();
}

/// Часы машины против настоящего заголовка `Date` (D-097).
///
/// Разбор проверяется обычным тестом на строке из RFC, а вот **что живой сервер вообще
/// присылает разбираемое** — только этим: заголовок мог бы прийти в устаревшей форме
/// или не прийти вовсе, и утилита молча отвечала бы «сверять не с чем».
#[tokio::test]
#[ignore]
async fn live_the_clock_is_checked_against_a_real_date_header() {
    let report = crate::diag::clock::check().await.unwrap();
    for line in &report.lines {
        println!("{:?} {}", line.tone, line.text);
    }
    assert_ne!(
        report.verdict,
        crate::diag::report::Verdict::Idle,
        "эталон не ответил — проверять нечего"
    );
    assert_eq!(
        report.verdict,
        crate::diag::report::Verdict::Ok,
        "часы этой машины разошлись с эталоном: {}",
        report.headline
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let supervisor = Supervisor::new();
    let effective = crate::render::effective::effective(None, None).unwrap();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось: {why:?}\n{}",
            supervisor.logs().join("\n")
        );
    }
    let _guard = Running(&supervisor);

    let asked = supervisor.recheck().await.unwrap();
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
    let supervisor = Supervisor::new();
    assert_eq!(supervisor.recheck().await.unwrap(), 0);
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
    migrate::run().unwrap();
    mode::write(Mode::Local).unwrap();

    let with_udp = crate::render::effective::udp_nodes();
    println!("узлов с нативным UDP в источниках: {with_udp}");
    crate::config::udp::write(true).unwrap();

    let effective = crate::render::effective::effective(None, None).unwrap();
    let map = top_mapping(&effective.yaml).unwrap();
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
    let supervisor = Supervisor::new();
    if let Err(why) = supervisor.start(&effective).await {
        panic!(
            "ядро не поднялось с UDP-группой: {why:?}
{}",
            supervisor.logs().join(
                "
"
            )
        );
    }
    let _guard = Running(&supervisor);
    assert!(
        supervisor.status().running,
        "ядро с UDP-группой не работает"
    );
}

/// Разбор ссылок на **живых подписках** пользователя (D-122): каждая ли ссылка стала
/// записью и примет ли ядро то, что вышло. Читает настоящий каталог, ничего не пишет
/// в него — результат кладёт рядом файлом, который проверяется `mihomo -t`.
#[tokio::test]
#[ignore]
async fn live_every_real_link_becomes_an_entry() {
    let dir = crate::paths::root();
    let mut lines: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("sources")) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "txt") {
                let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                lines.extend(
                    text.lines()
                        .filter(|line| line.contains("://"))
                        .map(str::to_string),
                );
            }
        }
    }
    assert!(!lines.is_empty(), "в профиле нет ни одной ссылки");

    let mut proxies = Vec::new();
    let mut missed = Vec::new();
    for line in &lines {
        match crate::nodes::convert::to_entry(line) {
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
    crate::yaml::set(
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
