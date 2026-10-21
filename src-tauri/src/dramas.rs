//! Аудиодрамы к урокам: материал урока — живая сцена на три голоса. Джун
//! паникует и задаёт вопросы слушателя, Лена объясняет, Палыч ворчит и даёт
//! заповеди; после серии — вопросы, привязанные к сценам.
//!
//! Сценарий пишет сервер (platform/server/drama.mjs) моделью через мост,
//! озвучивает служба noah-video. Здесь — материал для модели (урок целиком
//! или тезисы уроков для сериала) и кэш серий: прослушанная серия второй раз
//! играет без сети.

use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const MAX_LESSON: usize = 8500;
/// Сериал — не длиннее стольких серий: каждая пишется и озвучивается отдельно.
const MAX_SERIES: usize = 5;

/// Урок целиком для одной серии.
pub fn material(course_id: &str, topic_id: &str) -> Result<(String, String), String> {
    let course = crate::learning::course(course_id)?;
    let topic = crate::learning::topic(&course, topic_id)?;
    let concepts = topic
        .concepts
        .iter()
        .take(14)
        .map(|c| format!("- {} — {}", c.term, c.definition))
        .collect::<Vec<_>>()
        .join("\n");
    let lesson: String = topic.lesson.chars().take(MAX_LESSON).collect();
    Ok((
        topic.title.clone(),
        format!(
            "Курс «{}». Тема «{}». {}\n\nПонятия:\n{concepts}\n\nУрок:\n{lesson}",
            course.title, topic.title, topic.summary
        ),
    ))
}

/// Тезисы уроков для сериала: эта тема и следующие за ней.
pub fn lessons(course_id: &str, topic_id: &str) -> Result<Vec<Value>, String> {
    let course = crate::learning::course(course_id)?;
    let start = course.topics.iter().position(|t| t.id == topic_id).ok_or("В курсе нет такой темы.")?;
    Ok(course.topics[start..]
        .iter()
        .take(MAX_SERIES)
        .map(|topic| {
            let outline = crate::tutor::sections(&topic.lesson)
                .iter()
                .filter_map(|part| part.lines().find_map(|line| line.strip_prefix("## ")).map(|h| format!("- {}", h.trim())))
                .collect::<Vec<_>>()
                .join("\n");
            let concepts = topic
                .concepts
                .iter()
                .take(10)
                .map(|c| format!("- {} — {}", c.term, c.definition))
                .collect::<Vec<_>>()
                .join("\n");
            json!({ "title": topic.title, "theses": format!("{}\nРазделы:\n{outline}\nПонятия:\n{concepts}", topic.summary) })
        })
        .collect())
}

/// Заказ: серия по уроку (`scope` = lesson) или сериал с этого урока (section).
pub async fn order(course: &str, topic: &str, scope: &str, settings: &Value, again: bool) -> Result<Value, String> {
    let (title, material) = material(course, topic)?;
    let mut body = json!({
        "course": course,
        "topic": topic,
        "topicTitle": title,
        "scope": if scope == "section" { "section" } else { "lesson" },
        "level": settings["level"],
        "junior": settings["junior"],
        "minutes": settings["minutes"],
        "again": again,
    });
    if scope == "section" {
        body["lessons"] = Value::Array(lessons(course, topic)?);
    } else {
        body["material"] = Value::String(material);
    }
    crate::platform::drama_order(&body).await
}

fn cache(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?.join("dramas");
    std::fs::create_dir_all(&dir).map_err(|err| format!("папка аудиодрам не создалась: {err}"))?;
    Ok(dir)
}

fn valid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Серия — из кэша, иначе с сервера (и в кэш).
pub async fn episode(app: &AppHandle, id: &str, n: u32) -> Result<Vec<u8>, String> {
    if !valid(id) || !(1..=10).contains(&n) {
        return Err("Такой серии нет.".into());
    }
    let path = cache(app)?.join(format!("{id}-{n}.mp3"));
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }
    let bytes = crate::platform::file_bytes(&format!("/api/noa/dramas/{id}/episode-{n}.mp3")).await?;
    let temp = path.with_extension("part");
    if std::fs::write(&temp, &bytes).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
    Ok(bytes)
}

/// Какой урок открыть с панелью драмы, когда голос заказал её.
static WANTED: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

pub fn take_wanted() -> Option<Value> {
    WANTED.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Просят аудиодраму, сериал или «спектакль» по уроку.
fn asks_drama(lower: &str) -> bool {
    const WORDS: &[&str] = &["аудиодрам", "аудио драм", "радиоспектакл", "аудиоспектакл", "аудиосериал", "аудио сериал", "драму по", "сериал по урок", "сериал по тем", "сериал по курс"];
    WORDS.iter().any(|word| lower.contains(word))
}

/// «Ноа, сделай аудиодраму по этому уроку», «сериал по курсу с этой темы».
pub async fn voice(app: &AppHandle, said: &str) -> Option<String> {
    use tauri::Emitter;
    let lower = said.to_lowercase().replace('ё', "е");
    if !asks_drama(&lower) {
        return None;
    }
    let scope = if lower.contains("сериал") || lower.contains("по курсу") || lower.contains("по разделу") { "section" } else { "lesson" };
    let (named_course, named_topic) = crate::learning::find(said);
    let (course, topic) = match (named_course, named_topic) {
        (Some(course), Some(topic)) if !lower.contains("этому") && !lower.contains("этой") => (course.id, topic.id),
        _ => match crate::learning::last_place() {
            Some((course, topic, _)) => (course, topic),
            None => return Some("Не знаю, по какому уроку: откройте урок или назовите тему.".into()),
        },
    };
    match order(&course, &topic, scope, &json!({}), false).await {
        Ok(job) => {
            let wanted = json!({ "course": course, "topic": topic, "drama": true });
            *WANTED.lock().unwrap_or_else(|err| err.into_inner()) = Some(wanted.clone());
            let _ = crate::overlay::show_learning(app);
            let _ = app.emit("learn:video", wanted);
            Some(if job["status"] == "done" {
                "Аудиодрама уже есть — открыла урок.".into()
            } else if scope == "section" {
                "Пишу сериал — первая серия будет минуты через три, остальные следом. Ход виден в уроке.".into()
            } else {
                "Пишу аудиодраму — минуты три. Ход виден в уроке.".into()
            })
        }
        Err(err) => Some(err),
    }
}

#[cfg(test)]
mod tests {
    use super::asks_drama;

    #[test]
    fn drama_requests_are_recognised() {
        for said in ["сделай аудиодраму по этому уроку", "хочу радиоспектакль по теме", "собери сериал по курсу", "аудио драму давай"] {
            assert!(asks_drama(said), "{said}");
        }
        for said in ["включи сериал на кинопоиске", "сделай видео по разделу", "открой обучение"] {
            assert!(!asks_drama(said), "{said}");
        }
    }
}
