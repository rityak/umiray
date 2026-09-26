//! Команды про источники узлов: подписки и ручные ссылки (D-032).

use serde::Serialize;
use tauri::State;

use crate::app::state::AppState;
use crate::error::AppError;
use crate::error::Result;
use crate::nodes::source_import;
use crate::nodes::sources::{self, Source};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// Служебные сообщения провайдера — их показываем, а не прячем.
    notices: Vec<String>,
    source: Source,
}

#[tauri::command]
pub fn sources_list() -> Vec<Source> {
    sources::list()
}

/// Одно поле на всё: ссылка на сервер дописывается в «мои ссылки», остальное считается
/// подпиской и заводит отдельный источник (D-032).
/// Дать живому ядру перечитать источник (S-012) и **не считать неудачу ошибкой команды**.
///
/// Список провайдеров ядро строит из конфига при запуске, поэтому источник, заведённый
/// на ходу, ему пока неизвестен — перечитывать нечего. Файл при этом уже на диске, и
/// сказать «не удалось» означало бы соврать: добавление удалось, не хватает только
/// переподключения. Возвращаем это отдельной строкой, как и прочие служебные сообщения.
///
/// Перечитанный источник рвёт открытые соединения (D-143): узлы в нём могли смениться,
/// а первый источник к тому же переводит направление с DIRECT на автовыбор (D-056).
/// Фоновое обновление подписок сюда не ходит и не рвёт ничего — человек его не просил.
pub async fn reload(app: &tauri::AppHandle, state: &AppState, id: &str) -> Vec<String> {
    // Новый источник — новый провайдер в собранном конфиге: он доезжает перезагрузкой
    // конфига (D-102), и только после неё ядру есть что перечитывать.
    let applied = crate::app::connect::apply(app, state).await.map(|_| ());
    let reloaded = applied.and(state.supervisor.reload(id).await);
    let cut = state.supervisor.close_connections().await;
    match reloaded.and(cut) {
        Ok(()) => Vec::new(),
        Err(_) => vec!["Переподключитесь, чтобы ядро увидело изменения.".into()],
    }
}

#[tauri::command]
pub async fn sources_add(
    input: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    let input = input.trim().to_string();
    if input.is_empty() {
        return Err(AppError::invalid("Пустая ссылка"));
    }
    let had_sources = !sources::list().is_empty();

    let (source, mut notices) = if input.starts_with("http://") || input.starts_with("https://") {
        source_import::add_subscription(&input).await?
    } else {
        (source_import::add_link(&input)?, Vec::new())
    };
    // Первый источник включает автовыбор: до него направление честно стояло на DIRECT
    // (D-056). Неудача записи — не повод считать импорт провалившимся, узлы уже на диске.
    let _ = state.note_source_added(had_sources);
    spawn_geo();
    notices.extend(reload(&app, &state, &source.id).await);
    Ok(Import { notices, source })
}

/// Добавить узел записью: собранный в окне руками (D-120).
///
/// Принимаем **отображение**, а не текст: форма и код в окне правят один и тот же объект,
/// и разбирать его второй раз здесь значило бы разойтись с ним на первом же поле.
#[tauri::command]
pub async fn sources_add_proxy(
    entry: serde_yaml::Mapping,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    let had_sources = !sources::list().is_empty();
    let source = source_import::add_proxy(entry)?;
    let _ = state.note_source_added(had_sources);
    let notices = reload(&app, &state, &source.id).await;
    spawn_geo();
    Ok(Import { notices, source })
}

/// Тот же узел текстом — для «кода» в окне сборки (D-120, D-074). Рендерит бэкенд:
/// YAML в окне пришлось бы писать вторым, своим, и он разошёлся бы с первым.
#[tauri::command]
pub fn sources_proxy_yaml(entry: serde_yaml::Mapping) -> Result<String> {
    // Порядок ключей тот же, что у записанного узла: объект приезжает из окна через JSON
    // и по дороге теряет его — иначе код открывался бы алфавитным списком.
    serde_yaml::to_string(&serde_yaml::Value::Mapping(sources::ordered(entry)))
        .map_err(|e| AppError::invalid(e.to_string()))
}

/// Добавить узел, набранный кодом. Отдельной командой от формы, а не флагом: это два
/// разных входа с разной проверкой — у текста она одна, «это вообще YAML».
#[tauri::command]
pub async fn sources_add_proxy_text(
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    let entry: serde_yaml::Mapping = serde_yaml::from_str(&text)
        .map_err(|e| AppError::invalid(format!("Это не конфиг узла: {e}")))?;
    sources_add_proxy(entry, app, state).await
}

/// Спросить файл системным окном и принять его (D-120). `None` — человек закрыл окно;
/// это не ошибка и сообщением не является.
#[tauri::command]
pub async fn sources_add_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Import>> {
    // Окно модальное и держит поток, пока человек не ответит, — поэтому не в рантайме.
    let picked = tauri::async_runtime::spawn_blocking(|| {
        crate::system::pick::file(
            "Конфиг WireGuard или AmneziaWG",
            &[
                ("Конфиг WireGuard (*.conf)", "*.conf"),
                ("Все файлы", "*.*"),
            ],
        )
    })
    .await
    .map_err(|e| AppError::io(format!("Окно выбора файла не открылось: {e}")))?;
    let Some(path) = picked else {
        return Ok(None);
    };

    let had_sources = !sources::list().is_empty();
    let source = source_import::import_file(&path)?;
    let _ = state.note_source_added(had_sources);
    let notices = reload(&app, &state, &source.id).await;
    spawn_geo();
    Ok(Some(Import { notices, source }))
}

/// Обновить **все** подписки разом — кнопка в полосе узлов. Ручные ссылки пропускаются
/// сами: у них нет адреса, откуда обновляться.
///
/// Отдаёт список того, что не получилось, а не ошибку: одна упавшая подписка не отменяет
/// остальных, и превращать её в отказ команды значило бы прятать удачные обновления.
/// Замер задержек после этого делает окно отдельной командой — он и так на кнопке.
#[tauri::command]
pub async fn sources_refresh_all(state: State<'_, AppState>) -> Result<Vec<String>> {
    let failures = crate::app::refresher::refresh_all(&state, None).await;
    // Кнопку нажал человек: обновлённые узлы могли сменить адреса (D-143).
    let _ = state.supervisor.close_connections().await;
    spawn_geo();
    Ok(failures)
}

#[tauri::command]
pub async fn sources_refresh(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    let (source, mut notices) = source_import::refresh(&id).await?;
    notices.extend(reload(&app, &state, &source.id).await);
    spawn_geo();
    Ok(Import { notices, source })
}

/// Спросить страны у новых узлов, не задерживая ответ команды (D-084).
///
/// Именно отдельной задачей: узлов бывает три десятка, каждый — запрос к чужому сервису,
/// и держать на этом окно, которое уже показало «источник добавлен», незачем. Фоновый
/// такт подобрал бы их и сам, но через минуту — а флага ждут сразу.
fn spawn_geo() {
    tauri::async_runtime::spawn(async {
        let _ = crate::nodes::geo::refresh().await;
    });
}

/// Что прислала панель, слово в слово (D-065). Правится именно это, а не собранный файл:
/// тот пересобирается при каждом обновлении.
#[tauri::command]
pub fn sources_read(id: String) -> String {
    sources::raw(&id)
}

/// Переписать источник руками. Состав узлов при этом меняется, но пересобирать ничего
/// не надо: то, что клиент собирает сам, собирается в момент запуска (D-071) — B-005
/// в этой раскладке невозможен.
#[tauri::command]
pub async fn sources_write(
    id: String,
    text: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Import> {
    let source = sources::write_raw(&id, &text)?;
    let notices = reload(&app, &state, &id).await;
    spawn_geo();
    Ok(Import { notices, source })
}

/// Удаление источника. Собранное клиентом переживёт это само, а вот применённый набор
/// пользователя мог ссылаться на источник поимённо — об этом предупреждаем словами.
#[tauri::command]
pub async fn sources_delete(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    // Предупреждение собираем **до** удаления: после него файла источника уже нет,
    // а ссылку на него в наборе пользователя надо успеть заметить.
    let warning = state.referenced_warning(&id);
    sources::delete(&id)?;
    // Узлы удалённого источника живут в ядре до перезагрузки конфига — и выход мог
    // стоять на одном из них (D-143).
    crate::app::connect::apply(&app, &state).await?;
    Ok(warning)
}
