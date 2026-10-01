//! Ключ ECDSA P-256 устройства WARP под MASQUE (D-165) в тех форматах, что ждут другие.
//!
//! Ядро читает приватный ключ как SEC1 (`x509.ParseECPrivateKey`), API Cloudflare — открытый
//! как SubjectPublicKeyInfo, а `ring` отдаёт PKCS#8. Переупаковка — по шаблону P-256,
//! и тест держит этот шаблон на настоящем ключе.

use crate::error::{AppError, Result};
use crate::nodes::codec::Base64;

/// Ключ устройства для MASQUE в base64: приватный — SEC1 (`x509.ParseECPrivateKey` ядра),
/// публичный — SubjectPublicKeyInfo (его ждёт API).
pub struct MasqueKey {
    pub private: String,
    pub public: String,
}

/// Начало PKCS#8 P-256 у `ring` до приватного скаляра — его шаблон
/// `ecPublicKey_p256_pkcs8_v1_template.der`; за скаляром — `a1 44 03 42 00` и точка.
const RING_P256_HEAD: [u8; 36] = [
    0x30, 0x81, 0x87, 0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02,
    0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x04, 0x6d, 0x30, 0x6b, 0x02,
    0x01, 0x01, 0x04, 0x20,
];
const PUBLIC_TAG: [u8; 5] = [0xa1, 0x44, 0x03, 0x42, 0x00];
/// OID prime256v1 с тегом `[0]`: в SEC1 кривая стоит в самом ключе, в PKCS#8 — снаружи.
const CURVE: [u8; 12] = [
    0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
];
/// Заголовок SubjectPublicKeyInfo P-256 до точки.
const SPKI_HEAD: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

impl MasqueKey {
    pub fn generate() -> Result<MasqueKey> {
        use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .map_err(|_| AppError::io("Не удалось завести ключ ECDSA"))?;
        MasqueKey::from_pkcs8(pkcs8.as_ref())
    }

    fn from_pkcs8(pkcs8: &[u8]) -> Result<MasqueKey> {
        let changed = || AppError::io("ring сменил формат ключа P-256 — переупаковка устарела");
        if pkcs8.len() != 138 || pkcs8[..36] != RING_P256_HEAD || pkcs8[68..73] != PUBLIC_TAG {
            return Err(changed());
        }
        let (scalar, point) = (&pkcs8[36..68], &pkcs8[73..]);
        let sec1 = [
            &[0x30, 0x77, 0x02, 0x01, 0x01, 0x04, 0x20][..],
            scalar,
            &CURVE,
            &PUBLIC_TAG,
            point,
        ]
        .concat();
        let spki = [&SPKI_HEAD[..], point].concat();
        Ok(MasqueKey {
            private: Base64::encode(&sec1),
            public: Base64::encode(&spki),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ключ `ring` переупакован в SEC1 с кривой и точкой — так его читает ядро. Шаблон
    /// `ring` проверяется на настоящем ключе: сменится — тест покраснеет, а не узел.
    #[test]
    fn the_masque_key_is_sec1_with_its_curve_and_point() {
        let key = MasqueKey::generate().unwrap();
        let sec1 = Base64::decode(&key.private).unwrap();
        let spki = Base64::decode(&key.public).unwrap();
        assert_eq!(sec1.len(), 121);
        assert_eq!(sec1[..7], [0x30, 0x77, 0x02, 0x01, 0x01, 0x04, 0x20]);
        assert_eq!(sec1[39..51], CURVE);
        assert_eq!(spki.len(), 91);
        assert_eq!(sec1[56..], spki[26..], "одна и та же открытая точка");
        assert_eq!(spki[26], 0x04, "точка несжатая");
        assert!(MasqueKey::from_pkcs8(&[0x30; 138]).is_err());
    }
}
