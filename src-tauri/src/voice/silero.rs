//! Живые голоса Silero на этом компьютере.
//!
//! Silero — лучшие свободные русские голоса, но работают они только через
//! PyTorch. Поэтому рядом с программой ставится свой Python (встраиваемая
//! сборка, без установки в систему), в него — PyTorch для процессора, и
//! запускается маленький сервер (`assets/silero_server.py`): модель грузится
//! один раз, дальше каждая фраза — один запрос, доли секунды.
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

const SERVER_SCRIPT: &str = include_str!("../../assets/silero_server.py");

/// Порт своего голосового сервера. Рядом с портом расшифровки, не ходовой.
const PORT: u16 = 8644;

pub const VOICES: &[(&str, &str)] = &[
    ("xenia", "Ксения — женский, живой"),
    ("baya", "Байя — женский, мягкий"),
    ("kseniya", "Ксения-2 — женский, ровный"),
    ("aidar", "Айдар — мужской"),
    ("eugene", "Евгений — мужской"),
];

pub const DEFAULT_VOICE: &str = "xenia";

/// Номер последней фразы: ответ, пришедший после остановки, не звучит.
static GENERATION: AtomicU64 = AtomicU64::new(0);
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

fn model(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(root(app)?.join(MODEL_FILE))
}

/// Всё ли скачано.
pub fn ready(app: &AppHandle) -> bool {
    python(app).map(|p| p.exists()).unwrap_or(false)
        && has_torch(app)
        && model(app).map(|p| p.exists()).unwrap_or(false)
}

fn progress(app: &AppHandle, percent: u8, status: &str) {
    assets::emit_to(app, assets::EVENT, percent, status, false, None);
}

/// Скачивает и ставит всё нужное: около 350 МБ загрузки, ~850 МБ на диске.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let result = install_steps(&app).await;
    match &result {
        Ok(()) => assets::emit_to(&app, assets::EVENT, 100, "готово", true, None),
        Err(err) => assets::emit_to(&app, assets::EVENT, 0, "ошибка", false, Some(err.clone())),
    }
    result
}

async fn install_steps(app: &AppHandle) -> Result<(), String> {
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

/// Поднимает сервер, если он не отвечает. Блокирующая: зовётся из отдельного потока.
fn ensure_server(app: &AppHandle) -> Result<(), String> {
    if alive() {
        return Ok(());
    }
    let _spawning = SPAWNING.lock().unwrap_or_else(|err| err.into_inner());
    if alive() {
        return Ok(());
    }
    if !ready(app) {
        return Err("голоса Silero ещё не скачаны".into());
    }
    if let Some(mut stale) = SERVER.lock().unwrap_or_else(|err| err.into_inner()).take() {
        let _ = stale.kill();
        let _ = stale.wait();
    }

    let root = root(app)?;
    // Скрипт пишется при каждом запуске: так он всегда той же версии, что программа.
    let script = root.join("silero_server.py");
    std::fs::write(&script, SERVER_SCRIPT).map_err(|err| err.to_string())?;
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

    for _ in 0..120 {
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
    Err("голосовой сервер не поднялся за 30 секунд".into())
}

/// Поднимает сервер заранее — пока человек ещё говорит, — и прогревает модель:
/// первая фраза после загрузки синтезируется втрое дольше остальных.
pub fn warm(app: &AppHandle) {
    if alive() || !ready(app) {
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("sufler-silero-warm".into())
        .spawn(move || {
            if let Err(err) = ensure_server(&app) {
                log::warn!("голосовой сервер не поднялся заранее: {err}");
                return;
            }
            let _ = tauri::async_runtime::block_on(request(DEFAULT_VOICE, 1.0, "Готово."));
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

/// Говорит текст голосом Silero. Возвращается, когда звук поставлен в очередь.
pub async fn speak(app: &AppHandle, voice: &str, rate: f32, text: &str) -> Result<(), String> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let voice = if voice.trim().is_empty() { DEFAULT_VOICE } else { voice.trim() };

    let starter = app.clone();
    tauri::async_runtime::spawn_blocking(move || ensure_server(&starter))
        .await
        .map_err(|err| err.to_string())??;
    let wav = request(voice, rate, text).await?;
    if GENERATION.load(Ordering::SeqCst) != generation {
        return Ok(());
    }
    let (samples, hz) = super::azure::decode_wav(&wav)?;
    super::audio::play(samples, hz);
    Ok(())
}

/// Отменяет фразу, которая ещё синтезируется.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}
