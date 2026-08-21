//! Разговор с локальной Ollama: что у неё установлено и как скачать модель.
//!
//! Зачем отдельно от ai_client: тот занят объяснениями и знает про модель ровно
//! одно — её имя. Здесь же всё, что нужно окну настройки, чтобы человек выбирал
//! модель из списка, а не вписывал `qwen2.5:7b` по памяти.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Куда стучаться, если в настройках не указано ничего осмысленного.
/// Именно 127.0.0.1, а не localhost: на машинах с включённым IPv6 localhost
/// иногда разрешается в ::1, где Ollama не слушает, и получается «сервис не
/// отвечает» при работающем сервисе.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:11434";

/// Полный адрес запроса к своей модели — то, что попадает в настройки по умолчанию.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";

/// Событие с ходом загрузки. Летит в окно настройки много раз в секунду.
const PULL_EVENT: &str = "model:pull";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub name: String,
    /// Гигабайты с одним знаком: человеку важен порядок, а не байты.
    pub size_gb: f64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Отвечает ли сервер прямо сейчас.
    pub running: bool,
    /// Лежит ли Ollama на диске. Раньше этого поля не было, и окно на всякий
    /// молчание советовало «установите с ollama.com» — а человеку с уже
    /// установленной Ollama, которая просто не поднялась после перезагрузки,
    /// этот совет говорит, что виноват он, и не говорит, что делать.
    pub present: bool,
    pub installed: Vec<Model>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Progress {
    model: String,
    /// 0..100. Пока Ollama не сообщила общий размер, шлём 0 — полоса стоит,
    /// но подпись уже объясняет, что происходит.
    percent: u8,
    /// Человеческая строка: «скачиваю», «проверяю», «готово».
    status: String,
    done: bool,
    error: Option<String>,
}

/// Ответ `/api/tags`.
#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
    #[serde(default)]
    size: u64,
}

/// Строка потока `/api/pull`.
#[derive(Deserialize)]
struct PullLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    error: Option<String>,
}

/// Базовый адрес Ollama по адресу чата из настроек.
///
/// В настройках лежит полный адрес запроса, а служебные ручки живут рядом
/// (`/api/tags`, `/api/pull`). Отрезаем хвост — но только у родного адреса
/// Ollama: если человек настроил облачный сервис, спрашивать у того про
/// локальные модели бессмысленно, там своя вселенная. В таком случае идём
/// к Ollama по умолчанию — она либо запущена, либо нет, и окно это покажет.
pub fn host_from(endpoint: &str) -> String {
    let endpoint = endpoint.trim().trim_end_matches('/');
    match endpoint.strip_suffix("/api/chat") {
        Some(host) if host.starts_with("http") => host.to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}

/// Путь к исполняемому файлу Ollama, если она стоит на этом компьютере.
///
/// Сначала консольный `ollama.exe`, которому нужна команда `serve`, и только
/// потом оконное `ollama app.exe`.
///
/// Порядок был обратным, и это была ошибка. Оконная версия поднимает не только
/// сервер, но и своё окно со значком в трее — чужое приложение, которое человек
/// не запускал и которое ему нечего показать. При запуске Суфлёра вместе с
/// системой оно всплывало поверх рабочего стола при каждом входе, и закрывать
/// его приходилось руками. Сервер нужен один и тот же, разница только в окне,
/// поэтому берём тот, у которого окна нет вовсе.
pub fn executable() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let dir = std::path::Path::new(&local).join("Programs").join("Ollama");
        for name in ["ollama.exe", "ollama app.exe"] {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
    }

    // Общий путь для всех систем: ищем в PATH руками, чтобы не тащить крейт
    // ради одного перебора каталогов.
    let names: &[&str] = if cfg!(windows) {
        &["ollama.exe"]
    } else {
        &["ollama"]
    };
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.exists())
    })
}

/* ── Установка Ollama ────────────────────────────────────────────────────── */

/// Событие с ходом установки. Отдельно от загрузки моделей: там качается модель,
/// здесь — сама программа, и путать эти два прогресса в одном событии нельзя.
const INSTALL_EVENT: &str = "ollama:install";

/// Где брать установщик. Только официальный репозиторий и только по https:
/// мы запускаем скачанное на машине человека, и подменённый файл здесь означал
/// бы чужой код с его правами.
const RELEASE_API: &str = "https://api.github.com/repos/ollama/ollama/releases/latest";
const ASSET_NAME: &str = "OllamaSetup.exe";

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}

/// Сколько весит установщик — чтобы окно сказало это до нажатия, а не после.
pub async fn install_size_gb() -> Option<f64> {
    let asset = latest_asset().await.ok()?;
    Some((asset.size as f64 / 1e9 * 10.0).round() / 10.0)
}

async fn latest_asset() -> Result<Asset, String> {
    let client = crate::net::client_builder()
        .user_agent("Sufler")
        .build()
        .map_err(|err| err.to_string())?;

    let release: Release = client
        .get(RELEASE_API)
        .send()
        .await
        .map_err(|err| format!("не удалось спросить о последней версии: {err}"))?
        .json()
        .await
        .map_err(|err| format!("ответ о версии не разобрался: {err}"))?;

    release
        .assets
        .into_iter()
        .find(|asset| asset.name == ASSET_NAME)
        .ok_or_else(|| format!("в последнем выпуске Ollama нет файла {ASSET_NAME}"))
}

/// Скачивает официальный установщик Ollama и запускает его.
///
/// Установщик весит полтора гигабайта, поэтому качаем потоком и докладываем
/// о ходе: молчаливое ожидание такой длины неотличимо от зависшей программы.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let asset = latest_asset().await?;
    emit_install(&app, 0, "скачиваю", false, None);

    let client = crate::net::client_builder()
        .user_agent("Sufler")
        // Полтора гигабайта на медленной линии — это надолго; общий предел
        // времени здесь только помешает.
        .timeout(std::time::Duration::from_secs(3 * 60 * 60))
        .build()
        .map_err(|err| err.to_string())?;

    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|err| format!("загрузка не началась: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("сервер ответил {}", response.status()));
    }

    let path = std::env::temp_dir().join(ASSET_NAME);
    let mut file = std::fs::File::create(&path).map_err(|err| format!("нет доступа к временной папке: {err}"))?;

    let mut written: u64 = 0;
    let mut last_percent = u8::MAX;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("загрузка прервалась: {err}"))?
    {
        use std::io::Write;
        file.write_all(&chunk).map_err(|err| format!("не удалось записать файл: {err}"))?;
        written += chunk.len() as u64;

        let percent = ((written as f64 / asset.size as f64) * 100.0) as u8;
        if percent != last_percent {
            last_percent = percent;
            emit_install(&app, percent, "скачиваю", false, None);
        }
    }
    drop(file);

    // Размер обязан совпасть с заявленным. Обрыв связи даёт «успешно
    // скачанный» огрызок, а запускать огрызок как установщик — плохая идея.
    if written != asset.size {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "файл скачался не полностью: {written} байт вместо {}",
            asset.size
        ));
    }

    emit_install(&app, 100, "проверяю подпись", false, None);
    verify_signature(&path)?;

    emit_install(&app, 100, "устанавливаю", false, None);
    run_installer(&path)?;

    emit_install(&app, 100, "готово", true, None);
    Ok(())
}

/// Проверяет, что установщик подписан и подпись действительна.
///
/// Скачанное мы запускаем с правами пользователя, поэтому одного https мало:
/// он говорит, что файл пришёл с GitHub, но не что его собрала Ollama.
/// Подпись говорит именно это.
#[cfg(target_os = "windows")]
fn verify_signature(path: &std::path::Path) -> Result<(), String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-AuthenticodeSignature -LiteralPath '{}').Status",
                path.display()
            ),
        ])
        .output()
        .map_err(|err| format!("не удалось проверить подпись: {err}"))?;

    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if status == "Valid" {
        log::info!("подпись установщика Ollama действительна");
        return Ok(());
    }

    let _ = std::fs::remove_file(path);
    Err(format!(
        "подпись установщика недействительна ({status}) — файл удалён, ничего не запускаем"
    ))
}

#[cfg(not(target_os = "windows"))]
fn verify_signature(_path: &std::path::Path) -> Result<(), String> {
    Err("установка Ollama из программы поддерживается только на Windows".into())
}

/// Запускает установщик тихо: без вопросов, но с полосой хода от самой Ollama.
///
/// Ждём завершения, а не запускаем и забываем: окну нужно знать, когда можно
/// спрашивать про модели, а до конца установки их ещё нет.
#[cfg(target_os = "windows")]
fn run_installer(path: &std::path::Path) -> Result<(), String> {
    let status = std::process::Command::new(path)
        // Ключи Inno Setup: без мастера, без перезагрузки.
        .args(["/SILENT", "/NORESTART"])
        .status()
        .map_err(|err| format!("установщик не запустился: {err}"))?;

    if !status.success() {
        return Err(format!("установщик завершился с кодом {status}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_installer(_path: &std::path::Path) -> Result<(), String> {
    Err("установка Ollama из программы поддерживается только на Windows".into())
}

fn emit_install(app: &AppHandle, percent: u8, status: &str, done: bool, error: Option<String>) {
    let _ = app.emit(
        INSTALL_EVENT,
        Progress {
            model: String::new(),
            percent,
            status: status.to_string(),
            done,
            error,
        },
    );
}

/// Запускает Ollama. Возвращается сразу: сервер поднимается пару секунд, и
/// ждать его здесь нечем — окно само переспросит состояние.
pub fn start() -> Result<(), String> {
    let exe = executable().ok_or("Ollama не найдена на этом компьютере")?;
    let windowed = exe
        .file_name()
        .map(|name| name.to_string_lossy().contains("app"))
        .unwrap_or(false);

    let mut command = std::process::Command::new(&exe);
    // Оконное приложение поднимает сервер само, консольному нужно сказать.
    if !windowed {
        command.arg("serve");
    }

    // Без этого флага на секунду мелькнёт чёрное окно консоли — со стороны
    // выглядит как сбой, хотя всё идёт правильно.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("не удалось запустить Ollama: {err}"))
}

/* ── Прогрев модели ───────────────────────────────────────────────────── */

/// Сколько держать модель в памяти после обращения.
///
/// Было пять минут (умолчание Ollama), потом тридцать. И того и другого мало:
/// человек отвлёкся на совещание, вернулся, выделил слово — и снова ждёт
/// тринадцать секунд загрузки вместо полусекунды ответа. Рабочий день — та
/// единица, в которой это удобно мерить: пока за компьютером работают, модель
/// под рукой; на ночь или после перезагрузки память освобождается сама.
pub const KEEP_ALIVE: &str = "8h";

/// Просит Ollama поднять модель в память заранее.
///
/// Пустой prompt — не запрос, а именно просьба загрузить: модель ничего не
/// генерирует, только раскладывает веса по видеопамяти. Вызывается при запуске
/// программы, чтобы первое выделение после включения компьютера не пришлось на
/// холодную загрузку — то самое «первый раз всегда ошибка».
///
/// `stream: false` обязателен: по умолчанию эта ручка отвечает потоком, и запрос
/// висел бы до конца потока вместо возврата сразу после загрузки весов.
pub async fn preload(host: &str, model: &str) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err("модель не выбрана".into());
    }

    // Своё время ожидания, заведомо больше обычного: здесь мы именно грузим
    // веса с диска, и десятки секунд — это норма, а не признак поломки.
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|err| err.to_string())?;

    {
        let mut warmed = WARMED.lock().unwrap_or_else(|err| err.into_inner());
        if !warmed.iter().any(|mine| same_model(mine, model)) {
            warmed.push(model.to_string());
        }
    }

    client
        .post(format!("{host}/api/generate"))
        .json(&serde_json::json!({
            "model": model,
            "prompt": "",
            "stream": false,
            "keep_alive": KEEP_ALIVE,
        }))
        .send()
        .await
        .map_err(|err| format!("прогрев не удался: {err}"))?
        .error_for_status()
        .map(|_| ())
        .map_err(|err| format!("Ollama ответила ошибкой на прогрев: {err}"))
}

/// Модели, которые в память загрузили мы.
static WARMED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Одна ли это модель, как бы ни был записан тег.
fn same_model(left: &str, right: &str) -> bool {
    left == right
        || left == format!("{right}:latest")
        || right == format!("{left}:latest")
}

/// Выгружает из видеопамяти лишние модели, загруженные нами же.
///
/// Ollama держит в памяти каждую модель, к которой обращались, — и держит долго,
/// мы сами её об этом просим. Пока модель одна, это ровно то, что нужно: ответ
/// приходит за полсекунды вместо тринадцати. Но стоит человеку попробовать в
/// настройках вторую и третью, и в видеопамяти оказываются все три разом.
///
/// Наблюдалось вживую: на карте с десятью гигабайтами висели qwen2.5:7b,
/// gemma3:4b и gemma3:1b — восемь с половиной гигабайт, — а рядом просилось
/// распознавание речи. Свободного места не осталось, и ответы, которые раньше
/// приходили за секунду, стали идти минуту или не приходить вовсе. Со стороны
/// это выглядит как «программа сломалась», хотя сломана была только память.
///
/// Поэтому: выбрали модель — остальные отпускаем. Ошибки здесь не важны, это
/// уборка; не вышло — значит, память освободится сама по истечении срока.
pub async fn unload_others(host: &str, keep: &str) {
    // Выгружаем только то, что грузили сами. Ollama — общая служба: рядом может
    // работать чужая программа или открытый `ollama run`, и выгонять их модель
    // из памяти мы не вправе. Со стороны соседа это выглядит как внезапная
    // задержка на десятки секунд, пока его модель грузится заново.
    let ours: Vec<String> = WARMED
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .iter()
        .cloned()
        .collect();
    let client = match crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(_) => return,
    };

    let loaded: Vec<String> = match client.get(format!("{host}/api/ps")).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(body) => body["models"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|m| m["name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            Err(_) => return,
        },
        Err(_) => return,
    };

    for name in loaded {
        // `llama3` и `llama3:latest` — одна и та же модель.
        if name == keep || name == format!("{keep}:latest") || keep == format!("{name}:latest") {
            continue;
        }
        if !ours.iter().any(|mine| same_model(mine, &name)) {
            continue;
        }
        // Нулевой срок жизни — просьба выгрузить прямо сейчас.
        let _ = client
            .post(format!("{host}/api/generate"))
            .json(&serde_json::json!({
                "model": name,
                "prompt": "",
                "stream": false,
                "keep_alive": 0,
            }))
            .send()
            .await;
        log::info!("выгружаю из памяти лишнюю модель {name}");
    }
}

/* ── Подбор модели под машину ─────────────────────────────────────────── */

/// Сколько на этой машине памяти, в гигабайтах.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hardware {
    /// Видеопамять самой большой видеокарты. Ноль — не нашли или её нет.
    pub vram_gb: f64,
    pub ram_gb: f64,
}

/// Какую модель ставить на этой машине.
///
/// Выбор не «самая большая, какая влезет», а «самая большая из тех, что отвечают
/// мгновенно». Суфлёр — не собеседник, у него одна работа: коротко объяснить
/// выделенный термин. Модель вдвое крупнее даёт на этой работе прибавку, которую
/// не видно, а ждать ответа заставляет заметно дольше — и вылезает из
/// видеопамяти, начиная считать на процессоре, что медленнее в разы.
///
/// Отсюда потолок: берём то, что целиком помещается в видеопамять с запасом на
/// контекст и на всё остальное, чем занята карта. Без видеокарты считать будет
/// процессор, и там оправдана только самая маленькая модель.
pub fn pick(hw: &Hardware) -> &'static str {
    // Запас в пару гигабайт — под контекст, под рабочий стол и под то, что на
    // карте уже что-то открыто. Без него модель «по размеру» упиралась бы в
    // потолок и половину слоёв считала на процессоре.
    let usable = hw.vram_gb - 2.0;

    if usable >= 5.0 {
        // ~4.7 ГБ. Заметно грамотнее в терминах и определениях.
        "qwen2.5:7b"
    } else if usable >= 3.5 {
        // ~3.3 ГБ. Ровно то, что нужно для коротких объяснений.
        "gemma3:4b"
    } else if usable >= 2.0 {
        // ~1.9 ГБ.
        "qwen2.5:3b"
    } else if hw.ram_gb >= 8.0 {
        // Видеокарты нет или она мала — считать будет процессор. Здесь важен
        // только размер: ~815 МБ отвечают за секунды, всё крупнее — минутами.
        "gemma3:1b"
    } else {
        "qwen2.5:0.5b"
    }
}

/// Сколько памяти на этой машине.
pub fn hardware() -> Hardware {
    Hardware {
        vram_gb: vram_gb(),
        ram_gb: ram_gb(),
    }
}

/// Видеопамять по данным nvidia-smi.
///
/// Через утилиту, а не через системный API, и это осознанно: WMI на Windows
/// врёт про карты больше четырёх гигабайт (поле 32-разрядное и переполняется),
/// а разбирать DXGI ради одного числа — несоразмерно. У кого нет nvidia-smi,
/// тот получит ноль и маленькую модель: медленнее, чем могло бы быть, но
/// работает везде. AMD и Intel сюда попадают именно так — честный TODO.
fn vram_gb() -> f64 {
    let mut command = std::process::Command::new("nvidia-smi");
    command.args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"]);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = command.output() else {
        return 0.0;
    };

    // Карт может быть несколько — берём самую большую: именно на неё Ollama и
    // положит модель.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<f64>().ok())
        .fold(0.0_f64, f64::max)
        / 1024.0
}

#[cfg(target_os = "windows")]
fn ram_gb() -> f64 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: структура заполнена целиком, длина проставлена — этого функция и ждёт.
    match unsafe { GlobalMemoryStatusEx(&mut status) } {
        Ok(()) => status.ullTotalPhys as f64 / 1024.0 / 1024.0 / 1024.0,
        Err(_) => 0.0,
    }
}

#[cfg(target_os = "linux")]
fn ram_gb() -> f64 {
    let Ok(text) = std::fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    text.lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.trim().trim_end_matches(" kB").trim().parse::<f64>().ok())
        .map(|kb| kb / 1024.0 / 1024.0)
        .unwrap_or(0.0)
}

#[cfg(target_os = "macos")]
fn ram_gb() -> f64 {
    std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok())
        .map(|bytes| bytes / 1024.0 / 1024.0 / 1024.0)
        .unwrap_or(0.0)
}

// Android и iOS: своей Ollama там нет и быть не может — подбирать нечего.
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn ram_gb() -> f64 {
    0.0
}

/// Что сейчас установлено. Молчаливо: неответ Ollama — не ошибка, а «не
/// запущена», и окно покажет это отдельным сообщением.
pub async fn status(host: &str) -> Status {
    let present = executable().is_some();
    let client = crate::net::client_builder().build().unwrap_or_default();
    let url = format!("{host}/api/tags");

    let response = match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => {
            return Status {
                running: false,
                present,
                installed: Vec::new(),
            }
        }
    };

    let Ok(tags) = response.json::<TagsResponse>().await else {
        return Status {
            running: false,
            present,
            installed: Vec::new(),
        };
    };

    let mut installed: Vec<Model> = tags
        .models
        .into_iter()
        .map(|m| Model {
            name: m.name,
            size_gb: (m.size as f64 / 1e9 * 10.0).round() / 10.0,
        })
        .collect();
    installed.sort_by(|a, b| a.name.cmp(&b.name));

    Status {
        running: true,
        present: true,
        installed,
    }
}

/// Скачивает модель, докладывая о ходе событиями.
///
/// Поток, а не один запрос: файл на несколько гигабайт качается минутами, и
/// молчаливое ожидание неотличимо от зависания. Ollama отдаёт построчный JSON,
/// поэтому читаем кусками и разбираем по строкам — целиком такой ответ
/// не разобрать.
pub async fn pull(app: AppHandle, host: String, model: String) -> Result<(), String> {
    let client = crate::net::client_builder().build().unwrap_or_default();
    let url = format!("{host}/api/pull");

    let mut response = client
        .post(&url)
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(|err| format!("Ollama не ответила: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("Ollama ответила ошибкой {}", response.status()));
    }

    let mut buffer = String::new();
    let mut last_percent = u8::MAX;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("загрузка прервалась: {err}"))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find('\n') {
            let line: String = buffer.drain(..=end).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Ok(parsed) = serde_json::from_str::<PullLine>(line) else {
                continue;
            };

            if let Some(error) = parsed.error {
                emit(&app, &model, 0, &error, true, Some(error.clone()));
                return Err(error);
            }

            let percent = if parsed.total > 0 {
                ((parsed.completed as f64 / parsed.total as f64) * 100.0) as u8
            } else {
                0
            };

            // Событие на каждую строку — это сотни событий в секунду на пустом
            // месте: Ollama шлёт их щедро. Окну хватает шага в один процент.
            if percent != last_percent || parsed.status.contains("success") {
                last_percent = percent;
                emit(&app, &model, percent, &parsed.status, false, None);
            }
        }
    }

    emit(&app, &model, 100, "готово", true, None);
    Ok(())
}

fn emit(app: &AppHandle, model: &str, percent: u8, status: &str, done: bool, error: Option<String>) {
    let _ = app.emit(
        PULL_EVENT,
        Progress {
            model: model.to_string(),
            percent,
            status: human(status),
            done,
            error,
        },
    );
}

/// Ollama рапортует по-английски и подробностями, которые человеку не нужны.
fn human(status: &str) -> String {
    let status = status.trim();
    if status.starts_with("pulling manifest") {
        return "готовлюсь".into();
    }
    if status.starts_with("pulling") {
        return "скачиваю".into();
    }
    if status.starts_with("verifying") {
        return "проверяю".into();
    }
    if status.starts_with("writing") || status.starts_with("extracting") {
        return "распаковываю".into();
    }
    if status.contains("success") {
        return "готово".into();
    }
    status.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(vram: f64, ram: f64) -> Hardware {
        Hardware {
            vram_gb: vram,
            ram_gb: ram,
        }
    }

    #[test]
    fn model_is_picked_by_video_memory() {
        // Запас в 2 ГБ учтён: 8 ГБ карта — это 6 ГБ под модель.
        assert_eq!(pick(&hw(12.0, 32.0)), "qwen2.5:7b");
        assert_eq!(pick(&hw(10.0, 32.0)), "qwen2.5:7b");
        assert_eq!(pick(&hw(6.0, 16.0)), "gemma3:4b");
        assert_eq!(pick(&hw(4.0, 16.0)), "qwen2.5:3b");
    }

    #[test]
    fn without_video_card_size_matters_more_than_quality() {
        // Считать будет процессор: важен не класс модели, а то, дождётся ли
        // человек ответа вообще.
        assert_eq!(pick(&hw(0.0, 32.0)), "gemma3:1b");
        assert_eq!(pick(&hw(2.0, 16.0)), "gemma3:1b");
        assert_eq!(pick(&hw(0.0, 4.0)), "qwen2.5:0.5b");
    }

    #[test]
    fn default_endpoint_points_at_default_host() {
        assert!(DEFAULT_ENDPOINT.starts_with(DEFAULT_HOST));
        assert_eq!(host_from(DEFAULT_ENDPOINT), DEFAULT_HOST);
    }

    #[test]
    fn host_is_cut_before_api() {
        assert_eq!(
            host_from("http://localhost:11434/api/chat"),
            "http://localhost:11434"
        );
        assert_eq!(
            host_from("http://192.168.0.5:11434/api/chat"),
            "http://192.168.0.5:11434"
        );
    }

    #[test]
    fn foreign_endpoint_falls_back_to_default() {
        // Настроен облачный сервис — спрашивать у него про локальные модели
        // бессмысленно, идём к своей Ollama.
        assert_eq!(
            host_from("https://openrouter.ai/api/v1/chat/completions"),
            DEFAULT_HOST
        );
        assert_eq!(host_from(""), DEFAULT_HOST);
        assert_eq!(host_from("совсем не адрес"), DEFAULT_HOST);
    }

    #[test]
    fn english_chatter_becomes_russian() {
        assert_eq!(human("pulling manifest"), "готовлюсь");
        assert_eq!(human("pulling 8934d96d3f08"), "скачиваю");
        assert_eq!(human("verifying sha256 digest"), "проверяю");
        assert_eq!(human("success"), "готово");
    }
}
