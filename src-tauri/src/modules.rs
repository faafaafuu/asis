//! Модули Ноа — для главного окна.
//!
//! Ноа — помощник, модули — его инструменты: у каждого своё окно, свои
//! команды голосом и в Telegram. Здесь — список модулей и их состояние одной
//! строкой. Встроенные берут состояние из того, что уже лежит в памяти и на
//! диске; свои модули (`plugins`) — из состояния их MCP-сервера. Главное окно
//! открывается мгновенно и в сеть не ходит.

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleCard {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub about: String,
    /// Состояние одной строкой: «3 дела, 1 просрочено».
    pub status: String,
    /// Как позвать голосом.
    pub voice: String,
    /// Есть ли у модуля своё окно.
    pub window: bool,
    /// Свой модуль — его можно удалить.
    pub custom: bool,
}

fn builtin(id: &str, title: &str, icon: &str, about: &str, status: String, voice: &str, window: bool) -> ModuleCard {
    ModuleCard {
        id: id.into(),
        title: title.into(),
        icon: icon.into(),
        about: about.into(),
        status,
        voice: voice.into(),
        window,
        custom: false,
    }
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

/// Все модули с состоянием: встроенные, затем свои.
pub fn overview(app: &AppHandle) -> Vec<ModuleCard> {
    let telegram = if crate::telegram::ready(app) { "подключён" } else { "не подключён" };
    let name = app.state::<crate::state::AppState>().wake_name();
    let say = |phrase: &str| format!("«{name}, {phrase}»");
    let mut cards = vec![
        builtin("tasks", "Задачи", "✓", "Дела со сроками, шаги и напоминания", tasks_status(),
            &say("напомни завтра в десять забрать посылку"), true),
        builtin("watchlist", "Активы", "◆", "Акции, валюты и оповещения о цене", watchlist_status(),
            &say("какой курс евро"), true),
        builtin("learning", "Обучение", "◈", "Курсы с уроками, задачами и экзаменами", learning_status(),
            &say("погоняй меня по курсу"), true),
        builtin("order", "Заказы", "▣", "Продукты по лучшей цене, корзина одним голосом", order_status(),
            &say("закажи молоко, хлеб и яйца"), true),
        builtin("alarms", "Будильники", "◷", "Будильники по дням недели и таймеры", alarms_status(),
            &say("разбуди в семь по будням"), false),
        builtin("telegram", "Telegram", "➤", &format!("{name} в мессенджере: текстом и голосовыми"), telegram.into(),
            "Пишите своему боту — отвечает тем же", false),
    ];
    cards.extend(crate::plugins::installed(app).into_iter().map(|manifest| ModuleCard {
        status: crate::plugins::status(&manifest.id),
        id: manifest.id,
        title: manifest.title,
        icon: if manifest.icon.is_empty() { "✦".into() } else { manifest.icon },
        about: manifest.about,
        voice: manifest.voice.replace("Ноа", &name),
        window: false,
        custom: true,
    }));
    cards
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
