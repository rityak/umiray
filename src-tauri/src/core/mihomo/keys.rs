//! Ключи, которые умеет само ядро: `mihomo generate` (D-165).
//!
//! Пара X25519 нужна WARP. Кривую в клиенте мы не держим — ядро уже на диске и умеет её,
//! а второй криптограф в программе — вторая поверхность для ошибок.

use std::process::Command;

use crate::core::mihomo::Mihomo;
use crate::error::{AppError, Result};
use crate::nodes::warp::WgKeys;

pub struct KeyGen;

impl KeyGen {
    /// `mihomo generate wg-keypair`: пара для WireGuard, она же ключ устройства WARP.
    pub fn wireguard() -> Result<WgKeys> {
        let binary = Mihomo::binary();
        if !binary.exists() {
            return Err(AppError::CoreMissing {
                path: binary.display().to_string(),
            });
        }
        let mut command = Command::new(binary);
        command.args(["generate", "wg-keypair"]);
        crate::system::console::Console::hide(&mut command, false);
        let output = command
            .output()
            .map_err(|e| AppError::io(format!("Не удалось запустить ядро: {e}")))?;
        parse(&String::from_utf8_lossy(&output.stdout))
            .ok_or_else(|| AppError::io("Ядро не выдало ключи WireGuard"))
    }
}

/// `PrivateKey: …` и `PublicKey: …` строками — так их печатает ядро.
fn parse(out: &str) -> Option<WgKeys> {
    let find = |label: &str| {
        out.lines()
            .find_map(|line| line.trim().strip_prefix(label))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    Some(WgKeys {
        private: find("PrivateKey:")?,
        public: find("PublicKey:")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_prints_both_keys_by_name() {
        let keys = parse("PrivateKey: cHJpdg==\nPublicKey: cHVi\n").unwrap();
        assert_eq!(keys.private, "cHJpdg==");
        assert_eq!(keys.public, "cHVi");
        assert!(parse("panic: Using: generate uuid/...").is_none());
    }
}
