//! Что эта ОС умеет хорошо (D-174). Окно прячет всё, чего нет в списке, — одним хуком,
//! а не ветками по имени системы.
//!
//! Возможность, которую здесь нельзя сделать хорошо и которая не кор, не делается
//! наполовину, а выключается. Новая возможность — строка в `Feature` и строка в `supported`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    /// Второе ядро. Его сборка — только под Windows (WinDivert).
    Qd,
    /// «Всегда от администратора» — задача планировщика (D-087).
    AlwaysAdmin,
    /// Прописать адрес ядра системе (D-047).
    SystemProxy,
    /// Запрет выхода мимо туннеля (D-073).
    KillSwitch,
    /// Замер задержки ICMP-эхом и запасной ICMP (D-069).
    IcmpPing,
    /// Подбор MTU мастером (S-031).
    MtuProbe,
    /// Автозапуск при входе (D-049).
    Autostart,
    /// Обновление клиента из окна (D-149). На Linux — только где есть менеджер пакетов того
    /// формата, из которого клиент поставлен: Arch ставит его из deb и обновляет сам (D-173).
    SelfUpdate,
}

pub struct Features;

impl Features {
    pub fn supported() -> Vec<Feature> {
        let mut features = vec![Feature::Autostart];
        if cfg!(windows) {
            features.extend([
                Feature::Qd,
                Feature::AlwaysAdmin,
                Feature::IcmpPing,
                Feature::MtuProbe,
                Feature::KillSwitch,
                Feature::SelfUpdate,
            ]);
        }
        #[cfg(target_os = "linux")]
        {
            use crate::system::helper::tool;
            if tool("nft").is_some() {
                features.push(Feature::KillSwitch);
            }
            let manager = match tauri::utils::platform::bundle_type() {
                Some(tauri::utils::config::BundleType::Deb) => tool("dpkg"),
                Some(tauri::utils::config::BundleType::Rpm) => tool("rpm"),
                _ => None,
            };
            if manager.is_some() {
                features.push(Feature::SelfUpdate);
            }
        }
        if crate::system::sysproxy::ProxySetting::supported() {
            features.push(Feature::SystemProxy);
        }
        features
    }

    pub fn has(feature: Feature) -> bool {
        Features::supported().contains(&feature)
    }
}
