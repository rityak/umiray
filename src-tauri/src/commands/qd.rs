use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::app::connect;
use crate::app::state::AppState;
use crate::error::{AppError, Result};
use crate::system::elevation;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QdStatus {
    present: bool,
    elevated: bool,
    running: bool,
    state: Option<Value>,
    problem: Option<String>,
}

#[tauri::command]
pub async fn qd_status(state: State<'_, AppState>) -> Result<QdStatus> {
    let present = state.qd.present();
    let elevated = elevation::is_elevated();
    let (reply, problem) = if present && elevated {
        match state.qd.call("GET", "/client/api/state", None).await {
            Ok(value) => (Some(value), None),
            Err(why) => (None, Some(why.to_string())),
        }
    } else {
        (None, None)
    };
    Ok(QdStatus {
        present,
        elevated,
        running: state.qd.running().await,
        state: reply,
        problem,
    })
}

#[tauri::command]
pub async fn qd_call(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value> {
    state.qd.call(&method, &path, body).await
}

#[tauri::command]
pub async fn qd_start(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Value> {
    if state.supervisor.status().running {
        connect::stop(&app, &state).await;
    }
    state.qd.call("POST", "/client/api/connect", None).await?;
    state.qd.call("GET", "/client/api/state", None).await
}

#[tauri::command]
pub async fn qd_stop(state: State<'_, AppState>) -> Result<Value> {
    if !state.qd.running().await {
        return Ok(Value::Null);
    }
    state
        .qd
        .call("POST", "/client/api/disconnect", None)
        .await?;
    state.qd.call("GET", "/client/api/state", None).await
}

const RULES_FILTER: &[(&str, &str)] = &[("Правила qd (*.qdr)", "*.qdr"), ("Все файлы", "*.*")];

#[tauri::command]
pub async fn qd_rules_export(state: State<'_, AppState>) -> Result<Option<String>> {
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
        crate::system::pick::save("Сохранить правила qd", RULES_FILTER, &name, "qdr")
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

#[tauri::command]
pub async fn qd_rules_import(state: State<'_, AppState>) -> Result<Option<Value>> {
    let picked = tauri::async_runtime::spawn_blocking(|| {
        crate::system::pick::file("Загрузить правила qd", RULES_FILTER)
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

#[tauri::command]
pub async fn qd_install(state: State<'_, AppState>) -> Result<String> {
    state.qd.install().await
}

#[tauri::command]
pub fn qd_logs(state: State<'_, AppState>) -> Vec<String> {
    state.qd.logs()
}
