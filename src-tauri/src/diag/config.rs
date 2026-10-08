//! Примет ли ядро то, что мы собрали, — не запуская VPN.
//!
//! У mihomo для этого есть свой ключ (`-t`): он читает конфиг, ругается и выходит.
//! Секунда без единого пакета в сеть — и ответ на добрую половину вопросов «почему
//! не подключается». Зовёт проверка перед подключением (D-106).
//!
//! Проверяем **отдельный файл**, а не `run/config.yaml`: тот принадлежит запущенному
//! ядру, и переписывать его ради проверки значит трогать работающий VPN.

use std::process::Command;

use crate::core::mihomo::Mihomo;
use crate::error::{AppError, Result};

/// Имя временного файла. Лежит рядом с рабочим конфигом: у ядра там уже есть права
/// и туда же смотрит `SAFE_PATHS`.
const PROBE: &str = "config.test.yaml";

/// Что сказало ядро о готовом конфиге. Пусто — принял.
///
/// Конфиг приходит уже собранным: собрать его здесь второй раз значило бы проверить
/// не то, что запустится.
pub struct Said {
    pub ok: bool,
    /// Вывод ядра построчно, пустые строки убраны.
    pub lines: Vec<String>,
}

impl Said {
    /// На что именно ругнулось ядро. Первая строка с «error» — она и есть причина;
    /// если такой нет, берём первую непустую. Обрезаем: это заголовок в одну строку,
    /// а ядро умеет отвечать абзацем.
    pub fn complaint(&self) -> String {
        match self
            .lines
            .iter()
            .find(|line| line.to_lowercase().contains("error"))
            .or_else(|| self.lines.first())
        {
            Some(line) => trim_to(line, 120),
            None => "ядро отказало без объяснения".to_string(),
        }
    }

    /// Та же жалоба словами ядра — без времени и уровня строки лога. Для окна, где
    /// `time="…" level=error msg="…"` читать некому.
    pub fn reason(&self) -> String {
        let complaint = self.complaint();
        complaint
            .split_once("msg=\"")
            .map(|(_, rest)| rest.trim_end_matches(['"', '…']).to_string())
            .unwrap_or(complaint)
    }
}

pub struct DryRun;

impl DryRun {
    /// Показать ядру готовый конфиг: примет ли (`mihomo -t`).
    ///
    /// Проверяем **отдельный файл**, а не `run/config.yaml`: тот принадлежит запущенному ядру,
    /// и переписывать его ради проверки значит трогать работающий VPN.
    pub fn accepts(yaml: &str) -> Result<Said> {
        let core = Mihomo::binary();
        std::fs::create_dir_all(Mihomo::workdir())?;
        let path = Mihomo::workdir().join(PROBE);
        crate::atomic::AtomicFile::write(
            &path,
            crate::render::mihomo::MihomoRenderer::unprivileged(yaml)?,
        )?;

        let mut command = Command::new(&core);
        command
            .arg("-t")
            .arg("-d")
            .arg(Mihomo::workdir())
            .arg("-f")
            .arg(&path);
        crate::system::console::Console::hide(&mut command, false);

        let output = command
            .output()
            .map_err(|e| AppError::io(format!("Не удалось запустить ядро для проверки: {e}")));
        // Файл убираем в любом случае: он временный, и оставлять его в рабочем каталоге
        // ядра нельзя — там лежит то, что ядро читает.
        let _ = std::fs::remove_file(&path);
        let output = output?;

        // Ядро на Go — его вывод в UTF-8, в отличие от системных утилит Windows (GOTCHAS).
        let lines = [&output.stdout, &output.stderr]
            .into_iter()
            .flat_map(|stream| {
                String::from_utf8_lossy(stream)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok(Said {
            ok: output.status.success(),
            lines,
        })
    }
}

/// Жалоба — заголовок в одну строку, и длинная реплика ядра его ломает.
fn trim_to(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit - 1).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(lines: &[&str]) -> Said {
        Said {
            ok: false,
            lines: lines.iter().map(|line| line.to_string()).collect(),
        }
    }

    #[test]
    fn the_complaint_is_the_line_with_the_error() {
        let out = said(&[
            "time=\"...\" level=info msg=\"start\"",
            "error: unsupported rule type FOO",
        ]);
        assert_eq!(out.complaint(), "error: unsupported rule type FOO");
    }

    #[test]
    fn without_an_error_line_the_first_one_is_taken() {
        assert_eq!(
            said(&["parse config error"]).complaint(),
            "parse config error"
        );
    }

    #[test]
    fn a_long_complaint_is_cut_to_one_line() {
        let long = "error: ".to_string() + &"a".repeat(300);
        let complaint = said(&[&long]).complaint();
        assert_eq!(complaint.chars().count(), 120);
        assert!(complaint.ends_with('…'));
    }

    #[test]
    fn the_reason_drops_the_log_prefix() {
        let out = said(&[
            "time=\"2026-10-01T19:12:50+08:00\" level=error msg=\"proxy 0: unsupport proxy type: vlesss\"",
        ]);
        assert_eq!(out.reason(), "proxy 0: unsupport proxy type: vlesss");
        assert_eq!(said(&["error: plain"]).reason(), "error: plain");
    }

    #[test]
    fn silence_is_reported_as_silence() {
        assert_eq!(said(&[]).complaint(), "ядро отказало без объяснения");
    }
}
