//! Процесс ядра — то, что у всех ядер одинаково (D-154): запуск в клетке, кольцо вывода,
//! остановка с отсрочкой и уборка осиротевших. Чем ядро отличается — как понять, что оно
//! готово, и как попросить его выйти, — остаётся в модуле самого ядра.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};

/// Сколько строк вывода держим. Единственный источник правды о том, почему ядро не встало.
const LOG_LINES: usize = 500;

/// Кольцо на `LOG_LINES` строк: пишут в него и ядро, и клиент (`note`). У каждого ядра
/// своё — строка клиента о qd в логе mihomo не видна тому, кто смотрит на qd.
#[derive(Clone, Default)]
pub struct LogRing(Arc<Mutex<VecDeque<String>>>);

impl LogRing {
    pub fn push(&self, line: String) {
        let mut lines = self.0.lock().unwrap();
        if lines.len() >= LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    pub fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().cloned().collect()
    }

    /// Хвост лога списком строк: интерфейс отрисует их сам, склеивать в текст ошибки нечего.
    pub fn tail(&self, count: usize) -> Vec<String> {
        let lines = self.0.lock().unwrap();
        lines
            .iter()
            .skip(lines.len().saturating_sub(count))
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    /// Своя строка в логе ядра. Формат — тот же `time=… level=… msg=…`, которым пишет
    /// mihomo: окно уже умеет его разбирать и фильтровать, время встаёт в ту же колонку,
    /// а `umiray:` говорит, кто автор строки.
    pub fn note(&self, level: &str, message: &str) {
        self.push(format!(
            "time=\"{}\" level={level} msg=\"umiray: {message}\"",
            crate::stamp::Stamp::local()
        ));
    }

    /// Вернуть в кольцо строки прошлой жизни ядра. Запуск кольцо чистит, поэтому надзор
    /// снимает хвост до подъёма и возвращает после — иначе причина падения пропадёт
    /// вместе с ним (D-057).
    pub fn recall(&self, lines: Vec<String>) {
        for line in lines {
            self.push(line);
        }
    }

    /// Читать поток в кольцо, пока он не закроется.
    pub fn pump(&self, stream: impl Read + Send + 'static) {
        let ring = self.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(|line| line.ok()) {
                ring.push(line);
            }
        });
    }

    /// То же, но первая строка с `prefix` уходит не в кольцо, а в ответ: так ядро,
    /// которое сообщает адрес своего API строкой вывода, отдаёт его запуску.
    pub fn pump_until(
        &self,
        stream: impl Read + Send + 'static,
        prefix: &'static str,
    ) -> tokio::sync::oneshot::Receiver<String> {
        let (tell, heard) = tokio::sync::oneshot::channel();
        let ring = self.clone();
        std::thread::spawn(move || {
            let mut tell = Some(tell);
            for line in BufReader::new(stream).lines().map_while(|line| line.ok()) {
                if line.starts_with(prefix) {
                    if let Some(tell) = tell.take() {
                        let _ = tell.send(line);
                        continue;
                    }
                }
                ring.push(line);
            }
        });
        heard
    }
}

pub struct CoreProcess;

impl CoreProcess {
    /// Запустить без окна консоли и сразу в клетку (D-058): с этой секунды ядро не переживёт
    /// падение клиента. Не принятый клеткой процесс гасится тут же — ядро, способное пережить
    /// клиента, хуже отказа (D-134). `own_group` — своя группа процессов (`Console::hide`).
    pub fn spawn(mut command: Command, own_group: bool, name: &str) -> Result<Child> {
        crate::system::console::Console::hide(&mut command, own_group);
        crate::system::job::Job::spawn(command).map_err(|e| AppError::CoreFailed {
            // Файл есть, а запускать нельзя: чаще всего каталог данных на разделе с `noexec`.
            message: if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("Не удалось запустить {name}: нет права на запуск файла — раздел с каталогом данных смонтирован с noexec?")
            } else {
                format!("Не удалось запустить {name}: {e}")
            },
            log: Vec::new(),
        })
    }

    /// Дождаться выхода не дольше `grace`, потом убить: остановка обязана состояться.
    /// Попросить выйти по-хорошему — дело ядра, способ у каждого свой (Ctrl+Break, stdin).
    pub fn finish(child: &mut Child, grace: Duration) {
        let began = Instant::now();
        while began.elapsed() < grace {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Ядро, пережившее прошлую жизнь клиента, — прибрать при запуске (D-059).
    ///
    /// По полному пути бинаря, а не по имени (D-154): одноимённый процесс из другого места —
    /// чужой. У qd есть самостоятельный клиент, и закрывать его при нашем запуске нельзя.
    /// stable и dev держат бинари раздельно и друг друга не трогают (D-150).
    pub fn sweep(binary: &std::path::Path) {
        crate::system::process::ProcessTable::kill_by_path(binary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_only_the_last_lines() {
        let ring = LogRing::default();
        for n in 0..LOG_LINES + 3 {
            ring.push(n.to_string());
        }
        let lines = ring.lines();
        assert_eq!(lines.len(), LOG_LINES);
        assert_eq!(lines[0], "3");
        assert_eq!(
            ring.tail(2),
            [(LOG_LINES + 1).to_string(), (LOG_LINES + 2).to_string()]
        );
    }

    /// Рукопожатие не должно попадать в лог, а всё после него — должно.
    #[test]
    fn the_first_line_with_the_prefix_goes_to_the_caller() {
        let ring = LogRing::default();
        let stream = std::io::Cursor::new("booting\n{\"hi\":1}\n{\"hi\":2}\nready\n");
        let heard = ring.pump_until(stream, "{\"hi\"");
        assert_eq!(heard.blocking_recv().unwrap(), "{\"hi\":1}");
        let deadline = Instant::now() + Duration::from_secs(2);
        while ring.lines().len() < 3 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(ring.lines(), ["booting", "{\"hi\":2}", "ready"]);
    }
}
