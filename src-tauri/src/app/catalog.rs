//! Узлы, какими их видит окно (D-155): состав с диска, поверх — замеры задержки и страны.
//!
//! Владеет таблицей замеров (D-062): она живёт дольше одной команды — собирает её отдельное
//! нажатие, а показывает каждый опрос, и держать её в окне значило бы терять при первом же
//! переходе в другой раздел. Как именно мерить, решает `app/measure.rs`.

use std::sync::Mutex;

use crate::app::state::AppState;
use crate::error::Result;
use crate::nodes::ping::{self, Method};
use crate::nodes::Node;

pub struct Catalog {
    pings: Mutex<ping::Table>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            pings: Mutex::new(ping::Table::new()),
        }
    }
}

impl Catalog {
    /// Узлы: состав знает диск, поверх ложатся замеры (D-061).
    ///
    /// У ядра список больше не спрашиваем: оно пропускает схемы, которых не понимает,
    /// и от этого узлы исчезали при подключении, а выбор слетал (B-006). Порядок здесь
    /// тоже свой — файловый, а не случайный, как у ответа ядра (B-007).
    pub fn nodes(&self) -> Vec<Node> {
        let mut nodes = crate::nodes::source_catalog::SourceCatalog::nodes();
        crate::nodes::Node::enrich(
            &mut nodes,
            &self.pings.lock().unwrap(),
            &crate::nodes::geo::GeoCache::load(),
        );
        nodes
    }

    /// Имена узлов, которые ядро **правда** поднимет: узел, которого оно не понимает,
    /// целью псевдонима быть не может (D-063).
    pub fn names(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|node| node.supported)
            .map(|node| node.name)
            .collect()
    }

    /// Померить, сколько до каждого сервера, и запомнить (D-062).
    ///
    /// Способ берётся из клиентского конфига (D-069). Замеры заменяются целиком, а не
    /// сливаются: сервер, переставший отвечать, обязан показать прочерк, а не вчерашнее число.
    pub async fn measure(&self, state: &AppState) -> Result<()> {
        let method = crate::app::client::ClientConfig::ping()?;
        let nodes = self.nodes();
        // Последовательный способ показывает числа по мере готовности (D-072), поэтому
        // прошлые стираем: иначе таблица десятки секунд мешает старые с новыми, и понять,
        // где чей замер, невозможно. Быстрые способы заменяют таблицу разом, как и раньше.
        if method == Method::ProxyKeepalive {
            self.pings.lock().unwrap().clear();
        }
        let table =
            crate::app::measure::Measure::run(&nodes, &state.mihomo, method, &|address, reply| {
                self.pings
                    .lock()
                    .unwrap()
                    .insert(address.to_string(), reply);
            })
            .await?;
        *self.pings.lock().unwrap() = table;
        Ok(())
    }

    /// Сколько часов помнить страну узла (D-084). Записать срок и сразу же спросить о том,
    /// чего ещё не знаем: включили — флаги должны появиться, а не ждать фонового такта.
    pub fn set_geo_hours(&self, hours: u64) -> Result<()> {
        crate::nodes::geo::GeoCache::set_hours(hours)?;
        tauri::async_runtime::spawn(async {
            let _ = crate::nodes::geo::GeoCache::refresh().await;
        });
        Ok(())
    }

    /// Сменить способ замера (D-069).
    ///
    /// Прошлые замеры при этом **забываются**: они получены другим способом, а колонка
    /// называет тот, который выбран, — и числа под этим заголовком были бы враньём.
    /// Пустая колонка честнее и заполняется сама: «Соединение» меряет непроверенный
    /// список при открытии.
    pub fn set_method(&self, method: Method) -> Result<()> {
        crate::app::client::ClientConfig::set_ping(method)?;
        self.pings.lock().unwrap().clear();
        Ok(())
    }
}
