//! Команды про файлы конфига (D-044).
//!
//! Редактор работает с **пользовательскими** файлами, а не с рабочим конфигом: тот всегда
//! генерируется.

use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::app::status::Status;
use crate::config::files;
use crate::error::Result;

/// Разделы окна с документами внутри (D-044, D-070). Команды принимают идентификатор
/// документа, а не размножаются по файлу на каждую.
///
/// Какой набор применён, знают настройки, а не `config` (D-071) — поэтому список строится
/// здесь, на границе, где доступно и то, и другое.
#[tauri::command]
pub fn config_list(state: State<AppState>) -> Vec<files::Section> {
    files::list(state.applied_preset().as_deref())
}

#[tauri::command]
pub fn config_read(id: String) -> Result<String> {
    files::read(&id)
}

/// Записать документ — и довести до работающего ядра то, что до него доходит (D-143).
///
/// Решает не вид документа, а разница собранных конфигов, как и везде (D-102): правка
/// применённого набора или общих групп меняет сборку и доезжает, с обрывом соединений;
/// правка неприменённого набора сборку не меняет и просто лежит на диске (D-071).
#[tauri::command]
pub async fn config_write(
    id: String,
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Status> {
    files::write(&id, &text)?;
    connect::apply(&app, &state).await
}

/// Собранный конфиг целиком — то, что уходит ядру (D-130). Только для показа: правится
/// он в документах. Служебный вход замера не подмешиваем — он нужен живому ядру, а не читателю.
#[tauri::command]
pub fn config_assembled(state: State<AppState>) -> Result<String> {
    crate::render::effective::effective(state.routing()?.as_deref(), None).map(|built| built.yaml)
}

/// Умолчание у файла клиента — шаблон, у части набора — собранное клиентом, поэтому
/// сброс живёт в `config`, а не в `files` (D-071).
#[tauri::command]
pub fn config_reset(id: String) -> Result<String> {
    crate::render::effective::reset(&id)
}
