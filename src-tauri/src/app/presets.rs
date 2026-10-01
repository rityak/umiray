//! Наборы маршрутизации как действие человека (D-071, D-155): завести, применить, удалить.
//!
//! Хранит наборы `config/presets.rs`; здесь — то, что вокруг хранения: какой набор
//! применён, что нельзя удалять, и что применение значит для работающего ядра.

use tauri::AppHandle;

use crate::app::state::AppState;
use crate::config::presets::Preset;
use crate::config::presets::PresetStore;
use crate::error::{AppError, Result};

pub struct Presets;

impl Presets {
    /// Завести набор из того, что клиент собирает сам из источников.
    ///
    /// Он же первый: разделу без единого набора нечего показывать, поэтому один существует
    /// всегда (D-071). Содержимое — собранное, а не шаблон с рассказом: человек открывает
    /// раздел, чтобы поправить рабочий конфиг, а не прочитать инструкцию.
    pub fn create(&self) -> Result<Preset> {
        PresetStore::create(
            PresetStore::default_name(),
            &crate::render::effective::ConfigRenderer::generated_rules()?,
        )
    }

    /// Применить набор: с этого момента маршрут решают его документы.
    ///
    /// Маршрутизация при этом включается — это и значит «применить» (D-166): набор, лежащий
    /// в стороне от сборки, применённым не является ни в каком смысле. Документы набора
    /// ядро читает на старте (D-010), поэтому работающее перезапускается (D-064).
    pub async fn select(&self, app: &AppHandle, state: &AppState, id: &str) -> Result<()> {
        self.choose(state, id)?;
        if state.mihomo.status().running {
            state.connection.restart(app, state).await?;
        }
        Ok(())
    }

    /// Запомнить выбор, не трогая ядро: вторая половина `select`, нужная и сама по себе —
    /// живым проверкам, где ядро поднимают руками.
    pub fn choose(&self, state: &AppState, id: &str) -> Result<()> {
        PresetStore::get(id)?;
        state.settings.update(|settings| {
            settings.preset = Some(id.to_string());
            settings.routing = true;
        })
    }

    /// Удалить набор.
    ///
    /// Применённый удалять нельзя: маршрут остался бы без документа, который его решает.
    /// Последний — тоже: разделу «Маршрутизация» нечего было бы показать, а завести новый
    /// можно и поверх старого.
    pub fn delete(&self, state: &AppState, id: &str) -> Result<()> {
        if state.routing.applied_preset(state).as_deref() == Some(id) {
            return Err(AppError::invalid(
                "Этот набор сейчас применён — сначала переключитесь на другой",
            ));
        }
        if PresetStore::list().len() <= 1 {
            return Err(AppError::invalid(
                "Это единственный набор: его правят, а не удаляют",
            ));
        }
        PresetStore::delete(id)?;
        // Запомненный, но не применённый: ссылку чистим, иначе сборка полезет за набором,
        // которого нет.
        if state.settings.get().preset.as_deref() == Some(id) {
            state.settings.update(|settings| settings.preset = None)?;
        }
        Ok(())
    }
}
