//! Голос: озвучивание объяснений.
//!
//! Два способа сказать одно и то же, как и с самими объяснениями:
//!
//!   `piper` — на этом же компьютере. Ничего не уходит наружу, работает без
//!     интернета. Голос живой, но слышно, что синтезированный.
//!   `azure` — нейроголоса Microsoft через Azure Speech по ключу. Звучат почти
//!     неотличимо от человека, но это сеть и чужой сервер (см. azure.rs).
//!
//! Умолчание — `piper`, по той же причине, по какой объяснения по умолчанию даёт
//! своя модель: работает всегда и ни от кого не зависит.

pub mod assets;
mod audio;
pub mod hotkey;
mod azure;
mod silero;
mod piper;
pub mod stt;
pub mod whisper;

use tauri::AppHandle;

use crate::config::VoiceConfig;

/// Говорит текст выбранным способом. Возвращается сразу, не дожидаясь конца речи.
pub async fn speak(app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<(), String> {
    let text = clean(text);
    if text.is_empty() {
        return Ok(());
    }

    match config.engine.as_str() {
        "silero" => {
            stop();
            match silero::speak(app, &config.silero_voice, config.rate, &text).await {
                Ok(()) => Ok(()),
                Err(err) => {
                    log::warn!("голос Silero: {err}; читаю своим голосом");
                    piper::speak(app, &config.voice, config.rate, &text)
                }
            }
        }
        "azure" => {
            // Прежняя фраза обрывается, как и у Piper: новый ответ важнее.
            stop();
            let key = crate::secret::reveal(&config.azure_key);
            match azure::speak(&key, &config.azure_region, &config.edge_voice, config.rate, &text).await {
                Ok(()) => Ok(()),
                // Сеть, ключ, лимит — молчать из-за этого нельзя: человек просил
                // прочитать вслух. Читаем своим голосом, причину — в журнал.
                Err(err) => {
                    log::warn!("голос Azure: {err}; читаю своим голосом");
                    piper::speak(app, &config.voice, config.rate, &text)
                }
            }
        }
        _ => piper::speak(app, &config.voice, config.rate, &text),
    }
}

/// Синтезирует текст своим голосом в WAV — для ответа голосовым сообщением.
pub fn synthesize(app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<Vec<u8>, String> {
    let text = clean(text);
    if text.is_empty() {
        return Err("нечего сказать".into());
    }
    piper::synthesize(app, &config.voice, config.rate, &text)
}

/// Список онлайн-голосов. Через обёртку: сам модуль azure закрытый, наружу
/// торчит только то, что нужно окну настройки.
pub fn azure_voices() -> &'static [(&'static str, &'static str)] {
    azure::VOICES
}

pub fn silero_voices() -> &'static [(&'static str, &'static str)] {
    silero::VOICES
}

/// Скачаны ли голоса Silero.
pub fn silero_ready(app: &AppHandle) -> bool {
    silero::ready(app)
}

/// Скачивает Python, PyTorch и голоса Silero.
pub async fn silero_install(app: AppHandle) -> Result<(), String> {
    silero::install(app).await
}

/// Готовит голос заранее, пока человек ещё говорит: Silero поднимает сервер
/// и прогревает модель, остальным способам готовиться не нужно.
pub fn warm(app: &AppHandle) {
    use tauri::Manager;
    let engine = app.state::<crate::state::AppState>().config().voice.engine.clone();
    if engine == "silero" {
        silero::warm(app);
    }
}

/// Пробный запрос к Azure. Озвучивание при неудаче тихо переходит на свой
/// голос, а окну настройки нужна сама причина.
pub async fn check_azure(config: &VoiceConfig) -> Result<(), String> {
    let key = crate::secret::reveal(&config.azure_key);
    azure::synthesize(&key, &config.azure_region, &config.edge_voice, config.rate, "Проверка.")
        .await
        .map(|_| ())
}

/// Насколько громко звучит речь прямо сейчас: от 0 до 1.
pub fn level() -> f32 {
    audio::level()
}

/// Идёт ли сейчас речь: звучит из колонок или ещё синтезируется.
///
/// Одной очереди воспроизведения мало — между предложениями она пустеет, пока
/// синтезатор считает следующее, и пауза посреди ответа выглядела бы концом.
pub fn speaking() -> bool {
    audio::speaking() || piper::busy()
}

/// Что именно сейчас считается речью: звук в колонках и идущий синтез.
///
/// Для сводки в журнале: когда микрофон глохнет «потому что говорим», по ней
/// видно, какая из двух частей так считает.
pub fn speech_state() -> (bool, bool) {
    (audio::speaking(), piper::busy())
}

/// Закрепляет звуковые устройства за долгоживущим потоком.
///
/// Зовётся первым делом при запуске — до окон, до микрофона, до всего.
pub fn claim_devices() {
    audio::claim();
}

/// Освобождает видеопамять, занятую распознаванием.
pub fn release_speech() {
    whisper::shutdown();
}

/// Сколько звучит сигнал появления. Нужно, чтобы не открыть микрофон под него.
pub fn chime_length() -> std::time::Duration {
    audio::chime_length()
}

/// Короткий сигнал о появлении помощника. Отдаёт свою длительность: пока он
/// звучит, микрофон открывать нельзя — он запишет его же.
pub fn chime_open() -> std::time::Duration {
    audio::chime_open()
}

/// Короткий сигнал об окончании разговора.
pub fn chime() {
    audio::chime();
}

/// Замолчать: и звук, и работу, которая его готовит.
pub fn stop() {
    piper::stop();
    azure::stop();
    silero::stop();
}

/// Готовит текст к произнесению.
///
/// Модель отвечает текстом для глаз: там встречаются и списки, и кавычки-ёлочки,
/// и длинные тире. Часть этого синтезатор проговаривает буквально («тире»,
/// «звёздочка»), часть просто спотыкает интонацию.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = true;

    for ch in text.chars() {
        let ch = match ch {
            // Маркеры списков и оформление — на паузу: перечисление должно
            // звучать перечислением, а не сплошной строкой.
            '·' | '•' | '—' | '–' | '*' | '`' => ' ',
            '«' | '»' | '"' => ' ',
            '\n' | '\r' | '\t' => ' ',
            other => other,
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoration_does_not_reach_the_voice() {
        assert_eq!(
            clean("Альбедо — доля отражённого света"),
            "Альбедо доля отражённого света"
        );
        assert_eq!(clean("· первый\n· второй"), "первый второй");
        assert_eq!(clean("он сказал «да»"), "он сказал да");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(clean("   \n\t "), "");
        assert_eq!(clean("— — —"), "");
    }
}
