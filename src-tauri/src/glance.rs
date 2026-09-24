//! Фокус на окне: Shift+пробел — Ноа смотрит на окно, в котором человек
//! работает, и отвечает про то, что в нём видно.
//!
//! Берётся не всё окно, а видимое сейчас, и из него — последнее: в терминале
//! и в чате свежее внизу. Так вопрос «объясни путь» про только что запущенный
//! traceroute или «где эта выставка» про пост в открытом канале понятен без
//! пересказа, а запрос остаётся в пару тысяч токенов.
//!
//! Текст читается автоматизацией интерфейса Windows — у терминала и браузера
//! она отдаёт видимые строки как есть. Где её нет (Telegram и прочие окна,
//! которые рисуют текст сами), окно снимается и распознаётся встроенным в
//! Windows распознаванием текста — без сети и без видеопамяти.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::ai_client::ThreadItem;
use crate::state::AppState;

/// Сколько знаков видимого текста отдаётся модели — последние.
const MAX_TEXT: usize = 3000;

/// Меньше этого автоматизация, считай, ничего не отдала — снимаем и
/// распознаём окно.
const ENOUGH_TEXT: usize = 150;

/// Сколько обменов помнить в разговоре об окне.
const DEPTH: usize = 6;

/// Сколько снятое окно считается тем, на что человек смотрит.
const FRESH: Duration = Duration::from_secs(10 * 60);

struct Glance {
    title: String,
    text: String,
    at: Instant,
    thread: Vec<ThreadItem>,
    /// Идёт голосовой разговор об окне: фразы уходят сюда, а не общему
    /// помощнику.
    voice: bool,
}

static GLANCE: Mutex<Option<Glance>> = Mutex::new(None);

/// Снимает видимое в окне и начинает разговор о нём. Ошибка — фраза для
/// человека.
pub fn capture(handle: isize) -> Result<(), String> {
    if handle == 0 {
        return Err("Не вижу, в каком окне вы работаете.".into());
    }
    let (title, text, from) = read(handle)?;
    let text = tail(&text, MAX_TEXT);
    if text.trim().is_empty() {
        return Err("В этом окне не вижу текста.".into());
    }
    log::info!("фокус на окне «{title}»: {} знаков, {from}", text.chars().count());
    *GLANCE.lock().unwrap_or_else(|err| err.into_inner()) = Some(Glance {
        title,
        text,
        at: Instant::now(),
        thread: Vec::new(),
        voice: true,
    });
    Ok(())
}

/// Идёт ли разговор об окне.
pub fn active() -> bool {
    GLANCE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .is_some_and(|glance| glance.voice && glance.at.elapsed() < FRESH)
}

/// Разговор кончился — об окне больше не говорим.
pub fn end() {
    if let Some(glance) = GLANCE.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
        glance.voice = false;
    }
}

/// Говорит ли фраза о том, что в окне: «эта выставка», «тут», «видишь».
///
/// Нужна разбору реплик: «по какому адресу эта выставка» похоже на поиск в
/// интернете, но ответ — в открытом посте.
pub fn refers_to_window(said: &str) -> bool {
    if !active() {
        return false;
    }
    let lower = said.to_lowercase().replace('ё', "е");
    lower
        .split(|ch: char| !ch.is_alphabetic())
        .any(|word| word.starts_with("эт") || ["тут", "здесь", "там", "видишь", "видно", "написано"].contains(&word))
}

/// Строка обстановки для разбора реплик: какое окно, что в нём и о чём
/// только что говорили. «Добавь на субботу сходить туда» без неё — дело
/// «сходить туда».
pub fn context_line() -> String {
    let guard = GLANCE.lock().unwrap_or_else(|err| err.into_inner());
    let Some(glance) = guard.as_ref().filter(|glance| glance.voice && glance.at.elapsed() < FRESH) else {
        return String::new();
    };
    let mut line = format!(
        "Человек смотрит на окно «{}», в нём видно: «{}». Вопросы про то, что в окне («эта \
         выставка», «тут», «этот путь»), — chat, а не lookup и не screen.",
        glance.title,
        tail(&glance.text, 800).replace('\n', " / ")
    );
    if let Some(last) = glance.thread.last() {
        line.push_str(&format!(
            " Только что спросил: «{}», ответ: «{}». «Туда», «это», «на неё» — о том же; \
             в title пиши конкретно, что и где.",
            last.q, last.a
        ));
    }
    line
}

/// Ответ на вопрос об окне — с тем, что в нём видно, и историей разговора.
pub async fn answer(app: &AppHandle, said: &str) -> Result<String, String> {
    let (title, text, thread) = {
        let guard = GLANCE.lock().unwrap_or_else(|err| err.into_inner());
        let glance = guard.as_ref().ok_or("Окно не снято.")?;
        (glance.title.clone(), glance.text.clone(), glance.thread.clone())
    };
    let (provider, limit, name) = {
        let state = app.state::<AppState>();
        let limit = state.config().ai.call_limit();
        (state.provider(), limit, state.wake_name())
    };
    // Начало «Ты — помощник по окну» — примета для моста к подпискам: такой
    // запрос он отправляет разово, с историей из запроса.
    let rules = format!(
        "Ты — помощник по окну, тебя зовут {name}; ты голосовой помощник на компьютере \
         человека. Он смотрит на окно «{title}» — ниже то, что в нём сейчас видно, последнее \
         внизу. «Это», «тут», «эта выставка», «этот путь» — про то, что в окне.\n\
         Как отвечать:\n\
         - По делу и простыми словами, как объяснил бы знающий друг: обычно две–пять фраз.\n\
         - Объясняешь вывод программы — иди по нему по порядку: что значит первая строка, \
         вторая и дальше, человеческим языком.\n\
         - Ответ звучит вслух: не зачитывай IP-адреса, длинные числа, хеши, пути и ссылки — \
         называй их по смыслу («первый узел — твой домашний роутер», «дальше сеть провайдера»).\n\
         - О содержимом окна не выдумывай: чего в нём нет, того нет. Общее знание добавляй, \
         когда оно помогает понять.\n\
         - Без вступлений и предложений помочь ещё. Обычный текст без списков и разметки, \
         по-русски.\n\n\
         Видно в окне:\n{text}"
    );
    let asked = tokio::time::timeout(limit, provider.converse(&rules, &thread, said, false)).await;
    let reply = match asked {
        Ok(Ok(reply)) if !reply.trim().is_empty() => reply.trim().to_string(),
        Ok(Ok(_)) => return Err("Модель прислала пустой ответ.".into()),
        Ok(Err(err)) => return Err(format!("Не получилось ответить: {}", err.user_text("модель не ответила"))),
        Err(_) => return Err("Модель не успела ответить.".into()),
    };
    if let Some(glance) = GLANCE.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
        glance.thread.push(ThreadItem {
            q: said.to_string(),
            a: reply.clone(),
        });
        let excess = glance.thread.len().saturating_sub(DEPTH);
        glance.thread.drain(..excess);
        glance.at = Instant::now();
    }
    Ok(reply)
}

/// Последние `limit` знаков, с начала строки.
fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.trim().to_string();
    }
    let cut: String = text.chars().skip(count - limit).collect();
    match cut.find('\n') {
        Some(at) if at < cut.len() / 2 => cut[at + 1..].trim().to_string(),
        _ => cut.trim().to_string(),
    }
}

/// Заголовок окна, видимый текст и откуда он взят.
#[cfg(target_os = "windows")]
fn read(handle: isize) -> Result<(String, String, &'static str), String> {
    let title = title(handle);
    let visible = visible_text(handle);
    if visible.trim().chars().count() >= ENOUGH_TEXT {
        return Ok((title, visible, "автоматизация интерфейса"));
    }
    let image = crate::shots::window_image(handle)?;
    let seen = crate::screen::recognize(&image).map_err(|err| {
        log::warn!("окно не распозналось: {err}");
        "Не смог прочитать окно.".to_string()
    })?;
    // Распознанное бывает беднее того, что отдала автоматизация, — берём
    // то, где текста больше.
    if seen.chars().count() >= visible.chars().count() {
        Ok((title, seen, "распознавание снимка"))
    } else {
        Ok((title, visible, "автоматизация интерфейса"))
    }
}

#[cfg(not(target_os = "windows"))]
fn read(_handle: isize) -> Result<(String, String, &'static str), String> {
    Err("Смотреть на окна умею только в Windows.".into())
}

#[cfg(target_os = "windows")]
fn title(handle: isize) -> String {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;

    let mut buffer = [0u16; 512];
    // SAFETY: читает заголовок окна в свой буфер его же размера.
    let length = unsafe { GetWindowTextW(HWND(handle as *mut core::ffi::c_void), &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length]).trim().to_string()
}

/// Видимый текст окна через TextPattern: у элемента в фокусе или у его
/// ближайших предков — документа, терминала, поля.
#[cfg(target_os = "windows")]
fn visible_text(handle: isize) -> String {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, UIA_TextPatternId,
    };

    // SAFETY: только чтение интерфейса чужого окна через COM; повторная
    // инициализация COM в потоке безвредна.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(automation) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else {
            return String::new();
        };
        let mut start: Vec<IUIAutomationElement> = Vec::new();
        if let Ok(focused) = automation.GetFocusedElement() {
            start.push(focused);
        }
        if let Ok(root) = automation.ElementFromHandle(HWND(handle as *mut core::ffi::c_void)) {
            start.push(root);
        }
        let Ok(walker) = automation.ControlViewWalker() else {
            return String::new();
        };
        let mut best = String::new();
        for element in start {
            let mut current = element;
            for _ in 0..5 {
                if let Ok(pattern) = current.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                    let mut text = String::new();
                    if let Ok(ranges) = pattern.GetVisibleRanges() {
                        for at in 0..ranges.Length().unwrap_or(0) {
                            if let Ok(piece) = ranges.GetElement(at).and_then(|range| range.GetText(-1)) {
                                text.push_str(&piece.to_string());
                                text.push('\n');
                            }
                        }
                    }
                    if text.trim().chars().count() > best.trim().chars().count() {
                        best = text;
                    }
                    break;
                }
                match walker.GetParentElement(&current) {
                    Ok(parent) => current = parent,
                    Err(_) => break,
                }
            }
        }
        // Пустые строки терминала внизу экрана — не текст.
        best.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_end_from_a_line_start() {
        let text = "первая строка\nвторая строка\nтретья";
        assert_eq!(tail(text, 100), text);
        assert_eq!(tail(text, 20), "вторая строка\nтретья");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod live {
    /// Ручная проверка: `cargo test --lib glance::live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_the_foreground_window() {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let handle = unsafe { GetForegroundWindow().0 as isize };
        let (title, text, from) = super::read(handle).expect("окно не прочиталось");
        let text = super::tail(&text, super::MAX_TEXT);
        println!("«{title}» — {from}, {} знаков\n---\n{}", text.chars().count(), text.chars().take(600).collect::<String>());
    }
}
