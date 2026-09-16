//! Модули Ноа — для главного окна.
//!
//! Ноа — помощник, модули — его инструменты: у каждого своё окно, свои
//! команды голосом и в Telegram. Здесь — список модулей и их состояние одной
//! строкой. Всё берётся из того, что уже лежит в памяти и на диске: главное
//! окно открывается мгновенно и в сеть не ходит.

use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleCard {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub about: &'static str,
    /// Состояние одной строкой: «3 дела, 1 просрочено».
    pub status: String,
    /// Как позвать голосом.
    pub voice: &'static str,
    /// Есть ли у модуля своё окно.
    pub window: bool,
}

/// Русское множественное число: 1 дело, 2 дела, 5 дел.
fn plural(n: usize, one: &str, few: &str, many: &str) -> String {
    let (tens, units) = (n % 100, n % 10);
    let word = if (11..=14).contains(&tens) {
        many
    } else if units == 1 {
        one
    } else if (2..=4).contains(&units) {
        few
    } else {
        many
    };
    format!("{n} {word}")
}

fn tasks_status() -> String {
    let now = chrono::Local::now();
    let all = crate::tasks::all();
    let open = all.iter().filter(|task| task.done_at.is_none()).count();
    let overdue = all.iter().filter(|task| task.overdue(now)).count();
    match (open, overdue) {
        (0, _) => "дел нет".into(),
        (_, 0) => plural(open, "дело", "дела", "дел"),
        _ => format!("{}, просрочено {overdue}", plural(open, "дело", "дела", "дел")),
    }
}

fn watchlist_status() -> String {
    let assets = crate::watchlist::assets();
    if assets.is_empty() {
        return "список пуст".into();
    }
    let alerts: usize = assets
        .iter()
        .map(|asset| asset.alerts.iter().filter(|alert| alert.fired.is_none()).count())
        .sum();
    let mut text = plural(assets.len(), "актив", "актива", "активов");
    if alerts > 0 {
        text.push_str(&format!(" · {}", plural(alerts, "оповещение", "оповещения", "оповещений")));
    }
    text
}

fn learning_status() -> String {
    let courses = crate::learning::overview();
    match courses.as_slice() {
        [] => "курсов нет".into(),
        [one] => format!("{} — {}%", one.title, one.percent),
        [first, ..] => format!(
            "{}, {} — {}%",
            plural(courses.len(), "курс", "курса", "курсов"),
            first.title,
            first.percent
        ),
    }
}

fn order_status() -> String {
    match crate::order::current() {
        Some(order) if !order.lines.is_empty() => {
            format!("{} в корзине, {}", plural(order.lines.len(), "товар", "товара", "товаров"), order.store)
        }
        _ => "заказов пока не было".into(),
    }
}

fn alarms_status() -> String {
    let live: Vec<_> = crate::alarms::list().into_iter().filter(|alarm| alarm.enabled).collect();
    match live.iter().min_by_key(|alarm| (alarm.hour, alarm.minute)) {
        None => "будильников нет".into(),
        Some(first) => format!(
            "{}, ближний на {:02}:{:02}",
            plural(live.len(), "будильник", "будильника", "будильников"),
            first.hour,
            first.minute
        ),
    }
}

/// Все модули с состоянием.
pub fn overview(app: &AppHandle) -> Vec<ModuleCard> {
    vec![
        ModuleCard {
            id: "tasks",
            title: "Задачи",
            icon: "✓",
            about: "Дела со сроками, шаги и напоминания",
            status: tasks_status(),
            voice: "«Ноа, напомни завтра в десять позвонить в банк»",
            window: true,
        },
        ModuleCard {
            id: "watchlist",
            title: "Активы",
            icon: "◆",
            about: "Крипта, акции, валюты и оповещения о цене",
            status: watchlist_status(),
            voice: "«Ноа, поставь алерт на биткоин на 100 тысяч»",
            window: true,
        },
        ModuleCard {
            id: "learning",
            title: "Обучение",
            icon: "◈",
            about: "Курсы с уроками, задачами и экзаменами",
            status: learning_status(),
            voice: "«Ноа, погоняй меня по докеру»",
            window: true,
        },
        ModuleCard {
            id: "order",
            title: "Заказы",
            icon: "▣",
            about: "Продукты по лучшей цене, корзина одним голосом",
            status: order_status(),
            voice: "«Ноа, закажи молоко, хлеб и яйца»",
            window: true,
        },
        ModuleCard {
            id: "alarms",
            title: "Будильники",
            icon: "◷",
            about: "Будильники по дням недели и таймеры",
            status: alarms_status(),
            voice: "«Ноа, разбуди в семь по будням»",
            window: false,
        },
        ModuleCard {
            id: "telegram",
            title: "Telegram",
            icon: "➤",
            about: "Ноа в мессенджере: текстом и голосовыми",
            status: if crate::telegram::ready(app) {
                "подключён".into()
            } else {
                "не подключён".into()
            },
            voice: "Пишите своему боту — отвечает тем же",
            window: false,
        },
    ]
}

/// Открывает окно модуля.
pub fn open(app: &AppHandle, id: &str) -> Result<(), String> {
    let shown = match id {
        "tasks" => crate::overlay::show_tasks(app),
        "watchlist" => crate::overlay::show_watchlist(app),
        "learning" => crate::overlay::show_learning(app),
        "order" => crate::overlay::show_order(app),
        _ => return Err(format!("у модуля «{id}» нет своего окна")),
    };
    shown.map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::plural;

    #[test]
    fn russian_plural() {
        assert_eq!(plural(1, "дело", "дела", "дел"), "1 дело");
        assert_eq!(plural(3, "дело", "дела", "дел"), "3 дела");
        assert_eq!(plural(11, "дело", "дела", "дел"), "11 дел");
        assert_eq!(plural(22, "дело", "дела", "дел"), "22 дела");
        assert_eq!(plural(25, "дело", "дела", "дел"), "25 дел");
    }
}
