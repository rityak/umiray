//! Статус для окна и то, из чего он собирается.
//!
//! `Status` — не «настройки», а **как есть сейчас**: работает ли ядро, стоит ли наш адрес
//! в реестре, есть ли права. Собирается из трёх источников сразу, поэтому и живёт отдельно
//! от команд, которые его отдают.

use serde::Serialize;

use crate::app::engine::{Capture, EngineState};
use crate::app::mode;
use crate::app::mode::Choice;
use crate::app::proxy::SystemProxy;
use crate::app::state::AppState;
use crate::app::tray;
use crate::config::mode::Mode;
use crate::core::EngineId;
use crate::system::autostart::Autostart;
use crate::system::elevation::Elevation;
use crate::system::sysproxy::WinProxy;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Какое ядро держит трафик сейчас (D-154). Шапка и трей говорят о нём, а разделы —
    /// о выбранном в настройках: переключатель ядер — вид, а не питание.
    active: Option<EngineId>,
    /// Работает ли mihomo. Остальные поля про режим и порт — тоже его: у qd своё
    /// состояние, `qd_status`.
    running: bool,
    /// В каком режиме работает ядро **сейчас**. Пусто — не работает.
    mode: Option<Mode>,
    /// Что выбрано в шапке (D-060). От работающего режима отличается свободно: выбор
    /// правит конфиг, а не процесс, и доезжает он перезапуском.
    desired_mode: mode::Choice,
    /// Почему работающему ядру нужен подъём заново — готовой строкой для человека.
    /// Пусто — не нужен. Считается здесь, а не в окне: правило одно (D-102), и второй
    /// его копии на фронте быть не должно. Строка, а не флаг: кнопка обязана объяснять,
    /// что именно она собирается перезапустить.
    restart_reason: Option<String>,
    /// Адрес локального прокси показываем настоящий: пользователь мог сменить порт оверрайдом.
    port: Option<u16>,
    core_present: bool,
    /// Скачан ли qd: переключатель ядер в шапке виден только тогда (D-161).
    qd_present: bool,
    /// TUN без прав администратора не поднимется, интерфейс должен это показывать заранее.
    elevated: bool,
    /// Поднимается ли клиент с правами **всегда** — то есть заведена ли задача (D-087).
    /// Как и автозапуск, это факт системы, а не настройка: задачу вправе убрать
    /// и планировщик.
    always_admin: bool,
    /// Стоит ли **сейчас** наш адрес в настройках Windows (D-047). Не «хотел ли пользователь»,
    /// а как есть в реестре: если запись не удалась, окно обязано показать это, а не желаемое.
    system_proxy: bool,
    /// Чужой прокси в системе, если он там есть: другой VPN-клиент вполне может уже
    /// занимать это место. Включение нашего его заменит, и предупредить об этом обязаны.
    foreign_proxy: Option<String>,
    /// Стоит ли автозапуск. Читается из реестра, а не из настроек: запись пользователь
    /// вправе убрать мимо нас, и окно обязано показать, как есть.
    autostart: bool,
    /// Когда ядро поднялось, в секундах эпохи; пусто — не работает. Из этого окно
    /// считает время работы. Именно отметка, а не готовая длительность: секунды в
    /// статусе, который опрашивается раз в полторы секунды, стареют быстрее, чем едут.
    started: Option<u64>,
    /// Что сейчас не так — важнейшей строкой из реестра жалоб (D-115): сторож, часы,
    /// исчезнувший брандмауэр. Пусто — жаловаться не на что. Как и `restart_reason`,
    /// это состояние, а не событие: висит, пока есть повод, и пропадает само.
    trouble: Option<String>,
    /// Стоит ли **сейчас** запрет выхода мимо туннеля (D-073). Именно стоит, а не выбран:
    /// тумблер живёт в настройках, а здесь факт, и разойтись им можно — включённый тумблер
    /// в режиме local не запирает ничего.
    kill_switch: bool,
    /// Значок трея — считается вместе со статусом, из тех же фактов, и окну не уезжает.
    #[serde(skip)]
    look: tray::Look,
}

impl Status {
    /// Статус как есть сейчас — из ядер, реестра Windows, настроек и реестра жалоб.
    pub fn gather(state: &AppState) -> Status {
        let core = state.mihomo.status();
        let running = state.running();
        // Один поход в реестр на весь статус: он опрашивается раз в 1.5 с, и читать
        // одно и то же дважды незачем.
        let registry = WinProxy::read().unwrap_or_default();
        let ours = SystemProxy::address(
            running
                .as_ref()
                .and_then(|(_, engine)| engine.capture.as_ref()),
        );
        let system_proxy = registry.enabled
            && ours
                .as_deref()
                .is_some_and(|address| registry.server == address);
        let desired_mode = Choice::get(state);
        Status {
            active: running.as_ref().map(|(id, _)| *id),
            look: look(running.as_ref().map(|(_, engine)| engine), system_proxy),
            running: core.running,
            mode: core.mode,
            desired_mode,
            restart_reason: restart_reason(state),
            trouble: state.notices.top(core.started),
            port: core.port,
            started: running.as_ref().and_then(|(_, engine)| engine.started),
            core_present: crate::core::mihomo::Mihomo::binary().exists(),
            qd_present: state.qd.present(),
            elevated: Elevation::is_elevated(),
            always_admin: Autostart::always_admin(),
            system_proxy,
            foreign_proxy: (registry.enabled && !system_proxy && !registry.server.is_empty())
                .then_some(registry.server),
            autostart: Autostart::enabled(),
            // Факт, а не намерение: тумблер живёт в настройках, а здесь — стоит ли запрет
            // на самом деле. Разойтись им можно (тумблер включён, но режим не TUN), и окно
            // обязано показывать работающее — то же правило, что у прокси (D-047, D-073).
            kill_switch: state.settings.get().kill_switch_backup.is_some(),
        }
    }

    pub fn look(&self) -> tray::Look {
        self.look
    }

    /// Причина, по которой окну стоит предложить перезапуск. Только для живых проверок:
    /// рабочему коду  уезжает целиком в вебвью и внутрь никто не смотрит.
    #[cfg(test)]
    pub fn restart_reason(&self) -> Option<&str> {
        self.restart_reason.as_deref()
    }

    /// Важнейшая жалоба. Тоже только для живых проверок.
    #[cfg(test)]
    pub fn trouble(&self) -> Option<&str> {
        self.trouble.as_deref()
    }
}

/// Как должен выглядеть значок. Отдельная функция, потому что состояние меняется в четырёх
/// местах, и повторять этот разбор в каждом — способ их рассинхронизировать.
///
/// Решает **работающее** ядро и то, как оно перехватывает трафик, а не выбранное в шапке
/// (D-066, D-154): иначе переключение шапки на qd гасило бы значок при живом mihomo.
fn look(running: Option<&EngineState>, system_proxy: bool) -> tray::Look {
    let Some(engine) = running.filter(|engine| engine.on) else {
        return tray::Look::Off;
    };
    match engine.capture {
        Some(Capture::Tun { .. }) => tray::Look::Tun,
        Some(Capture::Divert) => tray::Look::Divert,
        _ if system_proxy => tray::Look::System,
        _ => tray::Look::Local,
    }
}

/// Разошёлся ли работающий конфиг с тем, который собрался бы сейчас, — и если да,
/// доедет ли разница перезагрузкой (D-102).
///
/// Собираем на каждый опрос, а не помним намерение: файл правит и форма, и редактор,
/// и тумблер, и помнить за ними было бы вторым источником истины (D-052). Сборка стоит
/// около десяти миллисекунд, опрос идёт раз в полторы секунды.
///
/// Порт служебного входа берём **у работающего ядра**: свежий сделал бы конфиги разными
/// на ровном месте.
fn restart_reason(state: &AppState) -> Option<String> {
    let launched = state.mihomo.launched()?;
    let assembled = crate::render::effective::ConfigRenderer::effective(
        &state.routing.document(state).ok()?,
        state.mihomo.probe_port(),
    )
    .ok()?;
    match crate::core::mihomo::apply::Apply::needed(&launched, &assembled.yaml) {
        Ok(Some(crate::core::mihomo::apply::Apply::Restart(why))) => Some(why.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(capture: Option<Capture>) -> EngineState {
        EngineState {
            on: true,
            wanted: true,
            started: None,
            capture,
            recovering: false,
        }
    }

    /// Значок показывает **работающее**, а не выбранное (D-066, D-154). Режим правится
    /// в конфиге и до перезапуска не в силе, ядро выбирается в шапке и до нажатия питания
    /// не работает, а трей обязан говорить правду о том, как идёт трафик прямо сейчас.
    #[test]
    fn the_tray_follows_the_running_engine_not_the_chosen_one() {
        assert_eq!(
            look(None, false),
            tray::Look::Off,
            "ничего не работает — значок серый, что бы ни стояло в шапке"
        );
        let crashed = EngineState {
            on: false,
            ..on(None)
        };
        assert_eq!(
            look(Some(&crashed), false),
            tray::Look::Off,
            "упавшее ядро трафик не держит, хоть и должно"
        );
        let local = on(Some(Capture::LocalProxy { port: 2080 }));
        assert_eq!(look(Some(&local), false), tray::Look::Local);
        assert_eq!(
            look(Some(&local), true),
            tray::Look::System,
            "адрес правда стоит в реестре — это отдельное состояние (D-047)"
        );
        let tun = on(Some(Capture::Tun {
            device: "Meta".into(),
        }));
        assert_eq!(look(Some(&tun), true), tray::Look::Tun);
        assert_eq!(
            look(Some(&on(Some(Capture::Divert))), false),
            tray::Look::Divert
        );
    }
}
