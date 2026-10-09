//! Обслуживание клиента (D-155): сброс к состоянию «как после установки» и перезапуск
//! с правами администратора. Оба начинаются одинаково — погасить все ядра, — поэтому
//! живут рядом.

use tauri::AppHandle;

use crate::app::data::DataDir;
use crate::app::settings;
use crate::app::settings::SettingsStore;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::files::Documents;
use crate::config::presets::PresetStore;
use crate::core::EngineId;
use crate::error::{AppError, Result};

/// С этим аргументом клиента зовёт удаление (хук `NSIS_HOOK_PREUNINSTALL`, B-044).
pub const UNINSTALL: &str = "--uninstall";

pub struct Maintenance;

impl Maintenance {
    /// Прибраться перед удалением (B-044): после него клиента, который вернул бы Windows как
    /// было, больше не будет. Работающая копия уходит штатно — гасит ядро, снимает прокси
    /// и запрет, права у неё для этого есть. Упавшая раньше оставила снимок — снимаем его
    /// сами; запрет брандмауэра без прав не снять, и это лучшее, что можно сделать.
    ///
    /// Хук стоит до того, как установщик закрывает клиента сам — принудительно, без уборки.
    pub fn uninstall(identifier: &str) {
        crate::system::instance::Instance::send_away(
            identifier,
            std::time::Duration::from_secs(15),
        );
        // Базы нет — клиент не запускался, и снимать нечего; открыть её значило бы завести.
        if !crate::db::Db::exists() {
            return;
        }
        let state = AppState::new();
        let _ = state.proxy.release(&state);
        let _ = state.kill_switch.release(&state);
    }

    /// Сброс всего, кроме скачанных ядер и идентификатора устройства.
    ///
    /// Нужен именно как одна кнопка: разбираться, какой из файлов на диске испортился,
    /// пользователь не обязан, а по одному их чистить — это знать раскладку каталога.
    /// Ядра останавливаем сами: они держат рабочий каталог и читают файлы источников,
    /// а требовать «сначала отключитесь» — это перекладывать на пользователя то,
    /// что мы и так знаем.
    pub async fn reset(&self, app: &AppHandle, state: &AppState) -> Result<Status> {
        let _transition = state.connection.lock().await;
        stop_engines(state).await;
        state.kill_switch.release(state)?;
        state.proxy.release(state)?;
        factory()?;
        state.settings.reload();
        Ok(state.connection.shown(app, state))
    }

    /// Настройки копией базы через окно сохранения (D-163). Пусто — человек передумал.
    pub async fn export(&self) -> Result<Option<String>> {
        let picked = tauri::async_runtime::spawn_blocking(|| {
            crate::system::pick::FileDialog::save(
                "Экспорт настроек umiray",
                &[("Настройки umiray (*.db)", "*.db")],
                "umiray-settings.db",
                "db",
            )
        })
        .await
        .map_err(|e| AppError::io(format!("Окно сохранения не открылось: {e}")))?;
        let Some(path) = picked else {
            return Ok(None);
        };
        DataDir::export(&path)?;
        Ok(Some(path.display().to_string()))
    }

    /// Перезапустить приложение с правами администратора: только так включается TUN.
    /// Текущее окно закрываем сами — две копии одновременно ни к чему.
    pub async fn relaunch_elevated(&self, app: &AppHandle, state: &AppState) -> Result<()> {
        let _transition = state.connection.lock().await;
        crate::system::elevation::Elevation::relaunch_as_admin()?;
        stop_engines(state).await;
        app.exit(0);
        Ok(())
    }
}

/// Погасить все ядра — без обвязки: её снимает вызывающий, каждый по-своему.
async fn stop_engines(state: &AppState) {
    state.volt.stop();
    for id in EngineId::ALL {
        let _ = state.engine(id).stop().await;
    }
}

/// Стирает источники, возвращает конфиги к шаблонам и настройки к умолчаниям.
///
/// Что **не** трогаем и почему:
/// - бинари ядер — пятьдесят мегабайт, качать заново это наказание, а не сброс;
/// - `hwid.txt` — новый идентификатор съест ещё один слот устройства в подписке (GOTCHAS);
/// - `config.yaml.migrated` — единственная копия конфига пользователя до переезда.
///
/// Ядро к этому моменту должно быть остановлено: оно держит `run/` и читает источники.
fn factory() -> Result<()> {
    // Каталог целиком: в нём лежат `.raw`, `.txt`, `.patch.json` и `.json` на каждый
    // источник, и выборочная чистка означала бы помнить этот список в двух местах.
    crate::nodes::sources::SourceStore::clear()?;

    // Файлы клиента — к шаблонам. Наборы стираем целиком и заводим первый заново:
    // «как после установки» — это один набор из того, что собирается сейчас (D-071),
    // а источников к этому моменту уже нет.
    for id in Documents::templated() {
        Documents::reset(id)?;
    }
    for preset in PresetStore::list() {
        PresetStore::delete(&preset.id)?;
    }
    PresetStore::create(
        PresetStore::default_name(),
        &crate::render::effective::ConfigRenderer::generated_rules()?,
    )?;

    // Сгенерированный конфиг не трогаем руками: он пересоберётся при следующем запуске
    // из того, что осталось. Удалять его — значит держать знание о нём и здесь тоже.
    SettingsStore::save(&settings::Settings::default())
}
