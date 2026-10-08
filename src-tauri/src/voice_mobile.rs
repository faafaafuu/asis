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
/// На iPhone по умолчанию говорит Ирина (Piper), встроенная в приложение:
/// фраза готова за доли секунды и без сети. Голос Ноа с компьютера (Silero
/// «xenia») — с сервера по фразам, если его выбрали в профиле
/// (`voice.engine = "silero"`): так же звучит, но каждая фраза — секунда-пять
/// на сервере. Не вышел один — говорит другой, не вышли оба — системный.
pub async fn speak(_app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    let request = READ_REQUEST.load(Ordering::SeqCst);
    SPEAKING.store(true, Ordering::SeqCst);
    let result = if cfg!(target_os = "ios") {
        let server_first = config.engine == "silero";
        let first = if server_first { server_voice(config, &text, request).await } else { irina::speak(config, &text, request).await };
        match first {
            Ok(()) => Ok(()),
            Err(rest) if rest.is_empty() => Ok(()),
            Err(rest) => {
                let second = if server_first { irina::speak(config, &rest, request).await } else { server_voice(config, &rest, request).await };
                match second {
                    Ok(()) => Ok(()),
                    Err(left) if left.is_empty() => Ok(()),
                    Err(left) => system_speak(config, &left).await,
                }
            }
        }
    } else {
        system_speak(config, &text).await
    };
    SPEAKING.store(false, Ordering::SeqCst);
    result
}

/// Голос с сервера — если сервер только что не ответил, минуту не пробуем.
async fn server_voice(config: &VoiceConfig, text: &str, request: u64) -> Result<(), String> {
    if server_down() {
        return Err(text.to_string());
    }
    noa_voice(config, text, request).await
}

/// Голос на телефоне: `silero` — голос Ноа с сервера, иное — Ирина в телефоне.
pub fn phone_voice_is_server(config: &VoiceConfig) -> bool {
    config.engine == "silero"
}

/// Загрузить Ирину заранее: первая фраза — без секунды-двух на модель.
pub fn warm_up() {
    irina::warm();
}

/// Голос Ирина (Piper) прямо в телефоне — sherpa-onnx, модель лежит в
/// приложении (assets/noa-voice). Движок один, в своём потоке: модель
/// грузится секунду-две, держать её — сотня мегабайт памяти, поэтому она
/// встаёт при первой фразе и живёт, пока живо приложение.
#[cfg(target_os = "ios")]
mod irina {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};

    use super::{phrases, play, quick_start, READ_REQUEST};
    use crate::config::VoiceConfig;

    struct Job {
        text: String,
        speed: f32,
        path: PathBuf,
        done: mpsc::Sender<Result<PathBuf, String>>,
    }

    static WORKER: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();

    /// Папка голоса внутри приложения: Sufler.app/assets/noa-voice.
    fn voice_dir() -> Option<PathBuf> {
        let app = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let dir = app.join("assets").join("noa-voice");
        dir.is_dir().then_some(dir)
    }

    /// Первый файл с расширением `ext` в папке и подпапках.
    fn find(dir: &std::path::Path, test: &dyn Fn(&std::path::Path) -> bool) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if test(&path) {
                return Some(path);
            }
            if path.is_dir() {
                if let Some(found) = find(&path, test) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn engine() -> Result<sherpa_onnx::OfflineTts, String> {
        let dir = voice_dir().ok_or("голоса в приложении нет")?;
        let model = find(&dir, &|p| p.extension().is_some_and(|ext| ext == "onnx")).ok_or("нет модели .onnx")?;
        let tokens = find(&dir, &|p| p.file_name().is_some_and(|name| name == "tokens.txt")).ok_or("нет tokens.txt")?;
        let data = find(&dir, &|p| p.is_dir() && p.file_name().is_some_and(|name| name == "espeak-ng-data"))
            .ok_or("нет espeak-ng-data")?;
        let config = sherpa_onnx::OfflineTtsConfig {
            model: sherpa_onnx::OfflineTtsModelConfig {
                vits: sherpa_onnx::OfflineTtsVitsModelConfig {
                    model: Some(model.to_string_lossy().into_owned()),
                    tokens: Some(tokens.to_string_lossy().into_owned()),
                    data_dir: Some(data.to_string_lossy().into_owned()),
                    noise_scale: 0.667,
                    noise_scale_w: 0.8,
                    length_scale: 1.0,
                    ..Default::default()
                },
                num_threads: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        sherpa_onnx::OfflineTts::create(&config).ok_or_else(|| "движок голоса не запустился".to_string())
    }

    fn worker() -> mpsc::Sender<Job> {
        WORKER
            .get_or_init(|| {
                let (tx, rx) = mpsc::channel::<Job>();
                let _ = std::thread::Builder::new().name("noa-irina".into()).spawn(move || {
                    let mut tts: Option<sherpa_onnx::OfflineTts> = None;
                    for job in rx {
                        if tts.is_none() {
                            match engine() {
                                Ok(engine) => tts = Some(engine),
                                Err(err) => {
                                    let _ = job.done.send(Err(err));
                                    continue;
                                }
                            }
                        }
                        let Some(engine) = tts.as_ref() else { continue };
                        let options = sherpa_onnx::GenerationConfig { speed: job.speed, ..Default::default() };
                        let result = match engine.generate_with_config(&job.text, &options, None::<fn(&[f32], f32) -> bool>) {
                            Some(audio) if audio.save(&job.path.to_string_lossy()) => Ok(job.path),
                            Some(_) => Err("фраза не записалась".to_string()),
                            None => Err("фраза не озвучилась".to_string()),
                        };
                        let _ = job.done.send(result);
                    }
                });
                Mutex::new(tx)
            })
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    /// Поднять движок заранее, в его потоке: пустая короткая фраза.
    pub fn warm() {
        let _ = synth("Да.".into(), 1.0, 9);
    }

    /// Фраза — в WAV, в потоке движка. Ответ — через канал.
    fn synth(text: String, speed: f32, slot: usize) -> mpsc::Receiver<Result<PathBuf, String>> {
        let (done, wait) = mpsc::channel();
        let path = std::env::temp_dir().join(format!("noa-irina-{slot}.wav"));
        if worker().send(Job { text, speed, path, done: done.clone() }).is_err() {
            let _ = done.send(Err("поток голоса не запустился".into()));
        }
        wait
    }

    async fn ready(wait: mpsc::Receiver<Result<PathBuf, String>>) -> Result<PathBuf, String> {
        tauri::async_runtime::spawn_blocking(move || wait.recv().unwrap_or_else(|_| Err("поток голоса оборвался".into())))
            .await
            .map_err(|err| err.to_string())?
    }

    /// Говорит Ириной. Ошибка — текст, который осталось сказать (пустой — перебили).
    pub async fn speak(config: &VoiceConfig, text: &str, request: u64) -> Result<(), String> {
        let speed = if config.rate > 0.0 { config.rate as f32 } else { 1.0 };
        let list = quick_start(phrases(text, 220));
        let mut next = list.first().map(|first| synth(first.clone(), speed, 0));
        for (at, _) in list.iter().enumerate() {
            let Some(wait) = next.take() else { break };
            let path = match ready(wait).await {
                Ok(path) => path,
                Err(err) => {
                    crate::platform::diag(format!("голос Ирина: {err}"));
                    return Err(list[at..].join(" "));
                }
            };
            if let Some(following) = list.get(at + 1) {
                next = Some(synth(following.clone(), speed, (at + 1) % 2));
            }
            if READ_REQUEST.load(Ordering::SeqCst) != request {
                return Err(String::new());
            }
            if let Err(err) = play(&path).await {
                crate::platform::diag(format!("голос Ирина: фраза не проигралась: {err}"));
                return Err(list[at..].join(" "));
            }
            if READ_REQUEST.load(Ordering::SeqCst) != request {
                return Err(String::new());
            }
        }
        Ok(())
    }
}

/// Не iPhone — встроенного голоса нет.
#[cfg(not(target_os = "ios"))]
mod irina {
    use crate::config::VoiceConfig;

    pub async fn speak(_config: &VoiceConfig, text: &str, _request: u64) -> Result<(), String> {
        Err(text.to_string())
    }

    pub fn warm() {}
}

/// Проиграть файл фразы (MP3 или WAV) — плагин телефона, ждёт конца.
async fn play(path: &std::path::Path) -> Result<(), String> {
    crate::mobile::call_async::<serde_json::Value>("playAudio", serde_json::json!({ "path": path.to_string_lossy() }))
        .await
        .map(|_| ())
}

/// До какого времени голос с сервера не пробовать: не ответил — минуту
/// говорит встроенный, а не ждёт сервер на каждой фразе.
static SERVER_DOWN_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn server_down() -> bool {
    SERVER_DOWN_UNTIL
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .is_some_and(|until| std::time::Instant::now() < until)
}

fn mark_server_down() {
    *SERVER_DOWN_UNTIL.lock().unwrap_or_else(|err| err.into_inner()) =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
}

async fn system_speak(config: &VoiceConfig, text: &str) -> Result<(), String> {
    crate::mobile::call_async::<serde_json::Value>("speak", serde_json::json!({ "text": text, "rate": config.rate }))
        .await
        .map(|_| ())
}

/// Первая фраза — короткая: с неё начинается звук, и чем она короче, тем
/// раньше Ноа заговорит. Длинную первую делим по запятым и пробелам.
fn quick_start(mut list: Vec<String>) -> Vec<String> {
    let Some(first) = list.first().cloned() else { return list };
    if first.chars().count() <= 80 {
        return list;
    }
    let head = phrases(&first, 70);
    list.splice(0..1, head);
    list
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
        .timeout(std::time::Duration::from_secs(8))
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
        crate::platform::diag("голос Ноа: нет ключа площадки — говорит системный");
        return Err(text.to_string());
    }
    let voice = if config.silero_voice.trim().is_empty() { "xenia".to_string() } else { config.silero_voice.clone() };
    let list = quick_start(phrases(text, 180));
    let start = |phrase: &String| tauri::async_runtime::spawn(fetch_phrase(base.clone(), token.clone(), voice.clone(), phrase.clone()));
    let mut next = list.first().map(start);
    for (at, _) in list.iter().enumerate() {
        let Some(task) = next.take() else { break };
        let audio = match task.await.map_err(|err| err.to_string()).and_then(|got| got) {
            Ok(audio) => audio,
            Err(err) => {
                mark_server_down();
                crate::platform::diag(format!("голос Ноа не пришёл с {base}: {err}"));
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
            crate::platform::diag(format!("фраза голоса Ноа не проигралась ({} байт): {err}", audio.len()));
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
