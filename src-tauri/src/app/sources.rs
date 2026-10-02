//! Источники узлов как действие человека (D-032, D-155): добавить, обновить, поправить,
//! удалить — и довести изменение до работающего ядра.
//!
//! Хранит источники `nodes/sources.rs`, загружает и разбирает `nodes/source_import.rs`,
//! правит записи `nodes/source_editor.rs`. Здесь — то, что вокруг: первый источник меняет
//! направление, изменение доезжает до ядра и рвёт соединения, страны узлов обновляются.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app::node_check::NodeCheck;
use crate::app::state::AppState;
use crate::config::files::Documents;
use crate::config::presets::PresetStore;
use crate::error::{AppError, Result};
use crate::nodes::source_import::SourceImporter;
use crate::nodes::sources::Source;
use crate::nodes::sources::SourceStore;

/// Итог добавления или обновления: сам источник и что по дороге стоит сказать человеку.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// Служебные сообщения провайдера — их показываем, а не прячем.
    notices: Vec<String>,
    source: Source,
}

pub struct Sources;

impl Sources {
    /// Одно поле на всё: ссылка на сервер дописывается в «мои ссылки», остальное считается
    /// подпиской и заводит отдельный источник (D-032).
    pub async fn add(&self, app: &AppHandle, state: &AppState, input: &str) -> Result<Import> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AppError::invalid("Пустая ссылка"));
        }
        let had_sources = !SourceStore::list().is_empty();
        // Схема ссылки регистра не различает (RFC 3986): `HTTPS://` — тоже подписка.
        let scheme = input.to_ascii_lowercase();
        let (source, notices) = if scheme.starts_with("http://") || scheme.starts_with("https://") {
            SourceImporter::add_subscription(input).await?
        } else {
            (SourceImporter::add_link(input)?, Vec::new())
        };
        Ok(self.added(app, state, source, notices, had_sources).await)
    }

    /// Узел, собранный формой (D-120).
    pub async fn add_proxy(
        &self,
        app: &AppHandle,
        state: &AppState,
        entry: serde_yaml::Mapping,
    ) -> Result<Import> {
        NodeCheck::entry(&entry)?;
        let had_sources = !SourceStore::list().is_empty();
        let source = SourceImporter::add_proxy(entry)?;
        Ok(self
            .added(app, state, source, Vec::new(), had_sources)
            .await)
    }

    /// Узел, вставленный текстом конфига.
    pub async fn add_proxy_text(
        &self,
        app: &AppHandle,
        state: &AppState,
        text: &str,
    ) -> Result<Import> {
        let entry: serde_yaml::Mapping = serde_yaml::from_str(text)
            .map_err(|e| AppError::invalid(format!("Это не конфиг узла: {e}")))?;
        self.add_proxy(app, state, entry).await
    }

    /// Конфиг WireGuard, AmneziaWG, OpenVPN или usque файлом (D-120, D-164). Пусто — человек
    /// передумал.
    pub async fn add_file(&self, app: &AppHandle, state: &AppState) -> Result<Option<Import>> {
        // Окно модальное и держит поток, пока человек не ответит, — поэтому не в рантайме.
        let picked = tauri::async_runtime::spawn_blocking(|| {
            crate::system::pick::FileDialog::file(
                "Конфиг WireGuard, AmneziaWG, OpenVPN или usque",
                &[
                    (
                        "Конфиг VPN (*.conf, *.ovpn, *.json)",
                        "*.conf;*.ovpn;*.json",
                    ),
                    ("Все файлы", "*.*"),
                ],
            )
        })
        .await
        .map_err(|e| AppError::io(format!("Окно выбора файла не открылось: {e}")))?;
        let Some(path) = picked else {
            return Ok(None);
        };
        let entry = SourceImporter::file_entry(&path)?;
        Ok(Some(self.add_proxy(app, state, entry).await?))
    }

    /// Cloudflare WARP (D-165): ключ от ядра, регистрация у Cloudflare, узел — в «Свои узлы».
    /// Нажатие кнопки — согласие человека с условиями WARP: окно их называет.
    pub async fn add_warp(
        &self,
        app: &AppHandle,
        state: &AppState,
        tunnel: crate::nodes::warp::Tunnel,
    ) -> Result<Import> {
        let keys = crate::core::mihomo::keys::KeyGen::wireguard()?;
        let mask = crate::config::awg::Mask::get().option();
        let status = state.mihomo.status();
        let through = status.running.then_some(status.port).flatten();
        let entry = crate::nodes::warp::Warp::issue(tunnel, &keys, mask, through).await?;
        self.add_proxy(app, state, entry).await
    }

    /// Обновить одну подписку по кнопке.
    pub async fn refresh(&self, app: &AppHandle, state: &AppState, id: &str) -> Result<Import> {
        let refreshed = SourceImporter::refresh(id).await;
        // Удача снимает жалобу фонового обновления, отказ её ставит (D-038).
        crate::app::refresher::Refresher::complain(state);
        let (source, mut notices) = refreshed?;
        notices.extend(reload(app, state, &source.id).await);
        spawn_geo();
        Ok(Import { notices, source })
    }

    /// Все подписки разом. Отдаёт, какие не обновились: одна неудачная не отменяет остальные.
    pub async fn refresh_all(&self, state: &AppState) -> Vec<String> {
        let failures = crate::app::refresher::Refresher::refresh_all(state, None).await;
        // Кнопку нажал человек: обновлённые узлы могли сменить адреса (D-143).
        let _ = state.mihomo.close_connections().await;
        spawn_geo();
        failures
    }

    /// Сырьё источника поправили текстом.
    pub async fn write(
        &self,
        app: &AppHandle,
        state: &AppState,
        id: &str,
        text: &str,
    ) -> Result<Import> {
        NodeCheck::document(text)?;
        let source = SourceStore::write_raw(id, text)?;
        let notices = reload(app, state, id).await;
        spawn_geo();
        Ok(Import { notices, source })
    }

    /// Удалить источник. Отдаёт предупреждение, если на него ссылается написанное человеком.
    pub async fn delete(
        &self,
        app: &AppHandle,
        state: &AppState,
        id: &str,
    ) -> Result<Option<String>> {
        // Предупреждение собираем **до** удаления: после него файла источника уже нет,
        // а ссылку на него в наборе пользователя надо успеть заметить.
        let warning = referenced_warning(id);
        // Узлы удалённого источника живут в ядре до перезагрузки конфига — и выход мог
        // стоять на одном из них (D-143).
        state
            .connection
            .change(app, state, || SourceStore::delete(id))
            .await?;
        // Жалоба могла быть об этой подписке — её больше нет.
        crate::app::refresher::Refresher::complain(state);
        Ok(warning)
    }

    /// Поправить узел источника (D-114, D-119) — и дать живому ядру перечитать источник:
    /// правка узла меняет его запись, а не состав провайдеров, и перезагрузки конфига
    /// не требует (S-012). Соединения рвутся: узел мог сменить адрес (D-143).
    pub async fn edit(
        &self,
        state: &AppState,
        id: &str,
        change: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        change()?;
        state.mihomo.reload(id).await?;
        state.mihomo.close_connections().await
    }

    /// Общий хвост добавления: первый источник включает автовыбор (D-056), источник доезжает
    /// до ядра, страны и задержки новых узлов уточняются. Неудача записи направления — не повод
    /// считать импорт провалившимся: узлы уже на диске.
    async fn added(
        &self,
        app: &AppHandle,
        state: &AppState,
        source: Source,
        mut notices: Vec<String>,
        had_sources: bool,
    ) -> Import {
        let _ = state.routing.note_source_added(state, had_sources);
        notices.extend(reload(app, state, &source.id).await);
        spawn_geo();
        spawn_ping(app, &source.id);
        Import { notices, source }
    }
}

/// Дать живому ядру перечитать источник (S-012) и **не считать неудачу ошибкой**.
///
/// Список провайдеров ядро строит из конфига при запуске, поэтому источник, заведённый
/// на ходу, ему пока неизвестен — перечитывать нечего. Файл при этом уже на диске, и
/// сказать «не удалось» означало бы соврать: добавление удалось, не хватает только
/// переподключения. Возвращаем это отдельной строкой, как и прочие служебные сообщения.
///
/// Перечитанный источник рвёт открытые соединения (D-143): узлы в нём могли смениться,
/// а первый источник к тому же переводит направление с DIRECT на автовыбор (D-056).
/// Фоновое обновление подписок сюда не ходит и не рвёт ничего — человек его не просил.
async fn reload(app: &AppHandle, state: &AppState, id: &str) -> Vec<String> {
    // Новый источник — новый провайдер в собранном конфиге: он доезжает перезагрузкой
    // конфига (D-102), и только после неё ядру есть что перечитывать.
    let applied = state.connection.apply(app, state).await.map(|_| ());
    let reloaded = applied.and(state.mihomo.reload(id).await);
    let cut = state.mihomo.close_connections().await;
    match reloaded.and(cut) {
        Ok(()) => Vec::new(),
        Err(_) => vec!["Переподключитесь, чтобы ядро увидело изменения.".into()],
    }
}

/// Страны узлов — в фоне: запрос к внешнему сервису не должен задерживать ответ окну.
fn spawn_geo() {
    tauri::async_runtime::spawn(async {
        let _ = crate::nodes::geo::GeoCache::refresh().await;
    });
}

/// Задержки нового источника — в фоне (D-166): без них свежие узлы стояли прочерками,
/// пока человек не нажмёт «Проверить задержку», а мерить заодно весь список незачем.
/// Отказ молчит, как и прочие автоматические замеры (D-062).
fn spawn_ping(app: &AppHandle, source: &str) {
    let app = app.clone();
    let source = source.to_string();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _ = state.catalog.measure_source(&state, &source).await;
    });
}

/// Ссылается ли что-нибудь написанное человеком на этот источник — и если да, сказать
/// об этом словами.
///
/// Пересобрать чужие документы за него нельзя, а удаление источника оставит в `use`
/// ссылку в никуда. Ядро на такой конфиг отвечает отказом стартовать **целиком**
/// (B-005), а связь между «удалил подписку» и «VPN больше не поднимается» иначе
/// не увидеть: ошибка приходит только при следующем запуске.
///
/// Смотрим **везде, а не в применённом наборе**: группы общие и участвуют в сборке
/// всегда (D-075), а набор правил можно применить и завтра.
fn referenced_warning(id: &str) -> Option<String> {
    use crate::config::files;
    let groups = Documents::read(files::GROUPS).unwrap_or_default();
    if mentions(&groups, id) {
        return Some(
            "Ваши группы ссылаются на этот источник. Уберите ссылку — иначе ядро не запустится."
                .into(),
        );
    }
    let named: Vec<String> = PresetStore::list()
        .into_iter()
        .filter(|preset| PresetStore::content(&preset.id).is_ok_and(|rules| mentions(&rules, id)))
        .map(|preset| format!("«{}»", preset.name))
        .collect();
    (!named.is_empty()).then(|| {
        format!(
            "На этот источник ссылается маршрутизация: {}. Уберите ссылку — иначе ядро не запустится, когда набор применят.",
            named.join(", ")
        )
    })
}

/// Упоминается ли источник в документе. Простое вхождение подстроки, и этого достаточно:
/// идентификатор источника — шестнадцатеричная строка в шестнадцать знаков, случайно
/// такая в тексте не встречается. Ложное срабатывание здесь дешевле пропуска: цена
/// пропуска — ядро, которое не поднимется вовсе.
fn mentions(document: &str, id: &str) -> bool {
    !id.is_empty() && document.contains(id)
}

#[cfg(test)]
mod tests {
    use super::mentions;

    #[test]
    fn a_source_is_found_wherever_it_is_written() {
        let groups = "proxy-groups:
  - name: Европа
    use: [aaaa1111bbbb2222]
";
        assert!(mentions(groups, "aaaa1111bbbb2222"));
        assert!(!mentions(groups, "cccc3333dddd4444"));
        assert!(mentions(
            "rules:
  - RULE-SET,aaaa1111bbbb2222,umiray
",
            "aaaa1111bbbb2222"
        ));
        assert!(!mentions("", "aaaa1111bbbb2222"));
        assert!(
            !mentions("что угодно", ""),
            "пустой идентификатор не совпадает со всем"
        );
    }
}
