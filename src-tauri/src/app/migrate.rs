//! Разовый переезд со старой раскладки — и приведение каталога к тому, что ожидает
//! текущая сборка (D-025).
//!
//! Было: один `config.yaml`, в котором лежало всё сразу — и серверы от подписки, и правила,
//! написанные руками. Стало: серверы — данные источника, остальное — оверрайд пользователя.
//!
//! Правило разделения ровно то же, по которому файл и наполнялся: ключ `proxies` пишет
//! подписка, всё прочее пишет человек. Поэтому переезд **ничего не теряет**: каждый ключ
//! попадает ровно в один из двух файлов, а сборка склеивает их обратно.

use serde_yaml::Value;

use crate::app::settings::Settings;
use crate::app::settings::SettingsStore;
use crate::config::direction::Direction;
use crate::config::files;
use crate::config::files::Documents;
use crate::config::presets::PresetStore;
use crate::error::{AppError, Result};
use crate::nodes::source_editor::SourceEditor;
use crate::nodes::source_import::SourceImporter;
use crate::nodes::sources::SourceStore;
use crate::yaml::Yaml;

/// Пересобрать источники из сырья (D-122).
///
/// Не «переезд по флагу», а пересборка производного: файл, который читает ядро, — это
/// разбор сырья текущей сборкой, и меняется он вместе с разбором. Флаг версии здесь
/// был бы лишней сущностью: пересборка идёт из того же сырья и повторный запуск ничего
/// не меняет, а научившийся чему-то новому разбор доезжает сам.
///
/// Источники записей не трогаем: у них сырьё и есть документ, разбирать нечего.
/// Отказ на одном не уносит остальные — испорченный файл не повод не открыть окно.
fn reparse_sources() -> Result<()> {
    for source in SourceStore::list()
        .into_iter()
        .filter(|source| !source.records)
    {
        if let Err(why) = SourceEditor::reparse(&source.id) {
            eprintln!("источник «{}» не пересобрался: {why}", source.name);
        }
    }
    Ok(())
}

pub struct Migration;

impl Migration {
    /// Вызывается один раз при старте. Уже переехавшую установку не трогает.
    pub fn run() -> Result<()> {
        // Коллекции: сперва переезд со старой раскладки, потом раздача (D-100). Порядок
        // важен — раздача пропускает уже существующую папку, и переехавшее она не тронет.
        adopt_collections()?;
        crate::collections::Collections::seed()?;
        crate::collections::Collections::adopt_rule_titles()?;
        rename_override()?;
        refresh_stale_templates()?;
        adopt_routing_files()?;
        adopt_preset_groups()?;
        ensure_first_preset()?;
        // Настройки прошлой версии читаем **до** `adopt_direction`: он переписывает файл
        // на текущую схему, и после него ни режима, ни адреса подписки взять уже неоткуда.
        let old = SettingsStore::load_v1();
        adopt_direction()?;
        reparse_sources()?;

        let legacy = crate::paths::Paths::legacy_config();
        if !legacy.exists() {
            return Ok(());
        }

        let (profile_proxies, overrides) = split(&std::fs::read_to_string(&legacy)?)?;
        let overrides = repoint_rules(&overrides, &profile_proxies)?;

        // Конфиг ядра не перезаписываем: если оно уже есть, значит переезд когда-то состоялся,
        // а старый файл остался лежать рядом.
        if !crate::paths::Paths::advanced().exists() {
            seed_advanced(&overrides)?;
        }

        // Направление пересчитываем: `adopt_direction` выше смотрел на каталог, в котором
        // источников ещё не было, и честно получил `direct`. Здесь уже видно, что переезжает,
        // а `direct` при живой подписке означал бы молча выключить VPN тому, у кого он работал.
        let fresh = Settings {
            direction: adopted(false, !profile_proxies.is_empty()),
            ..Settings::default()
        };
        SettingsStore::save(&fresh)?;
        // Режим переехал из настроек в конфиг ядра (D-052) — туда его и переносим.
        if let Some(old) = old.as_ref() {
            crate::config::mode::Mode::write(old.mode)?;
        }

        if !profile_proxies.is_empty() {
            // Серверы кладём как YAML-провайдер: обратно в ссылки их не собрать, а ядро читает
            // и такой формат. Первое же «Обновить» заменит его свежим списком ссылок.
            let source = SourceImporter::adopt(
                &old.and_then(|old| old.subscription).unwrap_or_default(),
                profile_proxies,
            )?;
            let _ = source;
        }

        // Не удаляем: это единственная копия конфига у пользователя.
        let _ = std::fs::rename(&legacy, legacy.with_extension("yaml.migrated"));
        Ok(())
    }
}

/// Направление для файла настроек, который о нём ещё не знает (D-056).
///
/// Умолчание `direct` для такого файла означало бы молча выключить VPN тому, у кого он
/// работал: до появления направления трафик шёл через выбранный узел, и никакого «мимо
/// VPN» в помине не было. Поэтому смотрим, что там есть, и переносим смысл, а не значение.
fn adopt_direction() -> Result<()> {
    let Ok(text) = std::fs::read_to_string(crate::paths::Paths::settings()) else {
        return Ok(());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    if value.get("direction").is_some() {
        return Ok(());
    }
    let had_node = value
        .get("selected")
        .and_then(serde_json::Value::as_str)
        .is_some();
    let mut settings = SettingsStore::load();
    settings.direction = adopted(had_node, !SourceStore::list().is_empty());
    SettingsStore::save(&settings)
}

/// Правило переноса отдельно от чтения файла, чтобы проверялось без диска.
fn adopted(had_node: bool, has_sources: bool) -> Direction {
    match (had_node, has_sources) {
        // Узел был выбран — значит трафик шёл через него, и это ручной выбор.
        (true, true) => Direction::Manual,
        // Узла не было, но источники есть: раньше выход выбирала группа клиента.
        (false, true) => Direction::Auto,
        // Источников нет вовсе — выбирать не из чего, и «мимо VPN» тут правда.
        (_, false) => Direction::Direct,
    }
}

/// Шаблон прошлой сборки — не правки пользователя.
///
/// Файл, в котором нет ничего кроме комментариев, содержательно пуст: терять там нечего.
/// А человек, открывший «Маршрутизацию», должен видеть боевое правило, а не рассказ о том,
/// что будет, если файл оставить пустым. Свой текст это не трогает: непустой маппинг
/// остаётся как есть, каким бы старым он ни был.
fn refresh_stale_templates() -> Result<()> {
    for id in Documents::templated() {
        let text = Documents::read(id)?;
        let template = Documents::template(id)?;
        let empty = Yaml::top_mapping(&text)
            .map(|map| map.is_empty())
            .unwrap_or(false);
        if empty && text != template {
            Documents::write(id, template)?;
        }
    }
    Ok(())
}

/// Живого `rules.yaml` больше нет (D-071): то, что в нём лежало, — это набор.
///
/// Переносим содержимое в набор и убираем файл **переименованием, а не удалением**:
/// там могли быть правки, которых больше нигде нет. Если наборы уже заведены, содержимое
/// файла и есть их копия — второй экземпляр никому не нужен.
///
/// `groups.yaml` при этом не трогаем вовсе: с D-075 это снова живой файл — общий документ
/// клиента, — и старое содержимое в нём означает ровно то же, что и новое.
/// Переезд коллекций со старой раскладки (D-100): `catalog/` и `rulesets/` были двумя
/// папками в корне, стали одной — `collections/` с `rules/` внутри.
///
/// **Переносим, а не бросаем**: в обеих могли быть правки, а коллекция резолверов
/// и набор правил для того и лежат файлами, чтобы их правили. Уже переехавшую установку
/// не трогаем — признак этого один: `collections/` существует.
fn adopt_collections() -> Result<()> {
    let target = crate::paths::Paths::collections_dir();
    if target.exists() {
        return Ok(());
    }
    let catalog = crate::paths::Paths::legacy_catalog_dir();
    let rulesets = crate::paths::Paths::legacy_rulesets_dir();
    if !catalog.exists() && !rulesets.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&target)?;
    // Документы лежали в корне коллекций и там же остаются — переносим пофайлово,
    // а не папкой: в старой могло лежать что-то ещё, и оно не должно потеряться.
    if catalog.exists() {
        for entry in std::fs::read_dir(&catalog)?.flatten() {
            std::fs::rename(entry.path(), target.join(entry.file_name()))?;
        }
        let _ = std::fs::remove_dir(&catalog);
    }
    if rulesets.exists() {
        std::fs::rename(
            &rulesets,
            crate::paths::Paths::collection_folder(crate::collections::RULES),
        )?;
    }
    // Чего в старой раскладке не было — дораздаём: установка могла иметь только одну
    // из двух папок, и вторая коллекция иначе не появилась бы вовсе.
    crate::collections::Collections::fill_missing()
}

fn adopt_routing_files() -> Result<()> {
    let rules = crate::paths::Paths::rules();
    if !rules.exists() {
        return Ok(());
    }
    if PresetStore::list().is_empty() {
        let text = std::fs::read_to_string(&rules).unwrap_or_default();
        PresetStore::create(PresetStore::default_name(), &text)?;
    }
    let _ = std::fs::rename(&rules, rules.with_extension("yaml.migrated"));
    Ok(())
}

/// Группы уехали из набора в общий документ клиента (D-075).
///
/// Берём группы применённого набора — а если такого нет, первого, у кого есть свои, —
/// и кладём их в `groups.yaml`. `AUTO`, `umiray` и `probe` при этом вычищаются: это копии
/// клиентских, и оставить их своими значило бы заморозить то, что обязано пересобираться
/// при каждом запуске.
///
/// Остальные части не удаляем, а переименовываем: правки могли быть только там.
fn adopt_preset_groups() -> Result<()> {
    let parts: Vec<(String, std::path::PathBuf)> = PresetStore::list()
        .into_iter()
        .map(|preset| {
            (
                preset.id.clone(),
                crate::paths::Paths::preset_part(&preset.id, "groups"),
            )
        })
        .filter(|(_, path)| path.exists())
        .collect();
    if parts.is_empty() {
        return Ok(());
    }
    if !crate::paths::Paths::groups().exists() {
        let applied = SettingsStore::load().preset;
        let chosen = parts
            .iter()
            .find(|(id, _)| Some(id) == applied.as_ref())
            .or_else(|| {
                parts.iter().find(|(_, path)| {
                    !mine(&std::fs::read_to_string(path).unwrap_or_default()).is_empty()
                })
            });
        let groups = chosen
            .map(|(_, path)| std::fs::read_to_string(path).unwrap_or_default())
            .unwrap_or_default();
        let text = crate::render::effective::ConfigRenderer::groups_seed(&rendered(&groups))?;
        crate::paths::Paths::ensure_root()?;
        crate::atomic::AtomicFile::write(crate::paths::Paths::groups(), text)?;
    }
    for (_, path) in parts {
        let _ = std::fs::rename(&path, path.with_extension("yaml.migrated"));
    }
    Ok(())
}

/// Группы человека: без тех двух, которые клиент собирает сам, и без служебной.
fn mine(text: &str) -> Vec<crate::config::groups::Group> {
    let ours = [
        crate::config::direction::AUTO,
        crate::config::direction::SELECTOR,
        crate::config::direction::PROBE,
    ];
    crate::config::groups::GroupsCodec::parse(text)
        .unwrap_or_default()
        .into_iter()
        .filter(|group| !ours.contains(&group.name.as_str()))
        // Место в исходном документе после отбора уже ничего не значит: пишем начисто.
        .map(|group| crate::config::groups::Group {
            origin: None,
            ..group
        })
        .collect()
}

fn rendered(text: &str) -> String {
    let groups = mine(text);
    if groups.is_empty() {
        return String::new();
    }
    crate::config::groups::GroupsCodec::render("", &groups).unwrap_or_default()
}

/// Один набор существует всегда (D-071): разделу «Маршрутизация» иначе нечего показывать.
/// Содержимое первого — то, что клиент собирает сам из источников.
///
/// Проверяется при каждом запуске, а не один раз: набор можно удалить и мимо окна.
fn ensure_first_preset() -> Result<()> {
    if !PresetStore::list().is_empty() {
        return Ok(());
    }
    PresetStore::create(
        PresetStore::default_name(),
        &crate::render::effective::ConfigRenderer::generated_rules()?,
    )?;
    Ok(())
}

/// Оверрайд стал конфигом ядра и обязан быть полноценным (D-052).
///
/// Старый файл почти всегда пуст — таким его и задумывал D-035, — поэтому в обычном
/// случае на его место просто ложится шаблон, целиком, вместе с пояснениями.
fn rename_override() -> Result<()> {
    let old = crate::paths::Paths::legacy_override();
    if !old.exists() || crate::paths::Paths::advanced().exists() {
        return Ok(());
    }
    seed_advanced(&std::fs::read_to_string(&old)?)?;
    // Не удаляем: это копия того, что писал пользователь.
    let _ = std::fs::rename(&old, old.with_extension("yaml.migrated"));
    Ok(())
}

/// Написанное пользователем поверх шаблона: явные поля появляются, его правки выигрывают.
fn seed_advanced(theirs: &str) -> Result<()> {
    let text = seeded(
        Documents::template(files::ADVANCED)?,
        theirs,
        &Documents::keys_of_others(files::ADVANCED),
    )?;
    Documents::write(files::ADVANCED, &text)
}

/// Старый оверрайд поверх шаблона конфига ядра.
///
/// **Чужие ключи не переезжают.** В старом оверрайде вполне лежат `rules` и `proxy-groups` —
/// их писал прошлый переезд. Оставить их в «Настройках» значит подложить мину: оно
/// накладывается последним, и правка «Маршрутизации» молча перебивалась бы им. Их место
/// теперь в своих файлах, а без них правило и группы соберёт клиент.
///
/// Если после наложения от шаблона ничего не отличается, шаблон остаётся **дословно**:
/// пересборка стоила бы комментариев, а менять в файле нечего.
fn seeded(template: &str, theirs: &str, foreign: &[&str]) -> Result<String> {
    let mut theirs = Yaml::top_mapping(theirs)?;
    theirs.retain(|key, _| !key.as_str().is_some_and(|key| foreign.contains(&key)));

    let mut merged = Yaml::top_mapping(template)?;
    let clean = Yaml::top_mapping(template)?;
    Yaml::merge(&mut merged, theirs);
    if merged == clean {
        return Ok(template.to_string());
    }
    serde_yaml::to_string(&Value::Mapping(merged)).map_err(|e| AppError::invalid(e.to_string()))
}

/// Правила, целившиеся в конкретный сервер, переводим на группу выбора.
///
/// Узлы теперь живут внутри провайдера ядра, а **в правилах провайдерский узел по имени
/// не адресуется** — только через группу. Старое `MATCH,Sweden 0` иначе валит весь конфиг
/// с `proxy [Sweden 0] not found`, и клиент не стартует вовсе.
fn repoint_rules(overrides: &str, proxies: &[Value]) -> Result<String> {
    let names: std::collections::HashSet<String> = proxies
        .iter()
        .filter_map(|proxy| {
            proxy
                .get("name")?
                .as_str()
                .map(crate::nodes::link::LinkParser::normalize)
        })
        .collect();
    if names.is_empty() {
        return Ok(overrides.to_string());
    }

    let mut map = Yaml::top_mapping(overrides)?;
    let Some(rules) = map
        .get_mut(Value::from("rules"))
        .and_then(Value::as_sequence_mut)
    else {
        return Ok(overrides.to_string());
    };
    for rule in rules.iter_mut() {
        let Some(text) = rule.as_str() else { continue };
        let Some((head, target)) = text.rsplit_once(',') else {
            continue;
        };
        if names.contains(target) {
            *rule = Value::from(format!("{head},{}", crate::config::direction::SELECTOR));
        }
    }
    serde_yaml::to_string(&Value::Mapping(map)).map_err(|e| AppError::invalid(e.to_string()))
}

/// Серверы отдельно, всё остальное отдельно. Склейка двух результатов даёт исходный документ.
fn split(config: &str) -> Result<(Vec<Value>, String)> {
    let mut map = Yaml::top_mapping(config)?;
    let proxies = map
        .remove(Value::from("proxies"))
        .and_then(|value| value.as_sequence().cloned())
        .unwrap_or_default();
    let overrides = serde_yaml::to_string(&Value::Mapping(map))
        .map_err(|e| crate::error::AppError::invalid(e.to_string()))?;
    Ok((proxies, overrides))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Обычный случай переезда: старый оверрайд писал прошлый переезд, и в нём то же самое,
    /// что и в шаблоне. Пересобирать нечего — шаблон должен остаться дословно, с пояснениями.
    #[test]
    fn an_override_that_matches_the_template_leaves_it_word_for_word() {
        let template = Documents::template(files::ADVANCED).unwrap();
        let theirs = "mode: rule
log-level: info
allow-lan: false
";
        assert_eq!(seeded(template, theirs, &[]).unwrap(), template);
        assert_eq!(seeded(template, "", &[]).unwrap(), template);
    }

    /// Мина из настоящих данных: в старом оверрайде лежат `rules` и `proxy-groups`.
    /// «Настройки» накладывается последним — оставь их там, и правка «Маршрутизации»
    /// молча перебивалась бы файлом, в который пользователь даже не заглядывал.
    #[test]
    fn keys_owned_by_other_sections_do_not_move_into_the_advanced_file() {
        let template = Documents::template(files::ADVANCED).unwrap();
        let theirs = "mode: rule
rules:
- MATCH,umiray
proxy-groups: []
";
        let out = seeded(
            template,
            theirs,
            &Documents::keys_of_others(files::ADVANCED),
        )
        .unwrap();

        assert_eq!(
            out, template,
            "чужие ключи ушли, отличий от шаблона не осталось"
        );
        let map = Yaml::top_mapping(&out).unwrap();
        assert!(!map.contains_key(Value::from("rules")));
        assert!(!map.contains_key(Value::from("proxy-groups")));
    }

    /// А вот своё, настоящее, переезжать обязано.
    #[test]
    fn what_the_user_really_changed_survives_the_move() {
        let template = Documents::template(files::ADVANCED).unwrap();
        let out = seeded(
            template,
            "mixed-port: 7777
tun:
  stack: gvisor
",
            &[],
        )
        .unwrap();
        let map = Yaml::top_mapping(&out).unwrap();
        assert_eq!(map[Value::from("mixed-port")], Value::from(7777));
        assert_eq!(
            map[Value::from("tun")][Value::from("stack")],
            Value::from("gvisor"),
            "уточнение стека не должно потерять остальные поля tun"
        );
        assert_eq!(
            map[Value::from("tun")][Value::from("enable")],
            Value::from(false),
            "шаблон под ним остался целым"
        );
    }

    use crate::yaml::Yaml;

    const OLD: &str = r#"
mode: rule
mixed-port: 7777
proxies:
  - name: Sweden 0
    type: trojan
proxy-groups: []
rules:
  - MATCH,Sweden 0
"#;

    #[test]
    fn nothing_is_lost_when_the_old_config_is_split() {
        let (proxies, overrides) = split(OLD).unwrap();
        assert_eq!(proxies.len(), 1, "сервер уехал в источник");
        assert!(
            !overrides.contains("proxies"),
            "и не остался в оверрайде: {overrides}"
        );

        // Склейка обратно обязана дать исходный документ — иначе переезд теряет данные.
        let mut merged = Yaml::top_mapping("").unwrap();
        crate::yaml::Yaml::set(&mut merged, "proxies", Value::Sequence(proxies));
        Yaml::merge(&mut merged, Yaml::top_mapping(&overrides).unwrap());
        assert_eq!(merged, Yaml::top_mapping(OLD).unwrap());
    }

    /// Правило, целившееся в узел, обязано переехать на группу: иначе ядро не стартует
    /// вовсе — `proxy [Sweden 0] not found`. Проверено на живом конфиге.
    #[test]
    fn a_rule_aimed_at_a_node_is_repointed_to_the_group() {
        let (proxies, overrides) = split(OLD).unwrap();
        let fixed = repoint_rules(&overrides, &proxies).unwrap();
        assert!(fixed.contains("MATCH,umiray"), "{fixed}");
        assert!(!fixed.contains("MATCH,Sweden 0"));
    }

    #[test]
    fn a_rule_that_names_no_node_is_left_alone() {
        let (proxies, _) = split(OLD).unwrap();
        let fixed = repoint_rules(
            "rules:
- MATCH,DIRECT
",
            &proxies,
        )
        .unwrap();
        assert!(fixed.contains("MATCH,DIRECT"), "DIRECT — не узел: {fixed}");
    }

    #[test]
    fn a_config_without_servers_still_migrates() {
        let (proxies, overrides) = split("rules:\n  - MATCH,DIRECT\n").unwrap();
        assert!(proxies.is_empty(), "источник заводить не из чего");
        assert!(overrides.contains("MATCH,DIRECT"), "правила сохранены");
    }
}

#[cfg(test)]
mod direction_tests {
    use super::*;

    /// Файл прошлой сборки не знает про направление. Взять умолчание значило бы выключить
    /// VPN тому, у кого он работал, — поэтому переносится смысл, а не значение.
    #[test]
    fn a_file_without_a_direction_keeps_working_the_way_it_did() {
        assert_eq!(
            adopted(true, true),
            Direction::Manual,
            "узел был выбран — трафик шёл через него"
        );
        assert_eq!(
            adopted(false, true),
            Direction::Auto,
            "узла не было, но источники есть — выход выбирал клиент"
        );
        assert_eq!(
            adopted(false, false),
            Direction::Direct,
            "источников нет — «мимо VPN» это правда, а не отговорка"
        );
        assert_eq!(adopted(true, false), Direction::Direct);
    }
}
