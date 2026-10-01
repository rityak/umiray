//! Ссылка в запись `proxies:` (D-122).
//!
//! Это шов, которого раньше не было: до D-122 ссылки читало ядро, и узел подписки
//! оставался строкой. Цена была не в коде, а в окне — такой узел нельзя было ни показать
//! формой, ни поправить, ни назвать по имени в группе.
//!
//! Разбор **закрытый**: схема либо разложена целиком, либо не разложена вовсе. Догадка
//! здесь хуже отказа — узел, собранный наполовину, ядро примет молча и будет ходить не
//! туда. Чего не разобрали, видно в списке с пометкой и в конфиг не попадает.
//!
//! Имена полей записи — из документации mihomo, имена параметров ссылки — из стандарта
//! VShareLink, по которому их пишут панели. Соответствие между ними и есть весь модуль.
//! Схемы — все, у каких есть ссылка, из протоколов ядра (D-164); сверка с разбором самого
//! ядра — стенд S-029, и где он ошибается, мы ему не следуем.

use serde_yaml::{Mapping, Value};

use crate::nodes::link::LinkParser;
use crate::yaml::Yaml;

fn scheme_of(line: &str) -> Option<String> {
    line.split("://").next().map(str::to_lowercase)
}

pub struct Converter;

impl Converter {
    /// Разложить ссылку в запись узла. `None` — схему не знаем: выдумывать нельзя.
    pub fn to_entry(line: &str) -> Option<Mapping> {
        let scheme = scheme_of(line)?;
        let name = LinkParser::name_of(line).unwrap_or_else(|| "Узел".into());
        let (server, port) = endpoint(line, default_port(&scheme))?;
        let params = LinkParser::params(line);

        let mut entry = Mapping::new();
        Yaml::set(&mut entry, "name", Value::from(name));
        Yaml::set(&mut entry, "server", Value::from(server));
        Yaml::set(&mut entry, "port", Value::from(port));

        match scheme.as_str() {
            "tt" => trusttunnel(&mut entry, &params)?,
            "vless" => vless(&mut entry, line, &params)?,
            "vmess" => vmess(&mut entry, line)?,
            "trojan" => trojan(&mut entry, line, &params)?,
            "ss" | "shadowsocks" => shadowsocks(&mut entry, line)?,
            "ssr" => ssr(&mut entry, &params)?,
            "hysteria2" | "hy2" => hysteria2(&mut entry, line, &params)?,
            "tuic" => tuic(&mut entry, line, &params)?,
            "mierus" => mieru(&mut entry, line, &params)?,
            "anytls" => anytls(&mut entry, line, &params)?,
            "socks" | "socks5" => socks(&mut entry, line),
            "http" | "https" => http(&mut entry, line, &scheme),
            "wireguard" | "wg" => wireguard(&mut entry, line, &params)?,
            _ => return None,
        }
        Some(crate::nodes::sources::SourceStore::ordered(entry))
    }
}

/// Порт, который стандарт ссылки разрешает не писать. У остальных схем порта по умолчанию
/// нет, и без него записи не бывает — ядро её не примет.
fn default_port(scheme: &str) -> Option<u16> {
    matches!(scheme, "hysteria2" | "hy2" | "anytls").then_some(443)
}

/// Адрес и порт из `@host:port`.
fn endpoint(line: &str, default: Option<u16>) -> Option<(String, u16)> {
    let endpoint = LinkParser::endpoint_of(line)?;
    // IPv6 в ссылке пишется в скобках: `[::1]:443`.
    let (host, port) = match endpoint.strip_prefix('[') {
        Some(rest) => {
            let (host, tail) = rest.split_once(']')?;
            (host.to_string(), tail.trim_start_matches(':').to_string())
        }
        None => match endpoint.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), port.to_string()),
            None => (endpoint.clone(), String::new()),
        },
    };
    (!host.is_empty()).then_some(())?;
    let port = match port.as_str() {
        "" => default?,
        port => port.parse().ok()?,
    };
    Some((host, port))
}

type Params = std::collections::BTreeMap<String, String>;

fn text(params: &Params, key: &str) -> Option<String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Список через запятую — `alpn=h2,http/1.1`.
fn list(raw: &str) -> Value {
    Value::Sequence(
        raw.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(Value::from)
            .collect(),
    )
}

/// «Да» в ссылке пишут и единицей, и словом.
fn flag(params: &Params, key: &str) -> bool {
    matches!(
        text(params, key).as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Сертификат и имя в рукопожатии — то, что есть у всякого TLS, включая QUIC.
///
/// `sni` у vless и vmess зовётся `servername`, у остальных — `sni`: это разные поля ядра,
/// и общего имени у них нет. Пин сертификата каждая панель зовёт по-своему (`pcs`
/// у Xray, `pinSHA256` у hysteria2, `hpkp` у anytls), а поле ядра одно.
fn certificate(entry: &mut Mapping, params: &Params, sni: &str) {
    if let Some(value) = text(params, "sni").or_else(|| text(params, "peer")) {
        Yaml::set(entry, sni, Value::from(value));
    }
    if let Some(value) = text(params, "alpn") {
        Yaml::set(entry, "alpn", list(&value));
    }
    if [
        "allowInsecure",
        "allow_insecure",
        "insecure",
        "skip-cert-verify",
    ]
    .iter()
    .any(|key| flag(params, key))
    {
        Yaml::set(entry, "skip-cert-verify", Value::from(true));
    }
    if let Some(value) = ["pcs", "pinSHA256", "hpkp"]
        .iter()
        .find_map(|key| text(params, key))
    {
        Yaml::set(entry, "fingerprint", Value::from(value));
    }
}

/// TLS так, как его пишут панели: `security=tls|reality|none` плюс свои имена полей.
/// Поверх QUIC этого нет: там TLS всегда, а притвориться браузером (`fp`) нечем.
fn security(entry: &mut Mapping, params: &Params, sni: &str) {
    let kind = text(params, "security").unwrap_or_default();
    let reality = kind == "reality" || params.contains_key("pbk");
    if reality || kind == "tls" || kind == "xtls" {
        Yaml::set(entry, "tls", Value::from(true));
    }
    certificate(entry, params, sni);
    if let Some(value) = text(params, "fp") {
        Yaml::set(entry, "client-fingerprint", Value::from(value));
    }
    if reality {
        let mut opts = Mapping::new();
        if let Some(value) = text(params, "pbk") {
            Yaml::set(&mut opts, "public-key", Value::from(value));
        }
        if let Some(value) = text(params, "sid") {
            Yaml::set(&mut opts, "short-id", Value::from(value));
        }
        if !opts.is_empty() {
            Yaml::set(entry, "reality-opts", Value::Mapping(opts));
        }
    }
}

/// Транспорт: `type=ws|grpc|h2|http|xhttp` и поля выбранного. Поля чужого транспорта
/// не пишем — ядро их всё равно не прочитает, а в форме они были бы мусором.
fn transport(entry: &mut Mapping, params: &Params) {
    let Some(kind) = text(params, "type").filter(|kind| kind != "tcp") else {
        return;
    };
    Yaml::set(entry, "network", Value::from(kind.clone()));
    let host = text(params, "host");
    let path = text(params, "path");
    match kind.as_str() {
        "ws" => {
            let mut opts = Mapping::new();
            if let Some(path) = path {
                Yaml::set(&mut opts, "path", Value::from(path));
            }
            if let Some(host) = host {
                let mut headers = Mapping::new();
                Yaml::set(&mut headers, "Host", Value::from(host));
                Yaml::set(&mut opts, "headers", Value::Mapping(headers));
            }
            if !opts.is_empty() {
                Yaml::set(entry, "ws-opts", Value::Mapping(opts));
            }
        }
        "grpc" => {
            if let Some(name) = text(params, "serviceName").or_else(|| text(params, "servicename"))
            {
                let mut opts = Mapping::new();
                Yaml::set(&mut opts, "grpc-service-name", Value::from(name));
                Yaml::set(entry, "grpc-opts", Value::Mapping(opts));
            }
        }
        "h2" => {
            let mut opts = Mapping::new();
            if let Some(path) = path {
                Yaml::set(&mut opts, "path", Value::from(path));
            }
            if let Some(host) = host {
                Yaml::set(&mut opts, "host", list(&host));
            }
            if !opts.is_empty() {
                Yaml::set(entry, "h2-opts", Value::Mapping(opts));
            }
        }
        "http" => {
            let mut opts = Mapping::new();
            if let Some(path) = path {
                Yaml::set(&mut opts, "path", list(&path));
            }
            if !opts.is_empty() {
                Yaml::set(entry, "http-opts", Value::Mapping(opts));
            }
        }
        "xhttp" => {
            let mut opts = Mapping::new();
            if let Some(mode) = text(params, "mode") {
                Yaml::set(&mut opts, "mode", Value::from(mode));
            }
            if let Some(path) = path {
                Yaml::set(&mut opts, "path", Value::from(path));
            }
            if let Some(host) = host {
                Yaml::set(&mut opts, "host", Value::from(host));
            }
            if let Some(padding) = text(params, "x_padding_bytes") {
                Yaml::set(&mut opts, "x-padding-bytes", Value::from(padding));
            }
            if !opts.is_empty() {
                Yaml::set(entry, "xhttp-opts", Value::Mapping(opts));
            }
        }
        _ => {}
    }
}

fn vless(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let uuid = LinkParser::userinfo_of(line).filter(|uuid| !uuid.is_empty())?;
    Yaml::set(entry, "type", Value::from("vless"));
    Yaml::set(entry, "uuid", Value::from(uuid));
    Yaml::set(entry, "udp", Value::from(true));
    if let Some(flow) = text(params, "flow") {
        Yaml::set(entry, "flow", Value::from(flow.to_lowercase()));
    }
    if let Some(value) = text(params, "encryption").filter(|value| value != "none") {
        Yaml::set(entry, "encryption", Value::from(value));
    }
    security(entry, params, "servername");
    transport(entry, params);
    Some(())
}

/// `vmess://` — base64 от JSON, а не адрес с параметрами. Имена полей там свои
/// (`add`, `aid`, `scy`, `net`), и другого способа их прочитать нет.
fn vmess(entry: &mut Mapping, line: &str) -> Option<()> {
    let body = line.split_once("://")?.1;
    let body = body.split(['#', '?']).next()?;
    let json: serde_json::Value =
        serde_json::from_str(&crate::nodes::codec::Base64::decode_text(body)?).ok()?;
    let get = |key: &str| -> Option<String> {
        let value = json.get(key)?;
        let text = match value {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        (!text.is_empty() && text != "null").then_some(text)
    };

    Yaml::set(entry, "type", Value::from("vmess"));
    Yaml::set(entry, "server", Value::from(get("add")?));
    Yaml::set(
        entry,
        "port",
        Value::from(get("port")?.parse::<u16>().ok()?),
    );
    if let Some(name) = get("ps") {
        Yaml::set(entry, "name", Value::from(LinkParser::normalize(&name)));
    }
    Yaml::set(entry, "uuid", Value::from(get("id")?));
    Yaml::set(
        entry,
        "alterId",
        Value::from(
            get("aid")
                .and_then(|aid| aid.parse::<u32>().ok())
                .unwrap_or(0),
        ),
    );
    Yaml::set(
        entry,
        "cipher",
        Value::from(get("scy").unwrap_or_else(|| "auto".into())),
    );
    Yaml::set(entry, "udp", Value::from(true));

    let mut params: Params = Params::new();
    for (from, to) in [
        ("net", "type"),
        ("host", "host"),
        ("path", "path"),
        ("sni", "sni"),
        ("alpn", "alpn"),
        ("fp", "fp"),
        ("tls", "security"),
    ] {
        if let Some(value) = get(from) {
            params.insert(to.into(), value);
        }
    }
    security(entry, &params, "servername");
    transport(entry, &params);
    Some(())
}

fn trojan(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let password = LinkParser::userinfo_of(line).filter(|value| !value.is_empty())?;
    Yaml::set(entry, "type", Value::from("trojan"));
    Yaml::set(entry, "password", Value::from(password));
    Yaml::set(entry, "udp", Value::from(true));
    // Поля `tls` у trojan в ядре **нет**: шифрование там всегда, и ключ был бы молча
    // проигнорирован. Поэтому `security(..)` зовём после того, как он уже выставлен,
    // и тут же его снимаем — общая часть про SNI и ALPN нужна, а флаг нет.
    security(entry, params, "sni");
    entry.remove(Value::from("tls"));
    transport(entry, params);
    Some(())
}

/// `ss://` — метод и пароль либо открытым текстом, либо base64 до `@`.
fn shadowsocks(entry: &mut Mapping, line: &str) -> Option<()> {
    let userinfo = LinkParser::userinfo_of(line)?;
    let plain = if userinfo.contains(':') {
        userinfo.clone()
    } else {
        crate::nodes::codec::Base64::decode_text(&userinfo)?
    };
    let (cipher, password) = plain.split_once(':')?;
    Yaml::set(entry, "type", Value::from("ss"));
    Yaml::set(entry, "cipher", Value::from(cipher));
    Yaml::set(entry, "password", Value::from(password));
    Yaml::set(entry, "udp", Value::from(true));
    Some(())
}

/// `ssr://` — поля уже разложены `LinkParser::params`: у этой схемы всё тело в base64.
fn ssr(entry: &mut Mapping, params: &Params) -> Option<()> {
    Yaml::set(entry, "type", Value::from("ssr"));
    for (from, to) in [
        ("method", "cipher"),
        ("password", "password"),
        ("protocol", "protocol"),
        ("obfs", "obfs"),
    ] {
        Yaml::set(entry, to, Value::from(text(params, from)?));
    }
    if let Some(value) = text(params, "protoparam") {
        Yaml::set(entry, "protocol-param", Value::from(value));
    }
    if let Some(value) = text(params, "obfsparam") {
        Yaml::set(entry, "obfs-param", Value::from(value));
    }
    Yaml::set(entry, "udp", Value::from(true));
    Some(())
}

fn hysteria2(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let password = LinkParser::userinfo_of(line).filter(|value| !value.is_empty())?;
    Yaml::set(entry, "type", Value::from("hysteria2"));
    Yaml::set(entry, "password", Value::from(password));
    if let Some(obfs) = text(params, "obfs").filter(|obfs| obfs != "none") {
        Yaml::set(entry, "obfs", Value::from(obfs));
        if let Some(value) = text(params, "obfs-password").or_else(|| text(params, "obfs_password"))
        {
            Yaml::set(entry, "obfs-password", Value::from(value));
        }
    }
    certificate(entry, params, "sni");
    Some(())
}

/// `tuic://uuid:password@…` — v5; одно имя без пароля — токен v4. Стандарта у ссылки нет,
/// читаем её так же, как ядро (dae#182), плюс `allow_insecure`, который ядро выбросило,
/// а панели пишут.
fn tuic(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let (user, password) = LinkParser::credentials_of(line)?;
    (!user.is_empty()).then_some(())?;
    Yaml::set(entry, "type", Value::from("tuic"));
    match password {
        Some(password) => {
            (!password.is_empty()).then_some(())?;
            Yaml::set(entry, "uuid", Value::from(user));
            Yaml::set(entry, "password", Value::from(password));
        }
        None => Yaml::set(entry, "token", Value::from(user)),
    }
    if let Some(value) = text(params, "congestion_control") {
        Yaml::set(entry, "congestion-controller", Value::from(value));
    }
    if let Some(value) = text(params, "udp_relay_mode") {
        Yaml::set(entry, "udp-relay-mode", Value::from(value));
    }
    if flag(params, "disable_sni") {
        Yaml::set(entry, "disable-sni", Value::from(true));
    }
    certificate(entry, params, "sni");
    Some(())
}

/// `anytls://пароль@…` (anytls-go, `uri_scheme.md`). Логина у протокола нет: `user:pass`
/// бывает у чужих панелей, и тогда паролем служит вторая половина — так читает и ядро.
/// Поля `tls` нет, как у trojan: шифрование всегда.
fn anytls(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let (user, password) = LinkParser::credentials_of(line)?;
    let password = password.unwrap_or(user);
    (!password.is_empty()).then_some(())?;
    Yaml::set(entry, "type", Value::from("anytls"));
    Yaml::set(entry, "password", Value::from(password));
    Yaml::set(entry, "udp", Value::from(true));
    security(entry, params, "sni");
    entry.remove(Value::from("tls"));
    Some(())
}

/// `mierus://логин:пароль@host?port=…&protocol=TCP` — по одной паре на ссылку: несколько
/// пар `LinkParser::split` разводит раньше. Диапазон портов у ядра — отдельное поле
/// `port-range`, и `port` рядом с ним ядро не примет.
fn mieru(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let (user, password) = LinkParser::credentials_of(line)?;
    let password = password.filter(|password| !password.is_empty() && !user.is_empty())?;
    let transport = text(params, "protocol")?.to_uppercase();
    matches!(transport.as_str(), "TCP" | "UDP").then_some(())?;
    Yaml::set(entry, "type", Value::from("mieru"));
    if let Some(range) = text(params, "port").filter(|port| port.contains('-')) {
        entry.remove(Value::from("port"));
        Yaml::set(entry, "port-range", Value::from(range));
    }
    Yaml::set(entry, "transport", Value::from(transport));
    Yaml::set(entry, "username", Value::from(user));
    Yaml::set(entry, "password", Value::from(password));
    Yaml::set(entry, "udp", Value::from(true));
    for key in ["multiplexing", "handshake-mode", "traffic-pattern"] {
        if let Some(value) = text(params, key) {
            Yaml::set(entry, key, Value::from(value));
        }
    }
    Some(())
}

/// `tt://?` — поля разложены `LinkParser::params` (TLV внутри base64). Имя сервера в ссылке —
/// то, на что выписан сертификат: оно и есть SNI, если своего `custom_sni` нет.
///
/// Сертификат из ссылки — пин: сервер со своим сертификатом, и проверить его иначе нечем.
/// Префикс `client_random` ядро выставить не умеет, а сервер по нему решает, пускать ли, —
/// с ним отказ, а не узел, который молча не встанет. `anti_dpi` — приём клиента, серверу
/// не нужен; у ядра его нет.
fn trusttunnel(entry: &mut Mapping, params: &Params) -> Option<()> {
    text(params, "client_random_prefix")
        .is_none()
        .then_some(())?;
    Yaml::set(entry, "type", Value::from("trusttunnel"));
    Yaml::set(entry, "username", Value::from(text(params, "username")?));
    Yaml::set(entry, "password", Value::from(text(params, "password")?));
    Yaml::set(entry, "udp", Value::from(true));
    if let Some(sni) = text(params, "custom_sni").or_else(|| text(params, "hostname")) {
        Yaml::set(entry, "sni", Value::from(sni));
    }
    if flag(params, "skip_verification") {
        Yaml::set(entry, "skip-cert-verify", Value::from(true));
    } else if let Some(chain) = text(params, "certificate") {
        Yaml::set(entry, "fingerprint", Value::from(leaf_sha256(&chain)?));
    }
    if text(params, "upstream_protocol").as_deref() == Some("2") {
        Yaml::set(entry, "quic", Value::from(true));
    }
    Some(())
}

/// SHA-256 первого сертификата DER-цепочки (hex) — так ядро читает `fingerprint`.
/// Длина сертификата — из его заголовка `SEQUENCE`: цепочка склеена без разделителей.
fn leaf_sha256(chain_hex: &str) -> Option<String> {
    use sha2::Digest;
    let der: Vec<u8> = (0..chain_hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(chain_hex.get(at..at + 2)?, 16).ok())
        .collect::<Option<_>>()?;
    (*der.first()? == 0x30).then_some(())?;
    let first = *der.get(1)?;
    let (header, size) = match first {
        0..=0x7f => (2, usize::from(first)),
        0x81..=0x84 => {
            let count = usize::from(first & 0x7f);
            let size = der
                .get(2..2 + count)?
                .iter()
                .fold(0usize, |n, b| (n << 8) | usize::from(*b));
            (2 + count, size)
        }
        _ => return None,
    };
    let leaf = der.get(..header + size)?;
    Some(
        sha2::Sha256::digest(leaf)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn socks(entry: &mut Mapping, line: &str) {
    Yaml::set(entry, "type", Value::from("socks5"));
    Yaml::set(entry, "udp", Value::from(true));
    credentials(entry, line);
}

fn http(entry: &mut Mapping, line: &str, scheme: &str) {
    Yaml::set(entry, "type", Value::from("http"));
    if scheme == "https" {
        Yaml::set(entry, "tls", Value::from(true));
    }
    credentials(entry, line);
}

/// Логин и пароль до `@` — открытым текстом или base64, как их пишут обе стороны.
fn credentials(entry: &mut Mapping, line: &str) {
    let pair = match LinkParser::credentials_of(line) {
        Some((user, Some(password))) => Some((user, password)),
        Some((encoded, None)) => {
            crate::nodes::codec::Base64::decode_text(&encoded).and_then(|plain| {
                let (user, password) = plain.split_once(':')?;
                Some((user.to_string(), password.to_string()))
            })
        }
        None => None,
    };
    if let Some((user, password)) = pair.filter(|(user, _)| !user.is_empty()) {
        Yaml::set(entry, "username", Value::from(user));
        Yaml::set(entry, "password", Value::from(password));
    }
}

/// `wireguard://` — тот же перевод, что жил в `nodes/outbound.rs` до D-122.
///
/// Выход наружу дописывается целиком: ссылка не несёт `AllowedIPs`, а узел из подписки
/// заводят ровно ради него. `dns=` не переносим: у ядра он работает только вместе с
/// `remote-dns-resolve`, а именами занимается свой раздел — половина механизма хуже,
/// чем его отсутствие.
fn wireguard(entry: &mut Mapping, line: &str, params: &Params) -> Option<()> {
    let private = LinkParser::userinfo_of(line).filter(|value| !value.is_empty())?;
    let public = text(params, "publickey").or_else(|| text(params, "publicKey"))?;
    Yaml::set(entry, "type", Value::from("wireguard"));
    Yaml::set(entry, "private-key", Value::from(private));
    Yaml::set(entry, "public-key", Value::from(public));
    if let Some(psk) = text(params, "presharedkey").or_else(|| text(params, "presharedKey")) {
        Yaml::set(entry, "pre-shared-key", Value::from(psk));
    }

    let mut outward = vec![Value::from("0.0.0.0/0")];
    for address in text(params, "address")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let bare = address.split('/').next().unwrap_or(address);
        if bare.contains(':') {
            Yaml::set(entry, "ipv6", Value::from(bare));
            outward.push(Value::from("::/0"));
        } else {
            Yaml::set(entry, "ip", Value::from(bare));
        }
    }
    if let Some(mtu) = text(params, "mtu").and_then(|mtu| mtu.parse::<u32>().ok()) {
        Yaml::set(entry, "mtu", Value::from(mtu));
    }
    if let Some(keepalive) = text(params, "keepalive")
        .or_else(|| text(params, "persistentkeepalive"))
        .and_then(|value| value.parse::<u32>().ok())
    {
        Yaml::set(entry, "persistent-keepalive", Value::from(keepalive));
    }
    Yaml::set(entry, "allowed-ips", Value::Sequence(outward));
    Yaml::set(entry, "udp", Value::from(true));
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(entry: &'a Mapping, key: &str) -> Option<&'a Value> {
        entry.get(Value::from(key))
    }

    /// Живой вид ссылки от панели: reality со всеми полями, какие она кладёт.
    #[test]
    fn a_reality_link_becomes_the_entry_the_core_reads() {
        let line = "vless://11111111-2222-3333-4444-555555555555@a.example:443?type=tcp&security=reality&pbk=PBK&sid=SID&fp=chrome&sni=www.microsoft.com&flow=xtls-rprx-vision&encryption=none&spx=%2F#Швеция";
        let entry = Converter::to_entry(line).expect("разобрали");
        assert_eq!(field(&entry, "type").unwrap(), "vless");
        assert_eq!(field(&entry, "name").unwrap(), "Швеция");
        assert_eq!(field(&entry, "server").unwrap(), "a.example");
        assert_eq!(field(&entry, "port").unwrap(), 443);
        assert_eq!(field(&entry, "tls").unwrap().as_bool(), Some(true));
        assert_eq!(field(&entry, "servername").unwrap(), "www.microsoft.com");
        assert_eq!(field(&entry, "client-fingerprint").unwrap(), "chrome");
        assert_eq!(field(&entry, "flow").unwrap(), "xtls-rprx-vision");
        let reality = field(&entry, "reality-opts").unwrap().as_mapping().unwrap();
        assert_eq!(reality.get(Value::from("public-key")).unwrap(), "PBK");
        assert_eq!(reality.get(Value::from("short-id")).unwrap(), "SID");
        // `type=tcp` — это отсутствие транспорта, а не поле `network: tcp`.
        assert!(field(&entry, "network").is_none(), "{entry:?}");
        // `encryption=none` у ядра умолчание, и писать его значит спорить с ним же.
        assert!(field(&entry, "encryption").is_none());
        // Поля, которого ядро не знает, в записи быть не должно (D-120).
        assert!(field(&entry, "spx").is_none());
    }

    /// Транспорт пишется только свой: путь ws при gRPC ядро не прочитает.
    #[test]
    fn only_the_chosen_transport_travels() {
        let ws = Converter::to_entry("vless://u@a.example:443?type=ws&path=%2Fray&host=b.example&security=tls&sni=b.example&allowInsecure=1#N").unwrap();
        let opts = field(&ws, "ws-opts").unwrap().as_mapping().unwrap();
        assert_eq!(opts.get(Value::from("path")).unwrap(), "/ray");
        assert_eq!(
            opts.get(Value::from("headers"))
                .unwrap()
                .as_mapping()
                .unwrap()
                .get(Value::from("Host"))
                .unwrap(),
            "b.example"
        );
        assert_eq!(
            field(&ws, "skip-cert-verify").unwrap().as_bool(),
            Some(true)
        );

        let grpc = Converter::to_entry(
            "vless://u@a.example:443?type=grpc&serviceName=gun&path=%2Fнеправда#N",
        )
        .unwrap();
        assert!(field(&grpc, "ws-opts").is_none(), "{grpc:?}");
        assert_eq!(
            field(&grpc, "grpc-opts")
                .unwrap()
                .as_mapping()
                .unwrap()
                .get(Value::from("grpc-service-name"))
                .unwrap(),
            "gun"
        );
    }

    /// У `vmess://` всё внутри base64-JSON, и имена полей там свои.
    #[test]
    fn a_vmess_link_is_json_inside_base64() {
        use base64::Engine;
        let json = r#"{"v":"2","ps":"Токио","add":"b.example","port":"8443","id":"UUID","aid":"0","scy":"auto","net":"ws","host":"b.example","path":"/vm","tls":"tls","sni":"b.example"}"#;
        let line = format!(
            "vmess://{}",
            base64::engine::general_purpose::STANDARD.encode(json)
        );
        let entry = Converter::to_entry(&line).expect("разобрали");
        assert_eq!(field(&entry, "type").unwrap(), "vmess");
        assert_eq!(field(&entry, "name").unwrap(), "Токио");
        assert_eq!(field(&entry, "port").unwrap(), 8443);
        assert_eq!(field(&entry, "alterId").unwrap(), 0);
        assert_eq!(field(&entry, "cipher").unwrap(), "auto");
        assert_eq!(field(&entry, "tls").unwrap().as_bool(), Some(true));
        assert_eq!(field(&entry, "network").unwrap(), "ws");
    }

    #[test]
    fn trojan_hysteria2_and_shadowsocks_carry_their_secret() {
        let trojan = Converter::to_entry(
            "trojan://pa%40ss@a.example:443?sni=a.example&alpn=h2%2Chttp%2F1.1#T",
        )
        .unwrap();
        assert_eq!(field(&trojan, "password").unwrap(), "pa@ss");
        assert!(
            field(&trojan, "tls").is_none(),
            "такого поля у trojan в ядре нет"
        );
        assert_eq!(field(&trojan, "sni").unwrap(), "a.example");
        assert_eq!(
            field(&trojan, "alpn").unwrap().as_sequence().unwrap().len(),
            2
        );

        let hy2 = Converter::to_entry("hysteria2://pw@a.example:8443?obfs=salamander&obfs-password=op&sni=a.example&insecure=1#H").unwrap();
        assert_eq!(field(&hy2, "type").unwrap(), "hysteria2");
        assert_eq!(field(&hy2, "obfs").unwrap(), "salamander");
        assert_eq!(field(&hy2, "obfs-password").unwrap(), "op");
        assert_eq!(
            field(&hy2, "skip-cert-verify").unwrap().as_bool(),
            Some(true)
        );

        use base64::Engine;
        let userinfo = base64::engine::general_purpose::STANDARD.encode("aes-256-gcm:secret");
        let ss = Converter::to_entry(&format!("ss://{userinfo}@a.example:8388#S")).unwrap();
        assert_eq!(field(&ss, "cipher").unwrap(), "aes-256-gcm");
        assert_eq!(field(&ss, "password").unwrap(), "secret");
    }

    /// Выход наружу у wireguard дописывает клиент: ссылка его не несёт, а заводят узел
    /// ровно ради него.
    #[test]
    fn a_wireguard_link_gets_the_whole_route() {
        let line = "wireguard://cHJpdmF0ZQ%3D%3D@a.example:51820?publickey=cHVibGlj&address=10.0.0.2%2F32,fd00::2%2F128&mtu=1420#W";
        let entry = Converter::to_entry(line).unwrap();
        assert_eq!(field(&entry, "ip").unwrap(), "10.0.0.2");
        assert_eq!(field(&entry, "ipv6").unwrap(), "fd00::2");
        assert_eq!(field(&entry, "mtu").unwrap(), 1420);
        let route = field(&entry, "allowed-ips").unwrap().as_sequence().unwrap();
        assert_eq!(route.len(), 2, "и IPv4, и IPv6: {route:?}");
        assert_eq!(field(&entry, "udp").unwrap().as_bool(), Some(true));
    }

    /// TUIC v5 — `uuid:пароль`, v4 — один токен. Поля QUIC, а не TLS поверх TCP:
    /// ни `tls`, ни `client-fingerprint` у него нет.
    #[test]
    fn a_tuic_link_is_v5_with_a_password_and_v4_without() {
        let v5 = Converter::to_entry("tuic://11111111-2222-3333-4444-555555555555:p%40ss@a.example:443?congestion_control=bbr&udp_relay_mode=native&alpn=h3&sni=b.example&allow_insecure=1&disable_sni=1&fp=chrome#T").unwrap();
        assert_eq!(field(&v5, "type").unwrap(), "tuic");
        assert_eq!(
            field(&v5, "uuid").unwrap(),
            "11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(field(&v5, "password").unwrap(), "p@ss");
        assert_eq!(field(&v5, "congestion-controller").unwrap(), "bbr");
        assert_eq!(field(&v5, "udp-relay-mode").unwrap(), "native");
        assert_eq!(field(&v5, "sni").unwrap(), "b.example");
        assert_eq!(field(&v5, "alpn").unwrap().as_sequence().unwrap().len(), 1);
        assert_eq!(
            field(&v5, "skip-cert-verify").unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(field(&v5, "disable-sni").unwrap().as_bool(), Some(true));
        for absent in ["tls", "client-fingerprint", "token", "udp"] {
            assert!(field(&v5, absent).is_none(), "{absent}: {v5:?}");
        }

        let v4 = Converter::to_entry("tuic://TOKEN@a.example:443#T4").unwrap();
        assert_eq!(field(&v4, "token").unwrap(), "TOKEN");
        assert!(field(&v4, "uuid").is_none());
    }

    /// Диапазон портов у mieru — `port-range` **вместо** `port`: оба ядро не примет.
    #[test]
    fn a_mieru_link_names_its_transport_and_range() {
        let entry = Converter::to_entry("mierus://u:p%40ss@1.2.3.4?multiplexing=MULTIPLEXING_HIGH&port=9998-9999&protocol=udp#M").unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "mieru");
        assert_eq!(field(&entry, "server").unwrap(), "1.2.3.4");
        assert!(field(&entry, "port").is_none(), "{entry:?}");
        assert_eq!(field(&entry, "port-range").unwrap(), "9998-9999");
        assert_eq!(field(&entry, "transport").unwrap(), "UDP");
        assert_eq!(field(&entry, "username").unwrap(), "u");
        assert_eq!(field(&entry, "password").unwrap(), "p@ss");
        assert_eq!(field(&entry, "multiplexing").unwrap(), "MULTIPLEXING_HIGH");

        let tcp = Converter::to_entry("mierus://u:p@h.example?port=2999&protocol=TCP#M").unwrap();
        assert_eq!(field(&tcp, "port").unwrap(), 2999);
        assert!(Converter::to_entry("mierus://u:p@h.example?port=2999#без-транспорта").is_none());
    }

    /// TrustTunnel: адрес и учётка из TLV, имя сервера — SNI, сертификат — пин по листу.
    #[test]
    fn a_trusttunnel_link_pins_the_certificate_it_carries() {
        use crate::nodes::codec::Base64;
        let tlv = |tag: u8, value: &[u8]| {
            let mut out = vec![tag, value.len() as u8];
            out.extend_from_slice(value);
            out
        };
        // Два «сертификата» подряд: пин считается только по первому.
        let leaf = [0x30u8, 0x03, 1, 2, 3];
        let chain = [&leaf[..], &[0x30, 0x01, 9]].concat();
        let payload = [
            tlv(1, b"vpn.example.com"),
            tlv(2, b"1.2.3.4:443"),
            tlv(2, b"5.6.7.8:443"),
            tlv(5, b"alice"),
            tlv(6, b"secret"),
            tlv(8, &chain),
            tlv(9, &[2]),
            tlv(12, "Мой сервер".as_bytes()),
        ]
        .concat();
        let line = format!("tt://?{}", Base64::encode_url(&payload));
        let entry = Converter::to_entry(&line).unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "trusttunnel");
        assert_eq!(field(&entry, "name").unwrap(), "Мой сервер");
        assert_eq!(field(&entry, "server").unwrap(), "1.2.3.4");
        assert_eq!(field(&entry, "port").unwrap(), 443);
        assert_eq!(field(&entry, "sni").unwrap(), "vpn.example.com");
        assert_eq!(field(&entry, "username").unwrap(), "alice");
        assert_eq!(field(&entry, "quic").unwrap().as_bool(), Some(true));
        use sha2::Digest;
        let pin: String = sha2::Sha256::digest(leaf)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(field(&entry, "fingerprint").unwrap(), pin.as_str());

        let gated = [payload, tlv(11, b"aabb")].concat();
        assert!(
            Converter::to_entry(&format!("tt://?{}", Base64::encode_url(&gated))).is_none(),
            "client_random ядро не выставит, а сервер по нему решает"
        );
    }

    /// Имена полей у ssr свои (`method`, `obfsparam`), а у ядра — свои (`cipher`, `obfs-param`).
    #[test]
    fn an_ssr_link_is_base64_all_the_way_down() {
        use crate::nodes::codec::Base64;
        let body = format!(
            "a.example:8388:auth_aes128_sha1:aes-256-cfb:tls1.2_ticket_auth:{}/?obfsparam={}&protoparam={}&remarks={}",
            Base64::encode_url(b"p:w"),
            Base64::encode_url(b"cdn.example"),
            Base64::encode_url(b"1:key"),
            Base64::encode_url("Токио".as_bytes())
        );
        let line = format!("ssr://{}", Base64::encode_url(body.as_bytes()));
        let entry = Converter::to_entry(&line).unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "ssr");
        assert_eq!(field(&entry, "name").unwrap(), "Токио");
        assert_eq!(field(&entry, "server").unwrap(), "a.example");
        assert_eq!(field(&entry, "port").unwrap(), 8388);
        assert_eq!(field(&entry, "cipher").unwrap(), "aes-256-cfb");
        assert_eq!(field(&entry, "password").unwrap(), "p:w");
        assert_eq!(field(&entry, "protocol").unwrap(), "auth_aes128_sha1");
        assert_eq!(field(&entry, "obfs").unwrap(), "tls1.2_ticket_auth");
        assert_eq!(field(&entry, "obfs-param").unwrap(), "cdn.example");
        assert_eq!(field(&entry, "protocol-param").unwrap(), "1:key");

        let short = format!("ssr://{}", Base64::encode_url(b"a.example:8388:origin"));
        assert!(
            Converter::to_entry(&short).is_none(),
            "голова без метода и пароля — не узел"
        );
    }

    /// Порт у anytls и hysteria2 необязателен — 443 по стандарту их ссылок. У vless его
    /// нет, и там отказ остаётся отказом (`what_we_cannot_read_we_do_not_invent`).
    #[test]
    fn an_anytls_link_carries_its_password_and_may_skip_the_port() {
        let entry =
            Converter::to_entry("anytls://pa%3Ass@a.example/?sni=b.example&insecure=1&hpkp=AB#A")
                .unwrap();
        assert_eq!(field(&entry, "type").unwrap(), "anytls");
        assert_eq!(field(&entry, "port").unwrap(), 443);
        assert_eq!(field(&entry, "password").unwrap(), "pa:ss");
        assert_eq!(field(&entry, "sni").unwrap(), "b.example");
        assert_eq!(field(&entry, "fingerprint").unwrap(), "AB");
        assert_eq!(field(&entry, "udp").unwrap().as_bool(), Some(true));
        assert!(field(&entry, "tls").is_none(), "такого поля у anytls нет");
        assert!(field(&entry, "username").is_none(), "и такого тоже");

        let split = Converter::to_entry("anytls://user:pw@[::1]:8443#A").unwrap();
        assert_eq!(field(&split, "password").unwrap(), "pw");
        assert_eq!(field(&split, "server").unwrap(), "::1");

        let hy2 = Converter::to_entry("hysteria2://pw@a.example#H").unwrap();
        assert_eq!(field(&hy2, "port").unwrap(), 443);
    }

    /// Догадка хуже отказа: узел, собранный наполовину, ядро примет молча.
    #[test]
    fn what_we_cannot_read_we_do_not_invent() {
        for line in [
            "juicity://uuid:pass@a.example:443#J",
            // Hysteria v1 выпилен (S-029): UDP в ядре v1.19.30 сломан, узел работал бы наполовину.
            "hysteria://a.example:443?auth=pw&upmbps=10&downmbps=10#H1",
            "tuic://uuid:@a.example:443#без-пароля",
            "anytls://@a.example:443#без-пароля",
            "vless://@a.example:443#без-uuid",
            "vless://a.example:443#uuid@в-имени",
            "vless://uuid@a.example#без-порта",
            "не ссылка вовсе",
        ] {
            assert!(
                Converter::to_entry(line).is_none(),
                "выдумали запись из «{line}»"
            );
        }
    }

    /// Имя узла из фрагмента, а порядок ключей — человеческий: по нему запись и читают.
    #[test]
    fn the_entry_reads_in_the_order_a_person_expects() {
        let entry = Converter::to_entry("vless://u@a.example:443?security=tls&sni=s#Имя").unwrap();
        let keys: Vec<String> = entry
            .keys()
            .filter_map(|key| key.as_str().map(str::to_string))
            .collect();
        assert_eq!(keys[..4], ["name", "type", "server", "port"]);
    }
}
