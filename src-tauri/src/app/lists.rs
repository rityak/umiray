//! Rule sets для окна и для часов (D-157, D-158).
//!
//! Какие списки нужны и откуда, говорят документы наборов маршрутизации — раздел
//! `rule-sets`. Скачанное в `lists/` — кэш под них: окно качает список, когда его
//! добавляют, часы — недостающее и устаревшее, и они же убирают то, на что не ссылается
//! ни один набор.
//!
//! Порядок после любой перемены один: каждое ядро собирает из нейтральных файлов своё
//! (`Engine::lists_changed`), затем конфиг сверяется с работающим ядром. Изменилось только
//! содержимое списка — ядро перечитало провайдера, и сверка ничего не найдёт; изменился
//! состав — перезагрузка конфига.

use serde::Serialize;
use tauri::AppHandle;

use crate::app::state::AppState;
use crate::collections::Collections;
use crate::config::presets::PresetStore;
use crate::config::route::{RuleSetUse, Sections};
use crate::core::EngineId;
use crate::error::{AppError, Result};
use crate::lists::import::ListImporter;
use crate::lists::store::{ListStore, RuleList};
use crate::stamp::Stamp;

/// Сколько список считается свежим. Источники обновляются раз в сутки или чаще, реестр —
/// ежедневно; чаще качать 56 МБ antizapret незачем.
const FRESH_SECS: u64 = 24 * 3600;

/// Строка каталога для окна. Добавлен ли список в маршрут, знает черновик окна, а не мы.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: String,
    pub title: String,
    pub title_en: Option<String>,
    pub group: String,
    pub note: String,
    pub note_en: Option<String>,
    pub urls: Vec<String>,
}

pub struct Lists;

impl Lists {
    /// Всё скачанное — по нему окно показывает у строки маршрута числа, дату и отказ.
    pub fn list(&self) -> Vec<RuleList> {
        ListStore::list()
    }

    /// Каталог в его порядке — порядок в файле и есть задуманный: от главного к частному.
    pub fn catalog(&self) -> Result<Vec<Offer>> {
        Ok(Collections::lists()?
            .lists
            .into_iter()
            .map(|entry| Offer {
                id: entry.id,
                title: entry.title,
                title_en: entry.title_en,
                group: entry.group,
                note: entry.note,
                note_en: entry.note_en,
                urls: entry.urls,
            })
            .collect())
    }

    /// Скачать список для строки маршрута: из каталога или по адресу из документа.
    pub async fn fetch(
        &self,
        app: &AppHandle,
        state: &AppState,
        id: &str,
        url: Option<&str>,
    ) -> Result<RuleList> {
        let list = ListImporter::ensure(id, url).await?;
        self.changed(app, state).await?;
        Ok(list)
    }

    /// Свой список по адресу: имя выводится из подписи или адреса.
    pub async fn add_url(
        &self,
        app: &AppHandle,
        state: &AppState,
        url: &str,
        title: &str,
    ) -> Result<RuleList> {
        let list = ListImporter::add_url(url, title).await?;
        self.changed(app, state).await?;
        Ok(list)
    }

    /// Обновить один список или все. Отказ одного не отменяет остальных: причина у каждого
    /// ложится в его метаданные, а наверх уходит строкой.
    pub async fn refresh(&self, app: &AppHandle, state: &AppState, id: Option<&str>) -> Result<()> {
        let ids: Vec<String> = match id {
            Some(id) => vec![ListStore::get(id)?.id],
            None => ListStore::list().into_iter().map(|list| list.id).collect(),
        };
        let failures = refresh_each(&ids).await;
        self.changed(app, state).await?;
        failed(failures)
    }

    /// Скачать всё, на что ссылаются наборы и чего нет, — после записи кода, где список
    /// мог появиться строкой, а не кнопкой.
    pub async fn ensure(&self, app: &AppHandle, state: &AppState) -> Result<()> {
        let failures = ensure_referenced().await;
        self.changed(app, state).await?;
        failed(failures)
    }

    /// Заход фазы `tick`: докачать недостающее, обновить устаревшее, убрать ненужное.
    /// Без окна — поэтому только ядра: изменившийся состав доедет до конфига при
    /// следующей правке или запуске.
    pub async fn due(state: &AppState) -> Result<()> {
        let mut failures = ensure_referenced().await;
        let now = Stamp::now().unwrap_or_default();
        let stale: Vec<String> = ListStore::list()
            .into_iter()
            .filter(|list| {
                list.updated
                    .is_none_or(|updated| now.saturating_sub(updated) >= FRESH_SECS)
            })
            .map(|list| list.id)
            .collect();
        failures.extend(refresh_each(&stale).await);
        sweep()?;
        engines(state).await?;
        failed(failures)
    }

    async fn changed(&self, app: &AppHandle, state: &AppState) -> Result<()> {
        engines(state).await?;
        state.connection.apply(app, state).await?;
        Ok(())
    }
}

/// Списки из `rule-sets` всех наборов, без повторов. Документ, который не читается, не
/// мешает остальным: его ошибку покажет окно при открытии, а здесь только кэш.
fn referenced() -> Vec<RuleSetUse> {
    let mut all: Vec<RuleSetUse> = Vec::new();
    for preset in PresetStore::list() {
        let Ok(text) = PresetStore::content(&preset.id) else {
            continue;
        };
        let Ok(sections) = Sections::read(&text) else {
            continue;
        };
        for set in sections.rule_sets {
            if !all.iter().any(|seen| seen.id == set.id) {
                all.push(set);
            }
        }
    }
    all
}

async fn ensure_referenced() -> Vec<String> {
    let mut failures = Vec::new();
    for set in referenced() {
        if let Err(why) = ListImporter::ensure(&set.id, set.url.as_deref()).await {
            failures.push(format!("список «{}» не скачался: {why}", set.id));
        }
    }
    failures
}

/// Убрать из кэша то, на что не ссылается ни один набор: antizapret с его `.mrs` — это
/// сотня мегабайт, и держать их ради удалённой строки незачем.
fn sweep() -> Result<()> {
    let wanted: Vec<String> = referenced().into_iter().map(|set| set.id).collect();
    for list in ListStore::list() {
        if !wanted.contains(&list.id) {
            ListStore::delete(&list.id)?;
        }
    }
    Ok(())
}

/// Каждое ядро собирает из списков своё. У ядра без правил это пустой шаг (D-154).
async fn engines(state: &AppState) -> Result<()> {
    for id in EngineId::ALL {
        state.engine(id).lists_changed().await?;
    }
    Ok(())
}

async fn refresh_each(ids: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    for id in ids {
        if let Err(why) = ListImporter::refresh(id).await {
            failures.push(format!("список «{id}» не обновился: {why}"));
        }
    }
    failures
}

fn failed(failures: Vec<String>) -> Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::network(failures.join("; ")))
    }
}
