//! Когда система просит клиента уйти, не спрашивая окно.
//!
//! На Linux выключение и `kill` приходят сигналом, и по умолчанию процесс от него умирает
//! на месте: ядро уносит клетка, а системный прокси остаётся смотреть на мёртвый порт —
//! без интернета в браузере до следующего запуска umiray. Пойманный сигнал превращается
//! в штатный выход, и тот прибирает за собой, как выход из трея.

pub struct Session;

impl Session {
    /// Завершается, когда процесс попросили закончить работу. На Windows не завершается:
    /// там этот повод сигналом не приходит.
    pub async fn ended() {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let (Ok(mut term), Ok(mut int), Ok(mut hup)) = (
                signal(SignalKind::terminate()),
                signal(SignalKind::interrupt()),
                signal(SignalKind::hangup()),
            ) else {
                return std::future::pending().await;
            };
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
                _ = hup.recv() => {}
            }
        }
        #[cfg(windows)]
        std::future::pending::<()>().await
    }
}
