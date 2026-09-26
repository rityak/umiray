//! Закрытая форма идентификатора источника до построения пути.

use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceId(String);

impl SourceId {
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() == 16
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            Ok(Self(value.to_string()))
        } else {
            Err(AppError::invalid(format!(
                "Неверный идентификатор источника: {value}"
            )))
        }
    }

    pub fn new() -> Result<Self> {
        Self::parse(&crate::stamp::id()?)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_generated_shape_is_accepted() {
        assert!(SourceId::parse("0123456789abcdef").is_ok());
        for bad in [
            "own",
            "ABCDEF0123456789",
            "../0123456789abcd",
            "0123456789abcdeg",
        ] {
            assert!(SourceId::parse(bad).is_err(), "принят {bad}");
        }
    }
}
