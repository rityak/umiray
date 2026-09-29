//! Примет ли ядро то, что мы собрали, — не запуская VPN.
//!
//! У mihomo для этого есть свой ключ (`-t`): он читает конфиг, ругается и выходит.
//! Это самая дешёвая проверка во всём разделе — секунда без единого пакета в сеть,
//! и она отвечает на добрую половину вопросов «почему не подключается».
//!
//! Проверяем **отдельный файл**, а не `run/config.yaml`: тот принадлежит запущенному
//! ядру, и переписывать его ради проверки значит трогать работающий VPN.

use std::process::Command;
use std::time::Instant;

use crate::core::mihomo::Mihomo;
use crate::diag::report::{Report, Tone, Verdict};
use crate::error::{AppError, Result};
use crate::nodes::sources::SourceStore;

/// Имя временного файла. Лежит рядом с рабочим конфигом: у ядра там уже есть права
/// и туда же смотрит `SAFE_PATHS`.
const PROBE: &str = "config.test.yaml";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Что сказало ядро о готовом конфиге. Пусто — принял.
///
/// Типизированный слой под отчётом (D-097): им пользуется и утилита ниже, и проверка
/// перед подключением (D-106) — у той конфиг уже собран, и собирать его второй раз
/// значило бы проверить не то, что запустится.
pub struct Said {
    pub ok: bool,
    /// Вывод ядра построчно, пустые строки убраны.
    pub lines: Vec<String>,
    pub ms: u64,
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
}

pub struct DryRun;

impl DryRun {
    /// Показать ядру готовый конфиг: примет ли (`mihomo -t`).
    ///
    /// Проверяем **отдельный файл**, а не `run/config.yaml`: тот принадлежит запущенному ядру,
    /// и переписывать его ради проверки значит трогать работающий VPN.
    pub fn accepts(yaml: &str) -> Result<Said> {
        let started = Instant::now();
        let core = Mihomo::binary();
        std::fs::create_dir_all(Mihomo::workdir())?;
        let path = Mihomo::workdir().join(PROBE);
        crate::atomic::AtomicFile::write(&path, yaml)?;

        let mut command = Command::new(&core);
        command
            .arg("-t")
            .arg("-d")
            .arg(Mihomo::workdir())
            .arg("-f")
            .arg(&path)
            // Тот же уговор, что у супервизора: без него ядро не читает файлы провайдеров
            // из соседнего каталога и валит проверку не по делу (GOTCHAS).
            .env("SAFE_PATHS", SourceStore::dir());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }

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
            ms: started.elapsed().as_millis() as u64,
        })
    }

    /// `config-test`: собрать конфиг и показать его ядру.
    ///
    /// `rules` — маршрутизация применённого набора; её знает только `app`, поэтому она
    /// приходит снаружи (тот же уговор, что у `crate::render::effective::ConfigRenderer::effective`, D-071).
    pub fn test(rules: Option<&str>) -> Result<Report> {
        let mut report = Report::new("config-test");
        if !Mihomo::binary().exists() {
            report.say(Tone::Dim, "ядра нет — проверять нечем");
            return Ok(report.finish(Verdict::Idle, "ядро не установлено", 0));
        }

        let assembled = crate::render::effective::ConfigRenderer::effective(rules, None)?;
        report.say(
            Tone::Info,
            format!(
                "config-test file={PROBE} lines={}",
                assembled.yaml.lines().count()
            ),
        );
        report.say(
            Tone::Dim,
            format!("mihomo -t -d {} -f {PROBE}", Mihomo::workdir().display()),
        );

        let said = DryRun::accepts(&assembled.yaml)?;
        if said.lines.is_empty() {
            report.say(Tone::Dim, "ядро промолчало");
        }
        for line in &said.lines {
            report.say(if said.ok { Tone::Dim } else { Tone::Bad }, line.clone());
        }
        let (verdict, headline) = if said.ok {
            (Verdict::Ok, "конфиг принят".to_string())
        } else {
            (Verdict::Bad, said.complaint())
        };
        Ok(report.finish(verdict, headline, said.ms))
    }
}

/// На что именно ругнулось ядро. Первая строка с «error» — она и есть причина;
/// если такой нет, берём первую непустую.
/// Заголовок — одна строка в списке проб, и длинная реплика ядра его ломает.
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
            ms: 0,
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
    fn silence_is_reported_as_silence() {
        assert_eq!(said(&[]).complaint(), "ядро отказало без объяснения");
    }
}
