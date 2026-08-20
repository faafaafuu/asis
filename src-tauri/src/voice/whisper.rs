//! Расшифровка речи: whisper.cpp рядом, готовой сборкой.
//!
//! Сборок две, и выбирается она сама. С видеокартой NVIDIA берётся вариант с
//! CUDA: он тяжелее (полгигабайта против двадцати мегабайт), но расшифровывает
//! фразу за доли секунды вместо десятков секунд. Без видеокарты качать эти
//! полгигабайта незачем — там всё равно считает процессор.

use tauri::{AppHandle, Emitter};

use super::assets;

const EVENT: &str = "speech:install";

/// Версия закреплена: это исполняемый файл, который мы запускаем у человека.
const RELEASE: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2";
const CUDA_ZIP: &str = "whisper-cublas-12.4.0-bin-x64.zip";
const CPU_ZIP: &str = "whisper-blas-bin-x64.zip";

/// Модель. `large-v3-turbo` вместо `medium`: почти тот же размер (1.5 против
/// 1.4 ГБ), но заметно точнее и в разы быстрее — turbo для того и сделан.
const MODEL: &str = "ggml-large-v3-turbo.bin";
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";

fn root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(assets::dir(app)?.join("whisper"))
}

/// Путь к программе. Имя менялось между выпусками (`main.exe` в старых,
/// `whisper-cli.exe` в новых), поэтому ищем оба и в подпапках тоже.
pub fn binary(app: &AppHandle) -> Option<std::path::PathBuf> {
    let root = root(app).ok()?;
    for name in ["whisper-cli.exe", "main.exe"] {
        let direct = root.join(name);
        if direct.exists() {
            return Some(direct);
        }
        // Часть сборок кладёт всё в подпапку вроде `Release/`.
        let nested = std::fs::read_dir(&root).ok()?.flatten().find_map(|entry| {
            let candidate = entry.path().join(name);
            candidate.exists().then_some(candidate)
        });
        if nested.is_some() {
            return nested;
        }
    }
    None
}

pub fn model(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(root(app)?.join(MODEL))
}

pub fn ready(app: &AppHandle) -> bool {
    binary(app).is_some() && model(app).map(|p| p.exists()).unwrap_or(false)
}

/// Скачивает всё нужное для расшифровки.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let root = root(&app)?;
    std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;

    if binary(&app).is_none() {
        // Видеокарта решает, какую сборку брать. Проверка та же, что и при
        // подборе языковой модели.
        let cuda = crate::ollama::hardware().vram_gb >= 2.0;
        let name = if cuda { CUDA_ZIP } else { CPU_ZIP };
        log::info!("беру сборку whisper: {name}");

        let archive = root.join("whisper.zip");
        emit(&app, 0, "скачиваю распознавание", false, None);
        assets::download(&app, EVENT, &format!("{RELEASE}/{name}"), &archive, 0, 30).await?;

        emit(&app, 30, "распаковываю", false, None);
        assets::unzip(&archive, &root)?;
        let _ = std::fs::remove_file(&archive);

        if binary(&app).is_none() {
            return Err("в архиве whisper не нашлось программы".into());
        }
    }

    let model = model(&app)?;
    if !model.exists() {
        emit(&app, 35, "скачиваю модель речи", false, None);
        assets::download(&app, EVENT, MODEL_URL, &model, 35, 99).await?;
    }

    emit(&app, 100, "готово", true, None);
    Ok(())
}

/// Расшифровывает запись. На входе — готовый WAV, на выходе — текст.
pub fn transcribe(app: &AppHandle, wav: &[u8], language: &str) -> Result<String, String> {
    let exe = binary(app).ok_or("распознавание речи ещё не скачано")?;
    let model = model(app)?;
    if !model.exists() {
        return Err("модель распознавания ещё не скачана".into());
    }

    // Файл, а не поток: whisper.cpp читает именно файл. Имя со временем, чтобы
    // две записи подряд не наступили друг на друга.
    let path = std::env::temp_dir().join(format!(
        "sufler-{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::write(&path, wav).map_err(|err| format!("не удалось записать звук: {err}"))?;

    let mut command = std::process::Command::new(&exe);
    command
        .arg("-m")
        .arg(&model)
        .arg("-f")
        .arg(&path)
        .arg("-l")
        .arg(language)
        // Без отметок времени и без служебной болтовни: нам нужен только текст.
        .arg("-nt")
        .arg("-np")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|err| format!("распознавание не запустилось: {err}"));
    let _ = std::fs::remove_file(&path);
    let output = output?;

    if !output.status.success() {
        return Err("распознавание завершилось ошибкой".into());
    }
    Ok(clean(&String::from_utf8_lossy(&output.stdout)))
}

/// Приводит вывод программы к одной строке вопроса.
///
/// Whisper печатает расшифровку строками по фразам, а на тишине выдаёт пометки
/// вроде `[BLANK_AUDIO]` или `(тишина)` — их в вопрос пускать нельзя.
fn clean(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !(line.starts_with('[') && line.ends_with(']')))
        .filter(|line| !(line.starts_with('(') && line.ends_with(')')))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn emit(app: &AppHandle, percent: u8, status: &str, done: bool, error: Option<String>) {
    let _ = app.emit(
        EVENT,
        serde_json::json!({
            "percent": percent,
            "status": status,
            "done": done,
            "error": error,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_marks_do_not_become_a_question() {
        assert_eq!(clean("[BLANK_AUDIO]"), "");
        assert_eq!(clean("(тишина)"), "");
        assert_eq!(
            clean("  Что такое альбедо?\n\n [BLANK_AUDIO] \n"),
            "Что такое альбедо?"
        );
    }

    #[test]
    fn phrases_join_into_one_line() {
        assert_eq!(
            clean("Первая фраза.\nВторая фраза."),
            "Первая фраза. Вторая фраза."
        );
    }
}
