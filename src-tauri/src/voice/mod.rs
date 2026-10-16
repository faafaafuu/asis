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
    let text = clean(&crate::spoken::without_code(text));
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

/// Номер последней просьбы прочитать вслух. `stop` его сдвигает — и все
/// просьбы, отданные раньше, но ещё не дошедшие до синтезатора, отменяются.
static READ_REQUEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Когда попросили прочитать, мс от старта программы; 0 — не просили.
static READ_PENDING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Сколько чтение может готовиться: синтезатор просыпается до пары десятков секунд.
const READ_PENDING_MS: u64 = 20_000;

fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

/// Пробел попросил прочитать: номер просьбы, которую окно вернёт с текстом.
///
/// Между нажатием и первым звуком проходят секунды — окно ищет текст,
/// синтезатор просыпается. Раньше в эти секунды Ноа считалась молчащей:
/// второе нажатие не обрывало чтение, а запускало его заново, а обрыв не
/// отменял уже отданную просьбу, и чтение начиналось после него.
pub fn request_read() -> u64 {
    READ_PENDING.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
    READ_REQUEST.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

/// Эта просьба ещё в силе — её не отменили обрывом и не сменили новой.
pub fn read_is_current(request: u64) -> bool {
    READ_REQUEST.load(std::sync::atomic::Ordering::SeqCst) == request
}

/// Чтение закончилось или не состоялось.
pub fn read_done() {
    READ_PENDING.store(0, std::sync::atomic::Ordering::SeqCst);
}

fn read_pending() -> bool {
    let since = READ_PENDING.load(std::sync::atomic::Ordering::SeqCst);
    since != 0 && now_ms().saturating_sub(since) < READ_PENDING_MS
}

/// Есть ли чем говорить: хоть одно устройство вывода звука в системе.
pub fn can_play() -> bool {
    audio::has_output()
}

/// Идёт ли сейчас речь: звучит из колонок или ещё синтезируется.
///
/// Одной очереди воспроизведения мало — между предложениями она пустеет, пока
/// синтезатор считает следующее, и пауза посреди ответа выглядела бы концом.
pub fn speaking() -> bool {
    audio::speaking() || piper::busy() || silero::busy() || read_pending()
}

/// Что именно сейчас считается речью: звук в колонках и идущий синтез.
///
/// Для сводки в журнале: когда микрофон глохнет «потому что говорим», по ней
/// видно, какая из двух частей так считает.
pub fn speech_state() -> (bool, bool) {
    (audio::speaking(), piper::busy() || silero::busy())
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
    READ_REQUEST.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    read_done();
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
    // Построчно: строка — пункт списка или абзац, и звучать она должна
    // отдельным предложением. Раньше переносы и маркеры превращались в
    // пробелы, и «по шагам» из пяти пунктов читалось одной кашей — без
    // единой точки, без паузы и без понижения голоса между шагами.
    let lines: Vec<(bool, String)> = text
        .lines()
        .map(|line| (is_item(line), clean_line(line)))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    let count = lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(at, (item, line))| {
            // Заголовок перед пунктами — «По шагам:», остальное — с точкой.
            let ends = line.ends_with(['.', '!', '?', '…', ':', ';']);
            if ends || count == 1 {
                line
            } else if !item && at + 1 < count && line.chars().count() <= 30 && line.split_whitespace().count() <= 4 {
                format!("{line}:")
            } else {
                format!("{line}.")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Строка — пункт списка: начинается маркером или номером.
fn is_item(line: &str) -> bool {
    let line = line.trim_start();
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    line.starts_with(['-', '•', '·', '*', '–', '—']) || (digits > 0 && digits < 3 && line[digits..].starts_with(['.', ')']))
}

/// Одна строка: без маркера списка, разметки и украшений.
fn clean_line(line: &str) -> String {
    let mut line = line.trim();
    // Маркер пункта: «- », «• », «* », «1. », «2) », «#».
    line = line.trim_start_matches(['-', '•', '·', '*', '#', '>', '–', '—']).trim_start();
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 3 && line[digits..].starts_with(['.', ')']) {
        line = line[digits + 1..].trim_start();
    }
    let mut out = String::with_capacity(line.len());
    let mut last_space = true;
    for ch in line.chars() {
        let ch = match ch {
            '·' | '•' | '—' | '–' | '*' | '`' | '#' => ' ',
            '«' | '»' | '"' => ' ',
            '\t' => ' ',
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
    let out = out.trim().to_string();
    // «ПО ШАГАМ» прописными синтезатор читает как аббревиатуру по буквам.
    if out.chars().filter(|c| c.is_alphabetic()).count() > 3 && out.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase) {
        let lower = out.to_lowercase();
        let mut chars = lower.chars();
        return chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
    }
    out
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
        assert_eq!(clean("· первый\n· второй"), "первый.\nвторой.");
        assert_eq!(clean("он сказал «да»"), "он сказал да");
    }

    #[test]
    fn steps_are_read_as_steps() {
        let text = "ПО ШАГАМ\n- Go-приложение на порту 8080 выполняет основную работу\n- Sidecar-логгер перехватывает его stdout\n1. Логгер отправляет логи в CloudWatch";
        assert_eq!(
            clean(text),
            "По шагам:\nGo-приложение на порту 8080 выполняет основную работу.\nSidecar-логгер перехватывает его stdout.\nЛоггер отправляет логи в CloudWatch."
        );
        assert_eq!(clean("Короткий ответ без точки"), "Короткий ответ без точки");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(clean("   \n\t "), "");
        assert_eq!(clean("— — —"), "");
    }
}
