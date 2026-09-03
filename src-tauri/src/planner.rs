//! Задачи в разговоре.
//!
//! Что человек имел в виду, решает модель, а не список слов. Прежде здесь
//! стояло угадывание по основам («напомни», «задача», «сделал»), и оно
//! проваливалось на обычной живой речи: «добавь на сегодня доделать резюме
//! задача» и «какие у меня задачи на сегодня» мимо него проходили. Список слов
//! в принципе не может покрыть язык — можно лишь бесконечно его дополнять,
//! каждый раз узнавая о новой формулировке от человека, у которого не сработало.
//!
//! Поэтому каждая реплика разговора разбирается одним обращением к модели,
//! которое отвечает строгим JSON: что это было и о каком деле речь. Цена —
//! примерно секунда на реплику; она окупается тем, что распоряжение понимается
//! так, как оно сказано, а не так, как заранее угадали.
//!
//! Открытые дела перечисляются в самом запросе, поэтому «отметь резюме» и
//! «перенеси банк на завтра» указывают на конкретную задачу по номеру, а не
//! через сравнение слов.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone};
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::tasks::{self, Task};

/// Что человек имел в виду.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Обычный вопрос или разговор — задачи ни при чём.
    Chat,
    /// Завести дело.
    Add {
        title: String,
        due: Option<DateTime<Local>>,
        /// Просил именно в календарь, а не просто напомнить.
        calendar: bool,
    },
    /// Перечислить, что предстоит.
    List,
    /// Отметить сделанным.
    Done { task: Option<String> },
    /// Перенести на другой срок.
    Postpone {
        task: Option<String>,
        due: Option<DateTime<Local>>,
    },
    /// Помочь: разложить дело на шаги и подсказать, с чего начать.
    Breakdown { task: Option<String> },
}

/// Название дела, которому не хватает срока.
///
/// Между «напомни доделать резюме» и «завтра к обеду» проходит целый круг
/// разговора, и название надо где-то держать.
static AWAITING_TIME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Ждём ли, что человек назовёт срок.
pub fn awaiting_time() -> bool {
    AWAITING_TIME
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .is_some()
}

/// Забывает недоспрошенное дело. Зовётся, когда разговор кончается.
pub fn forget_pending() {
    *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = None;
}

/// Выполняет распоряжение и отдаёт то, что сказать вслух.
///
/// `None` означает «это был обычный вопрос» — фразу надо обработать как всегда.
pub async fn handle(app: &AppHandle, said: &str) -> Option<String> {
    // Дело ждёт срока — значит, сказанное сейчас и есть срок.
    if awaiting_time() {
        return Some(finish_pending(app, said).await);
    }

    let open = open_tasks();
    match read_intent(app, said, &open).await {
        Intent::Chat => None,
        Intent::Add {
            title,
            due,
            calendar,
        } => Some(add(app, title, due, calendar)),
        Intent::List => {
            // Заодно показываем окно: список из пяти дел на слух запоминается
            // плохо, а глазами читается сразу весь.
            if let Err(err) = crate::overlay::show_tasks(app) {
                log::warn!("окно задач не открылось: {err}");
            }
            Some(list())
        }
        Intent::Done { task } => Some(done(app, task.as_deref(), &open)),
        Intent::Postpone { task, due } => Some(postpone(app, task.as_deref(), due, &open)),
        Intent::Breakdown { task } => Some(breakdown(app, task.as_deref(), &open).await),
    }
}

/// Незакрытые дела — те, о которых может идти речь.
fn open_tasks() -> Vec<Task> {
    let mut open: Vec<Task> = tasks::all()
        .into_iter()
        .filter(|task| task.done_at.is_none())
        .collect();
    // Ближайшие сверху: о них и говорят чаще всего.
    open.sort_by_key(|task| task.due);
    open.truncate(20);
    open
}

/* ── Разбор реплики ──────────────────────────────────────────────────────── */

async fn read_intent(app: &AppHandle, said: &str, open: &[Task]) -> Intent {
    let Some(parsed) = interpret(app, &intent_rules(open), said).await else {
        // Не разобрали — считаем обычным вопросом. Промолчать в ответ на
        // вопрос хуже, чем не завести задачу: задачу человек повторит.
        return Intent::Chat;
    };

    let text = |key: &str| {
        parsed[key]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let due = parse_due(&text("due"));
    let task = pick_task(&parsed, open);

    match text("intent").as_str() {
        "add" => {
            let title = text("title");
            if title.is_empty() {
                Intent::Chat
            } else {
                Intent::Add {
                    title,
                    due,
                    calendar: parsed["calendar"].as_bool().unwrap_or(false),
                }
            }
        }
        "list" => Intent::List,
        "done" => Intent::Done { task },
        "postpone" => Intent::Postpone { task, due },
        "breakdown" => Intent::Breakdown { task },
        _ => Intent::Chat,
    }
}

/// Какое дело имелось в виду: модель называет его номером в переданном списке.
fn pick_task(parsed: &serde_json::Value, open: &[Task]) -> Option<String> {
    let number = parsed["task"].as_i64()?;
    if number < 1 {
        return None;
    }
    open.get((number - 1) as usize).map(|task| task.id.clone())
}

fn intent_rules(open: &[Task]) -> String {
    let list = if open.is_empty() {
        "Открытых дел нет.".to_string()
    } else {
        let lines: Vec<String> = open
            .iter()
            .enumerate()
            .map(|(at, task)| format!("[{}] {}", at + 1, task.title))
            .collect();
        format!("Открытые дела:\n{}", lines.join("\n"))
    };

    format!(
        "Ты — Ноа, голосовой помощник. Определи, чего хочет человек, и ответь \
         одним объектом JSON без пояснений.\n\
         \n\
         Поле intent — одно из:\n\
         chat — обычный вопрос, разговор, просьба что-то объяснить;\n\
         add — просит завести дело, напомнить о чём-то, записать, запланировать \
         встречу или добавить в календарь;\n\
         list — спрашивает, что у него запланировано, какие дела, что на сегодня;\n\
         done — сообщает, что уже что-то сделал;\n\
         postpone — просит перенести дело на другое время;\n\
         breakdown — просит помощи с делом: как за него взяться, с чего начать, \
         разбить на шаги.\n\
         \n\
         Остальные поля:\n\
         title — название дела для add: коротко, без слов «напомни» и «запиши»;\n\
         due — срок в виде ГГГГ-ММ-ДДTЧЧ:ММ или пустая строка, если не назван;\n\
         task — номер дела из списка ниже для done, postpone и breakdown, иначе 0;\n\
         calendar — true, если человек прямо просил в календарь.\n\
         \n\
         Если назван день без времени — ставь 18:00. Полночь никому не нужна: \
         напоминание в это время человек не услышит.\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}",
        now_line(),
        list,
        EXAMPLES
    )
}

/// Разобранные примеры.
///
/// Небольшие модели держат формат по образцу заметно лучше, чем по описанию.
/// Здесь же видно главное: обычный вопрос — это chat, и заводить по нему дело
/// не надо.
const EXAMPLES: &str = "Примеры при «Сейчас 2026-09-03 11:00, четверг, 3 сентября».\n\
     «что такое альбедо» → {\"intent\":\"chat\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false}\n\
     «добавь на сегодня доделать резюме задача» → \
     {\"intent\":\"add\",\"title\":\"доделать резюме\",\"due\":\"2026-09-03T18:00\",\"task\":0,\"calendar\":false}\n\
     «напомни завтра в три позвонить в банк» → \
     {\"intent\":\"add\",\"title\":\"позвонить в банк\",\"due\":\"2026-09-04T15:00\",\"task\":0,\"calendar\":false}\n\
     «поставь в календарь встречу с юристом в понедельник в десять» → \
     {\"intent\":\"add\",\"title\":\"встреча с юристом\",\"due\":\"2026-09-07T10:00\",\"task\":0,\"calendar\":true}\n\
     «какие у меня задачи на сегодня» → {\"intent\":\"list\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false}\n\
     «я доделал резюме» → {\"intent\":\"done\",\"title\":\"\",\"due\":\"\",\"task\":1,\"calendar\":false}\n\
     «перенеси банк на завтра» → \
     {\"intent\":\"postpone\",\"title\":\"\",\"due\":\"2026-09-04T15:00\",\"task\":2,\"calendar\":false}\n\
     «помоги мне с резюме, с чего начать» → \
     {\"intent\":\"breakdown\",\"title\":\"\",\"due\":\"\",\"task\":1,\"calendar\":false}";

/* ── Завести ─────────────────────────────────────────────────────────────── */

fn add(app: &AppHandle, title: String, due: Option<DateTime<Local>>, calendar: bool) -> String {
    if due.is_none() {
        // Срок не назван — спрашиваем, а не назначаем сами. Дело без срока не
        // напомнит о себе, и человек узнает об этом слишком поздно.
        *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = Some(title.clone());
        return format!("Записал: {title}. На когда?");
    }

    let task = tasks::add(title, due, None);
    log::info!("заведено дело «{}» на {:?}", task.title, task.due);
    crate::calendar::sync_task(app, &task, calendar);
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
    crate::calendar::sync_task(app, &task, false);
    changed(app);
    match task.due {
        Some(_) => format!("Записал: {}, {}.", task.title, spoken_due(task.due)),
        None => format!("Срок не понял, оставил без него: {}.", task.title),
    }
}

/// Отказ называть срок.
fn refuses_time(said: &str) -> bool {
    let lower = said.to_lowercase();
    [
        "не надо",
        "без срока",
        "потом",
        "когда-нибудь",
        "неважно",
        "не знаю",
        "не важно",
    ]
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

/* ── Отметить сделанным ──────────────────────────────────────────────────── */

fn done(app: &AppHandle, id: Option<&str>, open: &[Task]) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, какое дело закрыть.".into();
    };

    let Some(closed) = tasks::set_done(&task.id, true) else {
        return "Такого дела в списке нет.".into();
    };
    crate::calendar::forget_task(app, &closed);
    changed(app);

    let left = tasks::today(Local::now()).len();
    match left {
        0 => format!("Отметил: {}. На сегодня всё.", closed.title),
        _ => format!("Отметил: {}. Осталось {left}.", closed.title),
    }
}

/* ── Перенести ───────────────────────────────────────────────────────────── */

fn postpone(
    app: &AppHandle,
    id: Option<&str>,
    due: Option<DateTime<Local>>,
    open: &[Task],
) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, какое дело перенести.".into();
    };

    // Срок не назвали — переносим на завтра в то же время. Это самое частое
    // намерение, и переспрашивать ради него — лишний круг разговора.
    let to = due.unwrap_or_else(|| {
        task.due
            .map(|due| due + Duration::days(1))
            .unwrap_or_else(|| Local::now() + Duration::days(1))
    });

    let Some(moved) = tasks::postpone(&task.id, to) else {
        return "Такого дела в списке нет.".into();
    };
    crate::calendar::sync_task(app, &moved, false);
    changed(app);

    if moved.postponed >= 3 {
        return format!(
            "Перенёс: {}, {}. Это уже {}-й перенос — может, разбить его на шаги?",
            moved.title,
            spoken_due(moved.due),
            moved.postponed
        );
    }
    format!("Перенёс: {}, {}.", moved.title, spoken_due(moved.due))
}

/* ── Помочь с делом ──────────────────────────────────────────────────────── */

async fn breakdown(app: &AppHandle, id: Option<&str>, open: &[Task]) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, с каким делом помочь.".into();
    };

    let Some(parsed) = interpret(app, PLAN_RULES, &task.title).await else {
        return "Не смог придумать план. Повторите, пожалуйста.".into();
    };

    let steps: Vec<String> = parsed["steps"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|step| step.as_str())
                .map(|step| step.trim().to_string())
                .filter(|step| !step.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return "Не смог разбить это на шаги.".into();
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty())
        .map(str::to_string);

    tasks::set_plan(&task.id, steps.clone(), advice.clone());
    if let Err(err) = crate::overlay::show_tasks(app) {
        log::warn!("окно задач не открылось: {err}");
    }
    changed(app);

    let spoken = steps
        .iter()
        .enumerate()
        .map(|(at, step)| format!("{}. {step}", at + 1))
        .collect::<Vec<_>>()
        .join(" ");

    match advice {
        Some(advice) => format!("Разбил на шаги. {spoken} {advice}"),
        None => format!("Разбил на шаги. {spoken}"),
    }
}

/// Разбивает названную задачу на шаги. Зовётся из окна кнопкой.
pub async fn plan_task(app: &AppHandle, id: &str) -> Result<Option<Task>, String> {
    let Some(task) = tasks::all().into_iter().find(|task| task.id == id) else {
        return Ok(None);
    };

    let Some(parsed) = interpret(app, PLAN_RULES, &task.title).await else {
        return Err("модель не ответила".into());
    };

    let steps: Vec<String> = parsed["steps"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|step| step.as_str())
                .map(|step| step.trim().to_string())
                .filter(|step| !step.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return Err("не вышло разбить это на шаги".into());
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty())
        .map(str::to_string);

    Ok(tasks::set_plan(&task.id, steps, advice))
}

const PLAN_RULES: &str = "Разбей дело на 3–5 понятных шагов и дай один короткий совет, \
     с чего начать. Ответь одним объектом JSON без пояснений:\n\
     {\"steps\": [\"…\", \"…\"], \"advice\": \"…\"}\n\
     Шаги — в неопределённой форме, каждый на одно действие, не длиннее семи слов.\n\
     Совет — одно предложение.\n\
     Пример для дела «разослать резюме»:\n\
     {\"steps\":[\"обновить опыт за последний год\",\"собрать список из десяти вакансий\",\
     \"написать сопроводительное письмо\",\"отправить и записать даты\"],\
     \"advice\":\"Начните со списка вакансий — он покажет, что править в резюме.\"}";

/// Дело, о котором речь: названное моделью или единственное открытое.
fn only_one_or<'a>(id: Option<&str>, open: &'a [Task]) -> Option<&'a Task> {
    if let Some(id) = id {
        return open.iter().find(|task| task.id == id);
    }
    // Открыто ровно одно — понятно, о чём речь, даже если не назвали.
    match open {
        [single] => Some(single),
        _ => None,
    }
}

/* ── Разговор с моделью ──────────────────────────────────────────────────── */

async fn interpret(app: &AppHandle, rules: &str, said: &str) -> Option<serde_json::Value> {
    let provider = app.state::<AppState>().provider();
    let raw = match provider.interpret(rules, said).await {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("разбор реплики не удался: {err}");
            return None;
        }
    };

    // Модель охотно добавляет пояснения вокруг ответа. Берём то, что между
    // первой и последней фигурной скобкой.
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

fn time_rules() -> String {
    format!(
        "Человек называет срок дела. {}\n\
         Ответь одним объектом JSON и ничем больше: ни приветствия, ни пояснений.\n\
         due — ГГГГ-ММ-ДДTЧЧ:ММ по местному времени, либо пустая строка, если срок не назван.\n\
         Если названо только время без дня — возьми ближайший день, когда оно ещё не прошло.\n\
         Пример при «Сейчас 2026-09-03 11:00, четверг».\n\
         Сказано: завтра утром\n\
         Ответ: {{\"due\": \"2026-09-04T09:00\"}}",
        now_line()
    )
}

/// Точка отсчёта для модели: без неё «завтра» не во что превратить.
pub fn now_line() -> String {
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
    format!("{} {}", now.day(), MONTHS[(now.month0() as usize).min(11)])
}

/* ── Сроки ───────────────────────────────────────────────────────────────── */

/// Превращает то, что вернула модель, в срок.
pub fn parse_due(raw: &str) -> Option<DateTime<Local>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

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
pub fn spoken_due(due: Option<DateTime<Local>>) -> String {
    let Some(due) = due else {
        return "без срока".into();
    };

    let now = Local::now();
    let days = due
        .date_naive()
        .signed_duration_since(now.date_naive())
        .num_days();
    let time = due.format("%H:%M").to_string();

    match days {
        0 => format!("сегодня в {time}"),
        1 => format!("завтра в {time}"),
        2..=6 => format!("в {} в {time}", weekday(due)),
        _ => format!("{} в {time}", month_day(due)),
    }
}

/// Сообщает окнам, что список изменился.
pub fn changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("tasks:changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, title: &str) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            due: None,
            remind_at: None,
            done_at: None,
            created_at: Local::now(),
            reminded: false,
            event_id: None,
            steps: Vec::new(),
            advice: None,
            postponed: 0,
        }
    }

    #[test]
    fn a_deadline_is_read_in_local_time() {
        let parsed = parse_due("2026-08-25T15:30").expect("срок разобран");
        assert_eq!(
            parsed.format("%Y-%m-%d %H:%M").to_string(),
            "2026-08-25 15:30"
        );
        // Модель иногда добавляет секунды или пробел вместо T.
        assert!(parse_due("2026-08-25 15:30").is_some());
        assert!(parse_due("2026-08-25T15:30:00").is_some());

        assert!(parse_due("").is_none());
        assert!(parse_due("когда-нибудь").is_none());
    }

    #[test]
    fn a_task_is_chosen_by_its_number() {
        let open = vec![task("a", "позвонить в банк"), task("b", "доделать резюме")];

        let parsed: serde_json::Value = serde_json::from_str(r#"{"task": 2}"#).unwrap();
        assert_eq!(pick_task(&parsed, &open).as_deref(), Some("b"));

        // Ноль означает «дело не названо».
        let none: serde_json::Value = serde_json::from_str(r#"{"task": 0}"#).unwrap();
        assert!(pick_task(&none, &open).is_none());

        // Номер за пределами списка не должен выбирать наугад.
        let far: serde_json::Value = serde_json::from_str(r#"{"task": 9}"#).unwrap();
        assert!(pick_task(&far, &open).is_none());
    }

    #[test]
    fn the_only_open_task_needs_no_naming() {
        let single = vec![task("a", "позвонить в банк")];
        assert_eq!(
            only_one_or(None, &single).map(|task| task.id.as_str()),
            Some("a"),
            "когда дело одно, называть его незачем"
        );

        let many = vec![task("a", "первое"), task("b", "второе")];
        assert!(
            only_one_or(None, &many).is_none(),
            "из двух дел наугад выбирать нельзя"
        );
    }

    #[test]
    fn refusing_a_deadline_is_understood() {
        assert!(refuses_time("да не надо срока"));
        assert!(refuses_time("потом решу"));
        assert!(!refuses_time("завтра в три"));
    }
}
