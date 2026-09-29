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
        let (server, port) = endpoint(line)?;
        let params = LinkParser::params(line);

        let mut entry = Mapping::new();
        Yaml::set(&mut entry, "name", Value::from(name));
        Yaml::set(&mut entry, "server", Value::from(server));
        Yaml::set(&mut entry, "port", Value::from(port));

        match scheme.as_str() {
            "vless" => vless(&mut entry, line, &params)?,
            "vmess" => vmess(&mut entry, line)?,
            "trojan" => trojan(&mut entry, line, &params)?,
            "ss" | "shadowsocks" => shadowsocks(&mut entry, line)?,
            "hysteria2" | "hy2" => hysteria2(&mut entry, line, &params)?,
            "socks" | "socks5" => socks(&mut entry, line),
            "http" | "https" => http(&mut entry, line, &scheme),
            "wireguard" | "wg" => wireguard(&mut entry, line, &params)?,
            _ => return None,
        }
        Some(crate::nodes::sources::SourceStore::ordered(entry))
    }
}

/// Адрес и порт из `@host:port`. Без порта записи не бывает — ядро её не примет.
fn endpoint(line: &str) -> Option<(String, u16)> {
    let endpoint = LinkParser::endpoint_of(line)?;
    // IPv6 в ссылке пишется в скобках: `[::1]:443`.
    let (host, port) = match endpoint.strip_prefix('[') {
        Some(rest) => {
            let (host, tail) = rest.split_once(']')?;
            (host.to_string(), tail.trim_start_matches(':').to_string())
        }
        None => {
            let (host, port) = endpoint.rsplit_once(':')?;
            (host.to_string(), port.to_string())
        }
    };
    (!host.is_empty()).then_some(())?;
    Some((host, port.parse().ok()?))
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

/// TLS так, как его пишут панели: `security=tls|reality|none` плюс свои имена полей.
///
/// `sni` у vless и vmess зовётся `servername`, у trojan и hysteria2 — `sni`: это разные
/// поля ядра, и общего имени у них нет.
fn security(entry: &mut Mapping, params: &Params, sni: &str) {
    let kind = text(params, "security").unwrap_or_default();
    let reality = kind == "reality" || params.contains_key("pbk");
    if reality || kind == "tls" || kind == "xtls" {
        Yaml::set(entry, "tls", Value::from(true));
    }
    if let Some(value) = text(params, "sni").or_else(|| text(params, "peer")) {
        Yaml::set(entry, sni, Value::from(value));
    }
    if let Some(value) = text(params, "alpn") {
        Yaml::set(entry, "alpn", list(&value));
    }
    if let Some(value) = text(params, "fp") {
        Yaml::set(entry, "client-fingerprint", Value::from(value));
    }
    if flag(params, "allowInsecure") || flag(params, "insecure") || flag(params, "skip-cert-verify")
    {
        Yaml::set(entry, "skip-cert-verify", Value::from(true));
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
    // У hysteria2 то же самое: шифрование всегда, поля `tls` в его опциях нет.
    security(entry, params, "sni");
    entry.remove(Value::from("tls"));
    Some(())
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
    let Some(userinfo) = LinkParser::userinfo_of(line).filter(|value| !value.is_empty()) else {
        return;
    };
    let plain = if userinfo.contains(':') {
        userinfo.clone()
    } else {
        crate::nodes::codec::Base64::decode_text(&userinfo).unwrap_or_else(|| userinfo.clone())
    };
    if let Some((user, password)) = plain.split_once(':') {
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

    /// Догадка хуже отказа: узел, собранный наполовину, ядро примет молча.
    #[test]
    fn what_we_cannot_read_we_do_not_invent() {
        for line in [
            "tuic://uuid:pass@a.example:443?alpn=h3#TUIC",
            "anytls://pw@a.example:443#A",
            "vless://@a.example:443#без-uuid",
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
