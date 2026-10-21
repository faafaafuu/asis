//! Путеводители — второй вид материала в обучении, рядом с курсами. Курс
//! учит: уроки, задачи, экзамены. Путеводитель рассказывает: «Деревья Москвы»,
//! «Как построить дом самому», «Всё про умный дом» — разделы, особенности,
//! виды, цифры, советы и картинки. Его листают с интересом, а не сдают.
//!
//! Путеводители живут в аккаунте и собираются на сервере
//! (platform/server/guides.mjs): так их видят программа, телефон и Ноа
//! онлайн, а сборка не зависит от канала устройства. Здесь — копия на диске:
//! открытый однажды путеводитель с картинками читается и без сети.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

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
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
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

/// Запись копии на диск — по одному: слежение и чтение не перебивают друг друга.
static DISK: Mutex<()> = Mutex::new(());
/// Слежение за сборкой на сервере — одно на программу.
static WATCHING: AtomicBool = AtomicBool::new(false);

/* ── Копия на диске ────────────────────────────────────────────────────── */

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

fn valid_file(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 48
        && !file.contains("..")
        && file.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
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

fn read_copy(app: &AppHandle, id: &str) -> Result<Guide, String> {
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    let text = std::fs::read_to_string(file_of(app, id)?).map_err(|_| "Нет такого путеводителя.".to_string())?;
    serde_json::from_str(&text).map_err(|err| format!("путеводитель не читается: {err}"))
}

fn write_copy(app: &AppHandle, guide: &Guide) -> Result<(), String> {
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    let path = file_of(app, &guide.id)?;
    let text = serde_json::to_string_pretty(guide).map_err(|err| err.to_string())?;
    // Через временный файл: оборванная запись не портит копию.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(|err| format!("путеводитель не сохранился: {err}"))?;
    std::fs::rename(&temp, &path).map_err(|err| format!("путеводитель не сохранился: {err}"))?;
    Ok(())
}

fn drop_copy(app: &AppHandle, id: &str) {
    let _disk = DISK.lock().unwrap_or_else(|err| err.into_inner());
    if let Ok(path) = file_of(app, id) {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(dir) = pictures_of(app, id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Копии на диске — карточками: без сети полка показывает их.
fn copies(app: &AppHandle) -> Vec<Card> {
    let Ok(dir) = root(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut cards: Vec<Card> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let guide = read_copy(app, name.strip_suffix(".json")?).ok()?;
            Some(card_of(guide))
        })
        .collect();
    cards.sort_by(|a, b| b.created.cmp(&a.created));
    cards
}

fn card_of(guide: Guide) -> Card {
    Card {
        written: guide.sections.iter().filter(|s| !s.text.is_empty() || !s.items.is_empty()).count(),
        sections: guide.sections.len(),
        // Без сети недописанный не «пишется»: сборку на сервере отсюда не видно.
        status: if guide.status == "building" { "stopped".into() } else { guide.status },
        id: guide.id,
        title: guide.title,
        subtitle: guide.subtitle,
        emoji: guide.emoji,
        cover: guide.cover,
        message: guide.message,
        created: guide.created,
    }
}

fn changed(app: &AppHandle, id: &str) {
    let _ = app.emit("guides:changed", id);
}

/* ── Аккаунт ───────────────────────────────────────────────────────────── */

/// Путеводители аккаунта, новые первыми; без сети — копии на диске.
pub async fn list(app: &AppHandle) -> Vec<Card> {
    let cards = match crate::platform::guide_cards().await {
        Ok(value) => serde_json::from_value::<Vec<Card>>(value).unwrap_or_default(),
        Err(err) => {
            log::debug!("путеводители аккаунта не пришли ({err}) — показываю копии");
            return copies(app);
        }
    };
    if !migrated(app) {
        migrate(app, &cards).await;
    } else {
        // Удалённые на другом устройстве — убрать и копию.
        for copy in copies(app) {
            if !cards.iter().any(|card| card.id == copy.id) {
                drop_copy(app, &copy.id);
            }
        }
    }
    if cards.iter().any(|card| card.status == "building") {
        watch(app);
    }
    cards
}

/// Путеводитель с сервера — и в копию; без сети — копия.
pub async fn get(app: &AppHandle, id: &str) -> Result<Guide, String> {
    if !valid_id(id) {
        return Err("Нет такого путеводителя.".into());
    }
    let fresh = crate::platform::guide_get(id)
        .await
        .and_then(|value| serde_json::from_value::<Guide>(value).map_err(|err| err.to_string()));
    match fresh {
        Ok(guide) => {
            let _ = write_copy(app, &guide);
            if guide.status == "building" {
                watch(app);
            }
            Ok(guide)
        }
        Err(err) => read_copy(app, id).map_err(|_| err),
    }
}

/// Начать путеводитель — он пишется на сервере, окно слышит `guides:changed`.
pub async fn start(app: &AppHandle, goal: &str) -> Result<String, String> {
    let goal = goal.trim();
    if goal.chars().count() < 3 {
        return Err("Напишите, о чём путеводитель: например, «деревья Москвы» или «умный дом».".into());
    }
    let id = crate::platform::guide_start(goal).await?;
    changed(app, &id);
    watch(app);
    Ok(id)
}

/// Дописать прерванный путеводитель или добавить раздел по просьбе.
pub async fn resume(app: &AppHandle, id: &str, wish: Option<String>) -> Result<(), String> {
    let wish = wish.as_deref().map(str::trim).filter(|w| !w.is_empty());
    crate::platform::guide_resume(id, wish).await?;
    changed(app, id);
    watch(app);
    Ok(())
}

/// Остановить сборку: готовые разделы остаются, дописать можно потом.
pub async fn stop() -> Result<(), String> {
    crate::platform::guide_stop().await
}

pub async fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    crate::platform::guide_remove(id).await?;
    drop_copy(app, id);
    changed(app, id);
    Ok(())
}

/// Картинка путеводителя строкой data: — из копии, а нет — с сервера в копию.
pub async fn picture(app: &AppHandle, id: &str, file: &str) -> Result<String, String> {
    if !valid_file(file) {
        return Err("Нет такой картинки.".into());
    }
    let path = pictures_of(app, id)?.join(file);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        _ => {
            let bytes = crate::platform::guide_picture(id, file).await?;
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&path, &bytes);
            bytes
        }
    };
    let kind = match file.rsplit('.').next().unwrap_or_default() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    };
    Ok(format!("data:{kind};base64,{}", crate::practice::base64(&bytes)))
}

/// Отметка карточки для слежения: что поменялось — о том и сообщить.
fn mark(card: &Card) -> String {
    format!("{}:{}:{}:{};", card.id, card.status, card.written, card.sections)
}

/// Пока на сервере что-то пишется — раз в несколько секунд сверяться и
/// сообщать окнам о новых разделах.
fn watch(app: &AppHandle) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut seen = String::new();
        let mut misses = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let Ok(value) = crate::platform::guide_cards().await else {
                misses += 1;
                if misses > 30 {
                    break;
                }
                continue;
            };
            let cards: Vec<Card> = serde_json::from_value(value).unwrap_or_default();
            for card in &cards {
                if !seen.contains(&mark(card)) {
                    changed(&app, &card.id);
                }
            }
            seen = cards.iter().map(mark).collect();
            if !cards.iter().any(|card| card.status == "building") {
                break;
            }
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/* ── Переезд в аккаунт ─────────────────────────────────────────────────── */

fn migrated(app: &AppHandle) -> bool {
    root(app).map(|dir| dir.join(".account").exists()).unwrap_or(true)
}

/// Путеводители, собранные на этом устройстве до переезда на сервер, — в
/// аккаунт, один раз. Не вышло — попробуем при следующем списке.
async fn migrate(app: &AppHandle, account: &[Card]) {
    let mut ok = true;
    for copy in copies(app) {
        if account.iter().any(|card| card.id == copy.id) {
            continue;
        }
        let Ok(guide) = read_copy(app, &copy.id) else { continue };
        let Ok(text) = serde_json::to_string(&guide) else { continue };
        match crate::platform::guide_upload(&guide.id, &text).await {
            Ok(()) => log::info!("путеводитель «{}» перенесён в аккаунт", guide.title),
            Err(err) => {
                log::warn!("путеводитель «{}» не перенёсся в аккаунт: {err}", guide.title);
                ok = false;
            }
        }
    }
    if ok {
        if let Ok(dir) = root(app) {
            let _ = std::fs::write(dir.join(".account"), "путеводители этого устройства перенесены в аккаунт");
        }
        changed(app, "");
    }
}

/* ── Голос ─────────────────────────────────────────────────────────────── */

/// Что показать, когда окно обучения откроется: путеводитель (id) или полку
/// (пустая строка). Голос открывает окно, а оно спрашивает при загрузке.
static WANTED: Mutex<Option<String>> = Mutex::new(None);

pub fn want(id: Option<String>) {
    *WANTED.lock().unwrap_or_else(|err| err.into_inner()) = Some(id.unwrap_or_default());
}

pub fn take_wanted() -> Option<String> {
    WANTED.lock().unwrap_or_else(|err| err.into_inner()).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_files_cannot_escape_the_folder() {
        assert!(valid_id("g1734567890"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
        assert!(valid_file("s1-2.jpg"));
        assert!(!valid_file("../a.jpg"));
        assert!(!valid_file("a/b.jpg"));
    }

    #[test]
    fn server_cards_parse() {
        let cards: Vec<Card> = serde_json::from_value(serde_json::json!([
            {"id": "g1", "title": "Деревья", "subtitle": "", "emoji": "🌳", "cover": {"file": "cover.jpg", "source": "", "caption": "Липа"},
             "sections": 6, "written": 2, "status": "building", "message": "", "created": 5}
        ]))
        .expect("карточки");
        assert_eq!(cards[0].written, 2);
        assert_eq!(cards[0].cover.as_ref().map(|c| c.file.as_str()), Some("cover.jpg"));
    }

    #[test]
    fn a_server_guide_parses() {
        let guide: Guide = serde_json::from_value(serde_json::json!({
            "id": "g1", "title": "Деревья", "goal": "деревья", "coverHint": "Липа", "status": "ready", "created": 1,
            "sections": [{"id": "s1", "title": "Липы", "imageHint": "Липа", "text": ["a"],
                          "items": [{"name": "Липа", "text": "b", "tags": ["x"], "imageHint": "Липа", "picture": null}]}]
        }))
        .expect("путеводитель");
        assert_eq!(guide.cover_hint, "Липа");
        assert_eq!(guide.sections[0].items[0].image_hint, "Липа");
    }
}
