//! Корень сборки (D-155): всё, что живёт дольше одной команды, — одним объектом.
//!
//! Держит сервисы полями и не знает ни одного сценария: каждый сценарий — метод своего
//! сервиса. Сервис владеет своим состоянием, а к соседям обращается через этот контейнер,
//! который получает аргументом. Tauri хранит состояние приложения одним объектом —
//! этим он и является.

use crate::app::catalog::Catalog;
use crate::app::connect::Connection;
use crate::app::diagnostics::Diagnostics;
use crate::app::killswitch::KillSwitch;
use crate::app::maintenance::Maintenance;
use crate::app::notice::Notices;
use crate::app::presets::Presets;
use crate::app::proxy::SystemProxy;
use crate::app::qd::QdPanel;
use crate::app::routing::Routing;
use crate::app::settings::SettingsStore;
use crate::app::sources::Sources;
use crate::core::mihomo::Mihomo;
use crate::core::qd::Qd;

pub struct AppState {
    /// Настройки клиента: единственный, кто знает, где они лежат.
    pub settings: SettingsStore,
    /// Питание, «одно ядро», доведение правок до живого ядра, надзор.
    pub connection: Connection,
    /// Куда идёт трафик: направление, выбранный узел, псевдоним, применённый набор.
    pub routing: Routing,
    /// Наборы маршрутизации: завести, применить, удалить.
    pub presets: Presets,
    /// Узлы для окна: состав, замеры задержки, страны.
    pub catalog: Catalog,
    /// Источники узлов: добавить, обновить, поправить, удалить.
    pub sources: Sources,
    /// Системный прокси вокруг ядра — с памятью о том, как было.
    pub proxy: SystemProxy,
    /// Kill switch вокруг ядра — с памятью о том, как было.
    pub kill_switch: KillSwitch,
    /// Инструменты на том, что сейчас работает.
    pub diagnostics: Diagnostics,
    /// Сброс и перезапуск с правами.
    pub maintenance: Maintenance,
    /// Что сейчас не так — по одной строке от каждого, кто это заметил (D-115).
    pub notices: Notices,
    /// То, что есть только у qd и нужно его разделам.
    pub qd_panel: QdPanel,
    /// Ядра (D-154): реестр — `AppState::engine`.
    pub mihomo: Mihomo,
    pub qd: Qd,
}

impl AppState {
    /// Диск читается один раз при старте — дальше источник истины здесь.
    pub fn new() -> Self {
        Self {
            settings: SettingsStore::open(),
            connection: Connection::default(),
            routing: Routing,
            presets: Presets,
            catalog: Catalog::default(),
            sources: Sources,
            proxy: SystemProxy,
            kill_switch: KillSwitch,
            diagnostics: Diagnostics,
            maintenance: Maintenance,
            notices: Notices::default(),
            qd_panel: QdPanel,
            mihomo: Mihomo::new(),
            qd: Qd::new(),
        }
    }
}
