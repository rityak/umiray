//! Статус для окна и то, из чего он собирается.
//!
//! `Status` — не «настройки», а **как есть сейчас**: работает ли ядро, стоит ли наш адрес
//! в реестре, есть ли права. Собирается из трёх источников сразу, поэтому и живёт отдельно
//! от команд, которые его отдают.

use serde::Serialize;

use crate::app::mode;
use crate::app::state::AppState;
use crate::app::tray;
use crate::config::mode::Mode;
use crate::core;
use crate::error::Result;
use crate::paths;
use crate::system::{autostart, elevation, sysproxy};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
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
}

impl Status {
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

/// Адрес, который прописываем в систему. Только local-режим: в TUN перехватывается весь
/// трафик машины, и прописывать там нечего.
pub fn proxy_address(core: &core::Status) -> Option<String> {
    if core.mode != Some(Mode::Local) {
        return None;
    }
    core.port.map(|port| format!("127.0.0.1:{port}"))
}

/// Как должен выглядеть значок при таком статусе. Отдельная функция, потому что состояние
/// меняется в четырёх местах, и повторять этот разбор в каждом — способ их рассинхронизировать.
pub fn look(status: &Status) -> tray::Look {
    if !status.running {
        return tray::Look::Off;
    }
    match status.mode {
        Some(Mode::Tun) => tray::Look::Tun,
        _ if status.system_proxy => tray::Look::System,
        _ => tray::Look::Local,
    }
}

pub fn status(state: &AppState) -> Status {
    let core = state.supervisor.status();
    // Один поход в реестр на весь статус: он опрашивается раз в 1.5 с, и читать
    // одно и то же дважды незачем.
    let registry = sysproxy::read().unwrap_or_default();
    let ours = proxy_address(&core);
    let system_proxy = registry.enabled
        && ours
            .as_deref()
            .is_some_and(|address| registry.server == address);
    let desired_mode = mode::get(state);
    Status {
        running: core.running,
        mode: core.mode,
        desired_mode,
        restart_reason: restart_reason(state),
        trouble: state.notices.top(core.started),
        port: core.port,
        started: core.started,
        core_present: paths::core().exists(),
        elevated: elevation::is_elevated(),
        always_admin: autostart::always_admin(),
        system_proxy,
        foreign_proxy: (registry.enabled && !system_proxy && !registry.server.is_empty())
            .then_some(registry.server),
        autostart: autostart::enabled(),
        // Факт, а не намерение: тумблер живёт в настройках, а здесь — стоит ли запрет
        // на самом деле. Разойтись им можно (тумблер включён, но режим не TUN), и окно
        // обязано показывать работающее — то же правило, что у прокси (D-047, D-073).
        kill_switch: state.settings().kill_switch_backup.is_some(),
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
    let launched = state.supervisor.launched()?;
    let assembled = crate::render::effective::effective(
        state.routing().ok().flatten().as_deref(),
        state.supervisor.probe_port(),
    )
    .ok()?;
    match crate::core::apply::needed(&launched, &assembled.yaml) {
        Ok(Some(crate::core::apply::Apply::Restart(why))) => Some(why.to_string()),
        _ => None,
    }
}

/// Прописать наш адрес в систему, запомнив прежнюю настройку.
///
/// Неудача не роняет запуск: ядро уже работает, адрес виден в окне, и прописать его руками
/// пользователь может сам. Правду про реестр всё равно скажет `Status::system_proxy`.
pub fn engage_system_proxy(state: &AppState) -> Result<()> {
    let core = state.supervisor.status();
    let Some(address) = proxy_address(&core) else {
        return Ok(());
    };
    if let Some(mut backup) = state.settings().proxy_backup {
        // Кто-то сменил прокси после нас — не отбираем его обратно при reconnect.
        if !backup.ours.is_empty() && !sysproxy::is_ours(&backup.ours) {
            return Ok(());
        }
        if backup.ours == address && sysproxy::is_ours(&address) {
            return Ok(());
        }
        let old = backup.ours.clone();
        sysproxy::apply(&address)?;
        backup.ours = address;
        if let Err(why) = state.remember_proxy(Some(backup)) {
            if !old.is_empty() {
                let _ = sysproxy::apply(&old);
            }
            return Err(why);
        }
        return Ok(());
    }
    let mut previous = sysproxy::read()?;
    previous.ours = address.clone();
    // Снимок должен пережить падение между записью реестра и следующей строкой.
    state.remember_proxy(Some(previous))?;
    if let Err(why) = sysproxy::apply(&address) {
        let _ = state.remember_proxy(None);
        return Err(why);
    }
    Ok(())
}

/// Снять наш прокси и вернуть то, что стояло раньше. Вызывается и при остановке ядра,
/// и при выходе, и при старте — последнее и есть лекарство от залипания после падения.
pub fn release_system_proxy(state: &AppState) -> Result<()> {
    // Снимка нет — значит включали не мы, и в реестре чужая настройка: другой VPN,
    // корпоративный прокси. Выключить её «на всякий случай» значит сломать человеку сеть
    // тем, что он всего лишь закрыл наше окно.
    let Some(backup) = state.settings().proxy_backup else {
        return Ok(());
    };
    sysproxy::restore(&backup)?;
    state.remember_proxy(None)
}

/// Запереть выход мимо туннеля (D-073). Только в TUN: в local и system ядро — обычный
/// прокси, мимо которого приложение вправе ходить, и запирать машину за него мы не подряжались.
///
/// Молчаливый отказ намеренный: не встало правило — VPN всё равно работает, просто без
/// подстраховки, и ронять из-за этого подключение хуже. Что защиты нет, видно в статусе.
pub fn engage_kill_switch(state: &AppState) -> Result<()> {
    if !state.settings().kill_switch || state.supervisor.status().mode != Some(Mode::Tun) {
        return Ok(());
    }
    // При reconnect правила уже стоят, а снимок содержит состояние до первой установки.
    // Повторный `engage` снял бы снимок с нашего же Block и породил дубликаты правил.
    if state.settings().kill_switch_backup.is_some() {
        return Ok(());
    }
    let device = crate::config::files::read(crate::config::files::ADVANCED)
        .and_then(|text| crate::yaml::top_mapping(&text))
        .map(|map| crate::config::mode::tun_device(&map))
        .unwrap_or_else(|_| crate::config::mode::DEFAULT_DEVICE.to_string());
    let previous = crate::system::killswitch::profiles()?;
    let backup = crate::system::killswitch::Backup { profiles: previous };
    // Сначала сохраняем исходное состояние, затем меняем машину: падение между ними
    // оставит данные, по которым следующий запуск всё вернёт.
    state.remember_kill_switch(Some(backup.clone()))?;
    if let Err(why) = crate::system::killswitch::apply(&paths::core(), &device) {
        let _ = crate::system::killswitch::release(&backup);
        let _ = state.remember_kill_switch(None);
        return Err(why);
    }
    Ok(())
}

/// Снять запрет и вернуть умолчание брандмауэра. Как и у прокси, зовётся при остановке,
/// при выходе и **при старте** — последнее возвращает машине сеть после падения клиента.
pub fn release_kill_switch(state: &AppState) -> Result<()> {
    // Снимка нет — запрет не наш (или его нет вовсе), и трогать чужую настройку
    // брандмауэра мы не вправе.
    let Some(backup) = state.settings().kill_switch_backup else {
        return Ok(());
    };
    crate::system::killswitch::release(&backup)?;
    state.remember_kill_switch(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(
        running: bool,
        mode: Option<Mode>,
        system_proxy: bool,
        desired: mode::Choice,
    ) -> Status {
        Status {
            running,
            mode,
            desired_mode: desired,
            restart_reason: None,
            trouble: None,
            port: None,
            started: None,
            core_present: true,
            elevated: false,
            always_admin: false,
            system_proxy,
            foreign_proxy: None,
            autostart: false,
            kill_switch: false,
        }
    }

    /// Значок показывает **работающее**, а не выбранное (D-066). После D-060 это разные
    /// вещи: режим правится в конфиге и до перезапуска не в силе, а трей обязан говорить
    /// правду о том, как идёт трафик прямо сейчас.
    #[test]
    fn the_tray_follows_the_running_mode_not_the_chosen_one() {
        assert_eq!(
            look(&shown(false, None, false, mode::Choice::Tun)),
            tray::Look::Off,
            "ядро не работает — значок серый, что бы ни стояло в шапке"
        );
        assert_eq!(
            look(&shown(true, Some(Mode::Local), false, mode::Choice::Tun)),
            tray::Look::Local,
            "выбран TUN, работает local — значок про local"
        );
        assert_eq!(
            look(&shown(true, Some(Mode::Local), true, mode::Choice::Local)),
            tray::Look::System,
            "адрес правда стоит в реестре — это отдельное состояние (D-047)"
        );
        assert_eq!(
            look(&shown(true, Some(Mode::Tun), false, mode::Choice::Local)),
            tray::Look::Tun
        );
    }
}
