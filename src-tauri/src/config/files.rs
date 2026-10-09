//! Пользовательские документы: какие бывают, что в них по умолчанию, как прочитать
//! и записать (D-044, D-068, D-071, D-075).
//!
//! Документов два рода, и различие принципиальное:
//!
//! - **документы клиента** — `advanced` (конфиг ядра), `client` (настройки самого
//!   клиента, ядру не уходит) и `groups` (свои группы узлов, общие для всех направлений).
//!   Лежат строкой в базе (D-170), имеют шаблон, существуют всегда;
//! - **часть набора** — «Маршрутизация», и она единственная. Своего документа у неё нет:
//!   это часть набора (D-071), и адресуется она парой «часть/набор» — `rules/<id>`.
//!   Раздел окна при этом один, а документов в нём столько, сколько наборов.
//!
//! Документы клиента **полноценные**: выключенное написано явно, а не опущено. Опущенное поле
//! означает «решает ядро», и конфиг перестаёт отвечать на вопрос «а включено ли это» (D-052).

use serde::Serialize;

use crate::error::{AppError, Result};

/// Порт локального прокси. Живёт здесь, потому что здесь же стоит шаблон, который его
/// пишет: держать число в точке сборки значило бы, что файл и сборка знают его порознь.
pub const LOCAL_PROXY_PORT: u16 = if cfg!(debug_assertions) { 3091 } else { 3090 };
use crate::config::presets::PresetStore;
use crate::db::{Db, Table};
use crate::yaml::Yaml;

/// Файл расширенных настроек. В него же пишет переключатель режима (D-052), поэтому
/// идентификатор нужен не только окну.
pub const ADVANCED: &str = "advanced";

/// Файл настроек самого клиента (D-068). В конфиг ядра не входит.
pub const CLIENT: &str = "client";

/// Группы узлов. С D-075 это документ клиента, а не часть набора: группы общие для всех
/// направлений и для всех наборов правил.
pub const GROUPS: &str = "groups";

/// Стратегии VOLT — свои документы, а не строки внутри Umiray Settings (D-183): блок `volt`
/// там остаётся коротким, а экспорт несёт их вместе с остальными документами.
pub const VOLT_RELAY: &str = "volt-relay";
pub const VOLT_VPN: &str = "volt-vpn";
pub const VOLT_RELAY_DEFAULT: &str = include_str!("../../../collections/volt/relay.yaml");
pub const VOLT_VPN_DEFAULT: &str = include_str!("../../../collections/volt/vpn.yaml");

/// Документ клиента: строка базы со своим шаблоном.
struct File {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    /// Уходит ли документ ядру. Ложь — это настройки клиента: лишний ключ верхнего уровня
    /// ядро не примет, поэтому в сборку такой файл не берётся (D-068).
    core: bool,
    default: &'static str,
}

const FILES: [File; 5] = [
    File {
        id: ADVANCED,
        label: "Mihomo Settings",
        hint: "the full core config; capture controls also write here",
        core: true,
        default: ADVANCED_DEFAULT,
    },
    File {
        id: CLIENT,
        label: "Umiray Settings",
        hint: "client settings; this file is never sent to the core",
        core: false,
        default: CLIENT_DEFAULT,
    },
    File {
        id: GROUPS,
        label: "Groups",
        hint:
            "your node groups, shared by all routes; the client builds AUTO and auto groups itself",
        core: true,
        default: GROUPS_DEFAULT,
    },
    File {
        id: VOLT_RELAY,
        label: "VOLT Relay",
        hint: "how VOLT changes direct connections; never sent to the core",
        core: false,
        default: VOLT_RELAY_DEFAULT,
    },
    File {
        id: VOLT_VPN,
        label: "VOLT Proxy",
        hint: "how VOLT changes connections to proxy servers; never sent to the core",
        core: false,
        default: VOLT_VPN_DEFAULT,
    },
];

/// Раздел окна, документы которого — части наборов (D-071). Порядок тот же, что у наложения
/// при сборке: группы, потом правила, потом расширенное (D-029).
struct Routing {
    /// Он же имя части набора и идентификатор раздела.
    id: &'static str,
    label: &'static str,
    hint: &'static str,
}

const ROUTING: [Routing; 1] = [Routing {
    id: "rules",
    label: "Routing",
    hint: "top to bottom, the first match wins: your rules → high → medium → low → MATCH; rule sets first within a level",
}];

/// Описание документа для окна. Содержимое не тащим: документов несколько, а открыт один.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub id: String,
    pub label: String,
    pub hint: String,
    /// Уходит ли документ ядру. У клиентского нет «собранного» вида — показывать нечего.
    pub core: bool,
    /// Этот набор сейчас применён. У файлов клиента всегда ложь: применять их не нужно,
    /// они действуют всегда.
    pub applied: bool,
}

/// Раздел окна вместе с документами внутри.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub label: String,
    /// Документы раздела — наборы (D-071): их можно завести, применить и удалить.
    pub presets: bool,
    pub docs: Vec<Doc>,
}

/// К чему ведёт идентификатор документа. Разбор здесь, на границе: `id` приходит
/// из вебвью, и в путь он превращаться не должен.
enum Target {
    Client(&'static File),
    Part { part: &'static str, preset: String },
}

fn find(id: &str) -> Result<Target> {
    if let Some((part, preset)) = id.split_once('/') {
        let known = ROUTING
            .iter()
            .find(|routing| routing.id == part)
            .ok_or_else(|| AppError::invalid(format!("Неизвестный раздел конфига: {part}")))?;
        // Набор проверяет себя сам: форма идентификатора у него закрытая, и несуществующий
        // отвергается здесь же, а не превращается в путь.
        PresetStore::get(preset)?;
        return Ok(Target::Part {
            part: known.id,
            preset: preset.to_string(),
        });
    }
    FILES
        .iter()
        .find(|file| file.id == id)
        .map(Target::Client)
        .ok_or_else(|| AppError::invalid(format!("Неизвестный документ конфига: {id}")))
}

pub struct Documents;

impl Documents {
    /// Разделы окна с документами внутри.
    ///
    /// Какой набор применён, приходит снаружи: это настройка клиента, а `config` про настройки
    /// не знает — иначе получился бы круг `config → app → config`.
    pub fn list(applied: Option<&str>) -> Vec<Section> {
        let presets = PresetStore::list();
        let mut sections = vec![Section {
            // «Группы» — один документ клиента и никаких наборов (D-075): группы общие.
            id: GROUPS.into(),
            label: "Groups".into(),
            presets: false,
            docs: vec![doc(GROUPS)],
        }];
        sections.extend(ROUTING.iter().map(|routing| {
            Section {
                id: routing.id.into(),
                label: routing.label.into(),
                presets: true,
                docs: presets
                    .iter()
                    .map(|preset| Doc {
                        id: format!("{}/{}", routing.id, preset.id),
                        label: preset.name.clone(),
                        hint: routing.hint.into(),
                        core: true,
                        applied: applied == Some(preset.id.as_str()),
                    })
                    .collect(),
            }
        }));
        sections.push(Section {
            id: ADVANCED.into(),
            label: "Settings".into(),
            presets: false,
            // Клиент первым: свои настройки открывают чаще, чем конфиг ядра (D-117).
            docs: vec![doc(CLIENT), doc(ADVANCED)],
        });
        sections
    }

    /// Часть и набор, к которым ведёт документ. Пусто — это документ клиента, наборам он
    /// не принадлежит.
    pub fn part_and_preset(id: &str) -> Option<(&'static str, String)> {
        match find(id) {
            Ok(Target::Part { part, preset }) => Some((part, preset)),
            _ => None,
        }
    }

    /// Читает документ. Документ клиента при этом заводится из шаблона, если его ещё нет;
    /// у части набора отсутствие значит пустой документ.
    pub fn read(id: &str) -> Result<String> {
        match find(id)? {
            Target::Client(file) => match Db::get(Table::Documents, file.id, "")? {
                Some(text) => Ok(text),
                None => {
                    Db::put(Table::Documents, file.id, "", file.default)?;
                    Ok(file.default.into())
                }
            },
            Target::Part { part, preset } => PresetStore::read(&preset, part),
        }
    }

    pub fn write(id: &str, text: &str) -> Result<()> {
        // Проверяем до записи: битый YAML на диске означал бы, что ядро не поднимется,
        // а причина будет видна только в логе при следующем запуске.
        Yaml::top_mapping(text)?;
        match find(id)? {
            Target::Client(file) => Db::put(Table::Documents, file.id, "", text),
            Target::Part { part, preset } => PresetStore::write(&preset, part, text),
        }
    }

    /// Возвращает документ клиента к шаблону — и отдаёт его же, чтобы окно показало результат
    /// без второго чтения. Часть набора сюда не попадает: её умолчание — то, что собирает
    /// клиент, а собирает его рендер, поэтому сброс набора живёт в `crate::render::effective::ConfigRenderer::reset`.
    pub fn reset(id: &str) -> Result<String> {
        match find(id)? {
            Target::Client(file) => {
                Db::put(Table::Documents, file.id, "", file.default)?;
                Ok(file.default.into())
            }
            Target::Part { .. } => Err(AppError::invalid(
                "Часть набора сбрасывается к собранному клиентом",
            )),
        }
    }

    /// Шаблон документа клиента: им заводится отсутствующий и освежается пустой.
    pub fn template(id: &str) -> Result<&'static str> {
        match find(id)? {
            Target::Client(file) => Ok(file.default),
            Target::Part { .. } => Err(AppError::invalid("У части набора шаблона нет")),
        }
    }

    /// Документы с шаблоном — те, что переезд имеет право освежить.
    pub fn templated() -> Vec<&'static str> {
        FILES.iter().map(|file| file.id).collect()
    }
}

/// Документ окна по документу клиента. Порядок вкладок задаётся списком в `list`, а не порядком
/// объявления файлов: «Группы» — свой раздел, «Настройки» — два документа в одном.
fn doc(id: &str) -> Doc {
    let file = FILES.iter().find(|file| file.id == id).expect("свой файл");
    Doc {
        id: file.id.into(),
        label: file.label.into(),
        hint: file.hint.into(),
        core: file.core,
        applied: false,
    }
}

const CLIENT_DEFAULT: &str = r#"# Настройки самого клиента: то, чего нет в конфиге ядра (D-068).
# Ядру этот документ не уходит — он про поведение окна, а не про то, как ходит трафик.
#
# Документ и форма правят одно и то же: «Настройки» → Umiray Settings → «Форма» пишет сюда же,
# точечно. При записи из формы комментарии теряются — та же цена, что и в конфиге ядра.

# Чем мерить, сколько до сервера (D-069):
#
#   icmp             обычный ping до хоста — быстро, но на половине хостингов режется
#   tcp              время установления соединения с портом узла; заодно доказывает,
#                    что порт открыт
#   proxy            запрос через сам узел, два подхода, лучший из двух — нужно подключение
#   proxy-keepalive  второй запрос в уже поднятом туннеле: чистое время ответа без
#                    рукопожатия — нужно подключение. Меряет по одному узлу, поэтому
#                    на большом списке идёт заметно дольше остальных
#
# Если выбранный способ промолчал, клиент пробует ICMP: такое число показывается синим
# и со значком — «хост жив, а выбранная проверка результата не дала».
ping: tcp

# Куда бьёт проверка живости узла (D-108). Адрес один и тот же для ядра, которое
# обходит узлы провайдера, и для замера клиента через туннель: два разных адреса давали
# одному узлу два вердикта. Ответом считается только 204 — заглушка провайдера и страница
# captive portal отвечают двухсотым, и мёртвый выход иначе остаётся в группе живым.
# Только http:// и обязательно с путём: замер идёт через CONNECT на 80-й порт.
health-url: http://cp.cloudflare.com/generate_204

# Как часто группы перепроверяют узлы, секунд (60…86400): `AUTO`, автогруппы, UDP-группа
# и проверка источников. Свои группы — своим `interval` в «Группах».
health-interval: 300

# Уводить ли весь UDP в узлы, которые несут его датаграммой — hysteria2, tuic, wireguard
# (D-113): правило `NETWORK,udp,umiray-udp` перед `MATCH`. Включённое, оно включает и саму
# группу (`auto-groups: udp`). Таких узлов нет — ни группы, ни правила: пустую группу ядро
# не примет и не стартует.
# Направление «Прямое» тумблер гасит: правило идёт мимо переключателя направления.
udp-group: false

# Группы, которые клиент соберёт сам (D-172): по стране узла (`umiray-geo-pl`), по протоколу
# (`umiray-proto-vless`) — только где таких узлов два и больше, группа из одного узла ничего
# не выбирает, — и `umiray-udp` из узлов, чей протокол несёт UDP сам, а не внутри TCP (D-113).
# Все `url-test`. Группа сама трафик не уводит: выход выбирают в «Соединении» или правилом.
auto-groups:
  location: false
  protocol: false
  udp: false

# Кого нет в AUTO (D-172): источники целиком — и их будущие узлы тоже — и узлы по имени.
auto-exclude:
  sources: []
  nodes: []

# Через сколько часов перепрашивать страну узла — тот самый флаг рядом с именем (D-084).
# Страну определяет адрес сервера: он уходит к ipinfo.io, ответ кэшируется по
# «хост:порт» в базе клиента. Ноль — не спрашивать вовсе, тогда наружу не уходит ни один адрес.
geo-hours: 168

# Чем маскировать рукопожатие WireGuard (D-118). На пути до сервера его узнают по самому
# рукопожатию и после него душат поток — 204 Б/с против 9.4 МБ/с на одном и том же
# сервере (S-023). Поля уезжают в `amnezia-wg-option` записи узла; ноль — «не трогать».
#
#   jc, jmin, jmax   мусорные пакеты перед рукопожатием и их размер. Работают
#                    с ЛЮБЫМ сервером WireGuard: сами пакеты рукопожатия не меняются,
#                    а мусор ядерный WireGuard молча роняет. Хватает одного (замерено)
#   s1…s4            паддинг рукопожатия, ответа, cookie и транспорта
#   h1…h4            магические заголовки тех же четырёх типов пакетов
#
# ВНИМАНИЕ: s* и h* меняют формат самих пакетов и требуют сервера с AmneziaWG —
# на ванильном туннель с ними не встанет вовсе.
wireguard-mask:
  jc: 1
  jmin: 40
  jmax: 70
  s1: 0
  s2: 0
  s3: 0
  s4: 0
  h1: 0
  h2: 0
  h3: 0
  h4: 0
"#;

macro_rules! advanced_default {
    ($port:literal) => {
        concat!(r#"# Mihomo Settings — конфиг ядра целиком: всё, что не про источники, группы и маршрутизацию.
#
# Документ и окно правят одно и то же. Переключатель режима в шапке пишет сюда `tun.enable`
# сам, остальное ваше. При этой записи документ пересобирается, и комментарии теряются —
# такова цена за один документ на редактор и на переключатель.

# Как ядро выбирает выход: rule — по правилам из раздела «Маршрутизация».
mode: rule
log-level: info

# Выключенное написано явно, а не опущено: опущенное поле означает «решает ядро»,
# и конфиг перестаёт отвечать на вопрос «а включено ли это».
allow-lan: false
ipv6: false

# Local Proxy: один порт сразу на http и socks. Адрес для браузера — 127.0.0.1, порт ниже.
mixed-port: "#, $port, r#"

# TUN: виртуальный адаптер, весь трафик машины. Нужны права администратора.
tun:
  enable: false
  stack: mixed
  auto-route: true
  auto-detect-interface: true
  strict-route: true
  dns-hijack:
    - any:53
    - tcp://any:53

# Своё разрешение имён — во всех режимах, а не только в TUN (D-169): в Proxy и System
# через него резолвятся прямые соединения и правила по IP, а TUN без него трафик ловит,
# но разрезолвить не может.
dns:
  enable: true
  enhanced-mode: fake-ip
  nameserver:
    - https://1.1.1.1/dns-query

# Имя из TLS, HTTP и QUIC — для тех, кто пришёл голым адресом: браузер со своим DoH,
# программа с вшитым IP. Без него правило по домену на них промахивается.
sniffer:
  enable: true
  sniff:
    HTTP:
      ports: [80, 8080-8880]
      override-destination: true
    TLS:
      ports: [443, 8443]
    QUIC:
      ports: [443, 8443]
  skip-domain:
    - Mijia Cloud

# Выбранный узел помним сами: через перезапуск ядро его не удержало. А вот карту
# подменных адресов ядро просим помнить: без неё после подъёма 198.18.0.4 достаётся
# тому, кто спросил первым, и приложение с запомненным адресом уезжает по чужому правилу.
profile:
  store-selected: false
  store-fake-ip: true
"#)
    };
}

#[cfg(debug_assertions)]
const ADVANCED_DEFAULT: &str = advanced_default!("3091");
#[cfg(not(debug_assertions))]
const ADVANCED_DEFAULT: &str = advanced_default!("3090");

const GROUPS_DEFAULT: &str = r#"# Группы узлов — ваши, и они общие: одни и те же во всех направлениях и во всех наборах
# правил (D-075). Набор — это только маршрутизация.
#
# Две группы клиент собирает сам и держит в актуальном состоянии, поэтому имена `AUTO`
# и `umiray` заняты:
#
#   AUTO    load-balance по узлам всех источников (consistent-hashing: один адрес всегда
#           уходит через один и тот же сервер);
#   umiray  псевдоним выбранного — на него смотрит MATCH, а куда указывает он, решает
#           направление в «Соединении».
#
# Здесь — только ваши группы. Они станут отдельными пунктами внутри umiray:
#
#   proxy-groups:
#     - name: Польша
#       type: fallback
#       use: [<идентификатор источника>]
#       filter: (?i)pl|poland
#       url: http://www.google.com/generate_204
#       interval: 300
#
# use: — взять все узлы этих источников (список живой: новые узлы подписки приезжают сами);
# filter: — отобрать по имени узла регулярным выражением; proxies: — назвать узлы и группы
# поимённо.
#
# Форма правит этот же документ. При записи из формы комментарии теряются — та же цена,
# что и в «Настройках».
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::presets;
    use serde_yaml::Value;

    #[test]
    fn unknown_id_is_rejected_at_the_boundary() {
        // `id` приходит из вебвью: он не должен превращаться в путь на диске.
        assert!(find("../../settings").is_err());
        assert!(find("").is_err());
        assert!(find(ADVANCED).is_ok());
        assert!(find(CLIENT).is_ok());
        assert!(find(GROUPS).is_ok(), "группы — документ клиента (D-075)");
        // Часть набора без набора — тоже не путь.
        assert!(find("rules").is_err(), "часть адресуется вместе с набором");
        assert!(find("rules/../../settings").is_err());
        assert!(find("нет-такого/0123456789abcdef").is_err());
    }

    /// Клиентский файл не должен попасть ядру ни одним путём: лишний ключ верхнего уровня
    /// оно не примет и откажется стартовать целиком (D-068).
    #[test]
    fn the_client_file_never_reaches_the_core() {
        assert!(!doc(CLIENT).core);
        assert!(doc(ADVANCED).core);
    }

    /// Части набора и разделы маршрута — одно и то же, и разъезжаться им нельзя (D-071).
    /// С D-075 часть осталась одна: группы уехали в файл клиента.
    #[test]
    fn routing_sections_match_the_parts_of_a_set() {
        let ours: Vec<&str> = ROUTING.iter().map(|routing| routing.id).collect();
        assert_eq!(ours, presets::PARTS.to_vec());
        for id in [ADVANCED, CLIENT, GROUPS] {
            assert!(
                !presets::PARTS.contains(&id),
                "{id} в набор не входит: набор — это только маршрутизация"
            );
        }
    }

    #[test]
    fn every_template_is_valid_yaml() {
        for file in FILES.iter() {
            assert!(
                Yaml::top_mapping(file.default).is_ok(),
                "шаблон {} не разбирается",
                file.id
            );
        }
    }

    /// «Полноценный» — проверяемое свойство, а не пожелание: выключенное должно быть
    /// написано, иначе непонятно, чей режим и чей DNS в итоге работают.
    #[test]
    fn the_advanced_template_says_out_loud_what_is_off() {
        let out = Value::Mapping(Yaml::top_mapping(ADVANCED_DEFAULT).unwrap());
        assert_eq!(out["tun"]["enable"], Value::from(false));
        assert_eq!(
            out["dns"]["enable"],
            Value::from(true),
            "DNS не ждёт TUN: конфиг один на все режимы (D-169)"
        );
        assert_eq!(out["sniffer"]["enable"], Value::from(true));
        assert_eq!(out["allow-lan"], Value::from(false));
        assert_eq!(out["ipv6"], Value::from(false));
        assert_eq!(out["profile"]["store-selected"], Value::from(false));
        assert_eq!(
            out["profile"]["store-fake-ip"],
            Value::from(true),
            "карту подменных адресов просим помнить (S-021)"
        );
        assert_eq!(
            out["mixed-port"],
            Value::from(crate::config::files::LOCAL_PROXY_PORT),
            "порт в шаблоне и порт в коде обязаны совпадать"
        );
    }

    #[test]
    fn ids_are_unique() {
        for (index, file) in FILES.iter().enumerate() {
            assert!(
                FILES
                    .iter()
                    .skip(index + 1)
                    .all(|other| other.id != file.id),
                "повторяется id {}",
                file.id
            );
        }
    }

    /// Раздел окна и документ — разные вещи (D-070, D-071): «Настройки» держит два
    /// файла клиента, разделы маршрута — по документу на набор.
    #[test]
    fn sections_are_built_around_documents_and_not_around_files() {
        let sections = Documents::list(None);
        assert_eq!(
            sections.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec![GROUPS, "rules", ADVANCED]
        );
        let advanced = sections.iter().find(|s| s.id == ADVANCED).unwrap();
        assert!(!advanced.presets);
        assert_eq!(
            advanced
                .docs
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec![CLIENT, ADVANCED],
            "клиент первым (D-117)"
        );
        assert!(!advanced.docs[0].core, "у клиентского нет собранного вида");
        assert!(
            !sections[0].presets,
            "группы общие, наборов у них нет (D-075)"
        );
        assert_eq!(sections[0].docs.len(), 1);
        assert!(sections[1].presets, "документы «Маршрутизации» — наборы");
    }
}
