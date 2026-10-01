//! Клиент к HTTP API ядра — `external-controller` (D-007).
//!
//! Секрет и порт рождаются при каждом запуске ядра и живут только здесь: в вебвью не уходит
//! ни то, ни другое, интерфейс общается с ядром исключительно через команды Tauri.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct Controller {
    address: String,
    secret: String,
    /// Память о закрывшихся соединениях (S-018). Под `Arc`, потому что управляющий
    /// достаётся вызывающим копией: без общей ссылки каждая копия считала бы своё.
    /// Живёт ровно столько же, сколько ядро, — как и его собственные счётчики.
    ledger: Arc<Mutex<Ledger>>,
}

/// Чего ядро не помнит: сколько унесли с собой закрывшиеся соединения (D-111).
///
/// `GET /connections` отдаёт только открытые. Закрывшееся между тактами пропадает вместе
/// со своими байтами, сумма узла падает, и разница между отсчётами уходит в ноль —
/// у короткоживущих соединений (а это весь обычный веб) скорость оказывалась заниженной.
///
/// Поэтому здесь два поля: что видели в прошлый раз у каждого открытого — и сколько
/// уже унесли пропавшие, по узлам.
#[derive(Debug, Default)]
struct Ledger {
    seen: HashMap<String, Seen>,
    gone: HashMap<String, (u64, u64)>,
}

#[derive(Debug, Clone)]
struct Seen {
    node: String,
    up: u64,
    down: u64,
}

impl Ledger {
    /// Свести прошлый отсчёт с новым: пропавшие идут в `gone`, оставшиеся — в `seen`.
    /// Возвращает то, что унесли, — суммой по узлам.
    fn settle(&mut self, live: &[Connection]) -> &HashMap<String, (u64, u64)> {
        let mut now: HashMap<String, Seen> = HashMap::with_capacity(live.len());
        for connection in live {
            let Some(node) = connection.chains.first() else {
                continue;
            };
            now.insert(
                connection.id.clone(),
                Seen {
                    node: node.clone(),
                    up: connection.upload,
                    down: connection.download,
                },
            );
        }
        for (id, was) in &self.seen {
            if now.contains_key(id) {
                continue;
            }
            let carried = self.gone.entry(was.node.clone()).or_insert((0, 0));
            carried.0 += was.up;
            carried.1 += was.down;
        }
        self.seen = now;
        &self.gone
    }
}

/// Суммарно передано с начала работы ядра. Скорость считает интерфейс: он знает, сколько
/// прошло между опросами, а бэкенд состояния между вызовами не хранит.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traffic {
    pub up: u64,
    pub down: u64,
    pub connections: usize,
    /// Через кого идёт трафик прямо сейчас (S-018). Считается из того же ответа —
    /// второго запроса и второго такта не нужно.
    pub nodes: Vec<NodeTraffic>,
}

/// Что прошло через один узел по **открытым сейчас** соединениям.
///
/// Именно по открытым: закрытое соединение из ответа ядра пропадает, и накопить «сколько
/// всего» можно было бы только своей памятью. Вопрос, на который отвечает таблица, —
/// «кто везёт сейчас», и на него открытых соединений достаточно.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTraffic {
    pub node: String,
    pub up: u64,
    pub down: u64,
    pub connections: usize,
}

/// Запись `/proxies`: узел или группа. `now` есть только у групп.
#[derive(Deserialize)]
struct ProxyEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    now: Option<String>,
    #[serde(default)]
    all: Option<Vec<String>>,
}

/// Пройти по `now` от группы до узла. Отдельно от запроса — правило проверяется без ядра.
/// Потолок шагов — от петли групп друг на друга: ядро её не примет, но окно зависнуть
/// из-за неё не должно.
fn follow(from: &str, now: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut route = vec![from.to_string()];
    while route.len() < 8 {
        match now(route.last().unwrap()) {
            Some(next) if !next.is_empty() && !route.contains(&next) => route.push(next),
            _ => break,
        }
    }
    route
}

/// Группа выбора: где сейчас стоит галочка и из чего можно выбирать.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    pub now: String,
    pub options: Vec<String>,
}

impl Controller {
    /// Порт свободный ищем сами (D-007) — тем же способом, что и служебный вход замера.
    pub fn new() -> Result<Self> {
        Ok(Self {
            address: format!("127.0.0.1:{}", crate::core::Ports::free_port()?),
            secret: secret()?,
            ledger: Arc::new(Mutex::new(Ledger::default())),
        })
    }

    /// Порт управления. Нужен стенду как готовое уникальное имя для его папки:
    /// порт уже выбран так, чтобы не совпасть с чужим (D-098).
    pub fn port(&self) -> &str {
        self.address.rsplit(':').next().unwrap_or(&self.address)
    }

    /// Аргументы для ядра. Единственное место, где секрет покидает этот модуль.
    pub fn args(&self) -> [String; 4] {
        [
            "-ext-ctl".into(),
            self.address.clone(),
            "-secret".into(),
            self.secret.clone(),
        ]
    }

    /// Отвечает ли ядро. Это и есть настоящая готовность: в отличие от прокси-порта, она
    /// проверяется одинаково в обоих режимах — в TUN слушающего порта нет (B-001, D-013).
    pub async fn ready(&self) -> bool {
        self.get("/version").await.is_ok()
    }

    pub async fn traffic(&self) -> Result<Traffic> {
        let body: Connections = serde_json::from_str(&self.get("/connections").await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))?;
        let live = body.connections.unwrap_or_default();
        let nodes = {
            let mut ledger = self.ledger.lock().unwrap();
            by_node(&live, ledger.settle(&live))
        };
        Ok(Traffic {
            up: body.upload_total,
            down: body.download_total,
            connections: live.len(),
            nodes,
        })
    }

    /// Группы, в которых сервер выбирается вручную. Остальные типы (`url-test`, `fallback`)
    /// ядро ведёт само — показывать их как переключатель было бы враньём.
    ///
    /// `GLOBAL` ядро заводит всегда и само: в режиме `rule` переключение в ней ни на что
    /// не влияет. Замерено на живом ядре — она приходит второй Selector-группой.
    pub async fn groups(&self) -> Result<Vec<Group>> {
        let mut groups: Vec<Group> = self
            .proxies()
            .await?
            .into_values()
            .filter(|proxy| proxy.kind == "Selector" && proxy.name != "GLOBAL")
            .map(|proxy| Group {
                name: proxy.name,
                now: proxy.now.unwrap_or_default(),
                options: proxy.all.unwrap_or_default(),
            })
            .collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(groups)
    }

    /// Куда на самом деле уходит трафик группы: `umiray → AUTO → Poland 1` (D-145).
    ///
    /// Спрашиваем все группы, а не только `Selector`: `AUTO` — это `url-test`, и его
    /// `now` — ровно тот узел, который ядро выбрало. Цепочка кончается на том, у кого
    /// `now` нет, то есть на узле.
    pub async fn route(&self, from: &str) -> Result<Vec<String>> {
        let proxies = self.proxies().await?;
        Ok(follow(from, |name| {
            proxies.get(name).and_then(|proxy| proxy.now.clone())
        }))
    }

    async fn proxies(&self) -> Result<HashMap<String, ProxyEntry>> {
        #[derive(Deserialize)]
        struct Proxies {
            proxies: HashMap<String, ProxyEntry>,
        }
        let body: Proxies = serde_json::from_str(&self.get("/proxies").await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))?;
        Ok(body.proxies)
    }

    /// Прогнать проверку задержки у всех узлов источника разом (D-069).
    ///
    /// Именно `GET` и именно у провайдера: `PUT` на тот же путь — 404 (B-003), а узлы
    /// провайдера не адресуются поимённо — `GET /proxies/<узел>/delay` тоже 404
    /// (S-011, S-012).
    pub async fn healthcheck(&self, source: &str) -> Result<()> {
        self.get(&format!(
            "/providers/proxies/{}/healthcheck",
            encode(source)
        ))
        .await
        .map(|_| ())
    }

    /// Что ядро намеряло по каждому узлу, свежее — последним.
    ///
    /// Отдаём историю как есть, без выбора «лучшего»: правило выбора принадлежит замеру
    /// (`app/measure.rs`), а не клиенту HTTP.
    pub async fn delays(&self) -> Result<std::collections::HashMap<String, Vec<u32>>> {
        #[derive(Deserialize)]
        struct Entry {
            delay: u32,
        }
        #[derive(Deserialize)]
        struct Proxy {
            name: String,
            #[serde(default)]
            history: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Provider {
            #[serde(default)]
            proxies: Vec<Proxy>,
        }
        #[derive(Deserialize)]
        struct Providers {
            providers: std::collections::HashMap<String, Provider>,
        }

        let body: Providers = serde_json::from_str(&self.get("/providers/proxies").await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))?;
        Ok(body
            .providers
            .into_values()
            .flat_map(|provider| provider.proxies)
            .map(|proxy| {
                let history = proxy.history.into_iter().map(|entry| entry.delay).collect();
                (proxy.name, history)
            })
            .collect())
    }

    /// Перечитать rule set с диска без перезапуска ядра (D-157). Провайдера с таким именем
    /// может не быть — список ещё не участвует в правилах, — это не ошибка.
    pub async fn reload_rules(&self, name: &str) -> Result<()> {
        let response = crate::http::Http::direct()?
            .put(format!("{}/providers/rules/{}", self.base(), encode(name)))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::network(format!(
                "Ядро не перечитало список: {status}"
            )));
        }
        Ok(())
    }

    /// Обновить geo-базы (D-157): ядро скачивает их по своим `geox-url` и перечитывает.
    pub async fn update_geo(&self) -> Result<()> {
        let response = crate::http::Http::direct()?
            .post(format!("{}/configs/geo", self.base()))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            // Ядро отвечает `{"message": "…"}`; причина едет в подробностях, а не в заголовке
            // (D-028). Качает оно через свои же правила, то есть через VPN, — мёртвый узел
            // тоже причина.
            let said = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value["message"].as_str().map(str::to_string))
                .unwrap_or(body);
            return Err(AppError::CoreFailed {
                message: format!("Ядро не скачало geo-базы ({status})"),
                log: vec![without_query(&said)],
            });
        }
        Ok(())
    }

    /// Перечитать источник без перезапуска ядра (проверено, S-012).
    pub async fn reload(&self, source: &str) -> Result<()> {
        let response = crate::http::Http::direct()?
            .put(format!(
                "{}/providers/proxies/{}",
                self.base(),
                encode(source)
            ))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро не перечитало источник: {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Применить собранный конфиг **не перезапуская** ядро (S-019).
    ///
    /// Путь, а не тело: `PUT /configs` читает файл сам, и это к лучшему — конфиг у нас
    /// и так лежит на диске, потому что с него ядро и стартовало.
    ///
    /// `POST /restart` здесь не годится и не годится вообще: он меняет PID, надзор сочтёт
    /// это падением и поднимет второе ядро (D-057, S-019).
    pub async fn apply(&self, path: &std::path::Path) -> Result<()> {
        let response = crate::http::Http::direct()?
            .put(format!("{}/configs", self.base()))
            .bearer_auth(&self.secret)
            .json(&serde_json::json!({ "path": path.display().to_string() }))
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро не приняло новый конфиг: {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Перепроверить живость всех узлов **сейчас** (D-112).
    ///
    /// Фоновая проверка у провайдеров ленивая и раз в пять минут: после сна это значит
    /// до пяти минут маршрута через узел, которого уже нет. Здесь мы просим ядро сходить
    /// по всем провайдерам немедленно.
    ///
    /// Имена спрашиваем у самого ядра, а не берём из своих источников: провайдер, который
    /// оно не загрузило, перепроверять бессмысленно, а список у него уже есть.
    ///
    /// Отказ по отдельному провайдеру не отменяет остальных: один недоступный сервер
    /// не повод не проверить прочие.
    pub async fn recheck(&self) -> Result<usize> {
        #[derive(Deserialize)]
        struct Providers {
            providers: HashMap<String, serde_json::Value>,
        }
        let listed: Providers = serde_json::from_str(&self.get("/providers/proxies").await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))?;
        let mut asked = 0;
        for name in listed.providers.keys() {
            if self
                .get(&format!("/providers/proxies/{}/healthcheck", encode(name)))
                .await
                .is_ok()
            {
                asked += 1;
            }
        }
        Ok(asked)
    }

    /// Забыть карту подменных адресов (S-021). Ручное действие и только ручное: сброс
    /// возвращает ровно ту подмену, от которой `store-fake-ip` и защищает, — делать его
    /// самим значило бы лечить болезнь её же симптомом.
    pub async fn flush_fake_ip(&self) -> Result<()> {
        let response = crate::http::Http::direct()?
            .post(format!("{}/cache/fakeip/flush", self.base()))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро не сбросило подменные адреса: {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Закрыть все открытые соединения (D-143). Приложения переподключатся сами — уже
    /// по новому маршруту.
    pub async fn close_all(&self) -> Result<()> {
        let response = crate::http::Http::direct()?
            .delete(format!("{}/connections", self.base()))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро не закрыло соединения: {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Конфиг глазами самого ядра. Только для замеров: рабочему коду знать, что ядро
    /// думает о своём конфиге, незачем — он знает, чем его запускал.
    #[cfg(test)]
    pub async fn config(&self) -> Result<serde_json::Value> {
        serde_json::from_str(&self.get("/configs").await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))
    }

    pub async fn select(&self, group: &str, name: &str) -> Result<()> {
        let response = crate::http::Http::direct()?
            .put(format!("{}/proxies/{}", self.base(), encode(group)))
            .bearer_auth(&self.secret)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро отказалось переключать «{group}»: {}",
                response.status()
            )));
        }
        Ok(())
    }

    /// Спросить имя **резолвером ядра** (D-098).
    ///
    /// Единственный способ померить DoT, DoQ и DoH3, не втаскивая в клиент свои TLS
    /// и QUIC: у ядра они уже есть, а `/dns/query` отвечает тем же JSON, что и публичные
    /// DoH-точки. Проверено на v1.19.30.
    ///
    /// Чей это резолвер — решает конфиг ядра, которому задан вопрос. Стенд (`diag::bench`)
    /// поднимает своё ядро ровно с одним `nameserver`, и тогда ответ приходит именно от
    /// него.
    pub async fn resolve(&self, name: &str) -> Result<Vec<String>> {
        let path = format!("/dns/query?name={}&type=A", urlencoded(name));
        let body: DnsAnswer = serde_json::from_str(&self.get(&path).await?)
            .map_err(|e| AppError::network(format!("Ядро ответило неожиданным: {e}")))?;
        body.addresses()
    }

    fn base(&self) -> String {
        format!("http://{}", self.address)
    }

    async fn get(&self, path: &str) -> Result<String> {
        let response = crate::http::Http::direct()?
            .get(format!("{}{path}", self.base()))
            .bearer_auth(&self.secret)
            .send()
            .await
            .map_err(|e| AppError::network(format!("Ядро не ответило: {e}")))?;
        if !response.status().is_success() {
            return Err(AppError::network(format!(
                "Ядро ответило {}",
                response.status()
            )));
        }
        response
            .text()
            .await
            .map_err(|e| AppError::network(format!("Не удалось прочитать ответ ядра: {e}")))
    }
}

/// Ответ `/dns/query`. Форма — как у публичных DoH-точек: код состояния и записи
/// с числовым типом. Разбираем только A (тип 1): на вопрос «тот ли адрес» отвечает он.
#[derive(Deserialize)]
struct DnsAnswer {
    #[serde(rename = "Status")]
    status: u8,
    #[serde(rename = "Answer", default)]
    answer: Option<Vec<DnsRecord>>,
}

#[derive(Deserialize)]
struct DnsRecord {
    #[serde(rename = "type")]
    kind: u16,
    data: String,
}

impl DnsAnswer {
    fn addresses(&self) -> Result<Vec<String>> {
        if self.status != 0 {
            return Err(AppError::network(format!(
                "резолвер отказал, код {}",
                self.status
            )));
        }
        let found: Vec<String> = self
            .answer
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|record| record.kind == 1)
            .map(|record| record.data.clone())
            .collect();
        if found.is_empty() {
            return Err(AppError::network("ответ без адресов".to_string()));
        }
        Ok(found)
    }
}

/// Имя в строку запроса. Своими руками, потому что кодировать здесь нужно ровно один
/// параметр и ровно один раз: тащить ради этого крейт — та же история, что с DNS-пакетом.
fn urlencoded(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '.' | '_' | '~' => c.to_string(),
            other => other
                .to_string()
                .as_bytes()
                .iter()
                .map(|byte| format!("%{byte:02X}"))
                .collect(),
        })
        .collect()
}

/// Ответ `/connections` целиком. Отдельным типом, а не внутри метода: разбор проверяется
/// тестом, а для этого им должен пользоваться кто-то кроме сети.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connections {
    download_total: u64,
    upload_total: u64,
    #[serde(default)]
    connections: Option<Vec<Connection>>,
}

#[derive(Deserialize)]
struct Connection {
    /// Идентификатор соединения. По нему и видно, что оно закрылось: в следующем ответе
    /// ядра его просто нет.
    #[serde(default)]
    id: String,
    /// Цепочка прокси. **Первым стоит сам узел** — измерено на живом ядре (S-018),
    /// в том числе для узла провайдера, который поимённо не адресуется.
    #[serde(default)]
    chains: Vec<String>,
    #[serde(default)]
    upload: u64,
    #[serde(default)]
    download: u64,
}

/// Сложить трафик по узлам: открытые соединения плюс то, что унесли закрывшиеся.
/// Больше всего везущий — первым: таблица отвечает на «через кого идёт больше»,
/// и порядок ответа — часть.
///
/// Узел, у которого сейчас нет ни одного открытого соединения, из списка **не выпадает**,
/// пока о нём помнит `gone`. Иначе он исчез бы и вернулся с полной суммой, а окно
/// приняло бы её за трафик одного такта — всплеск на ровном месте.
fn by_node(live: &[Connection], gone: &HashMap<String, (u64, u64)>) -> Vec<NodeTraffic> {
    let mut sums: HashMap<&str, NodeTraffic> = HashMap::new();
    for (node, (up, down)) in gone {
        sums.insert(
            node.as_str(),
            NodeTraffic {
                node: node.clone(),
                up: *up,
                down: *down,
                connections: 0,
            },
        );
    }
    for connection in live {
        let Some(node) = connection.chains.first() else {
            continue;
        };
        let entry = sums.entry(node).or_insert_with(|| NodeTraffic {
            node: node.clone(),
            up: 0,
            down: 0,
            connections: 0,
        });
        entry.up += connection.upload;
        entry.down += connection.download;
        entry.connections += 1;
    }
    let mut nodes: Vec<NodeTraffic> = sums.into_values().collect();
    nodes.sort_by(|a, b| {
        (b.down + b.up)
            .cmp(&(a.down + a.up))
            .then(a.node.cmp(&b.node))
    });
    nodes
}

/// Имя группы уезжает в путь URL, а в нём бывают пробелы и юникод.
fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Адреса в кавычках — без запроса: у подписанной ссылки на выпуск GitHub в нём токен,
/// и в окне и логе ему не место. Хост и путь при этом видны — по ним понятно, что не скачалось.
fn without_query(text: &str) -> String {
    text.split('"')
        .enumerate()
        .map(|(at, part)| match part.split_once('?') {
            Some((head, _)) if at % 2 == 1 && head.contains("://") => head,
            _ => part,
        })
        .collect::<Vec<_>>()
        .join("\"")
}

/// Новый секрет на каждый запуск: он живёт ровно столько же, сколько процесс ядра.
fn secret() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| AppError::io(format!("Не удалось сгенерировать секрет: {e}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    /// Токен подписанной ссылки не доезжает до окна, а сама ссылка — доезжает.
    #[test]
    fn a_signed_link_loses_its_token() {
        let said = r#"can't download GeoSite database file: Get "https://release-assets.githubusercontent.com/a/b?sig=SECRET&jwt=x": EOF"#;
        assert_eq!(
            super::without_query(said),
            r#"can't download GeoSite database file: Get "https://release-assets.githubusercontent.com/a/b": EOF"#
        );
    }

    /// `umiray → AUTO → узел`: окно показывает, куда трафик уходит на самом деле (D-145).
    #[test]
    fn a_route_follows_groups_down_to_the_node() {
        let now = |name: &str| match name {
            "umiray" => Some("AUTO".to_string()),
            "AUTO" => Some("Poland 1".to_string()),
            _ => None,
        };
        assert_eq!(follow("umiray", now), ["umiray", "AUTO", "Poland 1"]);
        assert_eq!(follow("Poland 1", now), ["Poland 1"]);
    }

    #[test]
    fn a_loop_of_groups_does_not_hang_the_route() {
        let now = |name: &str| Some(if name == "A" { "B" } else { "A" }.to_string());
        assert_eq!(follow("A", now), ["A", "B"]);
    }

    use super::*;

    /// Ответ живого ядра v1.19.30 на `/dns/query?name=example.com&type=A` — снят
    /// с работающего клиента и вставлен как есть.
    #[test]
    fn the_core_answer_gives_up_its_addresses() {
        let body = r#"{"AD":false,"Answer":[
            {"TTL":236,"data":"8.6.112.0","name":"example.com.","type":1},
            {"TTL":236,"data":"8.47.69.0","name":"example.com.","type":1}],
            "CD":false,"Question":[{"Name":"example.com.","Qtype":1,"Qclass":1}],
            "RA":true,"RD":true,"Status":0,"TC":false}"#;
        let parsed: DnsAnswer = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.addresses().unwrap(), vec!["8.6.112.0", "8.47.69.0"]);
    }

    /// Отказ резолвера и пустой ответ — разные беды, но обе не адреса.
    #[test]
    fn a_refusal_is_not_an_address() {
        let refused: DnsAnswer = serde_json::from_str(r#"{"Status":2}"#).unwrap();
        assert!(refused
            .addresses()
            .unwrap_err()
            .to_string()
            .contains("код 2"));
        let empty: DnsAnswer = serde_json::from_str(r#"{"Status":0,"Answer":[]}"#).unwrap();
        assert!(empty.addresses().is_err());
    }

    /// Записи не-A (CNAME, AAAA) в ответе бывают всегда, и адресами они не являются.
    #[test]
    fn only_a_records_are_addresses() {
        let body = r#"{"Status":0,"Answer":[
            {"type":5,"data":"cdn.example.net."},
            {"type":1,"data":"93.184.216.34"}]}"#;
        let parsed: DnsAnswer = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.addresses().unwrap(), vec!["93.184.216.34"]);
    }

    /// Имя уезжает в строку запроса, и всё, что не буква, обязано быть закодировано:
    /// иначе первый же пробел или кириллица ломают запрос молча.
    #[test]
    fn a_name_is_escaped_for_the_query_string() {
        assert_eq!(urlencoded("example.com"), "example.com");
        assert_eq!(urlencoded("a b"), "a%20b");
        assert_eq!(
            urlencoded("почта.рф"),
            "%D0%BF%D0%BE%D1%87%D1%82%D0%B0.%D1%80%D1%84"
        );
    }

    /// S-018: `chains[0]` — сам узел, и по нему трафик складывается. Ответ ядра здесь
    /// настоящий, сокращённый до нужных полей.
    #[test]
    fn open_connections_add_up_per_node() {
        let body = r#"{
          "downloadTotal": 41548737, "uploadTotal": 1391,
          "connections": [
            {"chains": ["CapyHub LTE 1", "AUTO", "umiray"], "upload": 100, "download": 9060679},
            {"chains": ["CapyHub LTE 1", "AUTO", "umiray"], "upload": 20, "download": 2264125},
            {"chains": ["vless-tls", "AUTO", "umiray"], "upload": 7, "download": 2191347},
            {"chains": [], "upload": 5, "download": 5}
          ]
        }"#;
        let parsed: Connections = serde_json::from_str(body).unwrap();
        let live = parsed.connections.unwrap();
        let nodes = by_node(&live, &HashMap::new());
        assert_eq!(
            nodes.len(),
            2,
            "соединение без цепочки узлу не приписывается"
        );
        assert_eq!(nodes[0].node, "CapyHub LTE 1", "больше всех везёт — первым");
        assert_eq!(nodes[0].down, 11_324_804);
        assert_eq!(nodes[0].connections, 2);
        assert_eq!(nodes[1].node, "vless-tls");
        assert_eq!(nodes[1].up, 7);
    }

    /// S-018, вторая половина: закрывшееся соединение уносило свои байты, сумма узла
    /// падала, и разница между отсчётами уходила в ноль. Теперь унесённое помнит клиент.
    #[test]
    fn a_closed_connection_does_not_take_its_bytes_away() {
        let answer = |body: &str| -> Vec<Connection> {
            serde_json::from_str::<Connections>(body)
                .unwrap()
                .connections
                .unwrap_or_default()
        };
        let first = answer(
            r#"{"downloadTotal":0,"uploadTotal":0,"connections":[
                {"id":"a","chains":["узел"],"upload":10,"download":1000},
                {"id":"b","chains":["узел"],"upload":5,"download":500}
            ]}"#,
        );
        // Второй отсчёт: «b» закрылось, «a» доехало ещё немного.
        let second = answer(
            r#"{"downloadTotal":0,"uploadTotal":0,"connections":[
                {"id":"a","chains":["узел"],"upload":12,"download":1200}
            ]}"#,
        );

        let mut ledger = Ledger::default();
        let was = by_node(&first, ledger.settle(&first));
        assert_eq!(was[0].down, 1500);
        let now = by_node(&second, ledger.settle(&second));
        assert!(
            now[0].down >= was[0].down,
            "сумма узла обязана только расти: было {}, стало {}",
            was[0].down,
            now[0].down
        );
        assert_eq!(now[0].down, 1700, "1200 у живого плюс 500 у закрывшегося");
        assert_eq!(now[0].connections, 1, "открытым считается только живое");
    }

    /// Узел, у которого закрылось всё, из списка не выпадает: исчезнув и вернувшись,
    /// он показал бы всю накопленную сумму как трафик одного такта.
    #[test]
    fn a_node_whose_connections_all_closed_stays_in_the_list() {
        let live: Vec<Connection> = serde_json::from_str::<Connections>(
            r#"{"downloadTotal":0,"uploadTotal":0,"connections":[
                {"id":"a","chains":["узел"],"upload":10,"download":1000}
            ]}"#,
        )
        .unwrap()
        .connections
        .unwrap();

        let mut ledger = Ledger::default();
        ledger.settle(&live);
        let quiet = by_node(&[], ledger.settle(&[]));
        assert_eq!(quiet.len(), 1);
        assert_eq!(quiet[0].node, "узел");
        assert_eq!(quiet[0].down, 1000);
        assert_eq!(quiet[0].connections, 0, "везти он перестал, но вёз");
    }

    /// Ядро без соединений — это ноль узлов, а не ошибка разбора.
    #[test]
    fn a_quiet_core_gives_no_nodes() {
        let parsed: Connections =
            serde_json::from_str(r#"{"downloadTotal":0,"uploadTotal":0,"connections":null}"#)
                .unwrap();
        assert!(by_node(&parsed.connections.unwrap_or_default(), &HashMap::new()).is_empty());
    }

    #[test]
    fn group_names_survive_the_url() {
        assert_eq!(encode("umiray"), "umiray");
        assert_eq!(
            encode("Выбор серверов"),
            "%D0%92%D1%8B%D0%B1%D0%BE%D1%80%20%D1%81%D0%B5%D1%80%D0%B2%D0%B5%D1%80%D0%BE%D0%B2"
        );
        assert_eq!(encode("a/b"), "a%2Fb", "слэш не должен ломать путь");
    }

    #[test]
    fn every_start_gets_its_own_port_and_secret() {
        let one = Controller::new().unwrap();
        let two = Controller::new().unwrap();
        assert_ne!(one.secret, two.secret, "секрет одноразовый");
        assert_ne!(one.address, two.address, "порт свой у каждого запуска");
        assert!(one.args().contains(&one.secret));
    }
}
