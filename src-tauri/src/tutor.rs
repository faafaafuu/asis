//! Разговор об уроке: обсуждение раздела, темы или вопроса — голосом и текстом,
//! и подробный разбор раздела.
//!
//! Модель получает не весь курс, а то, что у человека перед глазами: раздел
//! урока с его понятиями или вопрос с эталоном и собственным ответом. Тогда
//! на «первый пункт» и «вот тут не понял» ей есть на что опереться, а запрос
//! остаётся в несколько тысяч токенов.
//!
//! Голос и текст — один разговор: реплики обоих копятся в одной истории, и
//! начатое голосом можно продолжить в окне.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::ai_client::ThreadItem;
use crate::learning::{self, Course, Topic};

/// О чём разговор: раздел урока (`topic` и `section`) или вопрос (`question`
/// и то, что на него ответили).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub course: String,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub section: Option<usize>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub answer: Option<String>,
}

struct Discussion {
    target: Target,
    /// Тема курса, к которой относится разговор, — для «спроси меня».
    topic: String,
    /// Что показать модели: раздел, понятия, вопрос с эталоном.
    material: String,
    thread: Vec<ThreadItem>,
    /// Идёт голосом: фразы разговора без рук уходят репетитору. Кончился
    /// голосовой разговор — обсуждение остаётся в окне, с той же историей.
    voice: bool,
}

static DISCUSSION: Mutex<Option<Discussion>> = Mutex::new(None);

/// Сколько обменов помнить. Больше — модель пересказывает прежнее, а запрос
/// растёт с каждым ходом.
const DEPTH: usize = 6;

/// Предел раздела, который уходит модели: длинного раздела хватает целиком,
/// а случайно огромный не раздувает каждый ход.
const MAX_SECTION: usize = 6000;

/// Урок по разделам «## …»; вступление до первого раздела идёт вместе с ним.
///
/// Так же делит урок окно обучения (`sections` в learning.js): номер раздела,
/// пришедший оттуда, указывает сюда на тот же кусок.
pub fn sections(lesson: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in lesson.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("## ") && current.iter().any(|l| l.starts_with("## ")) {
            parts.push(current.join("\n"));
            current.clear();
        }
        current.push(line);
    }
    if !current.concat().trim().is_empty() {
        parts.push(current.join("\n"));
    }
    if parts.is_empty() {
        parts.push(lesson.to_string());
    }
    parts
}

fn heading(section: &str) -> Option<String> {
    section
        .lines()
        .find_map(|line| line.strip_prefix("## "))
        .map(|text| text.trim().to_string())
}

/// Понятия темы, названные в тексте: их определения — опора для объяснения.
fn concepts_in(topic: &Topic, text: &str) -> String {
    let lower = text.to_lowercase();
    topic
        .concepts
        .iter()
        .filter(|c| {
            let term = c.term.to_lowercase();
            std::iter::once(term.as_str())
                .chain(term.split([',', '(', ')']).flat_map(|part| part.split(" и ")))
                .map(str::trim)
                .any(|part| part.chars().count() >= 3 && lower.contains(part))
        })
        .map(|c| format!("- {} — {}", c.term, c.definition))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Раздел урока для модели: где он в уроке, сам текст и его понятия.
fn section_material(course: &Course, topic: &Topic, at: Option<usize>) -> (String, String) {
    let parts = sections(&topic.lesson);
    let at = at.unwrap_or(0).min(parts.len() - 1);
    let section: String = parts[at].chars().take(MAX_SECTION).collect();
    let outline = parts
        .iter()
        .enumerate()
        .filter_map(|(n, part)| heading(part).map(|h| format!("{}. {h}{}", n + 1, if n == at { " ← сейчас" } else { "" })))
        .collect::<Vec<_>>()
        .join("\n");
    let concepts = concepts_in(topic, &section);
    let mut material = format!(
        "Курс «{}», тема «{}». Человек читает раздел {} из {}.\n",
        course.title,
        topic.title,
        at + 1,
        parts.len()
    );
    if !outline.is_empty() {
        material.push_str(&format!("\nРазделы урока:\n{outline}\n"));
    }
    material.push_str(&format!("\nТекст раздела:\n{section}\n"));
    if !concepts.is_empty() {
        material.push_str(&format!("\nПонятия из раздела:\n{concepts}\n"));
    }
    let name = heading(&section).unwrap_or_else(|| topic.title.clone());
    (material, name)
}

/// Вопрос для модели: сам вопрос, ответ человека, эталон и ключевые пункты.
fn question_material(course: &Course, id: &str, answer: &str) -> Result<(String, String), String> {
    let (q, topic) = learning::question(course, id).ok_or("Такого вопроса нет.")?;
    let topic_title = topic.map(|t| t.title.clone()).unwrap_or_else(|| "итоговый экзамен".into());
    let mut material = format!("Курс «{}», тема «{topic_title}».\nВопрос: {}\n", course.title, q.q);
    if !q.options.is_empty() {
        material.push_str(&format!("Варианты: {}\n", q.options.join("; ")));
    }
    if !answer.trim().is_empty() {
        material.push_str(&format!("Ответ человека: {}\n", answer.trim()));
    }
    if let Some(right) = q.answer.and_then(|at| q.options.get(at)) {
        material.push_str(&format!("Верный вариант: {right}\n"));
    }
    if !q.reference.is_empty() {
        material.push_str(&format!("Пример сильного ответа (не единственно верный): {}\n", q.reference));
    }
    if !q.explain.is_empty() {
        material.push_str(&format!("Пояснение: {}\n", q.explain));
    }
    if !q.points.is_empty() {
        material.push_str(&format!("Что важно раскрыть: {}\n", q.points.join("; ")));
    }
    Ok((material, topic.map(|t| t.id.clone()).unwrap_or_default()))
}

/// Начинает разговор о разделе или вопросе — или продолжает, если он о том же.
/// Отдаёт, чем открыть разговор вслух: одной короткой фразой.
pub fn open(target: &Target) -> Result<String, String> {
    let course = learning::course(&target.course)?;
    let (material, topic, intro) = match (&target.question, &target.topic) {
        (Some(id), _) => {
            let (material, topic) = question_material(&course, id, target.answer.as_deref().unwrap_or(""))?;
            (material, topic, "Что в этом вопросе разобрать?".to_string())
        }
        (None, Some(topic_id)) => {
            let topic = learning::topic(&course, topic_id)?;
            let (material, name) = section_material(&course, topic, target.section);
            (material, topic.id.clone(), format!("«{name}». Что разобрать?"))
        }
        (None, None) => return Err("Не сказано, что обсуждать.".into()),
    };
    let mut guard = DISCUSSION.lock().unwrap_or_else(|err| err.into_inner());
    let same = guard.as_ref().is_some_and(|talk| talk.target == *target);
    if !same {
        log::info!("обсуждаем: курс {}, тема {topic}, раздел {:?}, вопрос {:?}", target.course, target.section, target.question);
        *guard = Some(Discussion {
            target: target.clone(),
            topic,
            material,
            thread: Vec::new(),
            voice: false,
        });
    }
    Ok(intro)
}

/// Начинает обсуждение голосом. Отдаёт вступительную фразу.
pub fn open_voice(target: &Target) -> Result<String, String> {
    let intro = open(target)?;
    if let Some(talk) = DISCUSSION.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
        talk.voice = true;
    }
    Ok(intro)
}

/// Идёт ли разговор об уроке голосом.
pub fn active() -> bool {
    DISCUSSION
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .is_some_and(|talk| talk.voice)
}

/// Курс и тема, о которых разговор голосом.
pub fn current_topic() -> Option<(String, String)> {
    DISCUSSION
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .filter(|talk| talk.voice)
        .map(|talk| (talk.target.course.clone(), talk.topic.clone()))
}

/// Голосовой разговор кончился. Обсуждение в окне продолжается.
pub fn end() {
    if let Some(talk) = DISCUSSION.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
        talk.voice = false;
    }
}

/// Распоряжения внутри обсуждения. Всё прочее сказанное — вопрос репетитору.
///
/// Разбор реплик на обсуждении урока только мешает: «я не понял шестой
/// раздел» он принимал за разбивку дела на шаги, «я скинул скриншот» — за
/// просьбу прислать картинку, и вместо объяснения звучало «не понял, с каким
/// делом помочь». Поэтому здесь узнаются только просьбы об опросе.
pub async fn command(app: &AppHandle, said: &str) -> Option<String> {
    let lower = said.to_lowercase().replace('ё', "е");
    const QUIZ: &[&str] = &["спроси меня", "задай вопрос", "задай мне вопрос", "погоняй", "проверь меня", "проэкзаменуй"];
    if QUIZ.iter().any(|phrase| lower.contains(phrase)) {
        return Some(learning::voice(app, "quiz", "").await);
    }
    None
}

/// Указания репетитору. Начало «Ты — репетитор» — примета для моста к
/// подпискам: такой запрос он отправляет разово, с историей из запроса, а не
/// в свою долгую сессию.
fn rules(app: &AppHandle, material: &str, voice: bool) -> String {
    let name = app.state::<crate::state::AppState>().wake_name();
    let length = if voice {
        "Ответ звучит вслух: три–шесть коротких фраз, без списков, разметки, кода и ссылок; \
         команды называй словами."
    } else {
        "Ответ читают в окне: до 150 слов; можно короткий список и `команды` в обратных кавычках."
    };
    format!(
        "Ты — репетитор {name}: помогаешь человеку разобраться в уроке. Ниже — то, что у \
         него сейчас перед глазами; «тут», «это», «первый пункт» относятся к этому.\n\n\
         {material}\n\
         Как отвечать:\n\
         - Объясняй сразу и по существу, простыми словами: что это, зачем нужно, кто что \
         делает и в каком порядке, что будет, если этого нет.\n\
         - Опирайся на живой пример или бытовую аналогию. Термин, которого нет в материале, \
         объясни одной фразой.\n\
         - Человек говорит «не понял» — не переспрашивай, что именно: объясни главное ещё \
         раз, проще и с другой стороны. Переспроси, только если вопрос совсем не разобрать.\n\
         - Вопрос шире материала, но по теме курса — отвечай.\n\
         - {length}\n\
         - Без вступлений, похвалы вопросу и предложений помочь ещё. В конце можно одним \
         коротким вопросом проверить, понятно ли.\n\
         Отвечай по-русски."
    )
}

/// Реплика в разговоре об уроке: ответ репетитора с учётом истории.
///
/// `voice` — ответ прозвучит вслух: он короче и без разметки. Обмен уходит и в
/// окно обучения, чтобы сказанное голосом было видно в том же разговоре.
pub async fn answer(app: &AppHandle, said: &str, voice: bool) -> Result<String, String> {
    let (material, thread) = {
        let guard = DISCUSSION.lock().unwrap_or_else(|err| err.into_inner());
        let talk = guard.as_ref().ok_or("Обсуждение не начато.")?;
        (talk.material.clone(), talk.thread.clone())
    };
    let (provider, limit) = {
        let state = app.state::<crate::state::AppState>();
        let limit = state.config().ai.call_limit();
        (state.provider(), limit)
    };
    let rules = rules(app, &material, voice);
    let asked = tokio::time::timeout(limit, provider.converse(&rules, &thread, said, false)).await;
    let text = match asked {
        Ok(Ok(text)) if !text.trim().is_empty() => text.trim().to_string(),
        Ok(Ok(_)) => return Err("Модель прислала пустой ответ.".into()),
        Ok(Err(err)) => return Err(format!("Не получилось ответить: {}", err.user_text("модель не ответила"))),
        Err(_) => return Err("Модель не успела ответить.".into()),
    };
    if let Some(talk) = DISCUSSION.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
        talk.thread.push(ThreadItem {
            q: said.to_string(),
            a: text.clone(),
        });
        let excess = talk.thread.len().saturating_sub(DEPTH);
        talk.thread.drain(..excess);
    }
    if voice {
        let _ = app.emit("learn:talk", serde_json::json!({ "q": said, "a": text }));
    }
    Ok(text)
}

/// Вопрос текстом из окна обучения.
pub async fn ask(app: &AppHandle, target: &Target, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Напишите вопрос.".into());
    }
    open(target)?;
    answer(app, text, false).await
}

/* ── Подробный разбор раздела ──────────────────────────────────────────── */

/// Разобранные разделы: «курс/тема#раздел» → текст раздела и разбор.
///
/// Разбор пишется один раз и лежит на диске: второй раз он открывается
/// сразу и бесплатно. Раздел поменялся — разбор пишется заново.
#[derive(Default, Serialize, Deserialize)]
struct Deep {
    #[serde(default)]
    sections: BTreeMap<String, DeepEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DeepEntry {
    /// Отпечаток текста раздела, по которому писался разбор.
    source: u64,
    text: String,
}

fn deep_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("learning-deep.json"))
}

fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn load_deep(app: &AppHandle) -> Deep {
    deep_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Подробный разбор раздела: то, что в уроке сказано одной строкой, — по шагам,
/// с механизмом, примером и типичной ошибкой.
///
/// Урок пишется сжато, чтобы его можно было прочитать за раз, и перечисление
/// «уровни модели, протоколы, устройства» в нём — строка, а не объяснение.
/// Разбор раскрывает каждый пункт отдельно: что он делает, кто в нём участвует
/// и как это видно на практике.
pub async fn deep(app: &AppHandle, course_id: &str, topic_id: &str, at: usize) -> Result<String, String> {
    let course = learning::course(course_id)?;
    let topic = learning::topic(&course, topic_id)?;
    let parts = sections(&topic.lesson);
    let at = at.min(parts.len() - 1);
    let key = format!("{course_id}/{topic_id}#{at}");
    let source = fingerprint(&parts[at]);
    if let Some(entry) = load_deep(app).sections.get(&key).filter(|entry| entry.source == source) {
        return Ok(entry.text.clone());
    }

    let (material, name) = section_material(&course, topic, Some(at));
    let rules = format!(
        "Ты — репетитор {}. Разбери раздел урока подробно — для человека, который видит \
         тему впервые и хочет её понять, а не выучить слова.\n\n{material}\n\
         Как писать:\n\
         - Каждую мысль раздела раскрой: что это, как устроено внутри — кто что делает и \
         в каком порядке, — зачем оно нужно и что сломается без него.\n\
         - Перечисление (уровни, этапы, компоненты, команды) разбирай по пунктам: у каждого \
         своё назначение, кто или что в нём работает, пример из практики.\n\
         - Пример — живой: команда с выводом, случай на сервере, бытовая аналогия.\n\
         - В конце — «Где ошибаются»: две-три типичные ошибки или путаницы.\n\
         - Не пересказывай раздел теми же словами — объясняй глубже.\n\
         - Markdown: подзаголовки ###, списки, `команды`, блоки кода. 400–900 слов.\n\
         - Без вступлений и без заключения «надеюсь, стало понятно».\n\
         Пиши по-русски.",
        app.state::<crate::state::AppState>().wake_name()
    );
    let (provider, limit) = {
        let state = app.state::<crate::state::AppState>();
        let limit = state.config().ai.call_limit() * 3;
        (state.provider(), limit)
    };
    log::info!("подробный разбор раздела «{name}»");
    let asked = tokio::time::timeout(limit, provider.converse(&rules, &[], &format!("Разбери раздел «{name}»."), true)).await;
    let text = match asked {
        Ok(Ok(text)) if text.trim().chars().count() > 200 => text.trim().to_string(),
        Ok(Ok(_)) => return Err("Модель ответила слишком коротко — попробуйте ещё раз.".into()),
        Ok(Err(err)) => return Err(format!("Разбор не получился: {}", err.user_text("модель не ответила"))),
        Err(_) => return Err("Модель не успела — попробуйте ещё раз.".into()),
    };

    let mut store = load_deep(app);
    store.sections.insert(key, DeepEntry { source, text: text.clone() });
    if let Some(path) = deep_path(app) {
        match serde_json::to_string_pretty(&store) {
            Ok(json) => {
                if let Err(err) = std::fs::write(&path, json) {
                    log::warn!("разбор раздела не сохранился: {err}");
                }
            }
            Err(err) => log::warn!("разбор раздела не сложился в JSON: {err}"),
        }
    }
    Ok(text)
}

/// Готовый разбор раздела, если он уже есть, — без обращения к модели.
pub fn deep_cached(app: &AppHandle, course_id: &str, topic_id: &str, at: usize) -> Option<String> {
    let course = learning::course(course_id).ok()?;
    let topic = learning::topic(&course, topic_id).ok()?;
    let parts = sections(&topic.lesson);
    let at = at.min(parts.len() - 1);
    let source = fingerprint(&parts[at]);
    load_deep(app)
        .sections
        .get(&format!("{course_id}/{topic_id}#{at}"))
        .filter(|entry| entry.source == source)
        .map(|entry| entry.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_split_like_the_window() {
        let lesson = "# Тема\n\nВступление.\n\n## Первый\n\nТекст.\n\n## Второй\n\nЕщё.";
        let parts = sections(lesson);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("Вступление") && parts[0].contains("## Первый"));
        assert_eq!(heading(&parts[1]).as_deref(), Some("Второй"));
    }

    #[test]
    fn lesson_without_sections_is_one_part() {
        assert_eq!(sections("Просто текст").len(), 1);
    }
}
