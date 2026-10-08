//! Сеть сменилась под ногами: наблюдатель и одно действие (D-112).
//!
//! Одна и та же беда с двух сторон. **После сна** все соединения мертвы, а клиент этого
//! не знает: окно зелёное, интернета нет, помогает только переподключение вручную.
//! **Смена интерфейса** — док-станция, Wi-Fi вместо кабеля, пройденный captive portal —
//! кончается ровно тем же: узлы, похороненные проверкой живости до перехода, останутся
//! похороненными до следующего фонового обхода, то есть до пяти минут.
//!
//! Наблюдатель поэтому один. `NotifyAddrChange` блокируется до изменения таблицы адресов,
//! а пробуждение эту таблицу меняет: интерфейсы при засыпании гаснут и при возврате
//! поднимаются заново. Это дешевле подписки на сообщения питания и отвечает на оба вопроса
//! сразу — цена названа в D-112: сон без единого изменения адресов мы пропустим.
//!
//! Модуль **только замечает**. Что делать с замеченным, решает `app::wake`: здесь нет
//! ни ядра, ни состояния приложения.

pub struct AddressWatcher;

impl AddressWatcher {
    /// Ждать следующего изменения. `false` — система отказала, и звать снова бессмысленно.
    ///
    /// Вызов блокирующий: своя нить, а не задача рантайма. Никакого опроса — нить спит
    /// в ядре Windows, пока таблица адресов не изменится.
    #[cfg(windows)]
    pub fn next_change() -> bool {
        use windows_sys::Win32::NetworkManagement::IpHelper::NotifyAddrChange;
        // Оба параметра пустые — синхронный режим: функция возвращается, когда изменение
        // уже случилось.
        unsafe { NotifyAddrChange(std::ptr::null_mut(), std::ptr::null()) == 0 }
    }

    /// Linux: сокет netlink, подписанный на изменения адресов, — тот же смысл, что
    /// у `NotifyAddrChange`: нить спит в ядре, пока таблица адресов не изменится.
    #[cfg(not(windows))]
    pub fn next_change() -> bool {
        // SAFETY: сокет создаётся, привязывается, читается и закрывается здесь же.
        unsafe {
            let socket = libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, libc::NETLINK_ROUTE);
            if socket < 0 {
                return false;
            }
            let mut address: libc::sockaddr_nl = std::mem::zeroed();
            address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
            address.nl_groups = (libc::RTMGRP_IPV4_IFADDR | libc::RTMGRP_IPV6_IFADDR) as u32;
            let bound = libc::bind(
                socket,
                (&raw const address).cast(),
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            ) == 0;
            let mut buffer = [0u8; 4096];
            let heard =
                bound && libc::recv(socket, buffer.as_mut_ptr().cast(), buffer.len(), 0) > 0;
            libc::close(socket);
            heard
        }
    }
}

#[cfg(test)]
mod tests {
    /// Живьём: сменить адрес на интерфейсе (`ip addr add`, от root) — наблюдатель проснётся.
    #[test]
    #[ignore]
    fn live_an_address_change_wakes_the_watcher() {
        let heard = std::thread::spawn(super::AddressWatcher::next_change);
        println!("теперь сменить адрес: sudo ip addr add 10.99.99.1/32 dev lo");
        assert!(heard.join().unwrap(), "наблюдатель не проснулся");
    }
}
