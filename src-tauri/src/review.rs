//! Вечерний разбор дня: что успел, что переносим.
//!
//! Раз в день, ближе к вечеру, программа спрашивает про каждое несделанное
//! дело, чей срок уже наступил. Ответы здесь простые — «да» или «нет», — и
//! разбираются они словами, без модели: на таком вопросе список слов работает
//! надёжно, а лишнее обращение к модели добавило бы секунду ожидания к каждому
//! из десятка вопросов.
//!
//! Разбор идёт по одному делу за раз, а не «расскажите, что успели». Так
//! человеку не приходится держать список в голове, а программе — угадывать, о
//! чём из перечисленного он сейчас говорит.

use chrono::{Duration, Local, Timelike};
use tauri::AppHandle;

use crate::tasks;

/// На каком шаге разбор.
#[derive(Clone)]
enum Step {
    /// Спросили «сделал?» про дело под этим номером.
    Asked { at: usize },
    /// Спросили «перенести на завтра?» про дело под этим номером.
    Moving { at: usize },
}

struct Session {
    /// Дела, о которых спрашиваем, в порядке очереди.
    queue: Vec<tasks::Task>,
    step: Step,
    done: usize,
    moved: usize,
}

static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

/// Когда разбор проводился последний раз. Чтобы не начинать его дважды за вечер.
static LAST_RUN: std::sync::Mutex<Option<chrono::NaiveDate>> = std::sync::Mutex::new(None);

/// Прекращает разбор. Зовётся, когда разговор кончается.
pub fn stop() {
    *SESSION.lock().unwrap_or_else(|err| err.into_inner()) = None;
}

/// Следит за временем и начинает разбор, когда пора.
#[cfg(desktop)]
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-review".into())
        .spawn(move || loop {
            // Раз в минуту: точность до минуты здесь и нужна, а чаще — впустую.
            std::thread::sleep(std::time::Duration::from_secs(60));
            if due_now(&app) {
                start(&app);
            }
        })
        .ok();
}

/// Пора ли начинать.
#[cfg(desktop)]
fn due_now(app: &AppHandle) -> bool {
    use tauri::Manager;

    let config = app.state::<crate::state::AppState>().config().review.clone();
    if !config.enabled {
        return false;
    }

    let now = Local::now();
    if now.hour() != config.hour || now.minute() < config.minute {
        return false;
    }

    // Один раз за вечер. Проверка минуты выше срабатывает шестьдесят раз подряд,
    // и без этой отметки человек получил бы разбор каждую минуту в течение часа.
    let mut last = LAST_RUN.lock().unwrap_or_else(|err| err.into_inner());
    if *last == Some(now.date_naive()) {
        return false;
    }

    // Разговор уже идёт — не влезаем. Разбор подождёт до завтра: перебивать
    // человека посреди его собственного вопроса хуже, чем пропустить вечер.
    if crate::in_conversation() {
        return false;
    }

    if tasks::unfinished_by(now).is_empty() {
        // Спрашивать нечего — и отмечаем вечер пройденным, чтобы не проверять
        // список каждую минуту до полуночи.
        *last = Some(now.date_naive());
        return false;
    }

    *last = Some(now.date_naive());
    true
}

/// Начинает разбор: задаёт первый вопрос и включает разговор.
#[cfg(desktop)]
pub fn start(app: &AppHandle) {
    let queue = tasks::unfinished_by(Local::now());
    if queue.is_empty() {
        return;
    }

    let first = queue[0].title.clone();
    let count = queue.len();
    *SESSION.lock().unwrap_or_else(|err| err.into_inner()) = Some(Session {
        queue,
        step: Step::Asked { at: 0 },
        done: 0,
        moved: 0,
    });

    log::info!("вечерний разбор: {count} дел");
    let opening = format!(
        "Подведём итог дня. {} Первое: {first} — сделал?",
        pending_line(count)
    );
    crate::announce(app, opening, true);
    crate::start_conversation(app);
}

fn pending_line(count: usize) -> String {
    match count {
        1 => "Осталось одно дело.".into(),
        2..=4 => format!("Осталось {count} дела."),
        _ => format!("Осталось {count} дел."),
    }
}

/// Разбирает ответ человека. `None` — разбор не идёт, фраза не наша.
#[cfg(desktop)]
pub fn answer(app: &AppHandle, said: &str) -> Option<String> {
    let mut guard = SESSION.lock().unwrap_or_else(|err| err.into_inner());
    let session = guard.as_mut()?;

    let reply = match session.step.clone() {
        Step::Asked { at } => match yes_no(said) {
            Some(true) => {
                let task = session.queue[at].clone();
                if let Some(closed) = tasks::set_done(&task.id, true) {
                    crate::calendar::forget_task(app, &closed);
                }
                session.done += 1;
                advance(session, at + 1)
            }
            Some(false) => {
                session.step = Step::Moving { at };
                format!("Перенести «{}» на завтра?", session.queue[at].title)
            }
            // Не «да» и не «нет» — переспрашиваем, а не гадаем.
            None => format!("Не понял. «{}» — сделал?", session.queue[at].title),
        },
        Step::Moving { at } => match yes_no(said) {
            Some(true) => {
                let task = session.queue[at].clone();
                let to = task
                    .due
                    .map(|due| due + Duration::days(1))
                    .unwrap_or_else(|| Local::now() + Duration::days(1));
                if let Some(moved) = tasks::postpone(&task.id, to) {
                    crate::calendar::sync_task(app, &moved, false);
                }
                session.moved += 1;
                advance(session, at + 1)
            }
            Some(false) => advance(session, at + 1),
            None => format!("Не понял. Перенести «{}» на завтра?", session.queue[at].title),
        },
    };

    let finished = matches!(&session.step, Step::Asked { at } if *at >= session.queue.len());
    let summary = finished.then(|| closing(session));
    if finished {
        *guard = None;
    }
    drop(guard);

    crate::planner::changed(app);
    Some(summary.unwrap_or(reply))
}

/// Переходит к следующему делу и возвращает вопрос о нём.
fn advance(session: &mut Session, at: usize) -> String {
    session.step = Step::Asked { at };
    match session.queue.get(at) {
        Some(task) => format!("{} — сделал?", task.title),
        // Очередь кончилась: вопрос не нужен, его заменит итог.
        None => String::new(),
    }
}

fn closing(session: &Session) -> String {
    let Session { done, moved, .. } = session;
    match (done, moved) {
        (0, 0) => "Ничего не отметили. Дела остались как были.".into(),
        (done, 0) => format!("Готово: закрыто {done}. Хорошего вечера."),
        (0, moved) => format!("Готово: перенесено {moved} на завтра. Хорошего вечера."),
        (done, moved) => {
            format!("Готово: закрыто {done}, перенесено {moved}. Хорошего вечера.")
        }
    }
}

/// Согласие или отказ. `None` — ни то ни другое.
///
/// Порядок проверки важен: «нет, не успел» содержит и «нет», и «успел», и по
/// одному только «успел» сошло бы за согласие. Отказ ищется первым, потому что
/// он и произносится первым словом.
fn yes_no(said: &str) -> Option<bool> {
    let lower = said.to_lowercase();

    const NO: &[&str] = &[
        "нет", "не успе", "не сдел", "не получ", "не дошли", "никак", "не смог",
    ];
    const YES: &[&str] = &[
        "да", "ага", "угу", "сделал", "успел", "готово", "закрыл", "done", "конечно",
    ];

    if NO.iter().any(|mark| lower.contains(mark)) {
        return Some(false);
    }
    if YES.iter().any(|mark| lower.contains(mark)) {
        return Some(true);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_not_mistaken_for_agreement() {
        // «не успел» содержит «успел» — отказ обязан победить.
        assert_eq!(yes_no("нет, не успел"), Some(false));
        assert_eq!(yes_no("не сделал"), Some(false));

        assert_eq!(yes_no("да, сделал"), Some(true));
        assert_eq!(yes_no("ага"), Some(true));

        // Невнятное остаётся невнятным: лучше переспросить.
        assert_eq!(yes_no("ну как тебе сказать"), None);
    }

    #[test]
    fn the_closing_line_counts_both_outcomes() {
        let session = |done, moved| Session {
            queue: Vec::new(),
            step: Step::Asked { at: 0 },
            done,
            moved,
        };
        assert!(closing(&session(2, 1)).contains("закрыто 2"));
        assert!(closing(&session(2, 1)).contains("перенесено 1"));
        assert!(closing(&session(0, 0)).contains("Ничего не отметили"));
    }
}
