//! Ядра: у каждого свой модуль — процесс, API и доставка бинаря (D-154).

use serde::{Deserialize, Serialize};

pub mod mihomo;
pub mod process;
pub mod qd;

/// Какое ядро. Имя, а не само ядро: его хранят настройки и передаёт окно, а ядро как
/// объект отдаёт `AppState::engine` (D-154). Неизвестное имя отклоняется на границе.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineId {
    #[default]
    Mihomo,
    Qd,
}

impl EngineId {
    pub const ALL: [EngineId; 2] = [EngineId::Mihomo, EngineId::Qd];
}

pub struct Ports;

impl Ports {
    /// Свободный порт на петле: биндим на нулевой, читаем номер, отпускаем.
    ///
    /// Гонка возможна теоретически, на практике между этим и стартом ядра проходят
    /// миллисекунды (D-007). Живёт здесь, потому что портов у ядра два — управляющий
    /// и служебный вход под замер (D-072), — и оба ищутся одинаково.
    pub fn free_port() -> crate::error::Result<u16> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| crate::error::AppError::io(format!("Не удалось выбрать порт: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| crate::error::AppError::io(format!("Не удалось выбрать порт: {e}")))?
            .port();
        drop(listener);
        Ok(port)
    }

    /// Где ядро будет слушать локальный прокси — тем же правилом, что у самого mihomo
    /// (`genAddr`): без `allow-lan` только петля, с ним — `bind-address` или все адреса.
    /// Адрес, который не читается, считаем «все»: тогда проверка строже, а не мягче.
    pub fn proxy_address(yaml: &str, port: u16) -> crate::error::Result<std::net::SocketAddr> {
        use serde_yaml::Value;
        let map = crate::yaml::Yaml::top_mapping(yaml)?;
        let lan = map
            .get(Value::from("allow-lan"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !lan {
            return Ok(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        }
        let host = map
            .get(Value::from("bind-address"))
            .and_then(Value::as_str)
            .and_then(|host| host.parse().ok())
            .unwrap_or(std::net::IpAddr::from([0, 0, 0, 0]));
        Ok(std::net::SocketAddr::new(host, port))
    }

    /// Свободен ли адрес: занимаем его сами и сразу отпускаем (D-133). Проверка ровно та,
    /// на которой споткнётся ядро, — поэтому и ответ про него, а не про догадку.
    pub fn vacant(address: std::net::SocketAddr) -> std::io::Result<()> {
        std::net::TcpListener::bind(address).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proxy_listens_where_mihomo_would() {
        let loopback = std::net::SocketAddr::from(([127, 0, 0, 1], 2080));
        assert_eq!(
            Ports::proxy_address("mixed-port: 2080\n", 2080).unwrap(),
            loopback
        );
        assert_eq!(
            Ports::proxy_address("allow-lan: false\n", 2080).unwrap(),
            loopback
        );
        assert_eq!(
            Ports::proxy_address("allow-lan: true\n", 2080).unwrap(),
            std::net::SocketAddr::from(([0, 0, 0, 0], 2080))
        );
        assert_eq!(
            Ports::proxy_address("allow-lan: true\nbind-address: 192.168.1.5\n", 2080).unwrap(),
            std::net::SocketAddr::from(([192, 168, 1, 5], 2080))
        );
        assert_eq!(
            Ports::proxy_address("allow-lan: true\nbind-address: '*'\n", 2080).unwrap(),
            std::net::SocketAddr::from(([0, 0, 0, 0], 2080))
        );
    }

    /// Ради этого случая проверка и заведена: ядро с занятым портом поднимается само,
    /// и сказать «не выйдет» можно только до него.
    #[test]
    fn a_taken_port_is_not_vacant() {
        let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = holder.local_addr().unwrap();
        assert!(
            Ports::vacant(address).is_err(),
            "порт занят, а проверка его пропустила"
        );
        drop(holder);
        assert!(
            Ports::vacant(address).is_ok(),
            "отпущенный порт считается занятым"
        );
    }
}

pub mod release;
pub mod volt;
pub mod volt_tune;
