//! Запоминание: колода карточек курса, повторение по расписанию, карта
//! понятий и шпаргалка.
//!
//! Урок и экзамен отвечают на вопрос «понял ли», а повторение — «помнит ли
//! через месяц». Второе и есть цель: материал, который понят, но не
//! повторялся, через три недели уходит почти целиком. Поэтому каждое понятие
//! курса становится карточкой, а карточки приходят к человеку по расписанию
//! из `srs` — ровно тогда, когда вот-вот забудутся.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use tauri::AppHandle;

use crate::learning::{self, Course, Question, Topic};
use crate::srs::{CardState, Grade, Level};

/// Сколько новых карточек в день. Больше — и через неделю повторений станет
/// столько, что человек бросит: каждая новая карточка приносит с собой
/// несколько повторов в ближайшие дни.
const NEW_PER_DAY: u32 = 20;

fn now() -> i64 {
    chrono::Local::now().timestamp()
}

fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/* ── Колода ────────────────────────────────────────────────────────────── */

/// Карточка колоды: что спросить, что ответить и за что зацепиться.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckCard {
    /// `тема/карточка` — ключ в прогрессе.
    pub key: String,
    pub topic: String,
    pub topic_title: String,
    pub concept: String,
    pub front: String,
    pub back: String,
    pub mnemonic: String,
    pub analogy: String,
    pub example: String,
    pub pitfall: String,
}

/// Все карточки курса по порядку тем: из каждого понятия — одна, плюс
/// карточки, которые курс задал сам.
///
/// Карточка понятия спрашивает «что такое X», а не наоборот: вспомнить
/// определение по термину — то, что нужно на собеседовании и в работе.
/// Обратные карточки («как называется…») удвоили бы нагрузку ради навыка,
/// который тренируется сам, пока читаешь определения.
pub fn deck(course: &Course) -> Vec<DeckCard> {
    let mut all = Vec::new();
    for topic in &course.topics {
        let by_id: HashMap<&str, &learning::Concept> =
            topic.concepts.iter().map(|c| (c.id.as_str(), c)).collect();
        for concept in &topic.concepts {
            all.push(DeckCard {
                key: format!("{}/c-{}", topic.id, concept.id),
                topic: topic.id.clone(),
                topic_title: topic.title.clone(),
                concept: concept.id.clone(),
                front: format!("Что такое {}?", concept.term),
                back: concept.definition.clone(),
                mnemonic: concept.mnemonic.clone(),
                analogy: concept.analogy.clone(),
                example: concept.example.clone(),
                pitfall: concept.pitfall.clone(),
            });
        }
        for card in &topic.cards {
            let concept = by_id.get(card.concept.as_str());
            all.push(DeckCard {
                key: format!("{}/{}", topic.id, card.id),
                topic: topic.id.clone(),
                topic_title: topic.title.clone(),
                concept: card.concept.clone(),
                front: card.front.clone(),
                back: card.back.clone(),
                mnemonic: concept.map(|c| c.mnemonic.clone()).unwrap_or_default(),
                analogy: String::new(),
                example: String::new(),
                pitfall: concept.map(|c| c.pitfall.clone()).unwrap_or_default(),
            });
        }
    }
    all
}

/* ── Сегодня ───────────────────────────────────────────────────────────── */

/// Сколько чего в курсе: для главной страницы и для «как мой прогресс».
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mastery {
    pub total: usize,
    /// Ещё не видел.
    pub new: usize,
    pub learning: usize,
    pub young: usize,
    /// Уверенно: держится три недели и дольше.
    pub mature: usize,
    /// Пора повторить сейчас.
    pub due: usize,
    /// Сколько новых ещё можно взять сегодня.
    pub new_left: usize,
}

pub fn mastery(course: &Course) -> Mastery {
    let own = learning::progress(&course.id);
    let at = now();
    let mut out = Mastery { total: 0, ..Mastery::default() };
    for card in deck(course) {
        out.total += 1;
        match own.cards.get(&card.key) {
            None => out.new += 1,
            Some(state) => {
                match state.level() {
                    Level::New => out.new += 1,
                    Level::Learning => out.learning += 1,
                    Level::Young => out.young += 1,
                    Level::Mature => out.mature += 1,
                }
                if state.is_due(at) {
                    out.due += 1;
                }
            }
        }
    }
    let taken = if own.new_day == today_key() { own.new_count } else { 0 };
    out.new_left = (NEW_PER_DAY.saturating_sub(taken) as usize).min(out.new);
    out
}

/// Уровень по темам: сколько понятий темы держится уверенно.
pub fn topic_mastery(course: &Course) -> BTreeMap<String, (usize, usize)> {
    let own = learning::progress(&course.id);
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for card in deck(course) {
        let entry = out.entry(card.topic.clone()).or_default();
        entry.0 += 1;
        if own.cards.get(&card.key).is_some_and(|s| s.level() == Level::Mature) {
            entry.1 += 1;
        }
    }
    out
}

/* ── Повторение ────────────────────────────────────────────────────────── */

/// Карточка к показу вместе с тем, насколько она уже усвоена.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCard {
    #[serde(flatten)]
    pub card: DeckCard,
    pub level: Level,
    /// Новая — показывается впервые.
    pub fresh: bool,
}

/// Очередь на сейчас: сначала то, что пора повторить, потом новые.
///
/// Повторения идут раньше новых намеренно. Новая карточка, выученная поверх
/// забытых старых, не прибавляет знаний — она меняет одно забытое на другое.
///
/// По теме новые берутся без дневной нормы: человек только что прочитал урок
/// и сам попросил закрепить его — ограничивать тут значит мешать. Норма
/// держит только общее «повторить на сегодня».
pub fn queue(course_id: &str, topic: Option<&str>) -> Result<Vec<ReviewCard>, String> {
    let course = learning::course(course_id)?;
    let own = learning::progress(course_id);
    let at = now();
    let cards: Vec<DeckCard> = deck(&course)
        .into_iter()
        .filter(|card| topic.map_or(true, |id| card.topic == id))
        .collect();

    let mut due: Vec<(i64, ReviewCard)> = cards
        .iter()
        .filter_map(|card| {
            let state = own.cards.get(&card.key)?;
            state.is_due(at).then(|| {
                (state.due, ReviewCard { card: card.clone(), level: state.level(), fresh: false })
            })
        })
        .collect();
    due.sort_by_key(|(when, _)| *when);

    let taken = if own.new_day == today_key() { own.new_count } else { 0 };
    let allowed = if topic.is_some() { usize::MAX } else { NEW_PER_DAY.saturating_sub(taken) as usize };
    // Новые — из тем, урок которых прочитан: карточка без урока — это
    // зубрёжка определения, которого не понимаешь.
    let read: Vec<String> = learning::read_topics(course_id);
    let fresh = cards
        .iter()
        .filter(|card| !own.cards.contains_key(&card.key))
        .filter(|card| topic.is_some() || read.contains(&card.topic))
        .take(allowed)
        .map(|card| ReviewCard { card: card.clone(), level: Level::New, fresh: true });

    Ok(due.into_iter().map(|(_, card)| card).chain(fresh).collect())
}

/// Что стало с карточкой после ответа.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graded {
    pub level: Level,
    /// Через сколько вернётся — словами: «через 3 дня».
    pub next: String,
    /// Вернётся в этот же сеанс.
    pub again: bool,
}

pub fn answer(course_id: &str, key: &str, grade: Grade) -> Result<Graded, String> {
    let course = learning::course(course_id)?;
    if !deck(&course).iter().any(|card| card.key == key) {
        return Err("Такой карточки в курсе нет.".into());
    }
    let at = now();
    let today = today_key();
    let state = learning::with(course_id, |progress| {
        let fresh = !progress.cards.contains_key(key);
        if fresh {
            if progress.new_day != today {
                progress.new_day = today.clone();
                progress.new_count = 0;
            }
            progress.new_count += 1;
        }
        crate::focus::touch(&mut progress.days);
        let state = progress.cards.entry(key.to_string()).or_default();
        state.answer(grade, at);
        state.clone()
    });
    Ok(Graded { level: state.level(), next: when(state.due - at), again: grade == Grade::Again })
}

/// Экзамен показал, что понятие не держится: вернуть его в повторение сейчас.
///
/// Карточку, которую человек ещё не видел, не трогаем: она и так придёт с
/// новыми. Видел, но на экзамене ошибся — значит, расписание ошиблось
/// в его пользу, и повторить надо сегодня.
pub fn relapse(course_id: &str, topic_id: Option<&str>, concept: &str) {
    let (topic_id, concept_id) = match concept.split_once('/') {
        Some((topic, id)) => (topic, id),
        None => match topic_id {
            Some(topic) => (topic, concept),
            None => return,
        },
    };
    if concept_id.trim().is_empty() {
        return;
    }
    let key = format!("{topic_id}/c-{concept_id}");
    let at = now();
    learning::with(course_id, |progress| {
        if let Some(state) = progress.cards.get_mut(&key) {
            state.due = at;
            state.interval = (state.interval * 0.5).min(state.interval);
            state.streak = 0;
        }
    });
}

/// Через сколько — словами.
fn when(seconds: i64) -> String {
    let minutes = seconds / 60;
    if minutes < 60 {
        return "через несколько минут".into();
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("через {hours} ч");
    }
    let days = (seconds as f64 / 86_400.0).round() as i64;
    match days {
        1 => "завтра".into(),
        d if d < 30 => format!("через {d} {}", plural(d, "день", "дня", "дней")),
        d => {
            let months = (d as f64 / 30.0).round() as i64;
            format!("через {months} {}", plural(months, "месяц", "месяца", "месяцев"))
        }
    }
}

fn plural(n: i64, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let (tens, units) = (n % 100, n % 10);
    if (11..=14).contains(&tens) {
        many
    } else if units == 1 {
        one
    } else if (2..=4).contains(&units) {
        few
    } else {
        many
    }
}

/* ── Понятия и карта ───────────────────────────────────────────────────── */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptView {
    #[serde(flatten)]
    pub concept: learning::Concept,
    pub level: Level,
    /// Связи, раскрытые до названий: `(ключ, тема, понятие)`.
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub topic: String,
    pub concept: String,
    pub term: String,
    /// Связь с понятием другой темы.
    pub cross: bool,
}

/// Понятие по ссылке `id` или `тема/id`.
fn resolve<'a>(course: &'a Course, from: &Topic, reference: &str) -> Option<(&'a Topic, &'a learning::Concept)> {
    let (topic_id, concept_id) = reference.split_once('/').unwrap_or((from.id.as_str(), reference));
    let topic = course.topics.iter().find(|t| t.id == topic_id)?;
    let concept = topic.concepts.iter().find(|c| c.id == concept_id)?;
    Some((topic, concept))
}

fn level_of(course_id: &str, topic_id: &str, concept_id: &str) -> Level {
    learning::progress(course_id)
        .cards
        .get(&format!("{topic_id}/c-{concept_id}"))
        .map(CardState::level)
        .unwrap_or(Level::New)
}

pub fn concepts(course_id: &str, topic_id: &str) -> Result<Vec<ConceptView>, String> {
    let course = learning::course(course_id)?;
    let topic = learning::topic(&course, topic_id)?;
    Ok(topic
        .concepts
        .iter()
        .map(|concept| ConceptView {
            level: level_of(course_id, &topic.id, &concept.id),
            links: links_of(&course, topic, concept),
            concept: concept.clone(),
        })
        .collect())
}

/// Связи понятия в обе стороны: и те, что назвало оно, и те, что назвали его.
///
/// Односторонняя связь в данных — обычное дело: автор курса упомянул «Pod»
/// у Deployment, но не наоборот. Человеку же связь нужна с обеих сторон —
/// от Pod тоже полезно увидеть, кто им управляет.
fn links_of(course: &Course, topic: &Topic, concept: &learning::Concept) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    let mut push = |t: &Topic, c: &learning::Concept| {
        if (t.id == topic.id && c.id == concept.id) || out.iter().any(|l| l.topic == t.id && l.concept == c.id) {
            return;
        }
        out.push(Link { topic: t.id.clone(), concept: c.id.clone(), term: c.term.clone(), cross: t.id != topic.id });
    };
    for reference in &concept.related {
        if let Some((t, c)) = resolve(course, topic, reference) {
            push(t, c);
        }
    }
    for other_topic in &course.topics {
        for other in &other_topic.concepts {
            let names_us = other.related.iter().any(|reference| {
                resolve(course, other_topic, reference).is_some_and(|(t, c)| t.id == topic.id && c.id == concept.id)
            });
            if names_us {
                push(other_topic, other);
            }
        }
    }
    out
}

/// Карта курса: темы, их понятия и связи между понятиями.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapView {
    pub topics: Vec<MapTopic>,
    pub edges: Vec<MapEdge>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapTopic {
    pub id: String,
    pub title: String,
    pub concepts: Vec<MapConcept>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapConcept {
    pub id: String,
    pub term: String,
    pub level: Level,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MapEdge {
    pub from: String,
    pub to: String,
    pub cross: bool,
}

pub fn map(course_id: &str) -> Result<MapView, String> {
    let course = learning::course(course_id)?;
    let topics = course
        .topics
        .iter()
        .map(|topic| MapTopic {
            id: topic.id.clone(),
            title: topic.title.clone(),
            concepts: topic
                .concepts
                .iter()
                .map(|c| MapConcept { id: c.id.clone(), term: c.term.clone(), level: level_of(course_id, &topic.id, &c.id) })
                .collect(),
        })
        .collect();
    let mut edges: Vec<MapEdge> = Vec::new();
    for topic in &course.topics {
        for concept in &topic.concepts {
            for reference in &concept.related {
                let Some((t, c)) = resolve(&course, topic, reference) else { continue };
                let (a, b) = (format!("{}/{}", topic.id, concept.id), format!("{}/{}", t.id, c.id));
                let (from, to) = if a < b { (a, b) } else { (b, a) };
                let edge = MapEdge { from, to, cross: t.id != topic.id };
                if !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
    }
    Ok(MapView { topics, edges })
}

/// Шпаргалка темы: своя, если курс её дал, иначе — из понятий.
///
/// Лист, на который смотрят перед собеседованием, — не пересказ урока, а
/// термины с определениями в одну строку и зацепками. Ровно это и есть в
/// понятиях.
pub fn cheatsheet(topic: &Topic) -> String {
    if !topic.cheatsheet.trim().is_empty() {
        return topic.cheatsheet.clone();
    }
    if topic.concepts.is_empty() {
        return String::new();
    }
    let mut out = format!("## {} — на одном листе\n\n", topic.title);
    for concept in &topic.concepts {
        out.push_str(&format!("- **{}** — {}", concept.term, concept.definition.trim()));
        if !concept.mnemonic.trim().is_empty() {
            out.push_str(&format!(" *Запомнить:* {}", concept.mnemonic.trim()));
        }
        out.push('\n');
    }
    let pitfalls: Vec<&learning::Concept> =
        topic.concepts.iter().filter(|c| !c.pitfall.trim().is_empty()).collect();
    if !pitfalls.is_empty() {
        out.push_str("\n## Где ошибаются\n\n");
        for concept in pitfalls {
            out.push_str(&format!("- **{}**: {}\n", concept.term, concept.pitfall.trim()));
        }
    }
    out
}

/* ── Голосом ───────────────────────────────────────────────────────────── */

/// Идёт повторение голосом: какая карточка спрошена и сколько уже пройдено.
struct Session {
    course: String,
    topic: Option<String>,
    card: DeckCard,
    shown: u32,
    remembered: u32,
    at: std::time::Instant,
}

static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

/// Брошенное на полуслове повторение забывается: через пять минут сказанное
/// — уже новая просьба, а не ответ на давнюю карточку.
const FORGET: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Начинает повторение голосом. Отдаёт, что сказать.
pub fn start(course: &Course, topic: Option<&Topic>) -> String {
    let queue = match queue(&course.id, topic.map(|t| t.id.as_str())) {
        Ok(queue) => queue,
        Err(err) => return err,
    };
    let Some(first) = queue.into_iter().next() else {
        return match topic {
            Some(topic) => format!("По теме «{}» повторять сейчас нечего — всё держится. Загляните завтра.", topic.title),
            None => "На сегодня повторять нечего: всё, что пора, повторено, а новые темы ещё не прочитаны.".into(),
        };
    };
    let count = mastery(course);
    let intro = format!(
        "Повторяем. На сегодня {} {}. Отвечайте своими словами; «не помню» — скажу ответ; «хватит» — закончим. ",
        count.due + count.new_left.min(20),
        plural((count.due + count.new_left.min(20)) as i64, "карточка", "карточки", "карточек"),
    );
    let question = first.card.front.clone();
    *SESSION.lock().unwrap_or_else(|err| err.into_inner()) = Some(Session {
        course: course.id.clone(),
        topic: topic.map(|t| t.id.clone()),
        card: first.card,
        shown: 0,
        remembered: 0,
        at: std::time::Instant::now(),
    });
    format!("{intro}{question}")
}

/// Идёт ли повторение голосом.
pub fn active() -> bool {
    SESSION
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .is_some_and(|s| s.at.elapsed() < FORGET)
}

/// Заканчивает повторение и отдаёт итог. `None` — повторения не было.
pub fn stop() -> Option<String> {
    let session = SESSION.lock().unwrap_or_else(|err| err.into_inner()).take()?;
    if session.at.elapsed() >= FORGET {
        return None;
    }
    let course = learning::course(&session.course).ok()?;
    let m = mastery(&course);
    Some(if session.shown == 0 {
        "Закончили повторение.".into()
    } else {
        format!(
            "Закончили: вспомнили {} из {}. Уверенно держится {} из {} понятий курса.",
            session.remembered, session.shown, m.mature, m.total
        )
    })
}

/// Ответ на карточку голосом. `None` — повторения нет, фраза не к нему.
pub async fn hear(app: &AppHandle, said: &str) -> Option<String> {
    let (course_id, topic, card, shown, remembered) = {
        let mut guard = SESSION.lock().unwrap_or_else(|err| err.into_inner());
        let session = guard.as_ref()?;
        if session.at.elapsed() >= FORGET {
            *guard = None;
            return None;
        }
        (session.course.clone(), session.topic.clone(), session.card.clone(), session.shown, session.remembered)
    };
    let lower = said.to_lowercase().replace('ё', "е");
    let words: Vec<&str> = lower.split(|ch: char| !ch.is_alphanumeric()).filter(|w| !w.is_empty()).collect();

    const STOP: &[&str] = &["хватит", "стоп", "закончим", "заканчиваем", "достаточно", "устал"];
    if words.len() <= 4 && words.iter().any(|word| STOP.contains(word)) {
        return stop();
    }

    let forgot = lower.contains("не помню") || lower.contains("не знаю") || lower.contains("забыл")
        || lower.contains("пропус") || lower.contains("подскаж");
    let (grade, reply) = if forgot {
        (Grade::Again, format!("{}{}", card.back.trim(), hook(&card)))
    } else {
        // Карточка — открытый вопрос с одним пунктом: её ответ целиком.
        let q = Question {
            id: card.key.clone(),
            kind: "open".into(),
            q: card.front.clone(),
            options: Vec::new(),
            answer: None,
            explain: String::new(),
            points: vec![card.back.clone()],
            reference: card.back.clone(),
            concept: card.concept.clone(),
        };
        let verdict = learning::grade(app, &q, &serde_json::Value::String(said.to_string())).await;
        match verdict.score {
            None => (Grade::Hard, format!("Не смог проверить. Ответ: {}", card.back.trim())),
            Some(score) if score >= 90 => (Grade::Good, "Верно.".to_string()),
            Some(score) if score >= 60 => (Grade::Hard, format!("В целом да. Точнее: {}", card.back.trim())),
            Some(_) => (Grade::Again, format!("Не совсем. {}{}", card.back.trim(), hook(&card))),
        }
    };
    let _ = answer(&course_id, &card.key, grade);

    let next = queue(&course_id, topic.as_deref())
        .ok()
        .and_then(|queue| queue.into_iter().find(|c| c.card.key != card.key));
    let mut guard = SESSION.lock().unwrap_or_else(|err| err.into_inner());
    match (next, guard.as_mut()) {
        (Some(next), Some(session)) => {
            session.card = next.card.clone();
            session.shown = shown + 1;
            session.remembered = remembered + u32::from(grade != Grade::Again);
            session.at = std::time::Instant::now();
            Some(format!("{reply} Дальше: {}", next.card.front))
        }
        (None, Some(session)) => {
            session.shown = shown + 1;
            session.remembered = remembered + u32::from(grade != Grade::Again);
            drop(guard);
            let summary = stop().unwrap_or_default();
            Some(format!("{reply} На сегодня всё. {summary}"))
        }
        _ => None,
    }
}

/// Зацепка к ответу, если она есть: забытое лучше возвращается с образом.
fn hook(card: &DeckCard) -> String {
    if !card.mnemonic.trim().is_empty() {
        format!(" Запомнить: {}", card.mnemonic.trim())
    } else if !card.analogy.trim().is_empty() {
        format!(" Похоже на: {}", card.analogy.trim())
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn course() -> Course {
        serde_json::from_str(
            r#"{"id":"t","title":"T","description":"","topics":[
              {"id":"k8s","title":"K8s","summary":"","lesson":"","tasks":[],"exam":[],
               "concepts":[
                 {"id":"pod","term":"Pod","definition":"Минимальная единица.","related":["deploy","net/svc"]},
                 {"id":"deploy","term":"Deployment","definition":"Управляет репликами.","mnemonic":"Д — доставка"}],
               "cards":[{"id":"x","front":"Сколько?","back":"Три.","concept":"pod"}]},
              {"id":"net","title":"Сеть","summary":"","lesson":"","tasks":[],"exam":[],
               "concepts":[{"id":"svc","term":"Service","definition":"Стабильный адрес."}]}]}"#,
        )
        .expect("разбор")
    }

    #[test]
    fn every_concept_becomes_a_card() {
        let keys: Vec<String> = deck(&course()).into_iter().map(|c| c.key).collect();
        assert_eq!(keys, ["k8s/c-pod", "k8s/c-deploy", "k8s/x", "net/c-svc"]);
    }

    #[test]
    fn links_go_both_ways_and_across_topics() {
        let course = course();
        let topic = &course.topics[1];
        let links = links_of(&course, topic, &topic.concepts[0]);
        assert_eq!(links.len(), 1, "Service назван у Pod — у Service видна связь с Pod");
        assert!(links[0].cross);
        assert_eq!(links[0].term, "Pod");
    }

    #[test]
    fn the_map_keeps_one_edge_per_pair() {
        let course = course();
        let mut edges = Vec::new();
        for topic in &course.topics {
            for c in &topic.concepts {
                for r in &c.related {
                    if let Some((t, other)) = resolve(&course, topic, r) {
                        edges.push((format!("{}/{}", topic.id, c.id), format!("{}/{}", t.id, other.id)));
                    }
                }
            }
        }
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn a_cheatsheet_is_built_from_concepts() {
        let sheet = cheatsheet(&course().topics[0]);
        assert!(sheet.contains("**Pod** — Минимальная единица."));
        assert!(sheet.contains("Запомнить:* Д — доставка"));
    }

    #[test]
    fn time_is_said_in_words() {
        assert_eq!(when(300), "через несколько минут");
        assert_eq!(when(86_400), "завтра");
        assert_eq!(when(3 * 86_400), "через 3 дня");
        assert_eq!(when(21 * 86_400), "через 21 день");
        assert_eq!(when(90 * 86_400), "через 3 месяца");
    }
}
