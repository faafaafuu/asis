//! Путеводители — второй вид материала в обучении, рядом с курсами. Курс
//! учит: уроки, задачи, экзамены. Путеводитель рассказывает: «Деревья Москвы»,
//! «Как построить дом самому», «Всё про умный дом» — разделы, особенности,
//! виды, цифры, советы и картинки. Его листают с интересом, а не сдают.
//!
//! Собирает модель человека, раздел за разделом: сначала план, потом каждый
//! раздел своим вызовом. Путеводитель сохраняется после каждого раздела —
//! читать можно, пока он дописывается. Картинки — из Википедии: модель
//! называет статью, программа берёт её иллюстрацию и хранит рядом с текстом,
//! так что путеводитель открывается и без сети.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Сколько разделов в плане — не меньше и не больше.
const MIN_SECTIONS: usize = 4;
const MAX_SECTIONS: usize = 10;
/// Картинок в разделе со списком видов — не больше стольких.
const MAX_ITEM_PICTURES: usize = 8;
/// Картинка больше этого — не путеводителю.
const MAX_PICTURE: usize = 4 * 1024 * 1024;
/// Ширины — из стандартного ряда Wikimedia: другие она отвергает (400).
const WIDE: u64 = 960;
const NARROW: u64 = 500;
/// Википедия отвечает 403 без внятного User-Agent — её правило для программ.
const USER_AGENT: &str = "NOAH/1.0 (https://noahlab.ru; guides)";

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Guide {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub emoji: String,
    /// О чём просил человек — по этому дописываются разделы.
    pub goal: String,
    pub intro: String,
    /// Статья Википедии для обложки.
    pub cover_hint: String,
    pub cover: Option<Picture>,
    pub sections: Vec<Section>,
    /// building — дописывается, ready — готов, stopped — сборку прервали,
    /// failed — не сложился даже план.
    pub status: String,
    pub message: String,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub emoji: String,
    /// Что в разделе — из плана; по нему раздел и пишется.
    pub about: String,
    pub lead: String,
    pub text: Vec<String>,
    pub facts: Vec<Fact>,
    pub items: Vec<Item>,
    pub tips: Vec<String>,
    /// Интересный факт — «а вы знали».
    pub wow: String,
    pub image_hint: String,
    pub picture: Option<Picture>,
}

impl Section {
    fn written(&self) -> bool {
        !self.text.is_empty() || !self.items.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    pub name: String,
    pub text: String,
    pub tags: Vec<String>,
    pub image_hint: String,
    pub picture: Option<Picture>,
}

/// Картинка из Википедии: файл рядом с путеводителем и откуда она.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct Picture {
    pub file: String,
    /// Статья, из которой картинка: подпись и ссылка «Фото: Википедия».
    pub source: String,
    pub caption: String,
}

/// Карточка для лобби — без текста разделов.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub emoji: String,
    pub cover: Option<Picture>,
    pub sections: usize,
    pub written: usize,
    pub status: String,
    pub message: String,
    pub created: u64,
}

/// Какой путеводитель сейчас пишется: одна сборка за раз.
static RUNNING: Mutex<Option<String>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);
/// Запись файлов — по одному: сборка и чтение не перебивают друг друга.
static DISK: Mutex<()> = Mutex::new(());

/* ── Хранение ──────────────────────────────────────────────────────────── */

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("папка программы недоступна: {err}"))?
        .join("guides");
    std::fs::create_dir_all(&dir).map_err(|err| format!("папка путеводителей не создалась: {err}"))?;
    Ok(dir)
}

/// id — только буквы, цифры и дефис: из него собираются пути.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn file_of(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Нет такого путеводителя.".into());
    }
    Ok(root(app)?.join(format!("{id}.json")))
}

fn pictures_of(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Нет такого путеводителя.".into());
    }
    Ok(root(app)?.join(id))
}

fn read(app: &AppHandle, id: &str) -> Result<Guide, String> {
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    let text = std::fs::read_to_string(file_of(app, id)?).map_err(|_| "Нет такого путеводителя.".to_string())?;
    let mut guide: Guide = serde_json::from_str(&text).map_err(|err| format!("путеводитель не читается: {err}"))?;
    // Сборку прервал выход из программы — путеводитель не «пишется» вечно.
    if guide.status == "building" && !running(&guide.id) {
        guide.status = "stopped".into();
    }
    Ok(guide)
}

fn write(app: &AppHandle, guide: &Guide) -> Result<(), String> {
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    let path = file_of(app, &guide.id)?;
    let text = serde_json::to_string_pretty(guide).map_err(|err| err.to_string())?;
    // Через временный файл: оборванная запись не портит путеводитель.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(|err| format!("путеводитель не сохранился: {err}"))?;
    std::fs::rename(&temp, &path).map_err(|err| format!("путеводитель не сохранился: {err}"))?;
    Ok(())
}

fn changed(app: &AppHandle, id: &str) {
    let _ = app.emit("guides:changed", id);
}

fn running(id: &str) -> bool {
    RUNNING.lock().unwrap_or_else(|err| err.into_inner()).as_deref() == Some(id)
}

/// Все путеводители, новые первыми.
pub fn list(app: &AppHandle) -> Vec<Card> {
    let Ok(dir) = root(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut cards: Vec<Card> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".json")?.to_string();
            let guide = read(app, &id).ok()?;
            Some(Card {
                written: guide.sections.iter().filter(|s| s.written()).count(),
                sections: guide.sections.len(),
                id: guide.id,
                title: guide.title,
                subtitle: guide.subtitle,
                emoji: guide.emoji,
                cover: guide.cover,
                status: guide.status,
                message: guide.message,
                created: guide.created,
            })
        })
        .collect();
    cards.sort_by(|a, b| b.created.cmp(&a.created));
    cards
}

pub fn get(app: &AppHandle, id: &str) -> Result<Guide, String> {
    read(app, id)
}

pub fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    if running(id) {
        STOP.store(true, Ordering::SeqCst);
    }
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    let path = file_of(app, id)?;
    std::fs::remove_file(&path).map_err(|_| "Нет такого путеводителя.".to_string())?;
    let _ = std::fs::remove_dir_all(pictures_of(app, id)?);
    drop(_disk);
    changed(app, id);
    Ok(())
}

/// Картинка путеводителя — строкой data: окно не ходит в сеть и на диск само.
pub fn picture(app: &AppHandle, id: &str, file: &str) -> Result<String, String> {
    if file.is_empty() || file.contains(['/', '\\']) || file.contains("..") {
        return Err("Нет такой картинки.".into());
    }
    let bytes = std::fs::read(pictures_of(app, id)?.join(file)).map_err(|_| "Нет такой картинки.".to_string())?;
    let kind = match file.rsplit('.').next().unwrap_or_default() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    };
    Ok(format!("data:{kind};base64,{}", crate::practice::base64(&bytes)))
}

/// Что показать, когда окно обучения откроется: путеводитель (id) или полку
/// (пустая строка). Голос открывает окно, а оно спрашивает при загрузке.
static WANTED: Mutex<Option<String>> = Mutex::new(None);

pub fn want(id: Option<String>) {
    *WANTED.lock().unwrap_or_else(|err| err.into_inner()) = Some(id.unwrap_or_default());
}

pub fn take_wanted() -> Option<String> {
    WANTED.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Остановить сборку: готовые разделы остаются, дописать можно потом.
pub fn stop() {
    if RUNNING.lock().unwrap_or_else(|err| err.into_inner()).is_some() {
        STOP.store(true, Ordering::SeqCst);
    }
}

/* ── Сборка ────────────────────────────────────────────────────────────── */

/// Начать новый путеводитель по просьбе человека. Отдаёт его id сразу —
/// дальше он пишется сам, окно слышит `guides:changed`.
pub fn start(app: &AppHandle, goal: &str) -> Result<String, String> {
    let goal = goal.trim();
    if goal.chars().count() < 3 {
        return Err("Напишите, о чём путеводитель: например, «деревья Москвы» или «умный дом».".into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let guide = Guide {
        id: format!("g{now}"),
        title: shorten(goal, 60),
        goal: goal.to_string(),
        emoji: "📖".into(),
        status: "building".into(),
        message: "Составляю план".into(),
        created: now,
        ..Guide::default()
    };
    claim(&guide.id)?;
    if let Err(err) = write(app, &guide) {
        release();
        return Err(err);
    }
    changed(app, &guide.id);
    run(app, guide.id.clone(), None);
    Ok(guide.id)
}

/// Дописать прерванный путеводитель или добавить раздел по просьбе.
pub fn resume(app: &AppHandle, id: &str, wish: Option<String>) -> Result<(), String> {
    let mut guide = read(app, id)?;
    claim(id)?;
    if let Some(wish) = wish.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
        guide.sections.push(Section {
            id: format!("s{}", guide.sections.len() + 1),
            title: shorten(wish, 40),
            emoji: "✨".into(),
            about: wish.to_string(),
            ..Section::default()
        });
    }
    guide.status = "building".into();
    guide.message = String::new();
    if let Err(err) = write(app, &guide) {
        release();
        return Err(err);
    }
    changed(app, id);
    run(app, id.to_string(), wish);
    Ok(())
}

fn claim(id: &str) -> Result<(), String> {
    let mut running = RUNNING.lock().unwrap_or_else(|err| err.into_inner());
    if running.is_some() {
        return Err("Уже пишу другой путеводитель — дождитесь его или остановите.".into());
    }
    *running = Some(id.to_string());
    STOP.store(false, Ordering::SeqCst);
    Ok(())
}

fn release() {
    *RUNNING.lock().unwrap_or_else(|err| err.into_inner()) = None;
    STOP.store(false, Ordering::SeqCst);
}

fn run(app: &AppHandle, id: String, wish: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = build(&app, &id, wish.is_some()).await;
        release();
        // Путеводитель могли удалить посреди сборки — тогда и записывать нечего.
        if let Ok(mut guide) = read(&app, &id) {
            match outcome {
                Ok(()) => {
                    guide.status = "ready".into();
                    guide.message = String::new();
                }
                Err(err) => {
                    log::warn!("путеводитель «{}»: {err}", guide.title);
                    guide.status = if guide.sections.is_empty() { "failed" } else { "stopped" }.into();
                    guide.message = err;
                }
            }
            let _ = write(&app, &guide);
        }
        changed(&app, &id);
    });
}

async fn build(app: &AppHandle, id: &str, added: bool) -> Result<(), String> {
    let mut guide = read(app, id)?;
    if guide.sections.is_empty() {
        plan(app, &mut guide).await?;
        guide.message = String::new();
        guide.status = "building".into();
        write(app, &guide)?;
        changed(app, id);
        if let Some(picture) = fetch_picture(app, id, &guide.cover_hint, "cover", WIDE).await {
            guide.cover = Some(picture);
            write(app, &guide)?;
            changed(app, id);
        }
    }
    let total = guide.sections.len();
    for at in 0..total {
        if guide.sections[at].written() {
            continue;
        }
        if STOP.load(Ordering::SeqCst) {
            return Err("Сборку остановили — готовые разделы на месте.".into());
        }
        let mut section = write_section(app, &guide, at, added && at + 1 == total).await?;
        let base = format!("s{}", at + 1);
        if let Some(picture) = fetch_picture(app, id, &section.image_hint, &base, WIDE).await {
            section.picture = Some(picture);
        }
        let mut pictured = 0;
        for (n, item) in section.items.iter_mut().enumerate() {
            if pictured >= MAX_ITEM_PICTURES || STOP.load(Ordering::SeqCst) {
                break;
            }
            if let Some(picture) = fetch_picture(app, id, &item.image_hint, &format!("{base}-{}", n + 1), NARROW).await {
                item.picture = Some(picture);
                pictured += 1;
            }
        }
        // Пока писался раздел, путеводитель могли удалить.
        let mut fresh = read(app, id)?;
        if let Some(slot) = fresh.sections.get_mut(at) {
            *slot = section;
        }
        fresh.status = "building".into();
        write(app, &fresh)?;
        changed(app, id);
        guide = fresh;
    }
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Plan {
    title: String,
    subtitle: String,
    emoji: String,
    intro: String,
    cover: String,
    sections: Vec<PlanSection>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PlanSection {
    title: String,
    emoji: String,
    about: String,
}

async fn plan(app: &AppHandle, guide: &mut Guide) -> Result<(), String> {
    let rules = format!(
        "Ты — {}, автор увлекательных путеводителей. Человек просит путеводитель — составь его план.\n\
         Путеводитель — не курс: без задач и экзаменов. Его листают с интересом, как хороший \
         научно-популярный журнал или энциклопедию для взрослых: разделы, особенности, виды, \
         сравнения, цифры, советы, картинки. Не сухо, но и без воды.\n\
         Ответь строго JSON, без текста вокруг:\n\
         {{\"title\": \"…\", \"subtitle\": \"…\", \"emoji\": \"…\", \"intro\": \"…\", \"cover\": \"…\", \
         \"sections\": [{{\"title\": \"…\", \"emoji\": \"…\", \"about\": \"…\"}}]}}\n\
         - title — название путеводителя, 2–6 слов; subtitle — о чём он, до 12 слов.\n\
         - emoji — один значок темы.\n\
         - intro — 2–3 предложения, которые хочется дочитать: чем тема интересна и что человек узнает.\n\
         - cover — точное название статьи русской Википедии, чья иллюстрация подойдёт для обложки.\n\
         - sections — {MIN_SECTIONS}–8 разделов, от общего к частному: title — 2–5 слов, emoji — \
         один значок, about — что в разделе, до 25 слов. Если тема — набор видов (деревья, породы, \
         устройства), отведи им разделы по группам. Последний раздел может быть практическим: с чего \
         начать, где посмотреть, сколько стоит.\n\
         Пиши по-русски.",
        name(app)
    );
    let text = model(app, &rules, &format!("Путеводитель: {}", guide.goal)).await?;
    let plan: Plan = parse(&text).ok_or("План путеводителя не сложился — попробуйте ещё раз или опишите тему иначе.")?;
    let mut sections: Vec<Section> = plan
        .sections
        .into_iter()
        .filter(|s| !s.title.trim().is_empty())
        .take(MAX_SECTIONS)
        .enumerate()
        .map(|(n, s)| Section {
            id: format!("s{}", n + 1),
            title: s.title.trim().to_string(),
            emoji: s.emoji.trim().to_string(),
            about: s.about.trim().to_string(),
            ..Section::default()
        })
        .collect();
    if sections.len() < 2 {
        return Err("В плане путеводителя нет разделов — попробуйте ещё раз.".into());
    }
    for section in &mut sections {
        if section.emoji.is_empty() {
            section.emoji = "•".into();
        }
    }
    if !plan.title.trim().is_empty() {
        guide.title = plan.title.trim().to_string();
    }
    guide.subtitle = plan.subtitle.trim().to_string();
    if !plan.emoji.trim().is_empty() {
        guide.emoji = plan.emoji.trim().to_string();
    }
    guide.intro = plan.intro.trim().to_string();
    guide.cover_hint = plan.cover.trim().to_string();
    guide.sections = sections;
    log::info!("путеводитель «{}»: план из {} разделов", guide.title, guide.sections.len());
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Written {
    title: String,
    emoji: String,
    lead: String,
    image: String,
    text: Vec<String>,
    facts: Vec<Fact>,
    items: Vec<WrittenItem>,
    tips: Vec<String>,
    wow: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WrittenItem {
    name: String,
    text: String,
    tags: Vec<String>,
    image: String,
}

async fn write_section(app: &AppHandle, guide: &Guide, at: usize, added: bool) -> Result<Section, String> {
    let section = &guide.sections[at];
    let others = guide
        .sections
        .iter()
        .enumerate()
        .filter(|(n, _)| *n != at)
        .map(|(_, s)| format!("- {} — {}", s.title, s.about))
        .collect::<Vec<_>>()
        .join("\n");
    let rename = if added {
        "- title и emoji — название раздела (2–5 слов) и значок: человек описал раздел своими словами.\n"
    } else {
        ""
    };
    let rules = format!(
        "Ты — {}, автор путеводителя «{}» ({}). Пишешь один его раздел.\n\
         Другие разделы — их содержание не повторяй:\n{others}\n\n\
         Ответь строго JSON, без текста вокруг:\n\
         {{\"title\": \"…\", \"emoji\": \"…\", \"lead\": \"…\", \"image\": \"…\", \"text\": [\"…\"], \
         \"facts\": [{{\"label\": \"…\", \"value\": \"…\"}}], \
         \"items\": [{{\"name\": \"…\", \"text\": \"…\", \"tags\": [\"…\"], \"image\": \"…\"}}], \
         \"tips\": [\"…\"], \"wow\": \"…\"}}\n\
         {rename}\
         - lead — одна фраза-подводка, до 25 слов.\n\
         - text — 2–4 абзаца по 40–90 слов. Можно выделить главное **жирным**.\n\
         - facts — 0–6 коротких характеристик: цифры, сроки, размеры, цены (label до 4 слов, value \
         до 8). Пусто, если цифр по делу нет.\n\
         - items — 0–8 карточек, если раздел перечисляет виды, породы, устройства, этапы, места: \
         name — название, text — чем примечателен и как узнать, до 35 слов, tags — 1–3 коротких \
         признака, image — точное название статьи русской Википедии с его изображением или \"\".\n\
         - tips — 0–4 практических совета, до 20 слов каждый.\n\
         - wow — один удивительный факт по теме раздела, до 30 слов, или \"\".\n\
         - image — точное название статьи русской Википедии, чья иллюстрация подойдёт разделу.\n\
         Пиши живо и точно, без канцелярита. Не выдумывай факты, цифры и названия: не уверен — не \
         пиши. По-русски.",
        name(app),
        guide.title,
        guide.subtitle,
    );
    let said = format!("Раздел «{}»: {}", section.title, section.about);
    let mut last = String::new();
    // Две попытки: длинный JSON изредка обрывается или ломается.
    for _ in 0..2 {
        let text = model(app, &rules, &said).await?;
        if let Some(written) = parse::<Written>(&text) {
            if written.text.is_empty() && written.items.is_empty() {
                last = "Раздел пришёл пустым.".into();
                continue;
            }
            let mut done = section.clone();
            if added && !written.title.trim().is_empty() {
                done.title = written.title.trim().to_string();
                if !written.emoji.trim().is_empty() {
                    done.emoji = written.emoji.trim().to_string();
                }
            }
            done.lead = written.lead.trim().to_string();
            done.image_hint = written.image.trim().to_string();
            done.text = written.text.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            done.facts = written
                .facts
                .into_iter()
                .filter(|f| !f.label.trim().is_empty() && !f.value.trim().is_empty())
                .take(8)
                .collect();
            done.items = written
                .items
                .into_iter()
                .filter(|i| !i.name.trim().is_empty())
                .take(12)
                .map(|i| Item {
                    name: i.name.trim().to_string(),
                    text: i.text.trim().to_string(),
                    tags: i.tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).take(3).collect(),
                    image_hint: i.image.trim().to_string(),
                    picture: None,
                })
                .collect();
            done.tips = written.tips.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).take(5).collect();
            done.wow = written.wow.trim().to_string();
            log::info!("путеводитель «{}»: раздел «{}» готов", guide.title, done.title);
            return Ok(done);
        }
        last = "Раздел не сложился.".into();
    }
    Err(format!("{last} Попробуйте дописать путеводитель ещё раз."))
}

fn name(app: &AppHandle) -> String {
    app.state::<crate::state::AppState>().wake_name()
}

/// Один длинный вызов модели: потолок — втрое выше обычного, раздел большой.
async fn model(app: &AppHandle, rules: &str, said: &str) -> Result<String, String> {
    let (provider, limit) = {
        let state = app.state::<crate::state::AppState>();
        let limit = state.config().ai.call_limit();
        (state.provider(), limit * 3)
    };
    match tokio::time::timeout(limit, provider.converse(rules, &[], said, true)).await {
        Ok(Ok(text)) if !text.trim().is_empty() => Ok(text),
        Ok(Ok(_)) => Err("Модель прислала пустой ответ.".into()),
        Ok(Err(err)) => Err(format!("Не получилось: {}", err.user_text("модель не ответила"))),
        Err(_) => Err("Модель не успела ответить.".into()),
    }
}

/// JSON из ответа модели — без ```json и текста вокруг, терпимо к слэшам.
fn parse<T: for<'de> Deserialize<'de>>(text: &str) -> Option<T> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    let value = crate::practice::lenient_json(&text[start..=end])?;
    serde_json::from_value(value).ok()
}

fn shorten(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

/* ── Картинки ──────────────────────────────────────────────────────────── */

fn client() -> Option<reqwest::Client> {
    crate::net::client_builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
        .ok()
}

/// Иллюстрация статьи Википедии — скачать и положить рядом с путеводителем.
/// Нет статьи или картинки — путеводитель просто без неё.
async fn fetch_picture(app: &AppHandle, id: &str, hint: &str, base: &str, width: u64) -> Option<Picture> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }
    let client = client()?;
    let page = match summary(&client, hint).await {
        Some(page) => page,
        None => {
            let title = search(&client, hint).await?;
            summary(&client, &title).await?
        }
    };
    let url = image_url(&page, width)?;
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        log::debug!("картинка «{hint}» не скачалась: {}", response.status());
        return None;
    }
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let ext = match kind.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/jpeg" => "jpg",
        _ => return None,
    };
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_PICTURE {
        return None;
    }
    let dir = pictures_of(app, id).ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let file = format!("{base}.{ext}");
    std::fs::write(dir.join(&file), &bytes).ok()?;
    Some(Picture {
        file,
        source: page
            .pointer("/content_urls/desktop/page")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        caption: page.get("title").and_then(|v| v.as_str()).unwrap_or(hint).to_string(),
    })
}

async fn summary(client: &reqwest::Client, title: &str) -> Option<serde_json::Value> {
    let url = format!("https://ru.wikipedia.org/api/rest_v1/page/summary/{}", encode(title));
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let page: serde_json::Value = response.json().await.ok()?;
    // Страница неоднозначности — не та статья, и картинки у неё нет.
    if page.get("type").and_then(|v| v.as_str()) == Some("disambiguation") {
        return None;
    }
    image_url(&page, NARROW).is_some().then_some(page)
}

async fn search(client: &reqwest::Client, text: &str) -> Option<String> {
    let url = format!(
        "https://ru.wikipedia.org/w/api.php?action=query&list=search&srlimit=1&format=json&srsearch={}",
        encode(text)
    );
    let value: serde_json::Value = client.get(&url).send().await.ok()?.json().await.ok()?;
    value.pointer("/query/search/0/title")?.as_str().map(str::to_string)
}

/// Картинка шириной `width`: уменьшенная копия Википедии, а если оригинал
/// меньше — он сам.
fn image_url(page: &serde_json::Value, width: u64) -> Option<String> {
    let original = page.get("originalimage");
    let original_width = original.and_then(|o| o.get("width")).and_then(|w| w.as_u64()).unwrap_or(0);
    let thumb = page.pointer("/thumbnail/source").and_then(|v| v.as_str());
    if let Some(thumb) = thumb {
        if original_width > width {
            if let Some(sized) = resized(thumb, width) {
                return Some(sized);
            }
        }
    }
    let original = original.and_then(|o| o.get("source")).and_then(|v| v.as_str());
    // Оригинал SVG окно покажет, но Википедия отдаёт его огромным — берём уменьшенную.
    match original {
        Some(src) if !src.ends_with(".svg") && original_width <= width => Some(src.to_string()),
        _ => thumb.map(str::to_string),
    }
}

/// «…/330px-Tilia.jpg?utm…» → «…/960px-Tilia.jpg?utm…».
fn resized(thumb: &str, width: u64) -> Option<String> {
    let slash = thumb.rfind('/')?;
    let (head, name) = thumb.split_at(slash + 1);
    let px = name.find("px-")?;
    if !name[..px].chars().all(|c| c.is_ascii_digit()) || px == 0 {
        return None;
    }
    Some(format!("{head}{width}{}", &name[px..]))
}

fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.trim().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            b' ' => out.push('_'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_are_resized() {
        assert_eq!(
            resized("https://thumb.wikimedia.org/a/b/Tilia.jpg/330px-Tilia.jpg?utm_source=ru", 960).as_deref(),
            Some("https://thumb.wikimedia.org/a/b/Tilia.jpg/960px-Tilia.jpg?utm_source=ru")
        );
        assert_eq!(resized("https://upload.wikimedia.org/a/b/Tilia.jpg", 960), None);
    }

    #[test]
    fn image_prefers_a_wide_thumbnail() {
        let page = serde_json::json!({
            "thumbnail": {"source": "https://u/x/Oak.jpg/320px-Oak.jpg"},
            "originalimage": {"source": "https://u/x/Oak.jpg", "width": 4000}
        });
        assert_eq!(image_url(&page, WIDE).as_deref(), Some("https://u/x/Oak.jpg/960px-Oak.jpg"));
        let small = serde_json::json!({
            "thumbnail": {"source": "https://u/x/Oak.jpg/320px-Oak.jpg"},
            "originalimage": {"source": "https://u/x/Oak.jpg", "width": 600}
        });
        assert_eq!(image_url(&small, WIDE).as_deref(), Some("https://u/x/Oak.jpg"));
    }

    #[test]
    fn ids_cannot_escape_the_folder() {
        assert!(valid_id("g1734567890"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }

    #[test]
    fn sections_parse_from_a_chatty_answer() {
        let text = "Вот раздел:\n```json\n{\"lead\": \"Липа\", \"text\": [\"Абзац\"], \"items\": \
                    [{\"name\": \"Липа\", \"text\": \"Пахнет\", \"tags\": [\"аллеи\"], \"image\": \"Липа\"}], \
                    \"facts\": [{\"label\": \"Высота\", \"value\": \"до 30 м\"}]}\n```";
        let written: Written = parse(text).expect("разбор");
        assert_eq!(written.items[0].name, "Липа");
        assert_eq!(written.facts[0].value, "до 30 м");
        assert!(written.tips.is_empty());
    }
}
