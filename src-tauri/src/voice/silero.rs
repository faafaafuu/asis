//! Живые голоса: Светлана (нейроголос Microsoft) и Silero на этом компьютере.
//!
//! Рядом с программой ставится свой Python (встраиваемая сборка, без
//! установки в систему) и запускается маленький сервер
//! (`assets/silero_server.py`), каждая фраза — один запрос.
//!
//! Светлана и Дмитрий — нейроголоса Microsoft через edge-tts: живая
//! интонация, нужен интернет, ставятся лёгкими — Python и библиотека, около
//! 20 МБ. Microsoft время от времени меняет подпись запросов; библиотека её
//! догоняет, и сервер обновляет её сам. Поэтому голос — через неё, а не
//! своим кодом в программе. Silero — свободные голоса без сети, но только
//! через PyTorch: ~850 МБ на диске.
//!
//! Сервер сам уходит через десять минут без запросов и вместе с программой —
//! сотни мегабайт памяти без дела не держатся.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::AppHandle;

use super::assets;

/// Версии закреплены: это исполняемые файлы, которые запускаются у человека.
const PYTHON_URL: &str =
    "https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip";
const GET_PIP_URL: &str = "https://bootstrap.pypa.io/get-pip.py";
const TORCH: &str = "torch==2.14.0";
const TORCH_INDEX: &str = "https://download.pytorch.org/whl/cpu";
const MODEL_URL: &str = "https://models.silero.ai/models/tts/ru/v5_5_ru.pt";
const MODEL_FILE: &str = "v5_5_ru.pt";
/// Нейроголоса Microsoft. Первая версия закреплена, дальше сервер обновляет
/// её сам, когда Microsoft перестаёт её принимать.
const EDGE_TTS: &str = "edge-tts==7.3.1";

const SERVER_SCRIPT: &str = include_str!("../../assets/silero_server.py");
/// Словарь произношения — рядом со скриптом: его читает разбор текста.
const PRONOUNCE: &str = include_str!("../../assets/pronounce.json");

/// Порт своего голосового сервера. Рядом с портом расшифровки, не ходовой.
const PORT: u16 = 8644;

pub const VOICES: &[(&str, &str)] = &[
    ("svetlana", "Светлана — нейроголос, живой (нужен интернет)"),
    ("dmitry", "Дмитрий — нейроголос, мужской (нужен интернет)"),
    ("xenia", "Ксения — женский, живой"),
    ("baya", "Байя — женский, мягкий"),
    ("kseniya", "Ксения-2 — женский, ровный"),
    ("aidar", "Айдар — мужской"),
    ("eugene", "Евгений — мужской"),
];

pub const DEFAULT_VOICE: &str = "svetlana";

/// Нейроголос Microsoft: ему не нужны PyTorch и модель.
pub fn is_neural(voice: &str) -> bool {
    matches!(voice, "svetlana" | "dmitry")
}

/// Ставится ли сейчас что-то: фоновую установку Светланы не начинаем дважды.
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Номер последней фразы: ответ, пришедший после остановки, не звучит.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Фраза, которая прямо сейчас синтезируется на голосовом сервере.
///
/// Синтез занимает секунды, и всё это время из колонок ещё тихо. Без отметки
/// программа считалась молчащей: пробел, нажатый в эти секунды, значил не
/// «замолчи и слушай», а «прочитай заново», — и чтение начиналось поверх.
static SYNTHESIZING: AtomicU64 = AtomicU64::new(0);

/// Идёт ли синтез: фраза заказана и ещё не поставлена в очередь звука.
pub fn busy() -> bool {
    let active = SYNTHESIZING.load(Ordering::SeqCst);
    active != 0 && active == GENERATION.load(Ordering::SeqCst)
}
static SERVER: Mutex<Option<std::process::Child>> = Mutex::new(None);
static SPAWNING: Mutex<()> = Mutex::new(());

fn root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(assets::dir(app)?.join("silero"))
}

fn python(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(root(app)?.join("python").join("python.exe"))
}

fn has_pip(app: &AppHandle) -> bool {
    root(app)
        .map(|root| root.join("python").join("Lib").join("site-packages").join("pip").exists())
        .unwrap_or(false)
}

fn has_torch(app: &AppHandle) -> bool {
    root(app)
        .map(|root| {
            root.join("python")
                .join("Lib")
                .join("site-packages")
                .join("torch")
                .join("__init__.py")
                .exists()
        })
        .unwrap_or(false)
}

fn has_edge(app: &AppHandle) -> bool {
    root(app)
        .map(|root| root.join("python").join("Lib").join("site-packages").join("edge_tts").join("__init__.py").exists())
        .unwrap_or(false)
}

fn model(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(root(app)?.join(MODEL_FILE))
}

/// Скачаны ли голоса Silero: Python, PyTorch и модель.
fn silero_ready(app: &AppHandle) -> bool {
    python(app).map(|p| p.exists()).unwrap_or(false)
        && has_torch(app)
        && model(app).map(|p| p.exists()).unwrap_or(false)
}

fn neural_ready(app: &AppHandle) -> bool {
    python(app).map(|p| p.exists()).unwrap_or(false) && has_edge(app)
}

/// Готов ли этот голос.
pub fn ready(app: &AppHandle, voice: &str) -> bool {
    if is_neural(voice) { neural_ready(app) } else { silero_ready(app) }
}

/// Каким голосом говорить сейчас. Светланы ещё нет — она тихо ставится в
/// фоне, а пока говорит Ксения, если Silero стоит. `None` — своим голосом.
pub fn usable(app: &AppHandle, voice: &str) -> Option<String> {
    let voice = if voice.trim().is_empty() { DEFAULT_VOICE } else { voice.trim() };
    if ready(app, voice) {
        return Some(voice.to_string());
    }
    if is_neural(voice) && !INSTALLING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        let wanted = voice.to_string();
        tauri::async_runtime::spawn(async move {
            match install(app, &wanted).await {
                // Сервер, поднятый до установки, нейроголоса не видит: следующая
                // фраза поднимет его заново.
                Ok(()) => restart_server(),
                Err(err) => log::warn!("нейроголос не поставился: {err}"),
            }
            INSTALLING.store(false, Ordering::SeqCst);
        });
    }
    (is_neural(voice) && silero_ready(app)).then(|| "xenia".to_string())
}

fn progress(app: &AppHandle, percent: u8, status: &str) {
    assets::emit_to(app, assets::EVENT, percent, status, false, None);
}

/// Ставит то, что нужно голосу: нейроголосу — Python и edge-tts (около
/// 20 МБ), Silero — ещё PyTorch и модель (~350 МБ загрузки, ~850 МБ на диске).
pub async fn install(app: AppHandle, voice: &str) -> Result<(), String> {
    let result = install_steps(&app, !is_neural(voice)).await;
    match &result {
        Ok(()) => assets::emit_to(&app, assets::EVENT, 100, "готово", true, None),
        Err(err) => assets::emit_to(&app, assets::EVENT, 0, "ошибка", false, Some(err.clone())),
    }
    result
}

async fn install_steps(app: &AppHandle, silero: bool) -> Result<(), String> {
    let root = root(app)?;
    let python_dir = root.join("python");
    std::fs::create_dir_all(&python_dir).map_err(|err| err.to_string())?;

    if !python(app)?.exists() {
        progress(app, 0, "скачиваю Python");
        let archive = root.join("python.zip");
        assets::download(app, assets::EVENT, PYTHON_URL, &archive, 0, 4).await?;
        assets::unzip(&archive, &python_dir)?;
        let _ = std::fs::remove_file(&archive);
        // Встраиваемый Python по умолчанию не видит site-packages.
        let pth = python_dir.join("python312._pth");
        let text = std::fs::read_to_string(&pth).map_err(|err| format!("{}: {err}", pth.display()))?;
        std::fs::write(&pth, text.replace("#import site", "import site")).map_err(|err| err.to_string())?;
    }

    if !has_pip(app) {
        progress(app, 5, "ставлю pip");
        let get_pip = root.join("get-pip.py");
        assets::download(app, assets::EVENT, GET_PIP_URL, &get_pip, 5, 6).await?;
        let args = [get_pip.to_string_lossy().to_string(), "--no-warn-script-location".into()];
        run_python(app, args.to_vec()).await?;
        let _ = std::fs::remove_file(&get_pip);
    }

    if !has_edge(app) {
        progress(app, 7, "ставлю нейроголоса");
        let args = ["-m", "pip", "install", "--no-warn-script-location", "--disable-pip-version-check", EDGE_TTS];
        run_python(app, args.iter().map(|arg| arg.to_string()).collect()).await?;
    }
    if !silero {
        return Ok(());
    }

    if !has_torch(app) {
        progress(app, 8, "ставлю PyTorch — это пара минут");
        let args = [
            "-m", "pip", "install", "--no-warn-script-location", "--disable-pip-version-check",
            TORCH, "--index-url", TORCH_INDEX,
        ];
        run_python(app, args.iter().map(|arg| arg.to_string()).collect()).await?;
    }

    let model = model(app)?;
    if !model.exists() {
        progress(app, 80, "скачиваю голоса");
        assets::download(app, assets::EVENT, MODEL_URL, &model, 80, 99).await?;
    }
    Ok(())
}

/// Запускает свой Python и ждёт конца. Ошибку отдаёт последними строками вывода.
async fn run_python(app: &AppHandle, args: Vec<String>) -> Result<(), String> {
    let exe = python(app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = std::process::Command::new(&exe);
        command.args(&args);
        hide_window(&mut command);
        let output = command
            .output()
            .map_err(|err| format!("Python не запустился: {err}"))?;
        if output.status.success() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        Err(format!(
            "установка не удалась: {}",
            tail[tail.len().saturating_sub(2)..].join(" / ")
        ))
    })
    .await
    .map_err(|err| err.to_string())?
}

fn hide_window(command: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

fn alive() -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], PORT)),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// Останавливает свой голосовой сервер: следующая фраза поднимет его заново.
fn restart_server() {
    if let Some(mut child) = SERVER.lock().unwrap_or_else(|err| err.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
        log::info!("голосовой сервер перезапускается: поставлен нейроголос");
    }
}

/// Поднимает сервер, если он не отвечает. Блокирующая: зовётся из отдельного потока.
fn ensure_server(app: &AppHandle) -> Result<(), String> {
    if alive() {
        return Ok(());
    }
    let _spawning = SPAWNING.lock().unwrap_or_else(|err| err.into_inner());
    if alive() {
        return Ok(());
    }
    if !silero_ready(app) && !neural_ready(app) {
        return Err("голоса ещё не скачаны".into());
    }
    // Прежний сервер убиваем, только если он умер или завис насовсем. Живой,
    // но ещё поднимающийся сервер не трогаем: раньше вторая фраза убивала его
    // за секунды до готовности и начинала заново — и ждать приходилось дважды.
    {
        let mut slot = SERVER.lock().unwrap_or_else(|err| err.into_inner());
        let still_starting = slot
            .as_mut()
            .map(|child| matches!(child.try_wait(), Ok(None)))
            .unwrap_or(false);
        if still_starting {
            drop(slot);
            for _ in 0..240 {
                std::thread::sleep(Duration::from_millis(250));
                if alive() {
                    log::info!("голосовой сервер Silero готов");
                    return Ok(());
                }
            }
            slot = SERVER.lock().unwrap_or_else(|err| err.into_inner());
        }
        if let Some(mut stale) = slot.take() {
            let _ = stale.kill();
            let _ = stale.wait();
        }
    }

    let root = root(app)?;
    // Скрипт пишется при каждом запуске: так он всегда той же версии, что программа.
    let script = root.join("silero_server.py");
    std::fs::write(&script, SERVER_SCRIPT).map_err(|err| err.to_string())?;
    std::fs::write(root.join("pronounce.json"), PRONOUNCE).map_err(|err| err.to_string())?;
    let log_path = root.join("server.log");
    let log = std::fs::File::create(&log_path).map_err(|err| err.to_string())?;

    let mut command = std::process::Command::new(python(app)?);
    command
        .arg(&script)
        .arg(model(app)?)
        .arg(PORT.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    hide_window(&mut command);
    let child = command
        .spawn()
        .map_err(|err| format!("голосовой сервер не запустился: {err}"))?;
    crate::jobs::adopt(&child);
    *SERVER.lock().unwrap_or_else(|err| err.into_inner()) = Some(child);
    log::info!("поднимаю голосовой сервер Silero");

    // Полторы минуты, а не полминуты. Холодный старт — это PyTorch и модель
    // голоса, и на загруженной машине (распознавание, своя модель, всё разом)
    // он шёл дольше тридцати секунд. Сдавались раньше — и следующая фраза
    // убивала почти поднявшийся сервер, чтобы начать его заново.
    for _ in 0..360 {
        std::thread::sleep(Duration::from_millis(250));
        if alive() {
            log::info!("голосовой сервер Silero готов");
            return Ok(());
        }
        let exited = SERVER
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten());
        if let Some(status) = exited {
            let output = std::fs::read_to_string(&log_path).unwrap_or_default();
            let tail: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
            return Err(format!(
                "голосовой сервер завершился ({status}): {}",
                tail[tail.len().saturating_sub(2)..].join(" / ")
            ));
        }
    }
    Err("голосовой сервер не поднялся за полторы минуты".into())
}

/// Поднимает сервер заранее — пока человек ещё говорит, — и прогревает модель:
/// первая фраза после загрузки синтезируется втрое дольше остальных.
pub fn warm(app: &AppHandle, voice: &str) {
    if alive() || !(silero_ready(app) || neural_ready(app)) {
        return;
    }
    let voice = if ready(app, voice) { voice.to_string() } else { DEFAULT_VOICE.to_string() };
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("sufler-silero-warm".into())
        .spawn(move || {
            if let Err(err) = ensure_server(&app) {
                log::warn!("голосовой сервер не поднялся заранее: {err}");
                return;
            }
            let _ = tauri::async_runtime::block_on(request(&voice, 1.0, "Готово."));
        });
}

async fn request(voice: &str, rate: f32, text: &str) -> Result<Vec<u8>, String> {
    let client = crate::net::client_builder()
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(format!("http://127.0.0.1:{PORT}/tts"))
        .json(&serde_json::json!({ "text": text, "speaker": voice, "rate": rate }))
        .send()
        .await
        .map_err(|err| format!("голосовой сервер не ответил: {err}"))?;
    if !response.status().is_success() {
        let reason = response.text().await.unwrap_or_default();
        return Err(format!("голосовой сервер отказал: {reason}"));
    }
    Ok(response.bytes().await.map_err(|err| err.to_string())?.to_vec())
}

/// Говорит текст голосом сервера. Возвращается, когда звук поставлен в очередь.
pub async fn speak(app: &AppHandle, voice: &str, rate: f32, text: &str) -> Result<(), String> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    SYNTHESIZING.store(generation, Ordering::SeqCst);
    let voice = if voice.trim().is_empty() { DEFAULT_VOICE } else { voice.trim() };

    // Отметку снимаем в любом случае — и когда сервер отказал, и когда фразу
    // сменила следующая: забытая отметка навсегда оставила бы нас «говорящими».
    let done = |generation: u64| {
        let _ = SYNTHESIZING.compare_exchange(generation, 0, Ordering::SeqCst, Ordering::SeqCst);
    };

    let starter = app.clone();
    let started = tauri::async_runtime::spawn_blocking(move || ensure_server(&starter)).await;
    let outcome = async {
        started.map_err(|err| err.to_string())??;
        // По кускам: первое предложение звучит почти сразу, а следующее
        // синтезируется, пока звучит предыдущее, — звук встаёт в очередь сам.
        // Целиком длинный ответ сначала долго молчал, а потом читался лекцией.
        for piece in pieces(text) {
            let wav = request(voice, rate, &piece).await?;
            if GENERATION.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            let (samples, hz) = if wav.starts_with(b"RIFF") { super::azure::decode_wav(&wav)? } else { decode_mp3(wav)? };
            super::audio::play(samples, hz);
        }
        Ok(())
    }
    .await;
    done(generation);
    outcome
}

/// MP3 нейроголоса (рядом нет ffmpeg, и сервер отдаёт как прислал Microsoft)
/// — в отсчёты одним каналом.
fn decode_mp3(data: Vec<u8>) -> Result<(Vec<f32>, u32), String> {
    use rodio::Source;
    let decoder = rodio::Decoder::new(std::io::Cursor::new(data)).map_err(|err| format!("звук не разобрался: {err}"))?;
    let channels = usize::from(decoder.channels().max(1));
    let hz = decoder.sample_rate();
    let all: Vec<f32> = decoder.collect();
    let samples = all.chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect();
    Ok((samples, hz))
}

/// Куски для озвучки: первый — одно предложение (чтобы начать сразу),
/// дальше — по нескольку предложений, до ~300 знаков, по границам фраз.
fn pieces(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '…' | '\n') && current.trim().chars().count() > 1 {
            sentences.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        sentences.push(current);
    }
    let mut out: Vec<String> = Vec::new();
    for raw in sentences {
        let ends_line = raw.ends_with('\n');
        let sentence = raw.trim();
        if sentence.is_empty() {
            continue;
        }
        let grouped = out.len() > 1;
        match out.last_mut() {
            Some(last) if grouped && last.chars().count() + sentence.chars().count() < 300 => {
                // Пункты списка — с новой строки: сервер ставит между ними паузу.
                last.push(if ends_line { '\n' } else { ' ' });
                last.push_str(sentence);
            }
            _ => out.push(sentence.to_string()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pieces;

    #[test]
    fn first_sentence_goes_alone_then_groups() {
        let text = "Первое. Второе предложение. Третье! Четвёртое?";
        let parts = pieces(text);
        assert_eq!(parts[0], "Первое.");
        assert_eq!(parts[1], "Второе предложение. Третье! Четвёртое?");
        assert_eq!(parts.len(), 2);
        assert_eq!(pieces("Без точки в конце"), vec!["Без точки в конце".to_string()]);
    }
}

/// Отменяет фразу, которая ещё синтезируется.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}
