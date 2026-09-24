//! Буфер обмена и экран: прочитать, что там написано, и ответить.
//!
//! «Это правда?» про новость — самый частый вопрос сюда, и отвечать на него по
//! памяти модели нельзя: сегодняшних новостей она не знает и охотно выдумает.
//! Поэтому текст берётся из свежего буфера обмена — скопированный текст как
//! есть, скриншот через распознавание текста Windows (оно встроено в систему и
//! работает без сети и без видеопамяти), — а проверка идёт по свежему поиску
//! (см. `web::fact_check`).
//!
//! Свежего в буфере нет — берётся текст окна, которое было впереди: у браузера
//! это текст открытой страницы.

use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Форматы буфера обмена: картинка — её кладут и «Ножницы», и Print Screen, —
/// и текст.
#[cfg(target_os = "windows")]
const CF_DIB: u32 = 8;
#[cfg(target_os = "windows")]
const CF_UNICODETEXT: u32 = 13;

/// Сколько содержимое буфера считается свежим: скопировали — и сразу спросили.
///
/// Старое без прямой просьбы не берётся: иначе «это правда?» про открытую
/// новость проверяло бы то, что скопировали вчера.
const FRESH: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Номер последнего изменения буфера и когда его заметили. Время `None` —
/// содержимое лежало ещё до запуска программы, и сколько ему, неизвестно.
static CLIPBOARD: std::sync::Mutex<Option<(u32, Option<std::time::Instant>)>> =
    std::sync::Mutex::new(None);

/// Картинка в памяти: строки сверху вниз, четыре байта на точку — B, G, R, A.
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Что лежит в буфере.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content {
    Image,
    Text,
    Nothing,
}

/// Следит за буфером обмена: запоминает, когда в нём появилось новое.
///
/// Дёшево: номер изменения Windows отдаёт без открытия буфера.
pub fn watch() {
    std::thread::Builder::new()
        .name("sufler-clipboard".into())
        .spawn(|| loop {
            let number = sequence();
            {
                let mut seen = CLIPBOARD.lock().unwrap_or_else(|err| err.into_inner());
                match *seen {
                    Some((known, _)) if known == number => {}
                    Some(_) => *seen = Some((number, Some(std::time::Instant::now()))),
                    None => *seen = Some((number, None)),
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
        })
        .ok();
}

/// Свежее ли содержимое буфера.
fn fresh() -> bool {
    let seen = *CLIPBOARD.lock().unwrap_or_else(|err| err.into_inner());
    matches!(seen, Some((number, Some(at))) if number == sequence() && at.elapsed() < FRESH)
}

fn content() -> Content {
    if has_image() {
        Content::Image
    } else if clipboard_text().is_some_and(|text| text.trim().chars().count() >= 20) {
        Content::Text
    } else {
        Content::Nothing
    }
}

/// Строка для разбора реплики: что свежего в буфере.
///
/// Разбору надо знать, есть ли на что смотреть: «это правда?» при свежем
/// скриншоте или скопированном тексте — вопрос про них, а без них — про
/// открытую страницу.
pub fn clipboard_line() -> String {
    if !fresh() {
        return String::new();
    }
    match content() {
        Content::Image => "В буфере обмена свежий скриншот.".into(),
        Content::Text => {
            let text = clipboard_text().unwrap_or_default();
            let start = text.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
            format!("В буфере обмена свежий скопированный текст: «{start}…».")
        }
        Content::Nothing => String::new(),
    }
}

/// Отвечает на вопрос про буфер обмена или открытое окно.
pub async fn answer(app: &AppHandle, question: &str, check: bool) -> String {
    let lower = question.to_lowercase();
    let asked_image = ["скрин", "снимок", "снимк", "картинк"].iter().any(|word| lower.contains(word));
    let asked_text = ["текст", "скопир", "буфер"].iter().any(|word| lower.contains(word));
    let recent = fresh();
    let source =
        tauri::async_runtime::spawn_blocking(move || pick_source(asked_image, asked_text, recent))
            .await
            .unwrap_or_else(|err| Err(format!("Не смог прочитать буфер обмена: {err}.")));
    let (text, from) = match source {
        Ok(found) => found,
        Err(message) => return message,
    };
    let text: String = text.chars().take(4000).collect();
    if text.trim().is_empty() {
        return format!("Не вижу текста {from}.");
    }
    log::info!("текст {from}: {} символов", text.chars().count());

    if check {
        return crate::web::fact_check(app, &text, question).await;
    }

    // Перевод и «что тут написано» — длиннее: там весь смысл в самом тексте.
    let sentences = if ["перевед", "переведи", "написано", "прочитай", "прочти"]
        .iter()
        .any(|word| lower.contains(word))
    {
        6
    } else {
        2
    };
    let name = app.state::<AppState>().wake_name();
    let rules = format!(
        "Ты — {name}, голосовой помощник. Ниже текст {from}, который видит человек. \
         Ответь на его вопрос по этому тексту коротко, по-русски, обычным текстом без \
         списков и ссылок. Если ответа в тексте нет, так и скажи. Не выдумывай.\
         \n\nТекст:\n{text}"
    );
    let provider = app.state::<AppState>().provider();
    crate::web::native_answer(provider.as_ref(), &rules, question, sentences)
        .await
        .unwrap_or_else(|| "Не смог ответить: модель не ответила.".into())
}

/// Откуда брать текст: свежий скриншот, свежий скопированный текст или окно
/// впереди. Прямо названное — «скриншот», «текст» — берётся, сколько бы ему
/// ни было. Ошибка — готовая фраза для человека.
fn pick_source(
    asked_image: bool,
    asked_text: bool,
    recent: bool,
) -> Result<(String, &'static str), String> {
    let content = content();
    if asked_image && content != Content::Image {
        return Err("В буфере обмена нет скриншота — сделайте его и спросите ещё раз.".into());
    }
    if content == Content::Image && (asked_image || (recent && !asked_text)) {
        let image = clipboard_image().ok_or_else(|| "Не смог прочитать скриншот.".to_string())?;
        return recognize(&image).map(|text| (text, "со скриншота")).map_err(|err| {
            log::warn!("скриншот не распознался: {err}");
            "Не смог прочитать скриншот.".to_string()
        });
    }
    if content == Content::Text && (asked_text || recent) {
        if let Some(text) = clipboard_text() {
            return Ok((text, "из буфера обмена"));
        }
    }
    if asked_text {
        return Err("В буфере обмена нет текста — скопируйте его и спросите ещё раз.".into());
    }
    Ok((crate::pc::last_window_text().replace(" | ", "\n"), "открытого окна"))
}

#[cfg(target_os = "windows")]
fn sequence() -> u32 {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

    // SAFETY: только номер последнего изменения буфера.
    unsafe { GetClipboardSequenceNumber() }
}

#[cfg(target_os = "windows")]
fn has_image() -> bool {
    use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;

    // SAFETY: только спрашивает систему, есть ли в буфере такой формат.
    unsafe { IsClipboardFormatAvailable(CF_DIB).is_ok() }
}

/// Открывает буфер: он бывает занят другой программой — несколько попыток.
#[cfg(target_os = "windows")]
fn open_clipboard() -> bool {
    use windows::Win32::System::DataExchange::OpenClipboard;

    for _ in 0..10 {
        // SAFETY: открытие буфера; закрывает его вызывающий.
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    false
}

#[cfg(target_os = "windows")]
fn clipboard_image() -> Option<Image> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    if !has_image() || !open_clipboard() {
        return None;
    }
    // SAFETY: буфер открыт выше и закрывается здесь же; блок памяти читается
    // под замком и копируется до того, как замок снят.
    unsafe {
        let copied = GetClipboardData(CF_DIB).ok().and_then(|handle| {
            let block = HGLOBAL(handle.0);
            let pointer = GlobalLock(block) as *const u8;
            if pointer.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(pointer, GlobalSize(block)).to_vec();
            let _ = GlobalUnlock(block);
            Some(bytes)
        });
        let _ = CloseClipboard();
        dib_to_bgra(&copied?)
    }
}

/// Картинка из буфера обмена в PNG — чтобы отправить её в Telegram.
#[cfg(target_os = "windows")]
pub fn clipboard_png() -> Option<Vec<u8>> {
    let image = clipboard_image()?;
    crate::shots::png(&image.bgra, image.width, image.height).ok()
}

#[cfg(not(target_os = "windows"))]
pub fn clipboard_png() -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "windows")]
fn clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    // SAFETY: только спрашивает систему, есть ли в буфере текст.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_err() || !open_clipboard() {
        return None;
    }
    // SAFETY: буфер открыт выше и закрывается здесь же; текст копируется до
    // того, как снят замок с памяти.
    unsafe {
        let text = GetClipboardData(CF_UNICODETEXT).ok().and_then(|handle| {
            let block = HGLOBAL(handle.0);
            let pointer = GlobalLock(block) as *const u16;
            if pointer.is_null() {
                return None;
            }
            let units = std::slice::from_raw_parts(pointer, GlobalSize(block) / 2);
            let length = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..length]);
            let _ = GlobalUnlock(block);
            Some(text)
        });
        let _ = CloseClipboard();
        text
    }
}

/// Распознаёт текст на картинке средствами Windows — строками, как на экране.
#[cfg(target_os = "windows")]
pub(crate) fn recognize(image: &Image) -> Result<String, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Storage::Streams::DataWriter;

    let failed = |err: windows::core::Error| err.message().to_string();
    init_com();
    let writer = DataWriter::new().map_err(failed)?;
    writer.WriteBytes(&image.bgra).map_err(failed)?;
    let buffer = writer.DetachBuffer().map_err(failed)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        image.width as i32,
        image.height as i32,
    )
    .map_err(failed)?;
    recognize_bitmap(&bitmap)
}

/// COM в этом потоке: без него WinRT не создаёт ни картинку, ни распознаватель.
#[cfg(target_os = "windows")]
fn init_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    // SAFETY: инициализация COM в текущем потоке; повторная безвредна.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Распознаёт текст готовой картинки Windows.
#[cfg(target_os = "windows")]
fn recognize_bitmap(bitmap: &windows::Graphics::Imaging::SoftwareBitmap) -> Result<String, String> {
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;

    let failed = |err: windows::core::Error| err.message().to_string();
    init_com();
    // Русский, если он есть в системе; иначе — языки, выбранные у человека.
    let engine = Language::CreateLanguage(&HSTRING::from("ru"))
        .and_then(|russian| OcrEngine::TryCreateFromLanguage(&russian))
        .or_else(|_| OcrEngine::TryCreateFromUserProfileLanguages())
        .map_err(failed)?;
    let result = engine.RecognizeAsync(bitmap).map_err(failed)?.join().map_err(failed)?;
    let lines = result.Lines().map_err(failed)?;
    let mut text = Vec::new();
    for at in 0..lines.Size().map_err(failed)? {
        if let Ok(line) = lines.GetAt(at).and_then(|line| line.Text()) {
            let line = line.to_string();
            if !line.trim().is_empty() {
                text.push(line);
            }
        }
    }
    Ok(text.join("\n"))
}

#[cfg(not(target_os = "windows"))]
fn sequence() -> u32 {
    0
}

#[cfg(not(target_os = "windows"))]
fn has_image() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn clipboard_image() -> Option<Image> {
    None
}

#[cfg(not(target_os = "windows"))]
fn clipboard_text() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
fn recognize(_image: &Image) -> Result<String, String> {
    Err("распознавание текста есть только в Windows".into())
}

/// Картинка из формата DIB — так её кладут в буфер «Ножницы» и Print Screen.
///
/// Понимаются 24 и 32 бита на точку, строки снизу вверх и сверху вниз. Палитры
/// (8 бит и меньше) скриншоты не используют — такие картинки не разбираются.
fn dib_to_bgra(dib: &[u8]) -> Option<Image> {
    const BI_RGB: u32 = 0;
    const BI_BITFIELDS: u32 = 3;
    let read_u32 = |at: usize| {
        dib.get(at..at + 4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };

    let header = read_u32(0)? as usize;
    let width = read_u32(4)? as i32;
    let height = read_u32(8)? as i32;
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let compression = read_u32(16)?;
    let colors = read_u32(32).unwrap_or(0) as usize;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) {
        return None;
    }
    // Маски BI_BITFIELDS идут сразу за коротким заголовком; в длинных
    // заголовках они уже внутри. Необязательная таблица цветов — следом.
    let offset = match compression {
        BI_RGB if header == 40 => header + colors * 4,
        BI_RGB => header,
        BI_BITFIELDS if header == 40 => header + 12 + colors * 4,
        BI_BITFIELDS => header,
        _ => return None,
    };

    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let step = bits as usize / 8;
    let stride = (w * step + 3) & !3;
    let pixels = dib.get(offset..offset + stride * h)?;
    let mut bgra = vec![0u8; w * h * 4];
    for row in 0..h {
        // Положительная высота — строки хранятся снизу вверх.
        let source = if height > 0 { h - 1 - row } else { row };
        let line = &pixels[source * stride..source * stride + w * step];
        for x in 0..w {
            let from = x * step;
            let to = (row * w + x) * 4;
            bgra[to..to + 3].copy_from_slice(&line[from..from + 3]);
            bgra[to + 3] = 255;
        }
    }
    Some(Image {
        width: w as u32,
        height: h as u32,
        bgra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: i32, height: i32, bits: u16) -> Vec<u8> {
        let mut dib = vec![0u8; 40];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&width.to_le_bytes());
        dib[8..12].copy_from_slice(&height.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&bits.to_le_bytes());
        dib
    }

    #[test]
    fn a_bottom_up_dib_is_turned_right_side_up() {
        // 2×2, 24 бита: строка — шесть байт точек и два байта выравнивания.
        // Первой хранится нижняя строка: синяя и зелёная, за ней верхняя:
        // красная и белая.
        let mut dib = header(2, 2, 24);
        dib.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]);
        dib.extend_from_slice(&[0, 0, 255, 255, 255, 255, 0, 0]);
        let image = dib_to_bgra(&dib).expect("картинка");
        assert_eq!((image.width, image.height), (2, 2));
        // Верхняя левая точка — красная, нижняя левая — синяя.
        assert_eq!(&image.bgra[0..4], &[0, 0, 255, 255]);
        assert_eq!(&image.bgra[8..12], &[255, 0, 0, 255]);
    }

    #[test]
    fn a_palette_image_is_not_read() {
        let mut dib = header(1, 1, 8);
        dib.extend_from_slice(&[0; 8]);
        assert!(dib_to_bgra(&dib).is_none());
    }
}

#[cfg(all(test, target_os = "windows"))]
mod live {
    /// `cargo test --lib screen::live -- --ignored --nocapture`
    #[test]
    #[ignore = "распознаёт картинку средствами Windows"]
    fn the_readme_picture_is_read() {
        use windows::core::HSTRING;
        use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
        use windows::Storage::{FileAccessMode, StorageFile};

        super::init_com();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("docs")
            .join("light.png")
            .canonicalize()
            .expect("картинка из README");
        let path = path.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))
            .and_then(|operation| operation.join())
            .expect("файл");
        let stream = file
            .OpenAsync(FileAccessMode::Read)
            .and_then(|operation| operation.join())
            .expect("поток");
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .and_then(|operation| operation.join())
            .expect("декодер");
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .and_then(|operation| operation.join())
            .expect("картинка");
        let bitmap = SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8).expect("BGRA");

        let started = std::time::Instant::now();
        let text = super::recognize_bitmap(&bitmap).expect("распознавание");
        let cyrillic = text
            .chars()
            .filter(|ch| matches!(ch, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё'))
            .count();
        println!(
            "{} мс: {} символов, кириллицей {cyrillic}",
            started.elapsed().as_millis(),
            text.chars().count()
        );
        println!("{}", text.chars().take(240).collect::<String>());
        assert!(cyrillic > 20, "{text}");
    }
}
