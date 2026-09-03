//! Задачи голосом: «Ноа, напомни завтра в три позвонить в банк».
//!
//! Распоряжение узнаётся в два приёма. Сначала по словам решается, о задачах
//! ли вообще речь: это дёшево и не требует ничего, кроме самой фразы. И только
//! если да — фраза уходит в модель, чтобы та достала из неё название и срок.
//!
//! Порядок именно такой, потому что фраз в разговоре много, а распоряжений
//! мало. Гонять модель на каждое сказанное слово ради разбора, который почти
//! всегда окажется ненужным, — значит платить задержкой за каждый обычный
//! вопрос.

use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone};
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::tasks;

/// О чём человек распорядился.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    /// Завести задачу.
    Add,
    /// Перечислить, что предстоит.
    List,
    /// Отметить сделанной.
    Done,
}

/// Слова, по которым узнаётся распоряжение. Проверяются вхождением: в живой
/// речи они склоняются, а перечислять формы бесполезно.
const ADD_MARKS: &[&str] = &[
    "напомни",
    "напомнить",
    "добавь задачу",
    "поставь задачу",
    "заведи задачу",
    "запиши задачу",
    "запиши, что",
    "запланируй",
    "не забыть",
    "не дай забыть",
    "нужно будет",
];

const LIST_MARKS: &[&str] = &[
    "что у меня",
    "какие задачи",
    "мои задачи",
    "что на сегодня",
    "что сегодня надо",
    "что мне надо сделать",
    "что предстоит",
    "список задач",
];

const DONE_MARKS: &[&str] = &[
    "я сделал",
    "уже сделал",
    "сделано",
    "выполнил",
    "готово с",
    "отметь",
    "закрой задачу",
    "убери задачу",
];

/// Распоряжение ли это и какое.
///
/// Порядок проверки не случаен: «что у меня на сегодня» содержит и слова
/// перечисления, и, в других формах, слова добавления. Перечисление проверяется
/// первым как более узкое.
pub fn intent(text: &str) -> Option<Intent> {
    let lower = text.to_lowercase();
    let has = |marks: &[&str]| marks.iter().any(|mark| lower.contains(mark));

    if has(LIST_MARKS) {
        return Some(Intent::List);
    }
    if has(DONE_MARKS) {
        return Some(Intent::Done);
    }
    if has(ADD_MARKS) {
        return Some(Intent::Add);
    }
    None
}

/// Название задачи, которой не хватает срока.
///
/// Между «напомни позвонить в банк» и «завтра в три» проходит целый круг
/// разговора, и название надо где-то держать. Одно на всю программу: человек
/// заводит задачи по одной.
static AWAITING_TIME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Ждём ли сейчас, что человек назовёт срок.
pub fn awaiting_time() -> bool {
    AWAITING_TIME
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .is_some()
}

/// Забывает недоспрошенную задачу. Зовётся, когда разговор кончается.
pub fn forget_pending() {
    *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = None;
}

/// Выполняет распоряжение и отдаёт то, что нужно сказать человеку вслух.
///
/// `None` означает «это было не про задачи» — фразу надо обработать как
/// обычный вопрос.
pub async fn handle(app: &AppHandle, said: &str) -> Option<String> {
    // Задача ждёт срока — значит, сказанное сейчас и есть срок.
    if awaiting_time() {
        return Some(finish_pending(app, said).await);
    }

    match intent(said)? {
        Intent::Add => Some(add(app, said).await),
        Intent::List => {
            // Заодно показываем окно: список из пяти дел на слух запоминается
            // плохо, а глазами он читается сразу весь.
            if let Err(err) = crate::overlay::show_tasks(app) {
                log::warn!("окно задач не открылось: {err}");
            }
            Some(list())
        }
        Intent::Done => Some(done(said)),
    }
}

/* ── Завести задачу ──────────────────────────────────────────────────────── */

async fn add(app: &AppHandle, said: &str) -> String {
    let Some(parsed) = interpret(app, &add_rules(), said).await else {
        return "Не разобрал задачу. Повторите, пожалуйста.".into();
    };

    let title = parsed["title"].as_str().unwrap_or_default().trim().to_string();
    if title.is_empty() {
        return "Не понял, что нужно сделать.".into();
    }

    let due = parse_due(parsed["due"].as_str().unwrap_or_default());
    if due.is_none() {
        // Срок не назван — спрашиваем, а не назначаем сами. Задача без срока
        // не напомнит о себе, и человек узнает об этом слишком поздно.
        *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = Some(title.clone());
        return format!("Записал: {title}. На когда?");
    }

    let task = tasks::add(title, due, None);
    changed(app);
    format!("Записал: {}, {}.", task.title, spoken_due(task.due))
}

/// Достаёт срок из ответа на вопрос «на когда?».
async fn finish_pending(app: &AppHandle, said: &str) -> String {
    let title = AWAITING_TIME
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
        .unwrap_or_default();

    // «Не надо», «потом», «без срока» — законный ответ: задача остаётся в
    // списке, просто молча.
    if refuses_time(said) {
        let task = tasks::add(title, None, None);
        changed(app);
        return format!("Оставил без срока: {}.", task.title);
    }

    let due = match interpret(app, &time_rules(), said).await {
        Some(parsed) => parse_due(parsed["due"].as_str().unwrap_or_default()),
        None => None,
    };

    let task = tasks::add(title, due, None);
    changed(app);
    match task.due {
        Some(_) => format!("Записал: {}, {}.", task.title, spoken_due(task.due)),
        None => format!("Срок не понял, оставил без него: {}.", task.title),
    }
}

/// Отказ называть срок.
fn refuses_time(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["не надо", "без срока", "потом", "когда-нибудь", "неважно", "не знаю"]
        .iter()
        .any(|mark| lower.contains(mark))
}

/* ── Перечислить ─────────────────────────────────────────────────────────── */

fn list() -> String {
    let now = Local::now();
    let due = tasks::today(now);

    if due.is_empty() {
        return "На сегодня ничего не запланировано.".into();
    }

    let overdue = due.iter().filter(|task| task.overdue(now)).count();
    let names: Vec<String> = due
        .iter()
        .take(5)
        .map(|task| match task.due {
            Some(_) => format!("{} — {}", task.title, spoken_due(task.due)),
            None => task.title.clone(),
        })
        .collect();

    let mut answer = format!("{}: {}", headline(due.len(), overdue), names.join("; "));
    if due.len() > names.len() {
        answer.push_str(&format!(" и ещё {}", due.len() - names.len()));
    }
    answer.push('.');
    answer
}

fn headline(total: usize, overdue: usize) -> String {
    if overdue > 0 {
        format!("Всего {total}, из них просрочено {overdue}")
    } else {
        format!("На сегодня {total}")
    }
}

/* ── Отметить сделанной ──────────────────────────────────────────────────── */

fn done(said: &str) -> String {
    let now = Local::now();
    let open: Vec<tasks::Task> = tasks::all()
        .into_iter()
        .filter(|task| task.done_at.is_none())
        .collect();

    if open.is_empty() {
        return "Незакрытых задач нет.".into();
    }

    // Ищем ту, о которой речь, по общим словам. Модель здесь не нужна: список
    // короткий, а совпадение слова из названия — признак надёжный.
    let words = significant(said);
    let best = open
        .iter()
        .map(|task| (task, overlap(&words, &significant(&task.title))))
        .filter(|(_, score)| *score > 0)
        .max_by_key(|(_, score)| *score);

    let Some((task, _)) = best else {
        // Одна незакрытая задача — понятно, о чём речь, даже если слова разные.
        if open.len() == 1 {
            tasks::set_done(&open[0].id, true);
            return format!("Отметил сделанной: {}.", open[0].title);
        }
        return "Не понял, какую задачу закрыть.".into();
    };

    tasks::set_done(&task.id, true);
    let left = tasks::today(now).len();
    match left {
        0 => format!("Отметил: {}. На сегодня всё.", task.title),
        _ => format!("Отметил: {}. Осталось {left}.", task.title),
    }
}

/// Слова, по которым имеет смысл сравнивать: длиной от четырёх букв.
///
/// Короткие — предлоги и частицы, они совпадают у любых двух фраз и только
/// мешают: «в», «на», «уже» есть везде.
fn significant(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        // Сравниваем по основе: «позвонить» и «позвонил» — одно и то же дело.
        .map(|word| word.chars().take(5).collect())
        .collect()
}

fn overlap(left: &[String], right: &[String]) -> usize {
    left.iter().filter(|word| right.contains(word)).count()
}

/* ── Разговор с моделью ──────────────────────────────────────────────────── */

async fn interpret(app: &AppHandle, rules: &str, said: &str) -> Option<serde_json::Value> {
    let provider = app.state::<AppState>().provider();
    let raw = match provider.interpret(rules, said).await {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("разбор распоряжения не удался: {err}");
            return None;
        }
    };

    // Модель охотно добавляет пояснения вокруг ответа. Берём то, что между
    // первой и последней фигурной скобкой, — этого достаточно.
    let Some((from, to)) = raw.find('{').zip(raw.rfind('}')) else {
        log::warn!("в ответе разбора нет JSON: «{raw}»");
        return None;
    };
    match serde_json::from_str(&raw[from..=to]) {
        Ok(parsed) => {
            log::info!("разобрано: {}", &raw[from..=to]);
            Some(parsed)
        }
        Err(err) => {
            log::warn!("ответ разбора не разобрался ({err}): «{raw}»");
            None
        }
    }
}

fn add_rules() -> String {
    format!(
        "Ты разбираешь распоряжение о задаче. {}
         Ответь одним объектом JSON и ничем больше: ни приветствия, ни пояснений.
         title — что сделать, коротко и без слов «напомни», «запиши», «не забудь».
         due — срок в виде ГГГГ-ММ-ДДTЧЧ:ММ по местному времени,          либо пустая строка, если срок не назван.
         Если названо только время без дня — возьми ближайший день, когда оно ещё не прошло.
         {}",
        now_line(),
        EXAMPLE_TASK
    )
}

fn time_rules() -> String {
    format!(
        "Человек называет срок задачи. {}
         Ответь одним объектом JSON и ничем больше: ни приветствия, ни пояснений.
         due — ГГГГ-ММ-ДДTЧЧ:ММ по местному времени, либо пустая строка, если срок не назван.
         Если названо только время без дня — возьми ближайший день, когда оно ещё не прошло.
         {}",
        now_line(),
        EXAMPLE_TIME
    )
}

/// Разобранный пример стоит десятка указаний.
///
/// Небольшие модели держат формат по образцу заметно лучше, чем по описанию, и
/// заодно перестают терять «завтра»: в примере видно, что от него дата
/// сдвигается на день, а не остаётся сегодняшней.
const EXAMPLE_TASK: &str = "Пример при «Сейчас 2026-09-03 11:00, четверг».
     Сказано: напомни завтра в три позвонить в банк
     Ответ: {\"title\": \"позвонить в банк\", \"due\": \"2026-09-04T15:00\"}";

const EXAMPLE_TIME: &str = "Пример при «Сейчас 2026-09-03 11:00, четверг».
     Сказано: завтра утром
     Ответ: {\"due\": \"2026-09-04T09:00\"}";

/// Точка отсчёта для модели: без неё «завтра» не во что превратить.
fn now_line() -> String {
    let now = Local::now();
    format!(
        "Сейчас {}, {}, {}.",
        now.format("%Y-%m-%d %H:%M"),
        weekday(now),
        month_day(now)
    )
}

fn weekday(now: DateTime<Local>) -> &'static str {
    match now.weekday().num_days_from_monday() {
        0 => "понедельник",
        1 => "вторник",
        2 => "среда",
        3 => "четверг",
        4 => "пятница",
        5 => "суббота",
        _ => "воскресенье",
    }
}

fn month_day(now: DateTime<Local>) -> String {
    const MONTHS: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    format!(
        "{} {}",
        now.day(),
        MONTHS[(now.month0() as usize).min(11)]
    )
}

/* ── Сроки ───────────────────────────────────────────────────────────────── */

/// Превращает то, что вернула модель, в срок.
fn parse_due(raw: &str) -> Option<DateTime<Local>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Сначала полный вид с поясом — вдруг модель добавила его сама.
    if let Ok(exact) = DateTime::parse_from_rfc3339(raw) {
        return Some(exact.with_timezone(&Local));
    }

    for shape in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, shape) {
            // Час перевода стрелок бывает несуществующим и бывает двойным.
            // В первом случае берём ближайший существующий, во втором — ранний.
            if let Some(exact) = Local.from_local_datetime(&naive).earliest() {
                return Some(exact);
            }
        }
    }

    log::warn!("срок «{raw}» не разобрался");
    None
}

/// Срок словами — так, как его произносят.
fn spoken_due(due: Option<DateTime<Local>>) -> String {
    let Some(due) = due else {
        return "без срока".into();
    };

    let now = Local::now();
    let days = due.date_naive().signed_duration_since(now.date_naive()).num_days();
    let time = due.format("%H:%M").to_string();

    match days {
        0 => format!("сегодня в {time}"),
        1 => format!("завтра в {time}"),
        2..=6 => format!("в {} в {time}", weekday(due)),
        _ => format!("{} в {time}", month_day(due)),
    }
}

/// Сообщает окнам, что список изменился.
fn changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("tasks:changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_are_told_apart_from_questions() {
        assert_eq!(intent("напомни завтра позвонить в банк"), Some(Intent::Add));
        assert_eq!(intent("что у меня на сегодня"), Some(Intent::List));
        assert_eq!(intent("я сделал отчёт"), Some(Intent::Done));

        // Обычные вопросы задачами не становятся.
        assert_eq!(intent("что такое альбедо"), None);
        assert_eq!(intent("расскажи анекдот"), None);
    }

    #[test]
    fn listing_wins_over_adding() {
        // Во фразе есть и «что у меня», и слово, похожее на добавление.
        assert_eq!(
            intent("что у меня запланировано на сегодня"),
            Some(Intent::List)
        );
    }

    #[test]
    fn a_deadline_is_read_in_local_time() {
        let parsed = parse_due("2026-08-25T15:30").expect("срок разобран");
        assert_eq!(parsed.format("%Y-%m-%d %H:%M").to_string(), "2026-08-25 15:30");
        assert!(parse_due("").is_none());
        assert!(parse_due("когда-нибудь").is_none());
    }

    #[test]
    fn a_task_is_matched_by_meaningful_words() {
        let said = significant("я сделал отчёт за август");
        let task = significant("Отправить отчёт за август");
        assert!(overlap(&said, &task) > 0, "общие слова должны находиться");

        let other = significant("Позвонить в банк");
        assert_eq!(overlap(&said, &other), 0, "чужая задача не совпадает");
    }
}
