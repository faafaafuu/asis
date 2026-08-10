//! Windows: получение выделения через UI Automation и хук на левый Ctrl (SPEC §9.1).
//!
//! Известное ограничение (SPEC §12.3): UI Automation реализован не во всех
//! приложениях — часть Electron- и кастомных тулкитов не отдаёт TextPattern.
//! Для них предусмотрен фолбэк через буфер обмена, выключенный по умолчанию:
//! он читает чужой буфер и требует явного согласия пользователя в конфигурации.

use std::sync::Mutex;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, POINT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData,
    CF_UNICODETEXT,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_ESCAPE, VK_LBUTTON, VK_LCONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::{Capability, Diagnostics, PlatformIntegration, ScreenRect, Selection};
use crate::config::TriggerConfig;

/// Состояние жеста между опросами: без него нельзя отличить «кнопка нажата»
/// от «кнопку только что отпустили».
#[derive(Default)]
struct GestureState {
    mouse_was_down: bool,
    /// Левый Ctrl был зажат в момент начала выделения, а не только в конце:
    /// пользователь вполне может отпустить Ctrl раньше кнопки мыши.
    left_ctrl_seen: bool,
}

pub struct Platform {
    gesture: Mutex<GestureState>,
    diag: Mutex<Diagnostics>,
}

impl Platform {
    fn note(&self, last: impl Into<String>, source: &str, captured: bool) {
        if let Ok(mut diag) = self.diag.lock() {
            diag.gestures += 1;
            if captured {
                diag.captured += 1;
            }
            diag.last = last.into();
            diag.source = source.into();
        }
    }
}

thread_local! {
    /// COM инициализируется на том потоке, где реально живёт опрос.
    static COM_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn ensure_com() {
    COM_READY.with(|ready| {
        if ready.get() {
            return;
        }
        // Ошибку не считаем фатальной: COM мог быть уже инициализирован в другой модели,
        // в этом случае вызовы UI Automation всё равно работают.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        ready.set(true);
    });
}

fn key_down(key: VIRTUAL_KEY) -> bool {
    // Старший бит — «клавиша нажата сейчас».
    unsafe { (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0 }
}

fn cursor() -> Option<(f64, f64)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point).ok()? };
    Some((point.x as f64, point.y as f64))
}

/// Текст и геометрия выделения в сфокусированном элементе.
///
/// Попыток несколько не от неуверенности: Chromium (значит — Chrome, Edge, Electron,
/// а это Telegram, Discord, VS Code, Slack) не держит дерево доступности постоянно.
/// Он строит его в ответ на первое обращение извне, и этот самый первый запрос
/// возвращает пустоту. Одна попытка означала бы «в браузере не работает никогда».
fn selection_via_uia() -> Option<(String, Option<ScreenRect>)> {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        if let Some(found) = selection_via_uia_once() {
            return Some(found);
        }
    }
    None
}

fn selection_via_uia_once() -> Option<(String, Option<ScreenRect>)> {
    ensure_com();
    unsafe {
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        let pattern = text_pattern_of(&automation, &element)?;
        let ranges = pattern.GetSelection().ok()?;
        if ranges.Length().ok()? == 0 {
            return None;
        }
        let range = ranges.GetElement(0).ok()?;

        // -1 — «весь текст диапазона без ограничения длины».
        let text = range.GetText(-1).ok()?.to_string();
        if text.trim().is_empty() {
            return None;
        }

        let rect = last_bounding_rect(&range);
        Some((text, rect))
    }
}

/// TextPattern у самого элемента или у ближайшего предка.
///
/// Фокус нередко стоит на маленьком внутреннем узле (строка, ячейка, инлайн-элемент),
/// а текстом целиком владеет контейнер выше — документ или поле ввода. Поэтому если
/// на элементе паттерна нет, поднимаемся на несколько уровней, а не сдаёмся сразу.
unsafe fn text_pattern_of(
    automation: &IUIAutomation,
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> Option<IUIAutomationTextPattern> {
    if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
        return Some(pattern);
    }

    let walker = automation.ControlViewWalker().ok()?;
    let mut current = element.clone();
    for _ in 0..4 {
        let parent = walker.GetParentElement(&current).ok()?;
        if let Ok(pattern) = parent.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            return Some(pattern);
        }
        current = parent;
    }
    None
}

/// Из массива прямоугольников выделения берём ПОСЛЕДНИЙ: для многострочного
/// выделения окно должно встать у конца текста, а не по центру (SPEC §4).
unsafe fn last_bounding_rect(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> Option<ScreenRect> {
    let array = range.GetBoundingRectangles().ok()?;
    if array.is_null() {
        return None;
    }

    let lower = SafeArrayGetLBound(array, 1).ok()?;
    let upper = SafeArrayGetUBound(array, 1).ok()?;
    let count = (upper - lower + 1) as usize;
    // Каждый прямоугольник — четвёрка double: left, top, width, height.
    if count < 4 {
        return None;
    }

    let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
    SafeArrayAccessData(array, &mut data).ok()?;
    let values = std::slice::from_raw_parts(data as *const f64, count);
    let base = (count / 4 - 1) * 4;
    let rect = ScreenRect {
        x: values[base],
        y: values[base + 1],
        width: values[base + 2],
        height: values[base + 3],
    };
    let _ = SafeArrayUnaccessData(array);

    (rect.width > 0.0 || rect.height > 0.0).then_some(rect)
}

/// Фолбэк для приложений без поддержки UI Automation (SPEC §9.1, §12.3):
/// симулируем Ctrl+C и читаем буфер обмена. Координат выделения этот путь не даёт —
/// попап встанет у курсора.
///
/// Две тонкости, без которых фолбэк опаснее, чем его отсутствие:
///
/// 1. Мы обязаны понять, скопировалось ли что-то ВООБЩЕ. Приложение могло проглотить
///    Ctrl+C, и тогда в буфере лежит то, что пользователь копировал час назад, — попап
///    объяснил бы совершенно постороннее слово с полной уверенностью. Отличить новое
///    содержимое от старого сравнением текста нельзя (можно скопировать то же слово
///    дважды), поэтому смотрим на счётчик изменений буфера, который ведёт сама система.
/// 2. Буфер обмена принадлежит пользователю. Забрать его себе и не вернуть — значит
///    незаметно стереть то, что человек нёс из одного окна в другое. Сохраняем прежний
///    текст и кладём обратно.
fn selection_via_clipboard() -> Option<String> {
    unsafe {
        let before = GetClipboardSequenceNumber();
        let saved = read_clipboard_text();

        send_ctrl_c();

        // Приложению нужно время положить текст в буфер, но сколько — заранее неизвестно:
        // редактор справится за миллисекунды, тяжёлая веб-страница — за сотни. Ждём
        // события, а не фиксированную паузу, иначе попап тормозил бы на ровном месте.
        let mut changed = false;
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            if GetClipboardSequenceNumber() != before {
                changed = true;
                break;
            }
        }

        if !changed {
            return None;
        }

        let text = read_clipboard_text();
        if let Some(previous) = saved {
            write_clipboard_text(&previous);
        }
        text
    }
}

unsafe fn send_ctrl_c() {
    let mut inputs = [INPUT::default(); 4];
    for (index, (key, up)) in [
        (VK_CONTROL, false),
        (VIRTUAL_KEY(b'C' as u16), false),
        (VIRTUAL_KEY(b'C' as u16), true),
        (VK_CONTROL, true),
    ]
    .into_iter()
    .enumerate()
    {
        inputs[index].r#type = INPUT_KEYBOARD;
        inputs[index].Anonymous.ki = KEYBDINPUT {
            wVk: key,
            dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
            ..Default::default()
        };
    }
    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
}

unsafe fn read_clipboard_text() -> Option<String> {
    OpenClipboard(None).ok()?;
    let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok();
    let text = handle.and_then(|handle| {
        let ptr = GlobalLock(HGLOBAL(handle.0)) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        let _ = GlobalUnlock(HGLOBAL(handle.0));
        Some(text)
    });
    let _ = CloseClipboard();
    text.filter(|t| !t.trim().is_empty())
}

/// Возвращает в буфер обмена то, что там было до нашего Ctrl+C.
///
/// После `SetClipboardData` память принадлежит системе — освобождать её нельзя,
/// поэтому выделенный блок намеренно не трогаем дальше.
unsafe fn write_clipboard_text(text: &str) {
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = utf16.len() * std::mem::size_of::<u16>();

    let Ok(block) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
        return;
    };
    let ptr = GlobalLock(block) as *mut u16;
    if ptr.is_null() {
        return;
    }
    std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
    let _ = GlobalUnlock(block);

    if OpenClipboard(None).is_ok() {
        let _ = EmptyClipboard();
        let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(block.0)));
        let _ = CloseClipboard();
    }
}

impl PlatformIntegration for Platform {
    fn capability(&self) -> Capability {
        // Отдельного разрешения Windows не требует: UI Automation доступен обычному
        // процессу. Полнота поддержки зависит от конкретного приложения — это
        // проверяется только в момент выделения.
        Capability::Ready
    }

    fn poll_trigger(&self, config: &TriggerConfig) -> Option<Selection> {
        let mut gesture = self.gesture.lock().ok()?;

        let mouse_down = key_down(VK_LBUTTON);
        if mouse_down && key_down(VK_LCONTROL) {
            gesture.left_ctrl_seen = true;
        }

        let released = gesture.mouse_was_down && !mouse_down;
        gesture.mouse_was_down = mouse_down;

        if !released {
            return None;
        }

        let had_left_ctrl = gesture.left_ctrl_seen;
        gesture.left_ctrl_seen = false;
        drop(gesture);

        // Именно левый Ctrl. Правый попап не открывает (SPEC §3, §12.5).
        if config.require_left_ctrl && !had_left_ctrl {
            return None;
        }

        let cursor = cursor().unwrap_or((0.0, 0.0));

        if let Some((text, rect)) = selection_via_uia() {
            let text = text.trim().to_string();
            self.note(format!("получено слово «{}»", short(&text)), "UI Automation", true);
            return Some(Selection {
                text,
                rect,
                cursor,
                context: String::new(),
            });
        }

        if config.clipboard_fallback {
            log::debug!("UI Automation не отдал выделение — пробуем буфер обмена");
            match selection_via_clipboard() {
                Some(text) => {
                    let text = text.trim().to_string();
                    self.note(
                        format!("получено слово «{}»", short(&text)),
                        "буфер обмена",
                        true,
                    );
                    return Some(Selection {
                        text,
                        rect: None,
                        cursor,
                        context: String::new(),
                    });
                }
                None => {
                    self.note(
                        "приложение не отдало выделение ни через систему, ни через копирование",
                        "—",
                        false,
                    );
                    return None;
                }
            }
        }

        self.note(
            "приложение не отдало выделение; включите запасной способ через копирование",
            "—",
            false,
        );
        log::debug!("выделение не получено: приложение не отдаёт TextPattern");
        None
    }

    fn diagnostics(&self) -> Diagnostics {
        self.diag.lock().map(|d| d.clone()).unwrap_or_default()
    }

    fn is_escape_pressed(&self) -> bool {
        key_down(VK_ESCAPE)
    }

    fn is_primary_mouse_down(&self) -> bool {
        key_down(VK_LBUTTON)
    }

    fn cursor_position(&self) -> Option<(f64, f64)> {
        cursor()
    }
}

/// Обрезка для строки статуса: в диагностику попадает выделение любой длины,
/// а окно настройки — не место для трёх абзацев. Режем по символам, а не по байтам:
/// кириллица в UTF-8 занимает два байта, и срез по индексу байта уронил бы процесс.
fn short(text: &str) -> String {
    let trimmed: String = text.chars().take(40).collect();
    if trimmed.chars().count() < text.chars().count() {
        format!("{trimmed}…")
    } else {
        trimmed
    }
}

pub fn create() -> Box<dyn PlatformIntegration> {
    Box::new(Platform {
        gesture: Mutex::new(GestureState::default()),
        diag: Mutex::new(Diagnostics::default()),
    })
}
