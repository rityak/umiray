//! Часы машины (D-097): не уехало ли системное время.
//!
//! Зачем отдельная проба. VMess и VLESS с AEAD аутентифицируются по метке времени,
//! и окно у неё — около двух минут. Уехавшие часы дают «то встаёт, то нет» и невнятную
//! ошибку рукопожатия в логе ядра: ни одна другая утилита раздела этого не покажет,
//! а пользователь будет искать причину в узле.
//!
//! Эталон — заголовок `Date` любого HTTPS-ответа: он обязателен по RFC 9110, и `http::client`
//! у нас уже есть. Ни NTP-клиента, ни библиотеки времени ради одной строки не заводим.
//! Цена, которую платим осознанно: секундная точность заголовка плюс время ответа в пути.
//! На фоне двухминутного окна это шум, и порог «хорошо» взят с запасом на него.
//!
//! Два слоя, как у всех (D-097): `epoch` разбирает заголовок, `tell` превращает расхождение
//! в вердикт — обе чистые и проверяются без сети; отчёт собран поверх них.

use crate::diag::report::{Report, Tone, Verdict};
use crate::error::Result;

/// Куда идём за эталоном: 204 без тела — самый дешёвый ответ, какой бывает,
/// и тот же адрес уже служит замером узлов (`nodes::ping`).
const REFERENCE: &str = "https://cp.cloudflare.com/generate_204";

/// Расхождение, которое ещё ничему не мешает: заголовок огрублён до секунды,
/// и ответ шёл к нам не мгновенно.
const CLOSE: i64 = 10;

/// Расхождение, при котором рукопожатие ещё проходит, но запас кончается: окно AEAD
/// около двух минут в обе стороны.
const TOLERATED: i64 = 60;

/// Замер: насколько часы машины разошлись с эталоном. Плюс — спешат, пусто — сверять
/// не с чем (эталон молчит или ответ не разобрался).
///
/// Типизированный слой поверх той же пробы (D-097): его спрашивает код, которому нужно
/// решение, — например шаг «после пробуждения» (D-115). Отчёт для окна собран поверх него.
pub async fn skew() -> Option<i64> {
    measure().await.skew
}

/// Что увидели: расхождение, сам заголовок и сколько шёл ответ.
struct Shot {
    skew: Option<i64>,
    date: Option<String>,
    ms: u64,
}

async fn measure() -> Shot {
    let began = std::time::Instant::now();
    // Мимо системного прокси: часы сверяют с интернетом, а не с нашим туннелем.
    let date = crate::http::direct()
        .ok()
        .map(|client| client.head(REFERENCE).send());
    let date = match date {
        Some(request) => request
            .await
            .ok()
            .and_then(|response| Some(response.headers().get("date")?.to_str().ok()?.to_string())),
        None => None,
    };
    let ms = began.elapsed().as_millis() as u64;
    let skew = date
        .as_deref()
        .and_then(epoch)
        .zip(crate::stamp::now())
        .map(|(theirs, ours)| ours as i64 - theirs as i64);
    Shot { skew, date, ms }
}

pub async fn check() -> Result<Report> {
    let mut report = Report::new("clock");
    report.say(Tone::Dim, format!("эталон: {REFERENCE}"));

    let shot = measure().await;
    let Some(date) = shot.date else {
        return Ok(report.finish(
            Verdict::Idle,
            "Эталон времени не ответил — сверять не с чем",
            shot.ms,
        ));
    };
    report.say(Tone::Dim, format!("ответ: {date}"));

    let Some(skew) = shot.skew else {
        return Ok(report.finish(
            Verdict::Bad,
            format!("Не разобрали время ответа: {date}"),
            shot.ms,
        ));
    };
    let (verdict, headline) = tell(skew);
    Ok(report.finish(verdict, headline, shot.ms))
}

/// Жалоба для окна: пусто — часы в порядке или сверить не удалось. Порог тот же,
/// что у красного вердикта: предупреждение в баннер не выносим — оно ничему не мешает.
pub fn complaint(skew: i64) -> Option<String> {
    (skew.abs() > TOLERATED).then(|| tell(skew).1)
}

/// Вердикт по расхождению. Знак наш: плюс — часы машины впереди эталона.
fn tell(skew: i64) -> (Verdict, String) {
    let off = gap(skew.unsigned_abs());
    if skew.abs() <= CLOSE {
        return (Verdict::Ok, format!("Часы точны: расхождение {off}"));
    }
    if skew.abs() <= TOLERATED {
        return (
            Verdict::Warn,
            format!("Часы {}: {off} — рукопожатию пока хватает", side(skew)),
        );
    }
    (
        Verdict::Bad,
        format!(
            "Часы {}: {off} — VMess и VLESS будут вставать через раз",
            side(skew)
        ),
    )
}

fn side(skew: i64) -> &'static str {
    if skew > 0 {
        "спешат"
    } else {
        "отстают"
    }
}

/// Расхождение по-человечески.
fn gap(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds} с")
    } else {
        format!("{} мин {} с", seconds / 60, seconds % 60)
    }
}

/// Секунды эпохи из заголовка `Date`. Формат один и фиксированный —
/// `Sun, 06 Nov 1994 08:49:37 GMT`; две устаревшие формы RFC 9110 разрешает принимать,
/// но не встречаются они уже лет двадцать, и разбирать их значит держать код,
/// который никогда не выполнится.
fn epoch(date: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split_whitespace().skip(1);
    let day: i64 = parts.next()?.parse().ok()?;
    let name = parts.next()?;
    let month = MONTHS.iter().position(|m| *m == name)? as i64 + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next()?.parse().ok()?;
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let days = civil(year, month, day);
    u64::try_from(days * 86_400 + hour * 3600 + minute * 60 + second).ok()
}

/// Дней от эпохи до этой даты — счёт Хиннанта: март считается первым месяцем,
/// и високосный день оказывается последним в году, отчего вся арифметика становится
/// одной формулой без таблиц и ветвлений.
fn civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пример из самого RFC — если разбор врёт, врёт он именно здесь.
    #[test]
    fn the_reference_date_reads_as_the_rfc_says() {
        assert_eq!(epoch("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
    }

    /// Границы года и високосный день: там, где формула ошибётся, она ошибётся на сутки.
    #[test]
    fn year_and_leap_boundaries_hold() {
        assert_eq!(epoch("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(epoch("Sat, 29 Feb 2020 00:00:00 GMT"), Some(1_582_934_400));
        assert_eq!(epoch("Wed, 31 Dec 2025 23:59:59 GMT"), Some(1_767_225_599));
    }

    #[test]
    fn a_header_we_cannot_read_is_not_a_time() {
        assert_eq!(epoch("вчера"), None);
        assert_eq!(epoch("Sun, 06 Ноя 1994 08:49:37 GMT"), None);
        assert_eq!(epoch("Sun, 06 Nov 1994 08:49 GMT"), None);
        assert_eq!(epoch("Sun, 40 Nov 1994 08:49:37 GMT"), None);
    }

    /// Критерий задачи: пять минут расхождения — красный вердикт, и число в нём названо.
    #[test]
    fn five_minutes_off_is_a_red_verdict_with_the_number() {
        let (verdict, headline) = tell(300);
        assert_eq!(verdict, Verdict::Bad);
        assert!(headline.contains("5 мин 0 с"), "{headline}");
        assert!(headline.contains("спешат"), "{headline}");
        let (verdict, headline) = tell(-300);
        assert_eq!(verdict, Verdict::Bad);
        assert!(headline.contains("отстают"), "{headline}");
    }

    /// Секунда в пути и огрубление заголовка — не повод пугать: до десяти секунд молчим.
    #[test]
    fn a_second_in_flight_is_not_a_complaint() {
        assert_eq!(tell(0).0, Verdict::Ok);
        assert_eq!(tell(-9).0, Verdict::Ok);
        assert_eq!(tell(11).0, Verdict::Warn);
        assert_eq!(tell(60).0, Verdict::Warn);
        assert_eq!(tell(61).0, Verdict::Bad);
    }
}
