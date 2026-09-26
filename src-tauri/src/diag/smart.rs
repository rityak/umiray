//! Замер плюс одно действие (D-105).
//!
//! Ради этого раздел диагностики и заводился (D-097): у каждой утилиты уже есть
//! типизированный замер, и «умная» функция — это он же плюс одна запись в документ
//! пользователя. Второй реализации замера здесь нет и быть не должно: и таблица
//! в отчёте, и запись в конфиг берут **один и тот же** ответ.
//!
//! Мерим заново, а не берём результат прошлого прогона: между «показали» и «нажали»
//! проходит сколько угодно времени, а записать в конфиг вчерашнего победителя — ровно
//! тот случай, когда клиент делает хуже молча.

use crate::config::advanced;
use crate::diag::report::{Report, Tone, Verdict};
use crate::diag::{dns, pmtu, Args};
use crate::error::{AppError, Result};

/// Выполнить действие утилиты. Отчёт — тот же тип, что у самой утилиты: окно показывает
/// его той же консолью, и рассказывать про «что я поменял» ему отдельным способом не нужно.
pub async fn apply(id: &str, args: Args) -> Result<Report> {
    match id {
        "dns-race" => resolvers(args).await,
        "pmtu" => mtu(args),
        other => Err(AppError::invalid(format!(
            "У утилиты «{other}» нет действия"
        ))),
    }
}

/// Прописать самые быстрые резолверы в `dns.nameserver`.
///
/// Пишем через форму «Ядра» (D-086), а не в файл руками: у поля один хозяин, и он же
/// проверяет остальное — выключенный `dns.enable` с пустым списком, например, конфиг
/// не соберёт.
async fn resolvers(args: Args) -> Result<Report> {
    let began = std::time::Instant::now();
    let mut report = Report::new("dns-race");
    let shots = dns::race(
        &args.domain(),
        args.timeout(),
        args.all,
        args.core.unwrap_or(true),
    )
    .await?;
    let picked: Vec<String> = dns::fastest(&shots, dns::BEST)
        .into_iter()
        .map(|index| shots[index].candidate.addr.clone())
        .collect();
    if picked.is_empty() {
        return Ok(report.finish(
            Verdict::Bad,
            "Ни один резолвер не ответил — писать нечего",
            began.elapsed().as_millis() as u64,
        ));
    }

    let mut options = advanced::read()?;
    let was = std::mem::replace(&mut options.nameserver, picked.clone());
    advanced::write(&options)?;

    report.say(Tone::Info, format!("было: {}", list(&was)));
    Ok(report.finish(
        Verdict::Ok,
        format!("В «Ядре» прописаны: {}", list(&picked)),
        began.elapsed().as_millis() as u64,
    ))
}

/// Прописать подобранный MTU в `tun.mtu`.
///
/// Число — не «сколько держит путь», а сколько остаётся туннелю: его собственный
/// заголовок съедает шестьдесят байт, и записать сюда MTU пути значило бы получить
/// ровно ту фрагментацию, от которой замер и спасает.
fn mtu(args: Args) -> Result<Report> {
    let began = std::time::Instant::now();
    let mut report = Report::new("pmtu");
    let host = args.host();
    let Some(path) = pmtu::path(&host)? else {
        return Ok(report.finish(
            Verdict::Idle,
            format!("{host} молчит по ICMP — подбирать нечего"),
            began.elapsed().as_millis() as u64,
        ));
    };
    let advice = path.saturating_sub(pmtu::TUNNEL);

    let mut options = advanced::read()?;
    let was = options.mtu;
    options.mtu = advice;
    advanced::write(&options)?;

    report.say(
        Tone::Info,
        format!(
            "было: {}",
            if was == 0 {
                "решало ядро".to_string()
            } else {
                was.to_string()
            }
        ),
    );
    Ok(report.finish(
        Verdict::Ok,
        format!("В «Ядре» прописан MTU {advice}: путь до {host} держит {path}"),
        began.elapsed().as_millis() as u64,
    ))
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        return "пусто".to_string();
    }
    items.join(", ")
}
