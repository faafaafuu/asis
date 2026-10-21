//! Ролики к урокам: короткое видео-схема по разделу урока или по всей теме.
//!
//! Собирает их сервер (platform/server/video.mjs и служба noah-video): модель
//! пишет сценарий и код на деталях noah_kit, служба озвучивает голосом Ноа и
//! рендерит. Здесь — материал для модели и кэш готовых роликов: скачанный
//! ролик второй раз открывается без сети.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Материал для модели: раздел урока (`section`) или вся тема.
pub fn material(course_id: &str, topic_id: &str, section: Option<usize>) -> Result<String, String> {
    let course = crate::learning::course(course_id)?;
    let topic = crate::learning::topic(&course, topic_id)?;
    let parts = crate::tutor::sections(&topic.lesson);
    let head = format!("Курс «{}». Тема «{}».", course.title, topic.title);
    let text = match section {
        Some(at) => {
            let part = parts.get(at).ok_or("В уроке нет такого раздела.")?;
            format!("{head} Ролик — про один раздел урока.\n\nТекст раздела:\n{part}")
        }
        None => {
            let outline = parts
                .iter()
                .filter_map(|part| part.lines().find_map(|line| line.strip_prefix("## ")).map(|title| format!("- {}", title.trim())))
                .collect::<Vec<_>>()
                .join("\n");
            let concepts = topic
                .concepts
                .iter()
                .take(12)
                .map(|c| format!("- {} — {}", c.term, c.definition))
                .collect::<Vec<_>>()
                .join("\n");
            let lesson: String = topic.lesson.chars().take(4000).collect();
            format!(
                "{head} Ролик — обзор всей темы: что это, из чего состоит и как работает.\n\nКоротко: {}\n\nРазделы урока:\n{outline}\n\nПонятия:\n{concepts}\n\nНачало урока:\n{lesson}",
                topic.summary
            )
        }
    };
    Ok(text)
}

fn cache(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?.join("videos");
    std::fs::create_dir_all(&dir).map_err(|err| format!("папка роликов не создалась: {err}"))?;
    Ok(dir)
}

fn valid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Файл ролика или обложки — из кэша, иначе с сервера (и в кэш).
pub async fn file(app: &AppHandle, id: &str, poster: bool) -> Result<Vec<u8>, String> {
    if !valid(id) {
        return Err("Такого ролика нет.".into());
    }
    let name = if poster { format!("{id}.jpg") } else { format!("{id}.mp4") };
    let path = cache(app)?.join(name);
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }
    let bytes = crate::platform::video_bytes(id, if poster { "poster.jpg" } else { "video.mp4" }).await?;
    let temp = path.with_extension("part");
    if std::fs::write(&temp, &bytes).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
    Ok(bytes)
}

/// Какой урок показать, когда окно обучения откроется: голос заказал ролик,
/// окно открывает тему на этом разделе — там виден ход сборки.
static WANTED: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

pub fn take_wanted() -> Option<serde_json::Value> {
    WANTED.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Просят собрать ролик к уроку, а не включить видео или записать экран.
fn asks_video(lower: &str) -> bool {
    const MAKE: &[&str] = &["сделай", "сними", "собери", "создай", "сгенерир", "смонтируй", "запили", "подготовь"];
    const WANT: &[&str] = &["хочу", "нужен", "нужно", "покажи", "давай"];
    const LESSON: &[&str] = &["урок", "раздел", "тем", "обучен", "курс", "объясн"];
    const OTHER: &[&str] = &["экран", "ютуб", "youtube", "вк ", "рутуб", "включи", "запусти", "фильм", "клип"];
    if !(lower.contains("видео") || lower.contains("ролик")) || OTHER.iter().any(|word| lower.contains(word)) {
        return false;
    }
    let about_lesson = LESSON.iter().any(|word| lower.contains(word)) || lower.contains(" про ") || lower.contains(" о ");
    MAKE.iter().any(|word| lower.contains(word)) || (WANT.iter().any(|word| lower.contains(word)) && about_lesson)
}

/// «Ноа, сделай видео по этому разделу», «сделай ролик про сервисы в
/// кубере». Тема — названная, иначе та, на которой остановились; раздел —
/// тот, что открыт, если не просили «по всей теме».
pub async fn voice(app: &AppHandle, said: &str) -> Option<String> {
    use tauri::Emitter;
    let lower = said.to_lowercase().replace('ё', "е");
    if !asks_video(&lower) {
        return None;
    }
    let whole = lower.contains("всей тем") || lower.contains("по теме") || lower.contains("целиком") || lower.contains("обзор");
    let (named_course, named_topic) = crate::learning::find(said);
    let (course, topic, section) = match (named_course, named_topic) {
        (Some(course), Some(topic)) if !lower.contains("этому") && !lower.contains("этот") => (course.id, topic.id, None),
        _ => {
            let Some((course, topic, place)) = crate::learning::last_place() else {
                return Some("Не знаю, про какую тему ролик: откройте урок или назовите тему.".into());
            };
            (course, topic, if whole { None } else { Some(place) })
        }
    };
    let material = match material(&course, &topic, section) {
        Ok(text) => text,
        Err(err) => return Some(err),
    };
    match crate::platform::video_order(&course, &topic, section, &material, false).await {
        Ok(job) => {
            let wanted = serde_json::json!({ "course": course, "topic": topic, "section": section });
            *WANTED.lock().unwrap_or_else(|err| err.into_inner()) = Some(wanted.clone());
            let _ = crate::overlay::show_learning(app);
            let _ = app.emit("learn:video", wanted);
            let what = if section.is_some() { "по этому разделу" } else { "по всей теме" };
            Some(if job["status"] == "done" {
                format!("Ролик {what} уже есть — открыла урок.")
            } else {
                format!("Собираю ролик {what} — около двух минут. Ход виден в уроке.")
            })
        }
        Err(err) => Some(err),
    }
}

#[cfg(test)]
mod tests {
    use super::asks_video;

    #[test]
    fn video_requests_are_told_from_other_video_talk() {
        for said in [
            "ноа сделай видео по этому разделу",
            "сделай ролик про сервисы в кубере",
            "сними видео по всей теме",
            "хочу ролик по уроку",
            "покажи видео про поды",
            "давай ролик по этой теме",
            "сгенерируй видео по разделу",
        ] {
            assert!(asks_video(said), "{said}");
        }
        for said in [
            "включи видео на ютубе",
            "запиши видео с экрана",
            "сними видео экрана",
            "покажи видео",
            "что за ролик сейчас играет",
            "открой обучение",
            "сделай погромче",
        ] {
            assert!(!asks_video(said), "{said}");
        }
    }
}
