//! Загрузка того, чем говорят: программы Piper и голоса к ней.
//!
//! Отдельно от синтеза: качается это один раз в жизни, а говорится постоянно.
//! Логика та же, что у установки Ollama (`ollama::install`) — событие с
//! процентами, проверка размера, ничего не запускаем недокачанным.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Событие с ходом загрузки. Отдельное от моделей и от Ollama: три разных
/// прогресса в одном событии окно не разберёт.
const EVENT: &str = "voice:install";

/// Программа синтеза. Версия закреплена намеренно: это исполняемый файл, который
/// мы запускаем на машине человека, и «последний релиз» тут означал бы, что
/// содержимое меняется само собой.
const PIPER_URL: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";

/// Голоса. Все medium: small заметно беднее интонацией, high вчетверо тяжелее
/// при разнице, которую на коротком объяснении не слышно.
pub const VOICES: &[(&str, &str)] = &[
    ("ru_RU-irina-medium", "Ирина — женский, спокойный"),
    ("ru_RU-dmitri-medium", "Дмитрий — мужской, мягкий"),
    ("ru_RU-denis-medium", "Денис — мужской, ровный"),
    ("ru_RU-ruslan-medium", "Руслан — мужской, низкий"),
];

pub const DEFAULT_VOICE: &str = "ru_RU-irina-medium";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Progress {
    percent: u8,
    status: String,
    done: bool,
    error: Option<String>,
}

/// Куда всё складывается. Рядом с остальными данными приложения, а не в Program
/// Files: установка идёт без прав администратора.
pub fn dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("voice"))
        .map_err(|err| format!("не удалось определить каталог данных: {err}"))
}

pub fn piper_exe(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(dir(app)?.join("piper").join("piper.exe"))
}

pub fn voice_path(app: &AppHandle, voice: &str) -> Result<std::path::PathBuf, String> {
    Ok(dir(app)?.join("voices").join(format!("{voice}.onnx")))
}

/// Всё ли на месте для озвучивания этим голосом.
pub fn ready(app: &AppHandle, voice: &str) -> bool {
    piper_exe(app).map(|p| p.exists()).unwrap_or(false)
        && voice_path(app, voice).map(|p| p.exists()).unwrap_or(false)
}

/// Скачивает Piper и голос, если их ещё нет.
pub async fn install(app: AppHandle, voice: String) -> Result<(), String> {
    let root = dir(&app)?;
    std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;

    if !piper_exe(&app)?.exists() {
        emit(&app, 0, "скачиваю синтезатор", false, None);
        let archive = root.join("piper.zip");
        download(&app, EVENT, PIPER_URL, &archive, 0, 45).await?;

        emit(&app, 45, "распаковываю", false, None);
        unzip(&archive, &root)?;
        let _ = std::fs::remove_file(&archive);

        if !piper_exe(&app)?.exists() {
            return Err("в архиве Piper не оказалось piper.exe".into());
        }
    }

    let onnx = voice_path(&app, &voice)?;
    if !onnx.exists() {
        if let Some(parent) = onnx.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }

        // Файла два: веса и описание. Без описания Piper не знает ни частоты
        // дискретизации, ни набора символов — то есть не работает вовсе.
        let base = voice_base_url(&voice);
        emit(&app, 50, "скачиваю голос", false, None);
        download(&app, EVENT, &format!("{base}.onnx"), &onnx, 50, 95).await?;
        download(&app, EVENT, &format!("{base}.onnx.json"), &json_beside(&onnx), 95, 99).await?;
    }

    emit(&app, 100, "готово", true, None);
    Ok(())
}

/// Путь описания рядом с весами: `голос.onnx` в `голос.onnx.json`.
///
/// Именно приписыванием, а не через `with_extension`: тот заменил бы `.onnx`
/// на `.json`, а Piper ищет файл ровно с двойным расширением.
pub fn json_beside(onnx: &std::path::Path) -> std::path::PathBuf {
    let mut name = onnx.as_os_str().to_os_string();
    name.push(".json");
    std::path::PathBuf::from(name)
}

/// Адрес голоса в репозитории Piper: `ru_RU-irina-medium` лежит по пути
/// `ru/ru_RU/irina/medium/`.
fn voice_base_url(voice: &str) -> String {
    let mut parts = voice.split("-");
    let locale = parts.next().unwrap_or("ru_RU");
    let name = parts.next().unwrap_or("irina");
    let quality = parts.next().unwrap_or("medium");
    let language = locale.split("_").next().unwrap_or("ru");
    format!(
        "https://huggingface.co/rhasspy/piper-voices/resolve/main/{language}/{locale}/{name}/{quality}/{voice}"
    )
}

/// Качает файл, пересчитывая проценты в отведённый ему отрезок общей полосы.
pub async fn download(
    app: &AppHandle,
    event: &str,
    url: &str,
    to: &std::path::Path,
    from_percent: u8,
    to_percent: u8,
) -> Result<(), String> {
    let client = crate::net::client_builder()
        .user_agent("Sufler")
        .timeout(std::time::Duration::from_secs(60 * 60))
        .build()
        .map_err(|err| err.to_string())?;

    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("загрузка не началась: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("сервер ответил {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    // Пишем во временный файл рядом: оборванная загрузка не должна оставить
    // после себя огрызок, который в следующий раз примут за готовый файл.
    let temp = to.with_extension("part");
    let mut file = std::fs::File::create(&temp).map_err(|err| err.to_string())?;

    let mut written: u64 = 0;
    let mut last = u8::MAX;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("загрузка прервалась: {err}"))?
    {
        use std::io::Write;
        file.write_all(&chunk).map_err(|err| err.to_string())?;
        written += chunk.len() as u64;

        if total > 0 {
            let span = f64::from(to_percent.saturating_sub(from_percent));
            let percent = from_percent + ((written as f64 / total as f64) * span) as u8;
            if percent != last {
                last = percent;
                emit_to(app, event, percent, "скачиваю", false, None);
            }
        }
    }
    drop(file);

    if total > 0 && written != total {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("файл скачался не полностью: {written} из {total}"));
    }
    std::fs::rename(&temp, to).map_err(|err| err.to_string())
}

/// Распаковка архива Piper.
///
/// Имена внутри проверяем: путь с двумя точками в архиве означает запись куда
/// угодно на диске, и это не теория, а известный приём.
pub fn unzip(archive: &std::path::Path, into: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|err| err.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|err| format!("архив не читается: {err}"))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|err| err.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err("в архиве есть запись с недопустимым путём".into());
        };
        let target = into.join(name);

        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|err| err.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|err| err.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn emit(app: &AppHandle, percent: u8, status: &str, done: bool, error: Option<String>) {
    emit_to(app, EVENT, percent, status, done, error);
}

/// Событие о ходе загрузки. Имя события — параметром: тем же загрузчиком
/// качается и голос, и распознавание, а окно должно их различать.
fn emit_to(
    app: &AppHandle,
    event: &str,
    percent: u8,
    status: &str,
    done: bool,
    error: Option<String>,
) {
    let _ = app.emit(
        event,
        Progress {
            percent,
            status: status.to_string(),
            done,
            error,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_name_becomes_repository_path() {
        assert_eq!(
            voice_base_url("ru_RU-irina-medium"),
            "https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU/irina/medium/ru_RU-irina-medium"
        );
        assert_eq!(
            voice_base_url("en_US-amy-low"),
            "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/low/en_US-amy-low"
        );
    }

    #[test]
    fn description_lies_next_to_the_weights() {
        let onnx = std::path::Path::new("C:/voices/ru_RU-irina-medium.onnx");
        assert_eq!(
            json_beside(onnx),
            std::path::PathBuf::from("C:/voices/ru_RU-irina-medium.onnx.json")
        );
    }

    #[test]
    fn default_voice_is_on_the_list() {
        assert!(VOICES.iter().any(|(name, _)| *name == DEFAULT_VOICE));
    }
}
