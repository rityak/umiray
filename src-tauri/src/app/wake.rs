//! Что делать, когда сеть сменилась под ногами (D-112).
//!
//! Наблюдатель живёт в `system::wake` и про клиента ничего не знает; здесь — обратное:
//! действие, которое не знает, как именно нас разбудили.
//!
//! Действие одно и то же на оба повода — после сна и после смены интерфейса: свериться
//! по часам (D-115), попросить ядро перепроверить узлы **сейчас** и тут же спросить
//! сторожа, идёт ли трафик (D-107).
//! Ни переподключения, ни перезапуска: соединения после сна мертвы, а сам процесс жив,
//! и рвать его из-за смены сети значило бы лечить симптом чужой болезни.

use tauri::{AppHandle, Manager};

use crate::app::notice::{Notice, CLOCK};
use crate::app::state::AppState;

/// Сколько ждать, прежде чем спрашивать. Пробуждение и смена интерфейса дают **пачку**
/// изменений подряд, и первые из них приходят раньше, чем адрес вообще получен: проверка
/// на этой секунде похоронила бы все узлы разом. Три секунды — это ожидание адреса,
/// а не пауза «на всякий случай».
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

pub struct Wake;

impl Wake {
    /// Завести наблюдателя. Своя нить, а не задача рантайма: `NotifyAddrChange` блокируется,
    /// и в асинхронной задаче она заняла бы рабочий поток целиком.
    pub fn watch(app: AppHandle) {
        std::thread::spawn(move || {
            while crate::system::wake::AddressWatcher::next_change() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(SETTLE).await;
                    Wake::look(&app.state::<AppState>()).await;
                });
                // Пачку изменений считаем одним событием: без этого одно пробуждение дало бы
                // десяток обходов всех узлов подряд.
                std::thread::sleep(SETTLE);
            }
        });
    }

    /// Перепроверить часы, узлы и сторожа. Часы — первыми и независимо от ядра: они уезжают
    /// именно во сне, а не в работе.
    pub async fn look(state: &AppState) {
        clock(state).await;
        if !state.mihomo.status().running {
            return;
        }
        crate::app::volt::network_changed(state).await;
        match state.mihomo.recheck().await {
            Ok(0) => return,
            // В кольцо, а не только на stderr: повод редкий — сон или смена сети, — и это
            // единственный ответ на «почему VPN моргнул», который увидит пользователь.
            Ok(asked) => state.mihomo.log().note(
                "info",
                &format!("сеть сменилась, узлы перепроверены (провайдеров: {asked})"),
            ),
            Err(why) => {
                state.mihomo.log().note(
                    "warning",
                    &format!("сеть сменилась, но перепроверить не вышло: {why}"),
                );
                return;
            }
        }
        crate::app::guard::Guard::look(state).await;
    }
}

/// Часы после сна (D-115). Повод именно этот: машина, проспавшая ночь, просыпается
/// с временем, которое ещё не синхронизировано, а VMess и VLESS с AEAD проверяют метку
/// времени — и рукопожатие начинает вставать через раз (`diag::clock`).
///
/// Эталон не ответил — жалобу не трогаем: сразу после пробуждения сети может не быть
/// вовсе, и «не сверили» это не «часы точны». Следующее пробуждение спросит заново.
async fn clock(state: &AppState) {
    let Some(skew) = crate::diag::clock::ClockProbe::skew().await else {
        return;
    };
    state.notices.set(
        CLOCK,
        crate::diag::clock::ClockProbe::complaint(skew).map(Notice::about),
    );
}
