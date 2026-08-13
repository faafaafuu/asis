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
const DEFAULT_HOST: &str = "http://127.0.0.1:11434";

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
/// Сначала оконное приложение: на Windows именно оно поднимает сервер и живёт
/// в трее, как это делает сам человек, запуская Ollama из меню «Пуск».
/// Консольный `ollama` — запасной вариант, ему нужна команда `serve`.
pub fn executable() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let dir = std::path::Path::new(&local).join("Programs").join("Ollama");
        for name in ["ollama app.exe", "ollama.exe"] {
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
