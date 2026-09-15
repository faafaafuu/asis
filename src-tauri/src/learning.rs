//! Обучение: курсы из тем, в теме — урок, задачи и мини-экзамен, в конце
//! курса — большой экзамен.
//!
//! Материал курса написан заранее и лежит в программе (`courses/`): своя
//! модель на 7 миллиардов параметров в предметах вроде DevOps ошибается, и
//! учить по её пересказу нельзя. Модель здесь только проверяет открытые
//! ответы — сверяет их с ключевыми пунктами эталона. Вопросы с вариантами
//! проверяются без неё.
//!
//! Прогресс — в `learning.json` рядом с настройками: что прочитано, лучшие
//! баллы задач и экзаменов, вопросы, на которых ошибся, — их можно
//! повторить отдельно.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Встроенные курсы: описание курса и файлы тем по порядку.
const BUILT_IN: &[(&str, &[&str])] = &[(
    include_str!("../courses/devops/course.json"),
    &[
        include_str!("../courses/devops/linux.json"),
        include_str!("../courses/devops/network.json"),
        include_str!("../courses/devops/git.json"),
        include_str!("../courses/devops/bash.json"),
        include_str!("../courses/devops/docker.json"),
        include_str!("../courses/devops/kubernetes.json"),
        include_str!("../courses/devops/cicd.json"),
        include_str!("../courses/devops/iac.json"),
        include_str!("../courses/devops/cloud.json"),
        include_str!("../courses/devops/monitoring.json"),
        include_str!("../courses/devops/security.json"),
        include_str!("../courses/devops/sre.json"),
    ],
)];

/// Проходной балл мини-экзамена и финального экзамена, в процентах.
pub const TOPIC_PASS: u32 = 70;
pub const FINAL_PASS: u32 = 75;
/// Сколько вопросов каждой темы попадает в финальный экзамен.
const FINAL_PER_TOPIC: usize = 2;
/// Ответ с таким баллом и выше считается верным.
const RIGHT: u32 = 60;

/* ── Материал ──────────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    /// `choice` — выбор варианта, `open` — ответ своими словами.
    pub kind: String,
    pub q: String,
    #[serde(default)]
    pub options: Vec<String>,
    /// Номер верного варианта, с нуля.
    #[serde(default)]
    pub answer: Option<usize>,
    /// Почему верно именно так — показывается после ответа.
    #[serde(default)]
    pub explain: String,
    /// Ключевые пункты открытого ответа: по ним модель ставит балл.
    #[serde(default)]
    pub points: Vec<String>,
    /// Образцовый ответ — показывается после проверки.
    #[serde(default)]
    pub reference: String,
}

impl Question {
    /// Вопрос без ответа — для экзамена: подсмотреть его в окне нельзя.
    fn blank(&self) -> Question {
        Question {
            answer: None,
            explain: String::new(),
            points: Vec::new(),
            reference: String::new(),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub id: String,
    pub title: String,
    /// Как тему называют вслух: «докер», «кубер», «сети».
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: String,
    /// Урок в Markdown.
    pub lesson: String,
    pub tasks: Vec<Question>,
    pub exam: Vec<Question>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    /// Сквозные вопросы финального экзамена — на стык тем.
    #[serde(default, rename = "final")]
    pub final_exam: Vec<Question>,
    #[serde(skip_deserializing)]
    pub topics: Vec<Topic>,
}

/// Все курсы. Разбираются один раз; испорченный файл пропускается с записью
/// в журнал — остальные курсы работают.
pub fn courses() -> &'static [Course] {
    static COURSES: OnceLock<Vec<Course>> = OnceLock::new();
    COURSES.get_or_init(|| {
        BUILT_IN
            .iter()
            .filter_map(|(course, topics)| {
                let mut course: Course = match serde_json::from_str(course) {
                    Ok(course) => course,
                    Err(err) => {
                        log::error!("курс не разобрался: {err}");
                        return None;
                    }
                };
                for text in topics.iter() {
                    match serde_json::from_str::<Topic>(text) {
                        Ok(topic) => course.topics.push(topic),
                        Err(err) => log::error!("тема курса {} не разобралась: {err}", course.id),
                    }
                }
                Some(course)
            })
            .collect()
    })
}

fn course(id: &str) -> Result<&'static Course, String> {
    courses()
        .iter()
        .find(|course| course.id == id)
        .ok_or_else(|| "Такого курса нет.".to_string())
}

fn topic<'a>(course: &'a Course, id: &str) -> Result<&'a Topic, String> {
    course
        .topics
        .iter()
        .find(|topic| topic.id == id)
        .ok_or_else(|| "Такой темы нет.".to_string())
}

/// Вопрос курса по номеру — из задач, экзаменов или финала.
fn question<'a>(course: &'a Course, id: &str) -> Option<(&'a Question, Option<&'a Topic>)> {
    for topic in &course.topics {
        if let Some(found) = topic.tasks.iter().chain(&topic.exam).find(|q| q.id == id) {
            return Some((found, Some(topic)));
        }
    }
    course.final_exam.iter().find(|q| q.id == id).map(|q| (q, None))
}

/* ── Прогресс ──────────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct TopicProgress {
    read: bool,
    /// Лучший балл по каждой задаче.
    tasks: BTreeMap<String, u32>,
    exam_best: Option<u32>,
    exam_attempts: u32,
    /// Вопросы, на которых ошибся, — для повторения.
    mistakes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CourseProgress {
    topics: BTreeMap<String, TopicProgress>,
    final_best: Option<u32>,
    final_attempts: u32,
    /// На какой теме остановились.
    current: Option<String>,
    updated: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    courses: BTreeMap<String, CourseProgress>,
}

static STORE: Mutex<Option<(PathBuf, Store)>> = Mutex::new(None);

/// Читает прогресс. Зовётся при запуске.
pub fn load(dir: PathBuf) {
    let path = dir.join("learning.json");
    let store = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Store>(&text).ok())
        .unwrap_or_default();
    *STORE.lock().unwrap_or_else(|err| err.into_inner()) = Some((path, store));
}

/// Меняет прогресс курса под замком и сохраняет.
fn with<T>(course: &str, change: impl FnOnce(&mut CourseProgress) -> T) -> T {
    let mut guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
    let (path, store) = guard.get_or_insert_with(|| (PathBuf::new(), Store::default()));
    let progress = store.courses.entry(course.to_string()).or_default();
    let value = change(progress);
    progress.updated = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
    if !path.as_os_str().is_empty() {
        match serde_json::to_string_pretty(store) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&*path, text) {
                    log::warn!("прогресс обучения не сохранился: {err}");
                }
            }
            Err(err) => log::warn!("прогресс обучения не сложился в JSON: {err}"),
        }
    }
    value
}

fn progress(course: &str) -> CourseProgress {
    STORE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .and_then(|(_, store)| store.courses.get(course).cloned())
        .unwrap_or_default()
}

/// Запоминает ответ на вопрос: неверный — в ошибки, верный — из ошибок.
fn note_answer(progress: &mut CourseProgress, topic: Option<&str>, id: &str, score: u32) {
    let Some(topic) = topic else { return };
    let entry = progress.topics.entry(topic.to_string()).or_default();
    entry.mistakes.retain(|known| known != id);
    if score < RIGHT {
        entry.mistakes.push(id.to_string());
    }
}

/* ── Обзор для окна ────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicCard {
    pub id: String,
    pub title: String,
    pub summary: String,
    /// `new`, `reading`, `practice`, `done`.
    pub status: String,
    pub read: bool,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub exam_best: Option<u32>,
    pub mistakes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseCard {
    pub id: String,
    pub title: String,
    pub description: String,
    pub topics: Vec<TopicCard>,
    /// Сколько курса пройдено, в процентах.
    pub percent: u32,
    pub final_best: Option<u32>,
    pub final_unlocked: bool,
    pub current: Option<String>,
    pub topic_pass: u32,
    pub final_pass: u32,
}

fn card(course: &Course, progress: &CourseProgress) -> CourseCard {
    let topics: Vec<TopicCard> = course
        .topics
        .iter()
        .map(|topic| {
            let own = progress.topics.get(&topic.id).cloned().unwrap_or_default();
            let tasks_done = topic
                .tasks
                .iter()
                .filter(|task| own.tasks.get(&task.id).is_some_and(|score| *score >= RIGHT))
                .count();
            let passed = own.exam_best.is_some_and(|best| best >= TOPIC_PASS);
            let status = if passed {
                "done"
            } else if tasks_done > 0 || own.exam_attempts > 0 {
                "practice"
            } else if own.read {
                "reading"
            } else {
                "new"
            };
            TopicCard {
                id: topic.id.clone(),
                title: topic.title.clone(),
                summary: topic.summary.clone(),
                status: status.into(),
                read: own.read,
                tasks_done,
                tasks_total: topic.tasks.len(),
                exam_best: own.exam_best,
                mistakes: own.mistakes.len(),
            }
        })
        .collect();
    CourseCard {
        id: course.id.clone(),
        title: course.title.clone(),
        description: course.description.clone(),
        percent: percent(&topics, progress.final_best),
        final_unlocked: topics.iter().all(|topic| topic.status == "done"),
        final_best: progress.final_best,
        current: progress.current.clone(),
        topics,
        topic_pass: TOPIC_PASS,
        final_pass: FINAL_PASS,
    }
}

/// Пройденная доля: у каждой темы урок — пятая часть, задачи — треть,
/// экзамен — остальное; финальный экзамен — десятая часть всего курса.
fn percent(topics: &[TopicCard], final_best: Option<u32>) -> u32 {
    if topics.is_empty() {
        return 0;
    }
    let per_topic: f64 = topics
        .iter()
        .map(|topic| {
            let read = if topic.read { 0.2 } else { 0.0 };
            let tasks = if topic.tasks_total == 0 {
                0.3
            } else {
                0.3 * topic.tasks_done as f64 / topic.tasks_total as f64
            };
            let exam = 0.5 * (f64::from(topic.exam_best.unwrap_or(0)) / f64::from(TOPIC_PASS)).min(1.0);
            read + tasks + exam
        })
        .sum::<f64>()
        / topics.len() as f64;
    let last = if final_best.is_some_and(|best| best >= FINAL_PASS) { 0.1 } else { 0.0 };
    ((per_topic * 0.9 + last) * 100.0).round() as u32
}

pub fn overview() -> Vec<CourseCard> {
    courses()
        .iter()
        .map(|course| card(course, &progress(&course.id)))
        .collect()
}

/// Тема для окна: урок, задачи с эталонами, прежние баллы задач и ошибки.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicView {
    pub topic: Topic,
    pub scores: BTreeMap<String, u32>,
    pub mistakes: Vec<Question>,
}

pub fn topic_view(course_id: &str, topic_id: &str) -> Result<TopicView, String> {
    let course = course(course_id)?;
    let topic = topic(course, topic_id)?;
    with(course_id, |progress| progress.current = Some(topic_id.to_string()));
    let own = progress(course_id).topics.get(topic_id).cloned().unwrap_or_default();
    let mut shown = topic.clone();
    // Вопросы экзамена уходят в окно без ответов: экзамен выдаётся отдельно.
    shown.exam = Vec::new();
    Ok(TopicView {
        mistakes: own
            .mistakes
            .iter()
            .filter_map(|id| question(course, id).map(|(q, _)| q.clone()))
            .collect(),
        scores: own.tasks,
        topic: shown,
    })
}

/// Урок прочитан.
pub fn mark_read(course_id: &str, topic_id: &str) -> Result<(), String> {
    let course = course(course_id)?;
    topic(course, topic_id)?;
    with(course_id, |progress| {
        progress.topics.entry(topic_id.to_string()).or_default().read = true;
        progress.current = Some(topic_id.to_string());
    });
    Ok(())
}

/* ── Проверка ──────────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub id: String,
    pub q: String,
    /// Балл 0–100. `None` — модель не ответила, оценить нужно самому.
    pub score: Option<u32>,
    pub right: bool,
    pub feedback: String,
    /// Эталон или объяснение верного варианта.
    pub reference: String,
    /// Какие пункты эталона раскрыты.
    pub covered: Vec<usize>,
    pub points: Vec<String>,
    pub options: Vec<String>,
    pub chosen: Option<usize>,
    pub answer: Option<usize>,
}

/// Проверяет один ответ. Ответ — номер варианта или текст.
pub async fn grade(app: &AppHandle, q: &Question, answer: &serde_json::Value) -> Verdict {
    let mut verdict = Verdict {
        id: q.id.clone(),
        q: q.q.clone(),
        score: Some(0),
        right: false,
        feedback: String::new(),
        reference: if q.kind == "choice" { q.explain.clone() } else { q.reference.clone() },
        covered: Vec::new(),
        points: q.points.clone(),
        options: q.options.clone(),
        chosen: None,
        answer: q.answer,
    };
    if q.kind == "choice" {
        let chosen = answer
            .as_u64()
            .map(|at| at as usize)
            .or_else(|| answer.as_str().and_then(|text| choice_of(text, &q.options)));
        verdict.chosen = chosen;
        let right = chosen.is_some() && chosen == q.answer;
        verdict.score = Some(if right { 100 } else { 0 });
        verdict.right = right;
        verdict.feedback = match (right, q.answer.and_then(|at| q.options.get(at))) {
            (true, _) => "Верно.".into(),
            (false, Some(correct)) => format!("Неверно. Правильно: {correct}."),
            (false, None) => "Неверно.".into(),
        };
        return verdict;
    }

    let text = answer.as_str().unwrap_or_default().trim();
    if text.chars().count() < 3 {
        verdict.feedback = "Ответа нет.".into();
        return verdict;
    }
    match grade_open(app, q, text).await {
        Some((covered, wrong, feedback)) => {
            let total = q.points.len().max(1) as f64;
            let mut score = (covered.len() as f64 / total * 100.0).round() as i64;
            if !wrong.is_empty() {
                score -= 20;
            }
            let score = score.clamp(0, 100) as u32;
            verdict.score = Some(score);
            verdict.right = score >= RIGHT;
            verdict.covered = covered;
            verdict.feedback = match (wrong.is_empty(), feedback.is_empty()) {
                (true, true) if score >= RIGHT => "Хороший ответ.".into(),
                (true, _) => feedback,
                (false, true) => format!("Ошибка: {wrong}"),
                (false, false) => format!("Ошибка: {wrong} {feedback}"),
            };
        }
        None => {
            verdict.score = None;
            verdict.feedback =
                "Модель не ответила — сверьтесь с эталоном и оцените себя сами.".into();
        }
    }
    verdict
}

/// Открытый ответ — модели: какие пункты эталона раскрыты и что неверно.
async fn grade_open(
    app: &AppHandle,
    q: &Question,
    answer: &str,
) -> Option<(Vec<usize>, String, String)> {
    let points = q
        .points
        .iter()
        .enumerate()
        .map(|(at, point)| format!("{}. {point}", at + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let rules = format!(
        "Ты — интервьюер на собеседовании DevOps-инженера. Сравни ответ кандидата с \
         ключевыми пунктами эталона. Пункт раскрыт, если кандидат передал его смысл \
         своими словами, пусть коротко; не упомянут или сказан неверно — не раскрыт. \
         Не придирайся к формулировкам. Верни только JSON без пояснений: \
         {{\"covered\":[номера раскрытых пунктов],\"wrong\":\"что в ответе фактически \
         неверно, иначе пустая строка\",\"feedback\":\"одно предложение по-русски: \
         чего не хватило\"}}.\n\nВопрос: {}\n\nКлючевые пункты:\n{points}",
        q.q
    );
    let provider = app.state::<crate::state::AppState>().provider();
    let raw = match provider.interpret(&rules, answer).await {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("проверка ответа не удалась: {err}");
            return None;
        }
    };
    let parsed: serde_json::Value = raw
        .find('{')
        .zip(raw.rfind('}'))
        .and_then(|(start, end)| serde_json::from_str(&raw[start..=end]).ok())?;
    let mut covered: Vec<usize> = parsed["covered"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_u64().or_else(|| item.as_str()?.trim().parse().ok()))
                .map(|at| at as usize)
                .filter(|at| (1..=q.points.len()).contains(at))
                .collect()
        })
        .unwrap_or_default();
    covered.sort_unstable();
    covered.dedup();
    let text = |key: &str| {
        parsed[key]
            .as_str()
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    };
    Some((covered, text("wrong"), text("feedback")))
}

/// Вариант по сказанному: «второй», «2», «б», или слова самого варианта.
fn choice_of(said: &str, options: &[String]) -> Option<usize> {
    const ORDINALS: &[&[&str]] = &[
        &["1", "первый", "первое", "один", "а", "a"],
        &["2", "второй", "второе", "два", "б", "b"],
        &["3", "третий", "третье", "три", "в", "c"],
        &["4", "четвертый", "четвертое", "четыре", "г", "d"],
        &["5", "пятый", "пятое", "пять", "д", "e"],
    ];
    let lower = said.to_lowercase().replace('ё', "е");
    let words: Vec<&str> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    for (at, names) in ORDINALS.iter().enumerate().take(options.len()) {
        if words.iter().any(|word| names.contains(word)) {
            return Some(at);
        }
    }
    // Назвали сам вариант: больше всего общих слов.
    let overlap = |option: &str| {
        let option = option.to_lowercase();
        words
            .iter()
            .filter(|word| word.chars().count() > 2 && option.contains(*word))
            .count()
    };
    options
        .iter()
        .enumerate()
        .map(|(at, option)| (at, overlap(option)))
        .filter(|(_, hits)| *hits > 0)
        .max_by_key(|(_, hits)| *hits)
        .map(|(at, _)| at)
}

/// Ответ на задачу или отдельный вопрос из окна.
pub async fn check(
    app: &AppHandle,
    course_id: &str,
    question_id: &str,
    answer: &serde_json::Value,
) -> Result<Verdict, String> {
    let course = course(course_id)?;
    let (q, topic) = question(course, question_id).ok_or("Такого вопроса нет.")?;
    let verdict = grade(app, q, answer).await;
    if let Some(score) = verdict.score {
        let is_task = topic.is_some_and(|topic| topic.tasks.iter().any(|task| task.id == q.id));
        with(course_id, |progress| {
            if let (true, Some(topic)) = (is_task, topic) {
                let best = progress
                    .topics
                    .entry(topic.id.clone())
                    .or_default()
                    .tasks
                    .entry(q.id.clone())
                    .or_insert(0);
                *best = (*best).max(score);
            }
            note_answer(progress, topic.map(|t| t.id.as_str()), &q.id, score);
        });
    }
    Ok(verdict)
}

/// Оценка себя самим — когда модель не ответила.
pub fn self_grade(course_id: &str, question_id: &str, knew: bool) -> Result<(), String> {
    let course = course(course_id)?;
    let (q, topic) = question(course, question_id).ok_or("Такого вопроса нет.")?;
    let score = if knew { 100 } else { 0 };
    with(course_id, |progress| {
        if let Some(topic) = topic {
            if topic.tasks.iter().any(|task| task.id == q.id) {
                let best = progress
                    .topics
                    .entry(topic.id.clone())
                    .or_default()
                    .tasks
                    .entry(q.id.clone())
                    .or_insert(0);
                *best = (*best).max(score);
            }
        }
        note_answer(progress, topic.map(|t| t.id.as_str()), &q.id, score);
    });
    Ok(())
}

/* ── Экзамены ──────────────────────────────────────────────────────────── */

/// Экзамен: `scope` — номер темы для мини-экзамена или `final`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exam {
    pub scope: String,
    pub title: String,
    pub pass: u32,
    pub questions: Vec<Question>,
}

pub fn exam(course_id: &str, scope: &str) -> Result<Exam, String> {
    let course = course(course_id)?;
    if scope == "final" {
        let mut questions: Vec<Question> = course.final_exam.iter().map(Question::blank).collect();
        let seed = chrono::Local::now().timestamp() as usize;
        for (at, topic) in course.topics.iter().enumerate() {
            let pool = &topic.exam;
            for step in 0..FINAL_PER_TOPIC.min(pool.len()) {
                let pick = (seed / 7 + at * 3 + step * 5) % pool.len();
                let q = &pool[(pick + step) % pool.len()];
                if !questions.iter().any(|known| known.id == q.id) {
                    questions.push(q.blank());
                }
            }
        }
        return Ok(Exam {
            scope: "final".into(),
            title: format!("Финальный экзамен: {}", course.title),
            pass: FINAL_PASS,
            questions,
        });
    }
    let topic = topic(course, scope)?;
    Ok(Exam {
        scope: topic.id.clone(),
        title: format!("Мини-экзамен: {}", topic.title),
        pass: TOPIC_PASS,
        questions: topic.exam.iter().map(Question::blank).collect(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamResult {
    pub score: u32,
    pub passed: bool,
    pub pass: u32,
    /// Открытые ответы, которые модель не проверила: они считаются нулём.
    pub unchecked: usize,
    pub items: Vec<Verdict>,
}

/// Сдаёт экзамен: ответы — пары «номер вопроса, ответ».
pub async fn submit(
    app: &AppHandle,
    course_id: &str,
    scope: &str,
    answers: &BTreeMap<String, serde_json::Value>,
) -> Result<ExamResult, String> {
    let course = course(course_id)?;
    let exam = exam(course_id, scope)?;
    let asked: Vec<String> = if scope == "final" {
        answers.keys().cloned().collect()
    } else {
        exam.questions.iter().map(|q| q.id.clone()).collect()
    };
    let mut items = Vec::new();
    for id in &asked {
        let Some((q, _)) = question(course, id) else { continue };
        let answer = answers.get(id).cloned().unwrap_or(serde_json::Value::Null);
        items.push(grade(app, q, &answer).await);
    }
    if items.is_empty() {
        return Err("Ответов нет.".into());
    }
    let unchecked = items.iter().filter(|item| item.score.is_none()).count();
    let total: u32 = items.iter().map(|item| item.score.unwrap_or(0)).sum();
    let score = (f64::from(total) / items.len() as f64).round() as u32;
    let pass = exam.pass;
    with(course_id, |progress| {
        for item in &items {
            if let (Some(score), Some((_, topic))) = (item.score, question(course, &item.id)) {
                note_answer(progress, topic.map(|t| t.id.as_str()), &item.id, score);
            }
        }
        if scope == "final" {
            progress.final_attempts += 1;
            progress.final_best = Some(progress.final_best.unwrap_or(0).max(score));
        } else {
            let own = progress.topics.entry(scope.to_string()).or_default();
            own.exam_attempts += 1;
            own.exam_best = Some(own.exam_best.unwrap_or(0).max(score));
        }
    });
    log::info!("экзамен {course_id}/{scope}: {score}%");
    Ok(ExamResult {
        score,
        passed: score >= pass,
        pass,
        unchecked,
        items,
    })
}

/* ── Голосом ───────────────────────────────────────────────────────────── */

/// Идёт опрос голосом: какой вопрос задан и сколько уже отвечено.
struct Quiz {
    course: String,
    topic: Option<String>,
    question: String,
    asked: u32,
    right: u32,
    at: std::time::Instant,
}

static QUIZ: Mutex<Option<Quiz>> = Mutex::new(None);

/// Опрос, брошенный на полуслове, забывается: через четверть часа ответ на
/// него — уже просто фраза.
const QUIZ_FORGET: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Курс и тема по сказанному: «докер», «девопс», «кубер».
pub fn find(said: &str) -> (Option<&'static Course>, Option<&'static Topic>) {
    let lower = said.to_lowercase().replace('ё', "е");
    let matches = |title: &str, aliases: &[String]| {
        let title = title.to_lowercase().replace('ё', "е");
        (!title.is_empty() && lower.contains(&title))
            || aliases.iter().any(|alias| {
                let alias = alias.to_lowercase().replace('ё', "е");
                !alias.is_empty() && lower.contains(&alias)
            })
    };
    for course in courses() {
        if let Some(topic) = course.topics.iter().find(|t| matches(&t.title, &t.aliases)) {
            return (Some(course), Some(topic));
        }
    }
    let course = courses()
        .iter()
        .find(|c| matches(&c.title, &c.aliases))
        .or_else(|| courses().first());
    (course, None)
}

/// Распоряжение голосом: открыть, прогресс, опрос.
pub async fn voice(app: &AppHandle, action: &str, about: &str) -> String {
    let (course, topic) = find(about);
    let Some(course) = course else {
        return "Курсов пока нет.".into();
    };
    match action.trim() {
        "quiz" => start_quiz(course, topic),
        "progress" => summary(course),
        _ => {
            if let Some(topic) = topic {
                with(&course.id, |progress| progress.current = Some(topic.id.clone()));
            }
            if let Err(err) = crate::overlay::show_learning(app) {
                log::warn!("окно обучения не открылось: {err}");
            }
            summary(course)
        }
    }
}

/// Прогресс словами.
pub fn summary(course: &Course) -> String {
    let card = card(course, &progress(&course.id));
    let done = card.topics.iter().filter(|topic| topic.status == "done").count();
    let mistakes: usize = card.topics.iter().map(|topic| topic.mistakes).sum();
    let next = card
        .topics
        .iter()
        .find(|topic| topic.status != "done")
        .map(|topic| format!(" Дальше — «{}».", topic.title))
        .unwrap_or_else(|| match card.final_best {
            Some(best) if best >= FINAL_PASS => " Финальный экзамен сдан.".into(),
            _ => " Остался финальный экзамен.".into(),
        });
    let weak = if mistakes > 0 {
        format!(" Ошибок на повторение: {mistakes}.")
    } else {
        String::new()
    };
    format!(
        "{}: пройдено {}%, тем сдано {done} из {}.{next}{weak}",
        course.title,
        card.percent,
        card.topics.len()
    )
}

/// Начинает опрос: сначала ошибки, потом ещё не отвеченное.
fn start_quiz(course: &Course, topic: Option<&Topic>) -> String {
    let Some(q) = next_question(course, topic.map(|t| t.id.as_str()), None) else {
        return "Вопросов по этой теме нет.".into();
    };
    let intro = match topic {
        Some(topic) => format!("Погоняю по теме «{}». Скажите «хватит», чтобы закончить. ", topic.title),
        None => format!("Погоняю по курсу «{}». Скажите «хватит», чтобы закончить. ", course.title),
    };
    *QUIZ.lock().unwrap_or_else(|err| err.into_inner()) = Some(Quiz {
        course: course.id.clone(),
        topic: topic.map(|t| t.id.clone()),
        question: q.id.clone(),
        asked: 0,
        right: 0,
        at: std::time::Instant::now(),
    });
    format!("{intro}{}", spoken(q))
}

/// Следующий вопрос: сперва ошибки, затем случайный из экзаменов и задач.
fn next_question<'a>(course: &'a Course, topic: Option<&str>, after: Option<&str>) -> Option<&'a Question> {
    let own = progress(&course.id);
    let topics: Vec<&Topic> = course
        .topics
        .iter()
        .filter(|t| topic.map_or(true, |id| t.id == id))
        .collect();
    let mistakes: Vec<&str> = topics
        .iter()
        .flat_map(|t| own.topics.get(&t.id).map(|p| p.mistakes.clone()).unwrap_or_default())
        .filter_map(|id| question(course, &id).map(|(q, _)| q.id.as_str()))
        .filter(|id| Some(*id) != after)
        .collect();
    if let Some(id) = mistakes.first() {
        return question(course, id).map(|(q, _)| q);
    }
    let pool: Vec<&Question> = topics
        .iter()
        .flat_map(|t| t.exam.iter().chain(&t.tasks))
        .filter(|q| Some(q.id.as_str()) != after)
        .collect();
    if pool.is_empty() {
        return None;
    }
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.subsec_nanos() as usize)
        .unwrap_or(0);
    pool.get(seed % pool.len()).copied()
}

/// Вопрос вслух: с вариантами — перечислить их.
fn spoken(q: &Question) -> String {
    if q.kind != "choice" {
        return q.q.clone();
    }
    const NAMES: &[&str] = &["первый", "второй", "третий", "четвёртый", "пятый"];
    let options = q
        .options
        .iter()
        .zip(NAMES)
        .map(|(option, name)| format!("{name}: {option}"))
        .collect::<Vec<_>>()
        .join("; ");
    format!("{} Варианты — {options}.", q.q)
}

/// Ответ на вопрос опроса. `None` — опроса нет, фраза не к нему.
pub async fn quiz_answer(app: &AppHandle, said: &str) -> Option<String> {
    let (course_id, topic_id, question_id, asked, right) = {
        let mut guard = QUIZ.lock().unwrap_or_else(|err| err.into_inner());
        let quiz = guard.as_ref()?;
        if quiz.at.elapsed() >= QUIZ_FORGET {
            *guard = None;
            return None;
        }
        (
            quiz.course.clone(),
            quiz.topic.clone(),
            quiz.question.clone(),
            quiz.asked,
            quiz.right,
        )
    };
    let lower = said.to_lowercase().replace('ё', "е");
    let words: Vec<&str> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let course = course(&course_id).ok()?;
    let (q, _) = question(course, &question_id)?;

    const STOP: &[&str] = &["хватит", "стоп", "закончим", "заканчиваем", "достаточно", "устал"];
    if words.len() <= 4 && words.iter().any(|word| STOP.contains(word)) {
        *QUIZ.lock().unwrap_or_else(|err| err.into_inner()) = None;
        return Some(if asked == 0 {
            "Закончили.".into()
        } else {
            format!("Закончили: верно {right} из {asked}. {}", summary(course))
        });
    }

    let skip = lower.contains("не знаю") || lower.contains("пропус") || lower.contains("дальше");
    let (reply, correct) = if skip {
        let answer = if q.kind == "choice" {
            q.answer.and_then(|at| q.options.get(at)).cloned().unwrap_or_default()
        } else {
            q.reference.clone()
        };
        let _ = self_grade(&course_id, &q.id, false);
        (format!("Правильный ответ: {answer}"), false)
    } else {
        let answer = serde_json::Value::String(said.to_string());
        let verdict = check(app, &course_id, &q.id, &answer).await.ok()?;
        let reply = match verdict.score {
            None => format!("Не смог проверить. Эталон: {}", verdict.reference),
            Some(score) if q.kind == "choice" => {
                let why = if q.explain.is_empty() { String::new() } else { format!(" {}", q.explain) };
                if score >= RIGHT {
                    format!("Верно.{why}")
                } else {
                    format!("{}{why}", verdict.feedback)
                }
            }
            Some(score) => {
                if score >= RIGHT {
                    format!("Засчитано, {score} из 100. {}", verdict.feedback)
                } else {
                    format!("{score} из 100. {} Эталон: {}", verdict.feedback, verdict.reference)
                }
            }
        };
        (reply, verdict.right)
    };

    let next = next_question(course, topic_id.as_deref(), Some(&q.id));
    let mut guard = QUIZ.lock().unwrap_or_else(|err| err.into_inner());
    match (next, guard.as_mut()) {
        (Some(next), Some(quiz)) => {
            quiz.question = next.id.clone();
            quiz.asked = asked + 1;
            quiz.right = right + u32::from(correct);
            quiz.at = std::time::Instant::now();
            Some(format!("{reply} Следующий вопрос: {}", spoken(next)))
        }
        _ => {
            *guard = None;
            Some(format!("{reply} Вопросы кончились."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_courses_are_whole() {
        let courses = courses();
        assert!(!courses.is_empty());
        for course in courses {
            assert_eq!(course.topics.len(), BUILT_IN[0].1.len(), "все темы разобрались");
            let mut ids = std::collections::HashSet::new();
            let all = course
                .topics
                .iter()
                .flat_map(|t| t.tasks.iter().chain(&t.exam))
                .chain(&course.final_exam);
            for q in all {
                assert!(ids.insert(q.id.clone()), "повтор номера {}", q.id);
                match q.kind.as_str() {
                    "choice" => {
                        assert!(q.options.len() >= 2, "{}: мало вариантов", q.id);
                        let answer = q.answer.expect("у выбора есть ответ");
                        assert!(answer < q.options.len(), "{}: ответ вне вариантов", q.id);
                    }
                    "open" => {
                        assert!(!q.points.is_empty(), "{}: нет пунктов", q.id);
                        assert!(!q.reference.is_empty(), "{}: нет эталона", q.id);
                    }
                    other => panic!("{}: неизвестный вид {other}", q.id),
                }
            }
            for topic in &course.topics {
                assert!(topic.lesson.len() > 500, "{}: урок пустой", topic.id);
                assert!(!topic.exam.is_empty() && !topic.tasks.is_empty(), "{}", topic.id);
            }
        }
    }

    #[test]
    fn a_choice_is_heard() {
        let options: Vec<String> = ["TCP", "UDP", "ICMP"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choice_of("второй", &options), Some(1));
        assert_eq!(choice_of("3", &options), Some(2));
        assert_eq!(choice_of("думаю, udp", &options), Some(1));
        assert_eq!(choice_of("понятия не имею", &options), None);
    }

    #[test]
    fn progress_is_counted() {
        let topic = |read, done, best| TopicCard {
            id: String::new(),
            title: String::new(),
            summary: String::new(),
            status: String::new(),
            read,
            tasks_done: done,
            tasks_total: 4,
            exam_best: best,
            mistakes: 0,
        };
        assert_eq!(percent(&[topic(false, 0, None)], None), 0);
        assert_eq!(percent(&[topic(true, 4, Some(90))], Some(80)), 100);
        assert_eq!(percent(&[topic(true, 2, None)], None), 32);
    }

    #[test]
    fn a_topic_is_found_by_name() {
        let (course, topic) = find("погоняй меня по докеру");
        assert!(course.is_some());
        assert_eq!(topic.map(|t| t.id.as_str()), Some("docker"));
    }
}
