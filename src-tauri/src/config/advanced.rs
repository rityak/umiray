//! Поля конфига ядра, у которых есть форма (D-052, D-086).
//!
//! Устроено ровно как режим перехвата (`config/mode.rs`), только полей одиннадцать,
//! а не одно: документ на диске главный, форма пишет в него точечно, редактор правит
//! тот же файл руками, и оба видят одно и то же.
//!
//! **Форма знает не весь файл, и это намеренно.** `tun.enable` принадлежит переключателю
//! режима в шапке (D-060), `mode`, `allow-lan`, `ipv6` и `profile` — только тексту.
//! У поля один хозяин, иначе форма и переключатель спорят за одну строку.
//!
//! **Комментарии при записи из формы теряются**: файл пересобирается через `serde_yaml`,
//! а он их не хранит. Та же цена, что и у переключателя режима.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::config::files::Documents;
use crate::config::files::{self, LOCAL_PROXY_PORT};
use crate::error::{AppError, Result};
use crate::yaml::Yaml;

/// Чем TUN разбирает пакеты. `mixed` — умолчание ядра и наше: `system` быстрее, но
/// на части машин не поднимается, `gvisor` работает везде и медленнее.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stack {
    System,
    Gvisor,
    #[default]
    Mixed,
}

/// Насколько подробно ядро пишет в лог. Тот самый лог, который читает раздел «Логи»:
/// `debug` делает его пригодным для разбора и бесполезным для чтения.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Silent,
    Error,
    Warning,
    #[default]
    Info,
    Debug,
}

/// Как ядро подменяет ответы DNS. `fake-ip` отдаёт выдуманный адрес и решает по имени —
/// без него правило по домену не сработает на том, что резолвится мимо нас.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Enhanced {
    #[default]
    FakeIp,
    RedirHost,
}

/// Что правит форма. Плоская, а не вложенная: раскладку по `tun` и `dns` знает запись,
/// а окну важно поле, а не то, в каком разделе файла оно лежит.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub log_level: LogLevel,
    /// Порт локального прокси: один сразу на http и socks.
    pub mixed_port: u16,
    /// Разбор TLS SNI у соединений, пришедших голым адресом. Без него правило по домену
    /// промахивается на всём, что резолвилось мимо нашего DNS.
    pub sniffer: bool,
    pub stack: Stack,
    /// Имя виртуального адаптера. Пусто — как назовёт ядро (`Meta`). За это же имя
    /// цепляется kill switch (D-073).
    pub device: String,
    /// Размер пакета адаптера. Ноль — не задавать, решает ядро.
    pub mtu: u32,
    /// Не выпускать в обход таблицы маршрутов туннеля.
    pub strict_route: bool,
    /// Куда заворачивать чужие запросы к DNS: `адрес:порт`.
    pub dns_hijack: Vec<String>,
    /// Один внешний UDP-порт на все адреса назначения (`endpoint-independent-nat`): игры,
    /// звонки и P2P видят «открытый» NAT. Спорное (D-169): стеку TUN это чуть дороже.
    pub open_nat: bool,
    pub dns_enable: bool,
    pub enhanced_mode: Enhanced,
    pub nameserver: Vec<String>,
    /// DoH сначала по HTTP/3 (`prefer-h3`). Только в форме, не в мастере (D-169): где QUIC
    /// пропускают, быстрее; где режут (S-034), первый запрос ждёт отказа и уходит на HTTP/2.
    pub prefer_h3: bool,
}

/// Ключи файла. Названы один раз: чтение и запись обязаны говорить об одном поле.
const LOG_LEVEL: &str = "log-level";
const MIXED_PORT: &str = "mixed-port";
const SNIFFER: &str = "sniffer";
const TUN: &str = "tun";
const DNS: &str = "dns";
const ENABLE: &str = "enable";
const STACK: &str = "stack";
const DEVICE: &str = "device";
const MTU: &str = "mtu";
const STRICT_ROUTE: &str = "strict-route";
const DNS_HIJACK: &str = "dns-hijack";
const ENHANCED_MODE: &str = "enhanced-mode";
const NAMESERVER: &str = "nameserver";
const OPEN_NAT: &str = "endpoint-independent-nat";
const PREFER_H3: &str = "prefer-h3";

/// Разумные границы MTU. Ниже 576 не пройдёт даже IPv4-минимум, выше 9000 —
/// jumbo-кадр, которого не переварит обычная сеть. Проверяем, потому что это **граница
/// с недоверенными данными**: число приходит из вебвью, а уезжает в сетевой адаптер.
const MTU_RANGE: std::ops::RangeInclusive<u32> = 576..=9000;

/// Значение поля или умолчание.
///
/// Отсутствие поля и мусор в нём — разные вещи, и смешивать их нельзя: первое значит
/// «ещё не писали», второе — «написано непонятное». Молча превращать второе в первое
/// значит спорить с тем, что человек набрал руками (то же правило, что у `client::ping`).
fn field<T>(map: Option<&Mapping>, key: &str) -> Result<T>
where
    T: Default + serde::de::DeserializeOwned,
{
    let Some(value) = map.and_then(|map| map.get(Value::from(key))) else {
        return Ok(T::default());
    };
    serde_yaml::from_value(value.clone())
        .map_err(|e| AppError::invalid(format!("В advanced.yaml непонятное значение «{key}»: {e}")))
}

fn nested<'a>(map: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    map.get(Value::from(key)).and_then(Value::as_mapping)
}

pub struct Advanced;

impl Advanced {
    pub fn read() -> Result<Options> {
        of(&Yaml::top_mapping(&Documents::read(files::ADVANCED)?)?)
    }

    pub fn write(options: &Options) -> Result<()> {
        let mut map = Yaml::top_mapping(&Documents::read(files::ADVANCED)?)?;
        apply(&mut map, options)?;
        let text = serde_yaml::to_string(&Value::Mapping(map))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        Documents::write(files::ADVANCED, &text)
    }
}

/// Поля документа. Отделено от диска, чтобы правила проверялись обычным `cargo test`,
/// не трогая настоящий `%LOCALAPPDATA%` пользователя.
fn of(map: &Mapping) -> Result<Options> {
    let tun = nested(map, TUN);
    let dns = nested(map, DNS);
    let port: u16 = field(Some(map), MIXED_PORT)?;
    Ok(Options {
        log_level: field(Some(map), LOG_LEVEL)?,
        // Ноль в файле — это не «порт 0», это «поля нет». Порт по умолчанию знает
        // шаблон, и знать его вторым числом здесь незачем.
        mixed_port: if port == 0 { LOCAL_PROXY_PORT } else { port },
        sniffer: field(nested(map, SNIFFER), ENABLE)?,
        stack: field(tun, STACK)?,
        device: field(tun, DEVICE)?,
        mtu: field(tun, MTU)?,
        strict_route: field(tun, STRICT_ROUTE)?,
        dns_hijack: field(tun, DNS_HIJACK)?,
        open_nat: field(tun, OPEN_NAT)?,
        dns_enable: field(dns, ENABLE)?,
        enhanced_mode: field(dns, ENHANCED_MODE)?,
        nameserver: field(dns, NAMESERVER)?,
        prefer_h3: field(dns, PREFER_H3)?,
    })
}

/// Пустые строки в списке — не значение, а недописанная строка формы.
fn cleaned(list: &[String]) -> Vec<String> {
    list.iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn seq(list: Vec<String>) -> Value {
    Value::Sequence(list.into_iter().map(Value::from).collect())
}

/// Имя варианта перечисления так, как его пишут в файле. Отдельной функцией, потому что
/// `AppError` не умеет превращаться из ошибки serde сам, а промахнуться здесь нечем:
/// варианты закрыты, и падение означало бы поломку сборки, а не плохие данные.
fn tag<T: Serialize>(value: T) -> Result<Value> {
    serde_yaml::to_value(value).map_err(|e| AppError::invalid(e.to_string()))
}

/// Убрать поле: пустое значение в форме означает «пусть решает ядро», а не «запиши пустоту».
fn clear(map: &mut Mapping, key: &str) {
    map.remove(Value::from(key));
}

/// Наложить поля формы на документ.
///
/// Проверки здесь, а не в команде: это **граница с недоверенными данными** — числа
/// и строки приходят из вебвью и уезжают в сетевой адаптер. `tun.enable` не трогаем
/// вовсе: он принадлежит переключателю режима (D-060).
fn apply(map: &mut Mapping, options: &Options) -> Result<()> {
    if options.mixed_port == 0 {
        return Err(AppError::invalid("Порт локального прокси не может быть 0"));
    }
    if options.mtu != 0 && !MTU_RANGE.contains(&options.mtu) {
        return Err(AppError::invalid(format!(
            "MTU {} вне разумных пределов: ждём {}–{} или 0, чтобы решало ядро",
            options.mtu,
            MTU_RANGE.start(),
            MTU_RANGE.end()
        )));
    }
    let nameserver = cleaned(&options.nameserver);
    if options.dns_enable && nameserver.is_empty() {
        return Err(AppError::invalid(
            "Своё разрешение имён включено, но ни одного DNS-сервера не указано",
        ));
    }

    Yaml::set(map, LOG_LEVEL, tag(options.log_level)?);
    Yaml::set(map, MIXED_PORT, Value::from(options.mixed_port));

    let tun = Yaml::sub(map, TUN);
    Yaml::set(tun, STACK, tag(options.stack)?);
    Yaml::set(tun, STRICT_ROUTE, Value::from(options.strict_route));
    Yaml::set(tun, DNS_HIJACK, seq(cleaned(&options.dns_hijack)));
    Yaml::set(tun, OPEN_NAT, Value::from(options.open_nat));
    match options.device.trim() {
        "" => clear(tun, DEVICE),
        name => Yaml::set(tun, DEVICE, Value::from(name)),
    }
    match options.mtu {
        0 => clear(tun, MTU),
        mtu => Yaml::set(tun, MTU, Value::from(mtu)),
    }

    let dns = Yaml::sub(map, DNS);
    Yaml::set(dns, ENABLE, Value::from(options.dns_enable));
    Yaml::set(dns, ENHANCED_MODE, tag(options.enhanced_mode)?);
    Yaml::set(dns, NAMESERVER, seq(nameserver));
    Yaml::set(dns, PREFER_H3, Value::from(options.prefer_h3));

    // Тумблер ставит только `enable`; что именно нюхать — `fill`, то есть один раз.
    // Правило то же, что у всей сборки (D-029): что задаёт способность — форсируем,
    // остальное только дописываем, и написанное человеком не трогаем.
    let sniffer = Yaml::sub(map, SNIFFER);
    Yaml::set(sniffer, ENABLE, Value::from(options.sniffer));
    Yaml::fill(sniffer, "sniff", default_sniff());
    Yaml::fill(sniffer, "skip-domain", seq(vec!["Mijia Cloud".into()]));
    Ok(())
}

/// Что разбирать по умолчанию: три протокола, по которым имя вообще можно достать.
/// `override-destination` только у HTTP — у TLS подмена адреса ломает соединения,
/// в которых сертификат выписан на исходный адрес.
fn default_sniff() -> Value {
    let ports = |list: Vec<Value>| {
        let mut map = Mapping::new();
        map.insert(Value::from("ports"), Value::Sequence(list));
        map
    };
    let mut http = ports(vec![Value::from(80), Value::from("8080-8880")]);
    http.insert(Value::from("override-destination"), Value::from(true));
    let tls = ports(vec![Value::from(443), Value::from(8443)]);
    let quic = ports(vec![Value::from(443), Value::from(8443)]);

    let mut sniff = Mapping::new();
    sniff.insert(Value::from("HTTP"), Value::Mapping(http));
    sniff.insert(Value::from("TLS"), Value::Mapping(tls));
    sniff.insert(Value::from("QUIC"), Value::Mapping(quic));
    Value::Mapping(sniff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Options {
        of(&Mapping::new()).unwrap()
    }

    /// Спорные поля лежат там, где их читает ядро, а не плоско, как в форме.
    #[test]
    fn the_disputed_options_land_in_their_sections() {
        let map = written(
            "",
            &Options {
                open_nat: true,
                prefer_h3: true,
                ..defaults()
            },
        );
        assert_eq!(map["tun"]["endpoint-independent-nat"], Value::from(true));
        assert_eq!(map["dns"]["prefer-h3"], Value::from(true));
        assert!(of(&map).unwrap().open_nat);
    }

    fn written(document: &str, options: &Options) -> Mapping {
        let mut map = Yaml::top_mapping(document).unwrap();
        apply(&mut map, options).unwrap();
        map
    }

    /// Пустой файл — это умолчания, а мусор — ошибка. Разница принципиальная: первое
    /// значит «ещё не выбирали», второе — «написано непонятное», и молча превращать
    /// второе в первое нельзя.
    #[test]
    fn a_missing_field_is_the_default_and_nonsense_is_an_error() {
        let empty = defaults();
        assert_eq!(empty.stack, Stack::Mixed);
        assert_eq!(empty.log_level, LogLevel::Info);
        assert_eq!(empty.enhanced_mode, Enhanced::FakeIp);
        assert_eq!(
            empty.mixed_port, LOCAL_PROXY_PORT,
            "порт берётся из шаблона"
        );
        assert_eq!(
            empty.mtu, 0,
            "ноль означает «решает ядро», а не «MTU нулевой»"
        );
        assert!(!empty.sniffer);
        assert!(
            !empty.open_nat && !empty.prefer_h3,
            "спорное без поля выключено"
        );

        for bad in [
            "tun: {stack: черепаха}",
            "log-level: болтливый",
            "dns: {enhanced-mode: fake}",
            "mixed-port: не число",
            "tun: {mtu: []}",
        ] {
            let map = Yaml::top_mapping(bad).unwrap();
            assert!(of(&map).is_err(), "{bad} должно быть ошибкой");
        }
    }

    /// Шаблон — это документация: всё, что в нём написано, обязано читаться формой.
    #[test]
    fn the_template_reads_back_through_the_form() {
        let map = Yaml::top_mapping(Documents::template(files::ADVANCED).unwrap()).unwrap();
        let options = of(&map).unwrap();
        assert_eq!(options.stack, Stack::Mixed);
        assert_eq!(options.mixed_port, LOCAL_PROXY_PORT);
        assert_eq!(options.log_level, LogLevel::Info);
        assert_eq!(options.nameserver, vec!["https://1.1.1.1/dns-query"]);
    }

    /// Главное правило файла: у поля один хозяин. Режим перехвата ставит переключатель
    /// в шапке (D-060), и форма обязана пройти мимо `tun.enable`, что бы в нём ни стояло.
    #[test]
    fn the_form_never_touches_what_belongs_to_the_mode_switch() {
        let map = written(
            "tun:\n  enable: true\nmode: rule\nipv6: true\n",
            &defaults(),
        );
        let tun = nested(&map, TUN).unwrap();
        assert_eq!(
            tun.get(Value::from(ENABLE)),
            Some(&Value::from(true)),
            "форма стёрла режим перехвата"
        );
        assert_eq!(map.get(Value::from("mode")), Some(&Value::from("rule")));
        assert_eq!(map.get(Value::from("ipv6")), Some(&Value::from(true)));
    }

    /// Пустое поле формы означает «решает ядро», и в файле его быть не должно: записанные
    /// `device: ""` и `mtu: 0` ядро прочитает буквально и поднимет не то.
    #[test]
    fn an_empty_field_is_removed_rather_than_written_as_empty() {
        let full = Options {
            device: "umiray0".into(),
            mtu: 1400,
            ..defaults()
        };
        let map = written("", &full);
        let tun = nested(&map, TUN).unwrap();
        assert_eq!(tun.get(Value::from(DEVICE)), Some(&Value::from("umiray0")));
        assert_eq!(tun.get(Value::from(MTU)), Some(&Value::from(1400)));

        let map = written("tun:\n  device: umiray0\n  mtu: 1400\n", &defaults());
        let tun = nested(&map, TUN).unwrap();
        assert_eq!(tun.get(Value::from(DEVICE)), None);
        assert_eq!(tun.get(Value::from(MTU)), None);
    }

    /// Числа и списки приезжают из вебвью — это граница, и негодное на ней отвергается,
    /// а не уезжает в сетевой адаптер.
    #[test]
    fn nonsense_from_the_window_is_refused_at_the_boundary() {
        let mut map = Mapping::new();
        assert!(apply(
            &mut map,
            &Options {
                mixed_port: 0,
                ..defaults()
            }
        )
        .is_err());
        assert!(apply(
            &mut map,
            &Options {
                mtu: 42,
                ..defaults()
            }
        )
        .is_err());
        assert!(apply(
            &mut map,
            &Options {
                mtu: 70000,
                ..defaults()
            }
        )
        .is_err());
        assert!(
            apply(
                &mut map,
                &Options {
                    dns_enable: true,
                    nameserver: vec!["  ".into()],
                    ..defaults()
                }
            )
            .is_err(),
            "включённый DNS без единого сервера ядро не поднимет"
        );
    }

    /// Недописанная строка формы — это не сервер. Список чистится и при записи, и при
    /// проверке: иначе «хотя бы один сервер» проходило бы на одном пробеле.
    #[test]
    fn blank_lines_of_the_form_do_not_become_values() {
        let options = Options {
            dns_enable: true,
            nameserver: vec!["  1.1.1.1 ".into(), "".into(), "  ".into()],
            dns_hijack: vec!["any:53".into(), " ".into()],
            ..defaults()
        };
        let map = written("", &options);
        let dns = nested(&map, DNS).unwrap();
        assert_eq!(
            dns.get(Value::from(NAMESERVER)),
            Some(&seq(vec!["1.1.1.1".into()]))
        );
        let tun = nested(&map, TUN).unwrap();
        assert_eq!(
            tun.get(Value::from(DNS_HIJACK)),
            Some(&seq(vec!["any:53".into()]))
        );
    }

    /// Тумблер сниффера ставит только `enable`: что именно нюхать, пользователь вправе
    /// переписать, и переключение тумблера не должно возвращать наш список обратно.
    #[test]
    fn the_sniffer_toggle_leaves_a_hand_written_sniff_alone() {
        let mine = "sniffer:\n  enable: false\n  sniff:\n    TLS:\n      ports: [443]\n";
        let map = written(
            mine,
            &Options {
                sniffer: true,
                ..defaults()
            },
        );
        let sniffer = nested(&map, SNIFFER).unwrap();
        assert_eq!(sniffer.get(Value::from(ENABLE)), Some(&Value::from(true)));
        let sniff = sniffer
            .get(Value::from("sniff"))
            .unwrap()
            .as_mapping()
            .unwrap();
        assert!(
            sniff.get(Value::from("HTTP")).is_none(),
            "свой список переписан нашим"
        );
    }

    /// Что записали, то и читается: без этого форма показывала бы одно, а ядро получало другое.
    #[test]
    fn everything_the_form_writes_reads_back_the_same() {
        let options = Options {
            log_level: LogLevel::Debug,
            mixed_port: 7890,
            sniffer: true,
            stack: Stack::Gvisor,
            device: "umiray0".into(),
            mtu: 1400,
            strict_route: true,
            dns_hijack: vec!["any:53".into()],
            open_nat: true,
            dns_enable: true,
            enhanced_mode: Enhanced::RedirHost,
            nameserver: vec!["https://1.1.1.1/dns-query".into(), "8.8.8.8".into()],
            prefer_h3: true,
        };
        assert_eq!(of(&written("", &options)).unwrap(), options);
    }
}
