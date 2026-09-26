//! Ошибки приложения (D-028).
//!
//! Варианты заведены не «по слоям кода», а по тому, **что интерфейс должен предложить
//! пользователю**. Если два случая ведут к одной и той же кнопке — это один вариант.
//! Наружу уходит `{ kind, message, details }`, фронтенд ветвится по `kind`.

use serde::ser::{Serialize, SerializeStruct, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Ядра нет на диске. Интерфейс предлагает скачать.
    #[error("Ядро не найдено: {path}")]
    CoreMissing { path: String },

    /// Нужны права администратора, либо в них отказали. Интерфейс предлагает перезапуск.
    #[error("{message}")]
    NeedsElevation { message: String },

    /// Ядро не заработало. Причина видна только в его выводе, поэтому лог едет вместе с ошибкой.
    #[error("{message}")]
    CoreFailed { message: String, log: Vec<String> },

    /// Подписка не дала серверов. Провайдер объясняет причину служебными записями —
    /// их надо показать, иначе пустой список выглядит поломкой клиента.
    #[error("{message}")]
    Subscription {
        message: String,
        notices: Vec<String>,
    },

    /// Данные не годятся: битый YAML, неразбираемая ссылка, пустой ввод.
    #[error("{0}")]
    Invalid(String),

    /// Внешняя причина: сеть, чужой сервер.
    #[error("{0}")]
    Network(String),

    /// Файловая система. Текст с контекстом: «не удалось записать ядро» полезнее, чем
    /// голое «access denied» без указания, что именно не записалось.
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl AppError {
    /// Машинно-читаемый вид, по которому интерфейс выбирает действие.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CoreMissing { .. } => "coreMissing",
            Self::NeedsElevation { .. } => "needsElevation",
            Self::CoreFailed { .. } => "coreFailed",
            Self::Subscription { .. } => "subscription",
            Self::Invalid(_) => "invalid",
            Self::Network(_) => "network",
            Self::Io(_) => "io",
        }
    }

    /// Подробности одним списком строк: лог ядра или сообщения провайдера.
    /// Один тип на оба случая — интерфейсу достаточно отрисовать их построчно.
    pub fn details(&self) -> &[String] {
        match self {
            Self::CoreFailed { log, .. } => log,
            Self::Subscription { notices, .. } => notices,
            _ => &[],
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }
}

/// Ручная сериализация: у Tauri ошибка команды обязана быть `Serialize`, а форма ответа
/// зафиксирована в D-028 и не должна зависеть от того, как разложены поля вариантов.
impl Serialize for AppError {
    // Псевдоним `Result` в этом модуле однопараметрический, поэтому здесь полное имя.
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 3)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.serialize_field("details", self.details())?;
        state.end()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn json(error: AppError) -> serde_json::Value {
        serde_json::to_value(error).unwrap()
    }

    #[test]
    fn shape_is_stable_across_variants() {
        let value = json(AppError::CoreMissing {
            path: "C:/x/mihomo.exe".into(),
        });
        assert_eq!(value["kind"], "coreMissing");
        assert!(value["message"].as_str().unwrap().contains("mihomo.exe"));
        assert_eq!(value["details"].as_array().unwrap().len(), 0);

        let value = json(AppError::Invalid("битый YAML".into()));
        assert_eq!(value["kind"], "invalid");
        assert_eq!(value["message"], "битый YAML");
    }

    #[test]
    fn details_carry_the_reason_instead_of_being_glued_into_the_message() {
        let value = json(AppError::CoreFailed {
            message: "Ядро завершилось (1)".into(),
            log: vec!["первая".into(), "вторая".into()],
        });
        assert_eq!(
            value["message"], "Ядро завершилось (1)",
            "лог не приклеен к тексту"
        );
        assert_eq!(value["details"][1], "вторая");

        let value = json(AppError::Subscription {
            message: "Подписка не вернула серверов".into(),
            notices: vec!["Достигнут лимит устройств".into()],
        });
        assert_eq!(value["kind"], "subscription");
        assert_eq!(value["details"][0], "Достигнут лимит устройств");
    }

    #[test]
    fn io_errors_convert_with_the_question_mark() {
        fn failing() -> Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "нет файла",
            ))?;
            Ok(())
        }
        let error = failing().unwrap_err();
        assert_eq!(error.kind(), "io");
        assert!(error.to_string().contains("нет файла"));
    }
}
