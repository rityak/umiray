//! Группы как вещи со своими именами и составом (D-172): переименование своей группы
//! со всем, что на неё смотрит, и группы, которые клиент собирает сам.
//!
//! Куда идёт трафик — забота `Routing`; здесь — что такое группа и как она называется.
//! До живого ядра каждую правку доводит `Connection::change`.

use crate::app::state::AppState;
use crate::config::auto::{AutoGroups, Exclude, Grouping};
use crate::config::direction::AUTO;
use crate::config::files::{Documents, GROUPS};
use crate::config::groups::GroupsCodec;
use crate::config::presets::{PresetStore, RULES};
use crate::config::rename;
use crate::config::udp::UdpGroup;
use crate::error::Result;
use crate::render::effective::ConfigRenderer;
use crate::render::mihomo_groups::Built;
use crate::render::plan::Route;

pub struct Groups;

impl Groups {
    /// Переименовать свою группу — вместе со всем, что на неё смотрит: другие группы,
    /// правила всех наборов, выбор в «Соединении» и её значок. Иначе её правила молча ушли бы
    /// в `umiray` (D-156). Имя проверяется до записи: занятое, служебное или с запятой
    /// сломало бы конфиг.
    pub fn rename(&self, state: &AppState, from: &str, to: &str) -> Result<()> {
        let mut taken: Vec<String> = user_groups()
            .into_iter()
            .filter(|name| name != from)
            .collect();
        taken.extend(state.catalog.names());
        let to = rename::check(to, &taken)?;
        if to == from {
            return Ok(());
        }
        let groups = rename::in_groups(&Documents::read(GROUPS)?, from, &to)?;
        // Сначала наборы, потом группы: упади запись посередине, правила целились бы
        // в имя, которого ещё нет, — сборка уведёт их в `umiray` (D-156), а не откажет.
        for preset in PresetStore::list() {
            let text = PresetStore::content(&preset.id)?;
            if let Some(changed) = rename::in_route(&text, from, &to)? {
                PresetStore::write(&preset.id, RULES, &changed)?;
            }
        }
        Documents::write(GROUPS, &groups)?;
        state.settings.update(|settings| {
            if settings.selected.as_deref() == Some(from) {
                settings.selected = Some(to.clone());
            }
            if let Some(icon) = settings.group_icons.remove(from) {
                settings.group_icons.insert(to.clone(), icon);
            }
        })
    }

    /// Кого нет в `AUTO`.
    pub fn set_exclude(&self, exclude: &Exclude) -> Result<()> {
        AutoGroups::set_exclude(exclude)
    }

    /// Какие группы клиент собирает сам: по стране и по протоколу.
    pub fn grouping(&self) -> Grouping {
        AutoGroups::grouping()
    }

    /// Снятая UDP-группа снимает и правило: вести UDP ему больше некуда (D-113).
    pub fn set_grouping(&self, grouping: Grouping) -> Result<()> {
        AutoGroups::set_grouping(grouping)?;
        if !grouping.udp && UdpGroup::on() {
            UdpGroup::write(false)?;
        }
        Ok(())
    }

    /// Правило «весь UDP — в `umiray-udp`» (D-113). Включённое, оно включает и группу: она
    /// остаётся и тогда, когда правило снимут.
    pub fn set_udp_rule(&self, on: bool) -> Result<()> {
        let grouping = AutoGroups::grouping();
        if on && !grouping.udp {
            AutoGroups::set_grouping(Grouping {
                udp: true,
                ..grouping
            })?;
        }
        UdpGroup::write(on)
    }
}

impl Groups {
    /// Группы клиента такими, какими их соберёт сборка этого маршрута. Своя группа с именем
    /// автогруппы главнее, поэтому имена своих групп идут в сборку занятыми.
    pub fn built(&self, route: &Route) -> Result<Vec<Built>> {
        ConfigRenderer::built(route, &user_groups())
    }

    /// Чем ещё бывает выход, кроме узла: свои группы и группы клиента. `AUTO` здесь лишний —
    /// он своё направление, а не `manual`.
    pub fn exits(&self, built: &[Built]) -> Vec<String> {
        user_groups()
            .into_iter()
            .chain(built.iter().map(|group| group.name.clone()))
            .filter(|name| name != AUTO)
            .collect()
    }
}

/// Имена своих групп из общего документа (D-075). Документ, который не разбирается, —
/// ни одной: его ошибку честно назовёт сборка.
fn user_groups() -> Vec<String> {
    Documents::read(GROUPS)
        .ok()
        .and_then(|text| GroupsCodec::parse(&text).ok())
        .map(|groups| groups.into_iter().map(|group| group.name).collect())
        .unwrap_or_default()
}
