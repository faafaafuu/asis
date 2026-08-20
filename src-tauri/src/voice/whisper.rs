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

/// Порт, на котором живёт свой сервер расшифровки.
///
/// Не 8080 и не другой ходовой номер: занятый порт означал бы, что мы стучимся
/// в чужую программу и шлём ей записи с микрофона.
const PORT: u16 = 8642;

/// Путь к программе. Имя менялось между выпусками (`main.exe` в старых,
/// `whisper-cli.exe` в новых), поэтому ищем оба и в подпапках тоже.
pub fn binary(app: &AppHandle) -> Option<std::path::PathBuf> {
    exe(app, &["whisper-cli.exe", "main.exe"])
}

/// Сервер расшифровки: та же сборка, соседний файл.
fn server_exe(app: &AppHandle) -> Option<std::path::PathBuf> {
    exe(app, &["whisper-server.exe"])
}

fn exe(app: &AppHandle, names: &[&str]) -> Option<std::path::PathBuf> {
    let root = root(app).ok()?;
    for name in names {
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

/// Идёт ли сейчас запуск сервера. Чтобы не поднимать его дважды.
static STARTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Поднимает сервер заранее, не дожидаясь конца фразы.
///
/// Первый запуск занимает секунды: полтора гигабайта модели надо положить
/// в видеопамять, а на новой видеокарте драйвер ещё и компилирует под неё ядра.
/// Раньше это происходило уже после того, как человек договорил, — он ждал
/// молча и не понимал, работает программа или нет. Теперь сервер поднимается,
/// пока фраза ещё произносится: к моменту, когда её надо расшифровать, он готов.
pub fn warm(app: &AppHandle) {
    use std::sync::atomic::Ordering;

    if server_alive() || STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-whisper-warm".into())
        .spawn(move || {
            if let Err(err) = ensure_server(&app) {
                log::warn!("сервер расшифровки не поднялся заранее: {err}");
            }
            STARTING.store(false, Ordering::SeqCst);
        })
        .ok();
}

/// Отвечает ли сервер расшифровки.
fn server_alive() -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], PORT)),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

/// Поднимает сервер расшифровки, если он ещё не отвечает.
///
/// Сервер, а не запуск программы на каждый вопрос, — и вот почему. Модель весит
/// полтора гигабайта, и загрузка её в видеопамять занимает около восьми секунд.
/// Сама расшифровка при этом — доли секунды. Запуская программу каждый раз, мы
/// платили бы эти восемь секунд за каждый вопрос; с сервером — один раз.
///
/// Поднимается при первом вопросе, а не при запуске программы: держать полтора
/// гигабайта видеопамяти занятыми у того, кто голосом не пользуется, незачем.
fn ensure_server(app: &AppHandle) -> Result<(), String> {
    if server_alive() {
        return Ok(());
    }

    let exe = server_exe(app).ok_or("распознавание речи ещё не скачано")?;
    let model = model(app)?;
    if !model.exists() {
        return Err("модель распознавания ещё не скачана".into());
    }

    let mut command = std::process::Command::new(&exe);
    command
        .arg("-m")
        .arg(&model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(PORT.to_string())
        // Лучевой поиск вместо жадного разбора.
        //
        // По умолчанию сервер берёт первое подходящее слово и идёт дальше
        // (`beam-size -1`). Это самый быстрый способ и самый неточный: одна
        // неудачно угаданная середина слова тянет за собой остаток фразы.
        // С лучом модель держит пять вариантов сразу и выбирает лучший по всей
        // фразе. На видеокарте это доли секунды разницы, а ошибок заметно меньше.
        .arg("--beam-size")
        .arg("5")
        .arg("--best-of")
        .arg("5")
        // Порог «здесь никто не говорит» строже умолчания (0.6): чем он ниже,
        // тем охотнее модель признаёт отрезок тишиной и молчит вместо того,
        // чтобы выдумывать. Для программы, которая слушает комнату постоянно,
        // лишнее молчание безопаснее лишней выдумки.
        .arg("--no-speech-thold")
        .arg("0.35")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .map_err(|err| format!("сервер расшифровки не запустился: {err}"))?;
    log::info!("поднимаю сервер расшифровки");

    // Первый запуск на новой видеокарте дольше: драйвер компилирует ядра под
    // неё и складывает в свой кеш. Дальше это уже секунды.
    for _ in 0..300 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if server_alive() {
            log::info!("сервер расшифровки готов");
            return Ok(());
        }
    }
    Err("сервер расшифровки не ответил".into())
}

/// Расшифровывает запись. На входе — готовый WAV, на выходе — текст.
pub async fn transcribe(
    app: &AppHandle,
    wav: Vec<u8>,
    language: &str,
    prompt: &str,
) -> Result<String, String> {
    ensure_server(app)?;

    let form = reqwest::multipart::Form::new()
        .text("response_format", "text")
        .text("language", language.to_string())
        // Подсказка о том, про что сейчас разговор.
        //
        // Человек выделил термин и спрашивает про него же — значит слово почти
        // наверняка прозвучит в вопросе. Названное заранее, оно перестаёт быть
        // для модели редким, и она перестаёт подменять его похожим по звучанию.
        // Особенно это заметно на именах, сокращениях и всём иностранном.
        .text("prompt", prompt.to_string())
        // Без перебора температур: он спасает на плохой записи, но там же
        // и выдумывает. Нам честное «не расслышал» полезнее выдумки.
        .text("temperature", "0.0")
        // Имя файла обязательно: без него сервер не признаёт часть формы файлом.
        .part(
            "file",
            reqwest::multipart::Part::bytes(wav)
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|err| err.to_string())?,
        );

    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;

    let response = client
        .post(format!("http://127.0.0.1:{PORT}/inference"))
        .multipart(form)
        .send()
        .await
        .map_err(|err| format!("сервер расшифровки не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("расшифровка ответила {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|err| format!("ответ расшифровки не прочитался: {err}"))?;
    let text = clean(&text);
    if is_hallucination(&text) {
        log::info!("расшифровка похожа на выдумку модели — пропускаю: «{text}»");
        return Ok(String::new());
    }
    Ok(text)
}

/// Фразы, которые модель выдумывает на тишине.
///
/// Это не догадка, а известное свойство Whisper: её учили на субтитрах к
/// роликам, и в тишине она выдаёт то, что в тех субтитрах встречалось чаще
/// всего, — подписи переводчиков, просьбы подписаться, «продолжение следует».
/// Для программы, которая слушает комнату постоянно, это не мелочь: выдуманная
/// фраза уходит вопросом, на неё приходит ответ, ответ звучит вслух — и всё
/// повторяется, пока человек не выключит звук.
///
/// Сравниваем по кускам, а не целиком: подпись автора субтитров каждый раз
/// немного другая, а слово «субтитр» в ней есть всегда.
const HALLUCINATIONS: &[&str] = &[
    "субтитр",
    "dimatorzok",
    "торжок",
    "продолжение следует",
    "спасибо за просмотр",
    "спасибо за внимание",
    "подписывайтесь",
    "подпишись",
    // «Редактор субтитров», «корректор» и «переводчик» встречаются в подписях
    // к роликам, но сами по себе это обычные слова: вопрос «что такое текстовый
    // редактор» — не выдумка. Ловим только те сочетания, которые в живой речи
    // не встречаются.
    "редактор субтитров",
    "корректор а.",
    "amara.org",
    "thanks for watching",
    "thank you for watching",
];

/// Похоже ли, что модель это выдумала, а не расслышала.
fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();
    if HALLUCINATIONS.iter().any(|mark| lower.contains(mark)) {
        return true;
    }

    // Совсем короткое — тоже выдумка. На тишине модель любит выдавать одинокое
    // «Да.», «Ну.», «You» — вопросом это быть не может, а петлю запускает так же.
    let letters = lower.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 4 {
        return true;
    }

    // Заклинившая модель повторяет одно слово: «Пока, пока, пока, пока».
    // Так она ведёт себя на шуме, и вопросом это не бывает никогда — живая речь
    // из одного слова, повторённого четырежды, не состоит.
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.len() >= 4 {
        let mut unique = words.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() <= 2 {
            return true;
        }
    }

    false
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
    fn invented_subtitles_do_not_become_questions() {
        assert!(is_hallucination("Субтитры сделал DimaTorzok"));
        assert!(is_hallucination("Продолжение следует..."));
        assert!(is_hallucination("Спасибо за просмотр!"));
        assert!(is_hallucination("Редактор субтитров А.Синецкая"));
        // А обычные слова из тех же подписей — не повод молчать.
        assert!(!is_hallucination("что такое текстовый редактор"));
        assert!(!is_hallucination("кто такой переводчик в компиляторе"));
        // Короткое тоже не вопрос.
        assert!(is_hallucination("Да."));
        assert!(is_hallucination("You"));
        // Заклинившая модель повторяет одно слово.
        assert!(is_hallucination("Пока, пока, пока, пока, пока."));
        assert!(is_hallucination("да да да да"));
    }

    #[test]
    fn real_questions_survive() {
        assert!(!is_hallucination("Что такое альбедо?"));
        assert!(!is_hallucination("Понял, спасибо"));
        assert!(!is_hallucination("а можно проще"));
    }

    #[test]
    fn phrases_join_into_one_line() {
        assert_eq!(
            clean("Первая фраза.\nВторая фраза."),
            "Первая фраза. Вторая фраза."
        );
    }
}
