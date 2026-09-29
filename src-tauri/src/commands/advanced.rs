//! Команды про форму конфига ядра (D-086).
//!
//! Домен `advanced`, а не `config`: тот про документ целиком — прочитать, записать,
//! сбросить, — а здесь отдельные его поля, у которых есть форма.

use tauri::State;

use crate::app::state::AppState;
use crate::config::advanced::Advanced;
use crate::config::advanced::Options;
use crate::error::Result;

#[tauri::command]
pub fn advanced_get() -> Result<Options> {
    Advanced::read()
}

/// Записать и вернуть **прочитанное с диска**, а не присланное: форма показывает файл,
/// и после записи она обязана показать его, а не свою память о нём.
///
/// И довести до живого ядра (D-102): подробность лога и серверы имён доезжают
/// перезагрузкой, порт и туннель — нет, и о них окно скажет отдельно.
#[tauri::command]
pub async fn advanced_set(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    options: Options,
) -> Result<Options> {
    state
        .connection
        .change(&app, &state, || Advanced::write(&options))
        .await?;
    Advanced::read()
}
