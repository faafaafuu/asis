//! Сколько Ноа заплатил сам, без подтверждения, — и можно ли ещё.
//!
//! Самостоятельная оплата ограничена дважды: суммой одного заказа и суммой за
//! сутки. Один потолок не защищает от второго заказа: десять заказов по
//! потолку — это десять потолков, а ошибка распознавания, повторённая в
//! разговоре, легко повторяется и в корзине. Всё, что не укладывается в оба
//! предела, ждёт подтверждения человеком.
//!
//! Сутки — скользящие: последние двадцать четыре часа, а не календарный день.
//! Иначе в 23:59 и в 00:01 можно было бы потратить двойной дневной предел за
//! две минуты.
//!
//! Учитываются только оплаты без подтверждения. То, что человек оплатил сам,
//! нажав кнопку, он и так видел — пределы здесь про то, чего он не видел.

use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Local};
use serde::{Deserialize, Serialize};

/// Одна оплата без подтверждения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payment {
    pub at: DateTime<Local>,
    /// Рубли.
    pub total: u32,
}

/// Можно ли платить без подтверждения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Заказ дороже предела одного заказа.
    OverOrder,
    /// Вместе с уже оплаченным за сутки выходит больше дневного предела.
    OverDay { spent: u32 },
}

/// Решает, укладывается ли заказ в пределы.
///
/// Предел заказа `0` — самостоятельная оплата выключена. Дневной предел `0`
/// — дневного предела нет, остаётся только предел заказа.
pub fn verdict(
    history: &[Payment],
    now: DateTime<Local>,
    total: u32,
    per_order: u32,
    per_day: u32,
) -> Verdict {
    if per_order == 0 || total > per_order {
        return Verdict::OverOrder;
    }
    let spent = spent_since(history, now - Duration::hours(24));
    if per_day > 0 && spent.saturating_add(total) > per_day {
        return Verdict::OverDay { spent };
    }
    Verdict::Allowed
}

fn spent_since(history: &[Payment], since: DateTime<Local>) -> u32 {
    history
        .iter()
        .filter(|payment| payment.at > since)
        .map(|payment| payment.total)
        .sum()
}

/// Где лежит учёт и что в нём.
static STORE: Mutex<Option<(PathBuf, Vec<Payment>)>> = Mutex::new(None);

/// Читает учёт из папки настроек. Зовётся один раз при запуске.
pub fn load(dir: PathBuf) {
    let path = dir.join("payments.json");
    let history = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<Payment>>(&raw).ok())
        .unwrap_or_default();
    *STORE.lock().unwrap_or_else(|err| err.into_inner()) = Some((path, history));
}

/// Оплаты без подтверждения за последние сутки и раньше — всё, что помним.
pub fn history() -> Vec<Payment> {
    STORE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .map(|(_, history)| history.clone())
        .unwrap_or_default()
}

/// Сколько оплачено без подтверждения за последние сутки.
pub fn spent_today() -> u32 {
    spent_since(&history(), Local::now() - Duration::hours(24))
}

/// Запоминает оплату без подтверждения и сразу пишет учёт на диск.
///
/// Сразу — потому что учёт и есть защита: программа, упавшая после оплаты и
/// не успевшая записать её, после перезапуска считала бы сутки чистыми.
pub fn record(total: u32) {
    let mut store = STORE.lock().unwrap_or_else(|err| err.into_inner());
    let Some((path, history)) = store.as_mut() else {
        log::warn!("учёт оплат не загружен — оплата на {total} ₽ не записана");
        return;
    };
    let now = Local::now();
    history.push(Payment { at: now, total });
    // Месяца хватает с запасом: решению нужны только последние сутки.
    history.retain(|payment| payment.at > now - Duration::days(30));

    match serde_json::to_string_pretty(history) {
        Ok(json) => {
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, json).is_ok() {
                if let Err(err) = std::fs::rename(&temp, &*path) {
                    log::warn!("учёт оплат не записался: {err}");
                }
            }
        }
        Err(err) => log::warn!("учёт оплат не собрался: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours_ago: i64, total: u32, now: DateTime<Local>) -> Payment {
        Payment {
            at: now - Duration::hours(hours_ago),
            total,
        }
    }

    #[test]
    fn one_order_under_the_order_limit_is_paid() {
        let now = Local::now();
        assert_eq!(verdict(&[], now, 2500, 3000, 5000), Verdict::Allowed);
    }

    #[test]
    fn an_order_over_the_order_limit_waits() {
        let now = Local::now();
        assert_eq!(verdict(&[], now, 3100, 3000, 5000), Verdict::OverOrder);
    }

    #[test]
    fn several_orders_fit_into_the_day() {
        let now = Local::now();
        let history = [at(5, 2000, now), at(2, 1500, now)];
        // 3500 уже потрачено, ещё 1500 — ровно дневной предел.
        assert_eq!(verdict(&history, now, 1500, 3000, 5000), Verdict::Allowed);
        // А 1600 — уже больше.
        assert_eq!(
            verdict(&history, now, 1600, 3000, 5000),
            Verdict::OverDay { spent: 3500 }
        );
    }

    #[test]
    fn the_day_is_the_last_twenty_four_hours() {
        let now = Local::now();
        // Оплата двадцатипятичасовой давности в сутки не входит.
        let history = [at(25, 3000, now), at(1, 2000, now)];
        assert_eq!(verdict(&history, now, 3000, 3000, 5000), Verdict::Allowed);
    }

    #[test]
    fn a_zero_order_limit_means_never_alone() {
        let now = Local::now();
        assert_eq!(verdict(&[], now, 100, 0, 5000), Verdict::OverOrder);
    }

    #[test]
    fn a_zero_day_limit_leaves_only_the_order_limit() {
        let now = Local::now();
        let history = [at(1, 3000, now), at(2, 3000, now)];
        assert_eq!(verdict(&history, now, 3000, 3000, 0), Verdict::Allowed);
    }
}
