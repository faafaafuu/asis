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
///
/// На iPhone говорит голос Ноа — тот же Silero, что на компьютере, с сервера
/// по фразам (`/api/noa/tts`): системный синтезатор звучал как робот. Пока
/// звучит фраза, следующая уже качается. Нет сети или входа — говорит
/// лучший установленный русский голос системы.
pub async fn speak(_app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    let request = READ_REQUEST.load(Ordering::SeqCst);
    SPEAKING.store(true, Ordering::SeqCst);
    let result = if cfg!(target_os = "ios") {
        match noa_voice(config, &text, request).await {
            Ok(()) => Ok(()),
            Err(rest) if rest.is_empty() => Ok(()),
            Err(rest) => system_speak(config, &rest).await,
        }
    } else {
        system_speak(config, &text).await
    };
    SPEAKING.store(false, Ordering::SeqCst);
    result
}

async fn system_speak(config: &VoiceConfig, text: &str) -> Result<(), String> {
    crate::mobile::call_async::<serde_json::Value>("speak", serde_json::json!({ "text": text, "rate": config.rate }))
        .await
        .map(|_| ())
}

/// Фразы по 1–2 предложения: короткий ответ сервера приходит быстро и не
/// рвётся каналом, а первая фраза звучит почти сразу.
fn phrases(text: &str, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for sentence in text.split_inclusive(['.', '!', '?', '…', '\n']) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }
        if !current.is_empty() && current.chars().count() + sentence.chars().count() > limit {
            out.push(std::mem::take(&mut current));
        }
        // Длинное предложение — по запятым и пробелам.
        if sentence.chars().count() > limit {
            for word in sentence.split_inclusive([',', ';', ' ']) {
                if !current.is_empty() && current.chars().count() + word.chars().count() > limit {
                    out.push(std::mem::take(&mut current).trim().to_string());
                }
                current.push_str(word);
            }
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(sentence);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Фраза голосом Ноа — MP3 с сервера.
async fn fetch_phrase(base: String, token: String, voice: String, phrase: String) -> Result<Vec<u8>, String> {
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(format!("{base}/api/noa/tts"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "text": phrase, "voice": voice, "lite": true }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("голос ответил {}", response.status()));
    }
    response.bytes().await.map(|bytes| bytes.to_vec()).map_err(|err| err.to_string())
}

/// Голосом Ноа. Ошибка — текст, который осталось сказать (пустой — перебили).
async fn noa_voice(config: &VoiceConfig, text: &str, request: u64) -> Result<(), String> {
    let (base, token) = crate::platform::settings();
    if token.is_empty() {
        return Err(text.to_string());
    }
    let voice = if config.silero_voice.trim().is_empty() { "xenia".to_string() } else { config.silero_voice.clone() };
    let list = phrases(text, 180);
    let start = |phrase: &String| tauri::async_runtime::spawn(fetch_phrase(base.clone(), token.clone(), voice.clone(), phrase.clone()));
    let mut next = list.first().map(start);
    for (at, _) in list.iter().enumerate() {
        let Some(task) = next.take() else { break };
        let audio = match task.await.map_err(|err| err.to_string()).and_then(|got| got) {
            Ok(audio) => audio,
            Err(err) => {
                log::warn!("голос Ноа не пришёл ({err}) — говорит системный");
                return Err(list[at..].join(" "));
            }
        };
        if let Some(following) = list.get(at + 1) {
            next = Some(start(following));
        }
        if READ_REQUEST.load(Ordering::SeqCst) != request {
            return Err(String::new());
        }
        let path = std::env::temp_dir().join(format!("noa-phrase-{}.mp3", at % 2));
        std::fs::write(&path, &audio).map_err(|err| err.to_string())?;
        let played = crate::mobile::call_async::<serde_json::Value>(
            "playAudio",
            serde_json::json!({ "path": path.to_string_lossy() }),
        )
        .await;
        if let Err(err) = played {
            log::warn!("фраза не проигралась ({err}) — говорит системный");
            return Err(list[at..].join(" "));
        }
        if READ_REQUEST.load(Ordering::SeqCst) != request {
            return Err(String::new());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::phrases;

    #[test]
    fn phrases_split_by_sentences_and_length() {
        let list = phrases("Первое. Второе предложение! Третье?", 20);
        assert_eq!(list, ["Первое.", "Второе предложение!", "Третье?"]);
        let long = "слово ".repeat(50);
        assert!(phrases(&long, 40).iter().all(|p| p.chars().count() <= 46));
    }
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
