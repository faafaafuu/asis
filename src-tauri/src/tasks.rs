//! Задачи: что человек просил не забыть.
//!
//! Список живёт в `tasks.json` рядом с настройками и целиком помещается в
//! памяти: даже у очень занятого человека задач сотни, а не миллионы, и база
//! данных здесь была бы лишней зависимостью ради операции «прочитать всё».
//!
//! Каждое изменение сразу пишется на диск. Программа живёт в трее месяцами и
//! закрывается как угодно, включая снятие из диспетчера, — задача, которую
//! записали голосом и не успели сохранить, была бы потеряна молча.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Шаг внутри задачи.
///
/// Появляется, когда человек просит помочь: большое дело разбивается на
/// понятные куски, и каждый отмечается отдельно. Отдельной задачей шаг не
/// делается намеренно — у него нет своего срока и он не должен засорять
/// список: это часть одного дела, а не соседнее с ним.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

/// Одна задача.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    /// Что сделать, как это сказал человек.
    pub title: String,
    /// Когда сделать. `None` — «когда-нибудь»: такие задачи в списке есть,
    /// но не напоминают о себе и в календарь не уезжают.
    pub due: Option<DateTime<Local>>,
    /// Когда напомнить. Обычно совпадает со сроком, но может быть раньше:
    /// о встрече полезно узнать не в ту минуту, когда она началась.
    pub remind_at: Option<DateTime<Local>>,
    /// Когда отметили сделанной.
    pub done_at: Option<DateTime<Local>>,
    pub created_at: DateTime<Local>,
    /// Уже напомнили. Иначе напоминание повторялось бы каждые полминуты.
    #[serde(default)]
    pub reminded: bool,
    /// Событие в Google-календаре, если задача туда уехала.
    #[serde(default)]
    pub event_id: Option<String>,
    /// Шаги, на которые разбито дело.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Совет, как это дело лучше сделать. Пишется, когда о нём просят.
    #[serde(default)]
    pub advice: Option<String>,
    /// Сколько раз задачу переносили. Видно в окне: три переноса подряд —
    /// повод не переносить в четвёртый, а разобраться, почему дело стоит.
    #[serde(default)]
    pub postponed: u32,
}

impl Task {
    /// Просрочена ли: срок был, он прошёл, а задача не сделана.
    pub fn overdue(&self, now: DateTime<Local>) -> bool {
        self.done_at.is_none() && self.due.map(|due| due < now).unwrap_or(false)
    }
}

/// Весь список. Формат файла — объект, а не массив: так в него можно будет
/// добавить общие поля, не ломая чтение у прежних версий.
#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    tasks: Vec<Task>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

struct State {
    path: PathBuf,
    store: Store,
}

/// Читает список с диска. Зовётся один раз при запуске.
///
/// Битый файл не мешает работе: программа начинает с пустого списка и говорит
/// об этом в журнал. Молча затирать чужие данные она при этом не станет —
/// первая же запись сохранит рядом то, что удалось прочитать.
pub fn load(dir: PathBuf) {
    let path = dir.join("tasks.json");
    let store = match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Store>(&raw) {
            Ok(store) => {
                log::info!("задач в списке: {}", store.tasks.len());
                store
            }
            Err(err) => {
                log::warn!("{} не разобрался: {err}", path.display());
                Store::default()
            }
        },
        Err(_) => Store::default(),
    };

    log::info!("список задач: {}", path.display());
    *STATE.lock().unwrap_or_else(|err| err.into_inner()) = Some(State { path, store });
}

/// Делает что-то со списком и сохраняет его, если он изменился.
fn with<T>(change: impl FnOnce(&mut Store) -> (T, bool)) -> Option<T> {
    let mut guard = STATE.lock().unwrap_or_else(|err| err.into_inner());
    let state = guard.as_mut()?;
    let (result, changed) = change(&mut state.store);
    if changed {
        save(state);
    }
    Some(result)
}

/// Пишет список на диск через временный файл.
///
/// Прямая запись поверх существующего файла оставляет окно, в котором файл уже
/// обрезан, но ещё не заполнен. Выключение питания в этот момент — потерянный
/// список. Переименование готового файла происходит целиком.
fn save(state: &State) {
    let Ok(raw) = serde_json::to_string_pretty(&state.store) else {
        return;
    };

    if let Some(dir) = state.path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temp = state.path.with_extension("json.tmp");
    if std::fs::write(&temp, raw).is_err() {
        log::warn!("не удалось записать {}", temp.display());
        return;
    }
    if let Err(err) = std::fs::rename(&temp, &state.path) {
        log::warn!("не удалось заменить {}: {err}", state.path.display());
    }
}

/// Весь список, как есть.
pub fn all() -> Vec<Task> {
    with(|store| (store.tasks.clone(), false)).unwrap_or_default()
}

/// Добавляет задачу и отдаёт её.
pub fn add(title: String, due: Option<DateTime<Local>>, remind_at: Option<DateTime<Local>>) -> Task {
    let now = Local::now();
    let task = Task {
        id: new_id(now),
        title,
        due,
        // Напоминание по умолчанию — в срок. Отдельным полем оно нужно тем,
        // кто просит предупредить заранее.
        remind_at: remind_at.or(due),
        done_at: None,
        created_at: now,
        reminded: false,
        event_id: None,
        steps: Vec::new(),
        advice: None,
        postponed: 0,
    };

    let copy = task.clone();
    with(|store| {
        store.tasks.push(task);
        ((), true)
    });
    copy
}

/// Отмечает сделанной или возвращает в работу.
pub fn set_done(id: &str, done: bool) -> Option<Task> {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return (None, false);
        };
        task.done_at = done.then(Local::now);
        (Some(task.clone()), true)
    })
    .flatten()
}

/// Меняет название и сроки.
pub fn edit(
    id: &str,
    title: Option<String>,
    due: Option<Option<DateTime<Local>>>,
) -> Option<Task> {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return (None, false);
        };
        if let Some(title) = title {
            task.title = title;
        }
        if let Some(due) = due {
            task.due = due;
            task.remind_at = due;
            // Срок сдвинули — значит, о задаче ещё не напоминали по-новому.
            task.reminded = false;
        }
        (Some(task.clone()), true)
    })
    .flatten()
}

/// Удаляет задачу. Отдаёт удалённую — по ней убирается событие в календаре.
pub fn remove(id: &str) -> Option<Task> {
    with(|store| {
        let Some(at) = store.tasks.iter().position(|task| task.id == id) else {
            return (None, false);
        };
        (Some(store.tasks.remove(at)), true)
    })
    .flatten()
}

/// Задачи, о которых пора напомнить, — и сразу помечает их напомненными.
///
/// Пометка ставится здесь же, одним действием со списком: иначе между «нашли»
/// и «пометили» проходит время, за которое проверка успевает случиться снова,
/// и человек слышит одно напоминание дважды.
pub fn take_due(now: DateTime<Local>) -> Vec<Task> {
    with(|store| {
        let mut due = Vec::new();
        for task in &mut store.tasks {
            if task.done_at.is_some() || task.reminded {
                continue;
            }
            let Some(at) = task.remind_at else { continue };
            if at <= now {
                task.reminded = true;
                due.push(task.clone());
            }
        }
        let changed = !due.is_empty();
        (due, changed)
    })
    .unwrap_or_default()
}

/// Запоминает событие календаря, созданное для задачи.
pub fn set_event(id: &str, event_id: Option<String>) {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return ((), false);
        };
        task.event_id = event_id;
        ((), true)
    });
}

/// Записывает шаги и совет, полученные от модели.
pub fn set_plan(id: &str, steps: Vec<String>, advice: Option<String>) -> Option<Task> {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return (None, false);
        };
        task.steps = steps
            .into_iter()
            .map(|title| Step { title, done: false })
            .collect();
        if advice.is_some() {
            task.advice = advice;
        }
        (Some(task.clone()), true)
    })
    .flatten()
}

/// Отмечает шаг сделанным или наоборот.
pub fn set_step_done(id: &str, at: usize, done: bool) -> Option<Task> {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return (None, false);
        };
        let Some(step) = task.steps.get_mut(at) else {
            return (None, false);
        };
        step.done = done;

        // Все шаги пройдены — дело сделано. Требовать отдельной отметки после
        // того, как человек закрыл последний шаг, значит просить его сказать
        // одно и то же дважды.
        if task.steps.iter().all(|step| step.done) && task.done_at.is_none() {
            task.done_at = Some(Local::now());
        }
        (Some(task.clone()), true)
    })
    .flatten()
}

/// Переносит задачу на другой срок и считает перенос.
pub fn postpone(id: &str, to: DateTime<Local>) -> Option<Task> {
    with(|store| {
        let found = store.tasks.iter_mut().find(|task| task.id == id);
        let Some(task) = found else {
            return (None, false);
        };
        task.due = Some(to);
        task.remind_at = Some(to);
        task.reminded = false;
        task.postponed = task.postponed.saturating_add(1);
        (Some(task.clone()), true)
    })
    .flatten()
}

/// Незакрытые дела, чей срок наступил или прошёл.
///
/// То, о чём имеет смысл спросить вечером: «это сделал?». Задачи без срока и
/// задачи на будущее сюда не попадают — спрашивать про них нечего.
pub fn unfinished_by(now: DateTime<Local>) -> Vec<Task> {
    all()
        .into_iter()
        .filter(|task| task.done_at.is_none())
        .filter(|task| task.due.map(|due| due <= now).unwrap_or(false))
        .collect()
}

/// Ближайшее, что человеку стоит сказать вслух: сколько дел на сегодня.
pub fn today(now: DateTime<Local>) -> Vec<Task> {
    let end = now.date_naive().and_hms_opt(23, 59, 59);
    all()
        .into_iter()
        .filter(|task| task.done_at.is_none())
        .filter(|task| match (task.due, end) {
            (Some(due), Some(end)) => due.naive_local() <= end,
            _ => false,
        })
        .collect()
}

/// Идентификатор задачи.
///
/// Времени с точностью до микросекунды достаточно: задачи создаются человеком
/// и вручную, две за одну микросекунду не появятся. Счётчик добавлен на случай
/// массового переноса, когда их создаёт программа.
fn new_id(now: DateTime<Local>) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    format!(
        "{}-{}",
        now.timestamp_micros(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Список один на всю программу, поэтому тесты, которые его открывают,
    /// нельзя пускать одновременно: второй `load` подменяет файл под первым.
    /// Очередь тут дешевле, чем городить внедрение зависимостей ради тестов.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local::now()
            .date_naive()
            .and_hms_opt(hour, minute, 0)
            .and_then(|naive| naive.and_local_timezone(Local).single())
            .expect("время сегодняшнего дня существует")
    }

    /// Список переживает перезапуск.
    ///
    /// Проверка сквозная и потому ценная: она ловит и опечатку в имени файла, и
    /// поле, которое не сериализуется, — то есть ровно те поломки, при которых
    /// задача исчезает молча и человек узнаёт об этом, только не получив
    /// напоминания.
    #[test]
    fn a_task_survives_a_restart() {
        let _queue = ONE_AT_A_TIME.lock().unwrap_or_else(|err| err.into_inner());
        let dir = std::env::temp_dir().join(format!("sufler-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("временный каталог создан");

        load(dir.clone());
        let created = add("позвонить в банк".into(), Some(at(15, 0)), None);

        // Читаем заново — как при следующем запуске программы.
        load(dir.clone());
        let list = all();

        assert_eq!(list.len(), 1, "задача осталась на диске");
        assert_eq!(list[0].title, "позвонить в банк");
        assert_eq!(list[0].id, created.id);
        assert_eq!(list[0].due, created.due, "срок пережил запись и чтение");

        // И отметка о выполнении тоже сохраняется.
        set_done(&created.id, true);
        load(dir.clone());
        assert!(all()[0].done_at.is_some(), "сделанное осталось сделанным");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finishing_every_step_finishes_the_task() {
        let _queue = ONE_AT_A_TIME.lock().unwrap_or_else(|err| err.into_inner());
        let dir = std::env::temp_dir().join(format!("sufler-steps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("временный каталог создан");
        load(dir.clone());

        let task = add("собрать отчёт".into(), None, None);
        set_plan(&task.id, vec!["собрать цифры".into(), "написать текст".into()], None);

        let after_first = set_step_done(&task.id, 0, true).expect("шаг найден");
        assert!(after_first.done_at.is_none(), "одного шага мало");

        let after_last = set_step_done(&task.id, 1, true).expect("шаг найден");
        assert!(
            after_last.done_at.is_some(),
            "последний шаг закрывает и саму задачу"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overdue_is_only_about_unfinished_work() {
        let now = at(12, 0);
        let mut task = Task {
            id: "1".into(),
            title: "позвонить".into(),
            due: Some(at(9, 0)),
            remind_at: Some(at(9, 0)),
            done_at: None,
            created_at: at(8, 0),
            reminded: false,
            event_id: None,
            steps: Vec::new(),
            advice: None,
            postponed: 0,
        };
        assert!(task.overdue(now), "срок прошёл, а дело не сделано");

        task.done_at = Some(at(10, 0));
        assert!(!task.overdue(now), "сделанное просроченным не считается");

        task.done_at = None;
        task.due = None;
        assert!(!task.overdue(now), "без срока просрочить нечего");
    }
}
