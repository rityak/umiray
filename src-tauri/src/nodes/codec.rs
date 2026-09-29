//! base64 в том виде, в каком его шлют панели: с набивкой и без, обычный и url-safe.
//!
//! Отдельный модуль, потому что декодируют двое и по разным поводам — тело подписки целиком
//! и отдельные ссылки внутри неё. Ходить за кодеком в разборщик ссылок было бы странно.

use base64::Engine;

pub struct Base64;

impl Base64 {
    pub fn decode(raw: &str) -> Option<Vec<u8>> {
        let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        [
            base64::engine::general_purpose::STANDARD,
            base64::engine::general_purpose::STANDARD_NO_PAD,
            base64::engine::general_purpose::URL_SAFE,
            base64::engine::general_purpose::URL_SAFE_NO_PAD,
        ]
        .iter()
        .find_map(|engine| engine.decode(&compact).ok())
    }

    /// Обратно в base64: имя внутри `vmess://` правится и упаковывается заново.
    pub fn encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    pub fn decode_text(raw: &str) -> Option<String> {
        String::from_utf8(Base64::decode(raw)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_flavours_read_the_same() {
        // "метод:пароль" в четырёх видах: с набивкой и без, обычный и url-safe.
        for encoded in [
            "YWVzLTI1Ni1nY206cHc=",
            "YWVzLTI1Ni1nY206cHc",
            "YWVzLTI1Ni1nY206cHc=",
            "YWVzLTI1Ni1nY206cHc",
        ] {
            assert_eq!(
                Base64::decode_text(encoded).as_deref(),
                Some("aes-256-gcm:pw")
            );
        }
        // url-safe отличается алфавитом: '-' и '_' вместо '+' и '/'.
        assert_eq!(Base64::decode_text("Pz8_Pz8").as_deref(), Some("?????"));
        assert!(Base64::decode_text("не base64 вовсе").is_none());
    }
}
