//! Голос на телефоне: речь и распознавание — системные, через плагин Android.
//!
//! На компьютере Ноа носит свой синтезатор (Piper, Silero, Azure) и свою
//! расшифровку (whisper) — на телефоне ни то ни другое не нужно: у Android
//! есть TextToSpeech и SpeechRecognizer, с русским и без загрузок. Модуль
//! повторяет то подмножество настольного `voice`, которым пользуются команды,
//! поэтому остальной код о телефоне не знает.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::AppHandle;

use crate::config::VoiceConfig;

/// Своих голосов на телефоне нет — говорит системный синтезатор.
pub mod assets {
    use tauri::AppHandle;

    pub const VOICES: &[(&str, &str)] = &[];

    pub fn ready(_app: &AppHandle, _voice: &str) -> bool {
        true
    }

    pub async fn install(_app: AppHandle, _voice: String) -> Result<(), String> {
        Ok(())
    }
}

/// Клавиш на телефоне нет: вместо них кнопки на экране.
pub mod hotkey {
    pub fn esc_went_to_voice(_window_ms: u64) -> bool {
        false
    }
    pub fn space_down() {}
    pub fn space_up() {}
}

/// Микрофон один — телефонный; выбирать не из чего.
pub mod stt {
    pub fn devices() -> Vec<String> {
        vec!["Микрофон телефона".into()]
    }
}

/// Расшифровка — системная, скачивать нечего.
pub mod whisper {
    use tauri::AppHandle;

    pub fn ready(_app: &AppHandle) -> bool {
        true
    }

    pub async fn install(_app: AppHandle) -> Result<(), String> {
        Ok(())
    }
}

/// Говорит ли сейчас Ноа.
static SPEAKING: AtomicBool = AtomicBool::new(false);

/// Номер последней просьбы прочитать — как на компьютере: окно отменяет
/// чтение, которое ещё не началось.
static READ_REQUEST: AtomicU64 = AtomicU64::new(0);

/// Произносит текст и ждёт конца фразы.
pub async fn speak(_app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    let rate = config.rate;
    SPEAKING.store(true, Ordering::SeqCst);
    let result = crate::mobile::call_async::<serde_json::Value>(
        "speak",
        serde_json::json!({ "text": text, "rate": rate }),
    )
    .await
    .map(|_| ());
    SPEAKING.store(false, Ordering::SeqCst);
    result
}

/// Слушает одну фразу и отдаёт её текстом. Пустая строка — ничего не сказали.
pub async fn listen() -> Result<String, String> {
    let heard: serde_json::Value =
        crate::mobile::call_async("listen", serde_json::json!({ "lang": "ru-RU" })).await?;
    Ok(heard
        .get("text")
        .and_then(|text| text.as_str())
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// Дослушать фразу сейчас, не дожидаясь паузы.
pub fn stop_listening() {
    let _ = crate::mobile::call::<serde_json::Value>("stopListening", serde_json::json!({}));
}

pub fn azure_voices() -> &'static [(&'static str, &'static str)] {
    &[]
}

pub fn silero_voices() -> &'static [(&'static str, &'static str)] {
    &[]
}

pub fn silero_ready(_app: &AppHandle) -> bool {
    false
}

pub async fn silero_install(_app: AppHandle) -> Result<(), String> {
    Err("На телефоне говорит системный синтезатор — ставить нечего.".into())
}

pub async fn check_azure(_config: &VoiceConfig) -> Result<(), String> {
    Err("На телефоне говорит системный синтезатор.".into())
}

pub fn read_is_current(request: u64) -> bool {
    READ_REQUEST.load(Ordering::SeqCst) == request
}

pub fn read_done() {}

pub fn speaking() -> bool {
    SPEAKING.load(Ordering::SeqCst)
}

/// Короткий сигнал — будильник, таймер.
pub fn chime() {
    let _ = crate::mobile::call::<serde_json::Value>("chime", serde_json::json!({}));
}

/// Замолчать и перестать слушать.
pub fn stop() {
    READ_REQUEST.fetch_add(1, Ordering::SeqCst);
    let _ = crate::mobile::call::<serde_json::Value>("stopSpeaking", serde_json::json!({}));
    SPEAKING.store(false, Ordering::SeqCst);
}

/// Диктовка — «Надиктовать» в обучении: запись идёт, пока не нажали «Готово».
static DICTATION: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<Result<String, String>>>> =
    std::sync::Mutex::new(None);

pub fn dictate_start() {
    let task = tauri::async_runtime::spawn(listen());
    let mut slot = DICTATION.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(old) = slot.replace(task) {
        old.abort();
    }
}

pub async fn dictate_stop() -> Result<String, String> {
    let task = DICTATION.lock().unwrap_or_else(|err| err.into_inner()).take();
    let Some(task) = task else {
        return Err("Запись не шла.".into());
    };
    tauri::async_runtime::spawn_blocking(stop_listening).await.ok();
    task.await.map_err(|err| err.to_string())?
}
