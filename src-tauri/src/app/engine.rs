//! Контракт ядра (D-154): то, что обвязке клиента нужно от **любого** ядра.
//!
//! Только общее: поднять, погасить, сказать, что происходит, отдать лог, скачаться.
//! Всё, что умеет одно ядро, — правила и группы mihomo, роли и egress qd — вызывается
//! у его типа напрямую и сюда не входит: иначе контракт рос бы от фич одного ядра.
//!
//! Живёт в `app`, а не в `core`: собственным хукам mihomo нужны маршруты и настройки
//! из `AppState`, и контракт в `core` дал бы цикл `core → app → core`.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use crate::app::state::AppState;
use crate::core::process::LogRing;
use crate::core::EngineId;
use crate::error::Result;

/// Асинхронный шаг, который можно держать в трейт-объекте и в константном реестре.
pub type Job<'a, T = ()> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Как ядро перехватывает трафик. Обвязка клиента решает по нему, а не по имени ядра:
/// новое ядро, честно назвавшее свой перехват, получает прокси, kill switch и значок даром.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    /// Локальный прокси на петле — его можно прописать в Windows (D-047).
    LocalProxy { port: u16 },
    /// Виртуальный адаптер, весь трафик машины — kill switch выпускает только через него
    /// и только бинарь ядра (D-073).
    Tun { device: String },
    /// Перехват по приложениям (WinDivert у qd): ни адреса, ни адаптера.
    Divert,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineState {
    /// Держит трафик прямо сейчас.
    pub on: bool,
    /// **Должно** держать: ставит удачный запуск, снимает явная остановка. Смерть процесса
    /// флаг не трогает — по нему падение и отличается от «выключили» (D-057).
    pub wanted: bool,
    /// Когда начало держать, в секундах эпохи.
    pub started: Option<u64>,
    /// Пусто — не держит.
    pub capture: Option<Capture>,
    /// Трафика нет, но процесс жив и возвращает его сам (qd после потери туннеля). Это
    /// не падение: подъём надзора поверх своего у ядра кончался бы остановкой (D-057).
    pub recovering: bool,
}

impl EngineState {
    /// Умерло само, хотя должно работать (D-057).
    pub fn crashed(&self) -> bool {
        self.wanted && !self.on && !self.recovering
    }
}

pub trait Engine: Send + Sync {
    /// Поднять — своими хуками. В фазе клиента это шаг «до»: отказ отменяет её целиком,
    /// и прокси с kill switch не ставятся вокруг ядра, которое не встало (D-101).
    fn start<'a>(&'a self, state: &'a AppState) -> Job<'a>;
    /// Погасить. Снимает `wanted`.
    fn stop(&self) -> Job<'_>;
    fn state(&self) -> EngineState;
    /// Освежить то, что `state` знает со слов ядра: туннель мог упасть и без нас.
    /// Зовётся надзором каждый такт; ядру, которое всё знает само, делать нечего.
    fn refresh(&self) -> Job<'_> {
        Box::pin(async { Ok(()) })
    }
    fn log(&self) -> &LogRing;
    /// Путь к бинарю: по нему уборка находит сирот (D-059), а kill switch — кого выпускать.
    fn binary(&self) -> PathBuf;
    /// Скачать и положить на место. Отдаёт версию.
    fn install(&self) -> Job<'_, String>;
    /// Скачанные списки на диске изменились (D-157): собрать из них свой формат и, если
    /// работает, перечитать. Ядру без правил делать нечего — отсюда умолчание.
    fn lists_changed(&self) -> Job<'_> {
        Box::pin(async { Ok(()) })
    }
}

impl AppState {
    pub fn logs(&self, id: EngineId) -> Vec<String> {
        let mut lines = self.engine(id).log().lines();
        lines.extend(self.volt.logs());
        lines.sort_by_cached_key(|line| {
            line.strip_prefix("time=\"")
                .and_then(|tail| tail.split('"').next())
                .unwrap_or("")
                .replace(' ', "T")
        });
        lines
    }

    /// Реестр ядер. Новое ядро — ветка здесь и вариант `EngineId`, больше нигде (D-154).
    pub fn engine(&self, id: EngineId) -> &dyn Engine {
        match id {
            EngineId::Mihomo => &self.mihomo,
            EngineId::Qd => &self.qd,
        }
    }

    /// Какое ядро держит трафик сейчас. Одно — за этим следит `app/connect.rs`.
    pub fn running(&self) -> Option<(EngineId, EngineState)> {
        EngineId::ALL
            .map(|id| (id, self.engine(id).state()))
            .into_iter()
            .find(|(_, state)| state.on)
    }

    /// Кто держит трафик или должен держать: упавшее с `wanted` поднимет надзор.
    pub fn busy(&self) -> Vec<EngineId> {
        EngineId::ALL
            .into_iter()
            .filter(|id| {
                let state = self.engine(*id).state();
                state.on || state.wanted
            })
            .collect()
    }

    /// Строка клиента в лог выбранного ядра — того, который человек сейчас видит.
    pub fn note(&self, level: &str, message: &str) {
        self.engine(self.settings.get().engine)
            .log()
            .note(level, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_bringing_its_traffic_back_itself_has_not_crashed() {
        let lost = EngineState {
            wanted: true,
            ..Default::default()
        };
        assert!(lost.crashed());
        assert!(!EngineState {
            recovering: true,
            ..lost
        }
        .crashed());
    }
}
