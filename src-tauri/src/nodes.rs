//! Узлы и то, откуда они берутся: подписки, ручные ссылки, правки поверх пришедшего.
//!
//! Границы домена: ссылка как текст (`link`), загрузка у панели (`subscription`), хранение
//! источников (`sources`), разница пользователя (`patches`), замер до сервера (`ping`).
//! Протоколы разбирает **ядро** (D-031) — кроме тех схем, которых оно из ссылки не читает:
//! они проходят через шов `outbound` (D-063).

use serde::Serialize;

/// Узел — одна конфигурация протокола, как её видит окно.
///
/// Состав списка знает **диск** (D-061): он полон, стабилен по порядку и доступен
/// до подключения. Работающее ядро тут больше не участвует — его список пропускал схемы,
/// которых не понимает, и от этого узлы исчезали, а выбор слетал (B-006).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub name: String,
    /// Протокол: `Vless`, `Trojan`, `Wireguard`.
    pub kind: String,
    /// Идентификатор источника, из которого узел пришёл.
    pub source: String,
    /// Дойдёт ли этот узел до ядра. Ложь означает, что ссылку не читает ни конвертер ядра,
    /// ни наш шов, — узел показывается с пометкой и не выбирается (D-063).
    pub supported: bool,
    /// Сколько до сервера по последнему замеру. Пусто — не мерили или не ответил.
    pub delay: Option<u32>,
    /// Чем померено на самом деле (D-062, D-069). При фолбэке это ICMP, а не выбранный
    /// способ: колонка показывает измеренное, а не заказанное.
    pub method: Option<ping::Method>,
    /// Число получено запасным способом: выбранная проверка промолчала, а хост жив.
    pub fallback: bool,
    pub address: Option<String>,
    /// Код страны из двух букв — по адресу, а не по имени (D-084). Пусто: не спрашивали,
    /// не узнали или геозапросы выключены.
    pub country: Option<String>,
    /// Правлен ли узел пользователем — поверх пришедшего лежит наша разница.
    pub edited: bool,
}

impl Node {
    /// Дописать узлам то, чего не знает файл источника: замеры до серверов.
    ///
    /// Адрес и правки проставляет уже `sources::nodes` — там же, где читается сама ссылка.
    /// Замеры живут дольше одного вызова (их собирает отдельная команда), поэтому приходят
    /// таблицей снаружи.
    pub fn enrich(nodes: &mut [Node], pings: &ping::Table, countries: &geo::Cache) {
        for node in nodes {
            let Some(address) = node.address.clone() else {
                continue;
            };
            node.country = countries
                .get(&address)
                .and_then(|known| known.country.clone());
            let Some(reply) = pings.get(&address) else {
                continue;
            };
            node.delay = Some(reply.ms);
            node.method = Some(reply.method);
            node.fallback = reply.fallback;
        }
    }
}

pub mod codec;
pub mod convert;
pub mod device;
pub mod entries;
pub mod geo;
pub mod health;
pub mod link;
pub mod ping;
pub mod source_build;
pub mod source_catalog;
pub mod source_editor;
pub mod source_id;
pub mod source_import;
pub mod sources;
pub mod subscription;
pub mod wgconf;
