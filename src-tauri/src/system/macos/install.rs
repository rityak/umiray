//! На macOS клиент запускается из своего `.app`: самозамены в каталоге данных нет.

pub struct Installation;

impl Installation {
    pub fn handoff(_identifier: &str) -> bool {
        false
    }
}
