//! qd как ядро клиента (D-154): его обвязка — процесс, потом туннель — и контракт.
//!
//! Процесс, рукопожатие и API — `core/qd.rs`. Ни конфига, ни порта у qd нет: входной
//! узел он выбирает сам, трафик перехватывает по приложениям.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::app::engine::{Capture, Engine, EngineState, Job};
use crate::app::lifecycle::Lifecycle;
use crate::app::lifecycle::{Hook, Phase, When};
use crate::app::state::AppState;
use crate::core::process::LogRing;
use crate::core::qd::Qd;
use crate::core::EngineId;
use crate::error::{AppError, Result};
use crate::system::elevation::Elevation;

/// Запуск qd: поднять процесс — отдельным шагом, чтобы по журналу было видно, что не
/// встало, сам qd или его туннель, — потом туннель.
const START: &[Hook<()>] = &[
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "process",
        label: "запуск qd",
        run: process,
    },
    Hook {
        phase: Phase::Start,
        when: When::Before,
        id: "connect",
        label: "подключение qd",
        run: connect,
    },
];

impl Engine for Qd {
    fn start<'a>(&'a self, state: &'a AppState) -> Job<'a> {
        Box::pin(
            async move { Lifecycle::run(START, Phase::Start, state, &mut (), self.log()).await },
        )
    }

    fn stop(&self) -> Job<'_> {
        Box::pin(self.disconnect())
    }

    fn state(&self) -> EngineState {
        let status = self.status();
        EngineState {
            on: status.on,
            wanted: status.wanted,
            started: status.started,
            capture: status.on.then_some(Capture::Divert),
            recovering: status.recovering,
        }
    }

    fn refresh(&self) -> Job<'_> {
        Box::pin(Qd::refresh(self))
    }

    fn log(&self) -> &LogRing {
        Qd::log(self)
    }

    fn binary(&self) -> PathBuf {
        Qd::binary()
    }

    fn install(&self) -> Job<'_, String> {
        Box::pin(Qd::install(self))
    }
}

/// Процесс нужен и без туннеля — разделам qd ради API; этот шаг его только гарантирует.
fn process<'a>(state: &'a AppState, _: &'a mut ()) -> Job<'a> {
    Box::pin(async move {
        state
            .qd
            .call("GET", "/client/api/state", None)
            .await
            .map(drop)
    })
}

fn connect<'a>(state: &'a AppState, _: &'a mut ()) -> Job<'a> {
    Box::pin(state.qd.connect())
}

/// Что есть только у qd и нужно его разделам (D-155): его состояние, прокси к API
/// и файлы правил. Питание, лог и установка — общие, через контракт ядра.
pub struct QdPanel;

/// Состояние qd для его разделов: скачан ли, есть ли права, жив ли процесс, что говорит сам.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QdStatus {
    present: bool,
    elevated: bool,
    running: bool,
    state: Option<Value>,
    problem: Option<String>,
}

/// Что стало со ссылкой qd (D-161).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adopted {
    /// Версия qd, если его пришлось скачать ради этой ссылки.
    downloaded: Option<String>,
    /// Ссылка ждёт первого подъёма qd: сейчас нет прав, и принять её некому.
    pending: bool,
}

const RULES_FILTER: &[(&str, &str)] = &[("Правила qd (*.qdr)", "*.qdr"), ("Все файлы", "*.*")];

impl QdPanel {
    /// Ссылка `qd://` из любого поля добавления (D-161): qd скачивается, если его нет,
    /// и принимает ссылку. Без прав qd не встаёт — ссылка ждёт его первого подъёма.
    pub async fn adopt(&self, state: &AppState, link: &str) -> Result<Adopted> {
        let link = link.trim();
        // Схема регистра не различает (RFC 3986), а qd ждёт её строчной — `QD://` из окна
        // приезжает как ссылка qd, и схему приводим здесь.
        if !link
            .get(..5)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("qd://"))
        {
            return Err(AppError::invalid("Это не ссылка qd"));
        }
        let link = &format!("qd://{}", &link[5..]);
        let downloaded = if state.qd.present() {
            None
        } else {
            Some(state.connection.install(state, EngineId::Qd).await?)
        };
        let body = serde_json::json!({ "uri": link });
        let pending = match state
            .qd
            .call("POST", "/client/api/import", Some(body))
            .await
        {
            Err(AppError::NeedsElevation { .. }) => {
                state.qd.hold(link)?;
                true
            }
            other => other.map(|_| false)?,
        };
        Ok(Adopted {
            downloaded,
            pending,
        })
    }

    /// Выключить qd — удалить бинарь (D-161). Работающий не трогаем, как и при замене
    /// (D-137); вид возвращается к mihomo, иначе кнопка питания поднимала бы пустоту.
    pub async fn remove(&self, state: &AppState) -> Result<()> {
        let _transition = state.connection.lock().await;
        let now = Engine::state(&state.qd);
        if now.on || now.wanted {
            return Err(AppError::invalid(
                "Сначала отключите qd: работающее ядро нельзя удалить",
            ));
        }
        state.qd.remove().await?;
        state
            .settings
            .update(|settings| settings.engine = EngineId::Mihomo)
    }

    /// Спросить состояние — значит поднять процесс, если он нужен и может встать:
    /// разделы qd без него пусты.
    pub async fn status(&self, state: &AppState) -> QdStatus {
        let present = state.qd.present();
        let elevated = Elevation::is_elevated();
        let (reply, problem) = if present && elevated {
            match state.qd.call("GET", "/client/api/state", None).await {
                Ok(value) => (Some(value), None),
                Err(why) => (None, Some(why.to_string())),
            }
        } else {
            (None, None)
        };
        QdStatus {
            present,
            elevated,
            running: state.qd.running(),
            state: reply,
            problem,
        }
    }

    /// Сохранить правила файлом `.qdr` через окно сохранения. Пусто — человек передумал.
    pub async fn export_rules(&self, state: &AppState) -> Result<Option<String>> {
        let exported = state
            .qd
            .call("GET", "/client/api/routing/export", None)
            .await?;
        let code = exported
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = exported
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("rules.qdr")
            .to_string();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            crate::system::pick::FileDialog::save(
                "Сохранить правила qd",
                RULES_FILTER,
                &name,
                "qdr",
            )
        })
        .await
        .map_err(|e| AppError::io(format!("Окно сохранения не открылось: {e}")))?;
        let Some(path) = picked else {
            return Ok(None);
        };
        std::fs::write(&path, code)
            .map_err(|e| AppError::io(format!("Не удалось записать {}: {e}", path.display())))?;
        Ok(Some(path.display().to_string()))
    }

    /// Загрузить правила из файла `.qdr`. Пусто — человек передумал.
    pub async fn import_rules(&self, state: &AppState) -> Result<Option<Value>> {
        let picked = tauri::async_runtime::spawn_blocking(|| {
            crate::system::pick::FileDialog::file("Загрузить правила qd", RULES_FILTER)
        })
        .await
        .map_err(|e| AppError::io(format!("Окно выбора файла не открылось: {e}")))?;
        let Some(path) = picked else {
            return Ok(None);
        };
        let code = std::fs::read_to_string(&path)
            .map_err(|e| AppError::io(format!("Не удалось прочитать {}: {e}", path.display())))?;
        let imported = state
            .qd
            .call(
                "POST",
                "/client/api/routing/import",
                Some(serde_json::json!({ "code": code })),
            )
            .await?;
        Ok(Some(imported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lifecycle::tests::ids_are_unique;

    #[test]
    fn every_step_has_its_own_id() {
        ids_are_unique(START);
    }

    #[test]
    fn a_qd_that_never_ran_captures_nothing() {
        let state = Engine::state(&Qd::new());
        assert!(!state.on && !state.wanted && state.capture.is_none());
    }
}
