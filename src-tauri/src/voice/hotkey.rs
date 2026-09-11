//! Клавиши голосового режима: пробел — прочитать, левый Alt с пробелом — сказать.
//!
//! Почему хук, а не опрос, как у жеста выделения. Опросом (`GetAsyncKeyState`)
//! клавишу можно заметить, но нельзя забрать себе: пробел всё равно уйдёт в
//! программу под попапом и напечатается там, а `Alt+Space` вдобавок откроет
//! системное меню окна. Забрать нажатие умеет только низкоуровневый хук.
//!
//! Отсюда же главное ограничение: хук стоит на всей системе, поэтому клавиши
//! перехватываются ТОЛЬКО пока попап открыт. В остальное время обработчик
//! пропускает всё насквозь, не глядя. Windows к тому же снимает хуки, которые
//! думают дольше положенного, — поэтому внутри только атомарные флаги и отправка
//! в канал, а вся работа происходит в другом потоке.
//!
//! Но и открытый попап не повод отнимать пробел у всего компьютера. Ответ
//! читается вслух десятки секунд, и всё это время человек продолжает работать:
//! переходит в другое окно, начинает печатать. Раньше пробел в эти секунды не
//! печатался нигде — клавиша уходила Суфлёру, и пользоваться компьютером под
//! чтение было нельзя. Поэтому пробел остаётся за попапом, только пока человек
//! ничего другого не делает: стоит ему перейти в другое окно или напечатать
//! хоть одну букву, пробел снова его.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;

/// Открыт ли попап. Пока false — хук не трогает ни одной клавиши.
static ARMED: AtomicBool = AtomicBool::new(false);

/// Окно, над которым открылся попап.
///
/// Пробел принадлежит попапу, пока впереди это окно: человек смотрит в текст,
/// рядом с которым всплыло объяснение. Ушёл в другое окно — значит, занялся
/// другим, и клавиша ему нужна там. Ноль — впереди в момент открытия было наше
/// собственное окно (индикатор голоса), и сравнивать не с чем.
static ARMED_OVER: AtomicIsize = AtomicIsize::new(0);

/// Человек начал печатать, пока попап на экране.
///
/// Печатает — значит, пробел ему нужен между словами, а не для чтения вслух.
/// Сбрасывается при каждом новом открытии попапа.
static TYPED: AtomicBool = AtomicBool::new(false);

/// Пробел уже нажат и удерживается. Windows шлёт нажатие снова и снова, пока
/// клавишу держат; без этого флага одно нажатие читало бы текст десятки раз.
static SPACE_HELD: AtomicBool = AtomicBool::new(false);

/// Идёт запись голоса. Нужен, чтобы отпускание пробела остановило именно запись,
/// а не сработало как что-то ещё.
static RECORDING: AtomicBool = AtomicBool::new(false);

static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();

/// Занят ли голос: слушает, думает или говорит. Пока занят — Esc значит
/// «хватит», и хук сообщает об этом.
static VOICE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Отдельный канал для Esc.
///
/// Не общий с остальными клавишами: те разбирает поток, который сам бывает
/// занят ответом по полминуты, и Esc лежал бы в очереди, пока ответ не
/// прозвучит, — ровно тогда, когда его жмут, чтобы ответ оборвать.
static CANCELS: OnceLock<Sender<()>> = OnceLock::new();

/// Сообщает хуку, занят ли голос. Зовётся индикатором при показе и скрытии.
pub fn voice_active(on: bool) {
    VOICE_ACTIVE.store(on, Ordering::Relaxed);
}

/// Идёт ли запись по клавише.
pub fn recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

/// Забыть о записи по клавише: её отменили клавишей Esc, и отпущенный пробел
/// уже не значит «вопрос закончен». Отдаёт, шла ли запись.
pub fn drop_recording() -> bool {
    RECORDING.swap(false, Ordering::Relaxed)
}

/// Приёмник нажатий Esc. Зовётся один раз при запуске.
pub fn cancels() -> Receiver<()> {
    let (tx, rx) = channel::<()>();
    let _ = CANCELS.set(tx);
    rx
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Пробел: прочитать вслух то, что сейчас в окне.
    Speak,
    /// Левый Alt с пробелом зажаты: пишем голос.
    TalkStart,
    /// Отпустили: расшифровываем и отправляем вопросом.
    TalkStop,
    /// Ctrl+Shift+Alt с пробелом: включить или выключить ожидание обращения.
    ToggleWake,
}

/// Включает и выключает перехват. Зовётся, когда попап появляется и исчезает.
pub fn arm(on: bool) {
    ARMED.store(on, Ordering::Relaxed);
    if on {
        TYPED.store(false, Ordering::Relaxed);
        let over = if foreground_is_ours() { 0 } else { foreground_window() };
        ARMED_OVER.store(over, Ordering::Relaxed);
    } else {
        // Попап закрыли с зажатым пробелом — отпускания мы уже не увидим,
        // и без сброса следующий пробел посчитался бы повтором.
        SPACE_HELD.store(false, Ordering::Relaxed);
        if RECORDING.swap(false, Ordering::Relaxed) {
            send(Event::TalkStop);
        }
    }
}

/// Пробел, нажатый в самом окне попапа.
///
/// Хук такой пробел не забирает: окно наше, и в его поле ввода пробел нужен
/// для слов. Но если поле пустое, человек не печатает — он просит прочитать,
/// и окно передаёт нажатие сюда, чтобы оно значило то же, что и снаружи.
pub fn press_speak() {
    send(Event::Speak);
}

fn send(event: Event) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.send(event);
    }
}

/// Ставит хук и отдаёт приёмник событий. Зовётся один раз при запуске.
pub fn install() -> Receiver<Event> {
    let (tx, rx) = channel::<Event>();
    let _ = EVENTS.set(tx);

    #[cfg(target_os = "windows")]
    std::thread::Builder::new()
        .name("sufler-hotkey".into())
        .spawn(windows_loop)
        .ok();

    rx
}

/// Клавиша, которой печатают, а не управляют.
///
/// Буквы, цифры, знаки препинания, цифровой блок и Backspace. Стрелки, Esc,
/// функциональные клавиши и модификаторы сюда не входят: ими листают и
/// переключаются, а не набирают текст, и отдавать из-за них пробел незачем.
fn typing_key(vk: u32) -> bool {
    matches!(
        vk,
        0x08 | 0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF | 0xE2
    )
}

/// Всё ли ещё человек смотрит туда, над чем открылся попап.
fn still_over_popup() -> bool {
    let over = ARMED_OVER.load(Ordering::Relaxed);
    over == 0 || foreground_window() == over || foreground_is_hud()
}

#[cfg(target_os = "windows")]
fn windows_loop() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG, WH_KEYBOARD_LL,
    };

    // SAFETY: обычная установка хука и цикл сообщений. Хук снимается вместе
    // с процессом — отдельная жизнь ему не нужна, поток живёт до конца работы.
    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0);
        if hook.is_err() {
            log::warn!("не удалось поставить хук на клавиатуру — пробел работать не будет");
            return;
        }

        // Хук без цикла сообщений не вызывается вовсе: система доставляет
        // события через очередь этого потока.
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LMENU, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    let pass = |_| unsafe { CallNextHookEx(None, code, wparam, lparam) };
    let held = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };

    if code != HC_ACTION as i32 {
        return pass(());
    }

    let message = wparam.0 as u32;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let up = message == WM_KEYUP || message == WM_SYSKEYUP;

    // Программно посланные нажатия (флаг LLKHF_INJECTED) не отсеиваем намеренно:
    // для человека с переназначенными клавишами — AutoHotkey и прочее — его
    // пробел приходит именно таким, и отличать его от «настоящего» значило бы
    // молча не работать у части людей.
    let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

    // Esc, пока голос занят, — остановить всё. Клавишу не забираем: в
    // программе под окном Esc тоже может что-то значить.
    if info.vkCode == VK_ESCAPE.0 as u32
        && down
        && (VOICE_ACTIVE.load(Ordering::Relaxed) || RECORDING.load(Ordering::Relaxed))
    {
        if let Some(tx) = CANCELS.get() {
            let _ = tx.send(());
        }
        return pass(());
    }

    if info.vkCode != VK_SPACE.0 as u32 {
        // Набрал букву — пробел дальше его. Сочетания с Ctrl, Alt и Win
        // печатью не считаются: Ctrl+C и Alt+Tab текст не набирают.
        //
        // Модификаторы проверяются только здесь, после дешёвых проверок: хук
        // зовётся на каждое нажатие во всей системе и обязан быть мгновенным.
        if down
            && ARMED.load(Ordering::Relaxed)
            && !TYPED.load(Ordering::Relaxed)
            && typing_key(info.vkCode)
            && !held(VK_CONTROL.0 as i32)
            && !held(VK_MENU.0 as i32)
            && !held(VK_LWIN.0 as i32)
            && !held(VK_RWIN.0 as i32)
        {
            TYPED.store(true, Ordering::Relaxed);
        }
        return pass(());
    }

    // Если сейчас впереди наше собственное окно с полем ввода — пробел не наш.
    //
    // Человек щёлкнул в поле «Спросить ещё…» и печатает вопрос руками; забирать
    // у него пробел означало бы, что в своём же поле ввода нельзя разделить два
    // слова. То же и с окном настройки. Пустое поле попап обрабатывает сам и
    // передаёт нажатие через `press_speak`.
    //
    // Индикатор голоса сюда не относится, хотя окно тоже наше. Он появляется
    // ровно в голосовом режиме и на Windows при показе становится передним —
    // то есть ровно тогда, когда пробел нужен нам больше всего, правило выше
    // молча его отдавало бы, и чтение вслух переставало бы работать.
    if foreground_is_ours() && !foreground_is_hud() {
        return pass(());
    }

    // Именно левый Alt: правый оставляем системе и раскладкам, где он AltGr.
    let alt = held(VK_LMENU.0 as i32);
    let ctrl = held(VK_CONTROL.0 as i32);
    let shift = held(VK_SHIFT.0 as i32);

    // Три модификатора сразу — сочетание, которое не занято ничем: обычные
    // Ctrl+Alt+пробел и Alt+Shift+пробел уже разобраны системой и программами.
    let toggle = ctrl && shift && alt;

    // Что именно мы забираем себе.
    //
    // Пробел — только пока попап на экране, человек не начал печатать и не ушёл
    // в другое окно. Иначе это обычная клавиша, и отбирать её недопустимо.
    //
    // Левый Alt с пробелом — всегда, даже когда попапа нет: этим сочетанием
    // задают вопрос голосом с чистого места, окно откроется само. Цена
    // осознанная: в Windows Alt+Space открывает системное меню окна, и пока
    // Суфлёр работает, оно этим сочетанием открываться не будет.
    //
    // И отпускание пробела, если мы уже пишем или забрали его нажатие: клавиши
    // могли отпустить в любом порядке, а пропущенное отпускание оставило бы
    // микрофон включённым или следующий пробел принятым за повтор.
    // Переключатель отдельно не проверяем: в нём тоже зажат Alt, условие покрыто.
    let popup_claims = ARMED.load(Ordering::Relaxed)
        && !TYPED.load(Ordering::Relaxed)
        && still_over_popup();
    let finishing = up && SPACE_HELD.load(Ordering::Relaxed);
    let ours = popup_claims || alt || RECORDING.load(Ordering::Relaxed) || finishing;
    if !ours {
        return pass(());
    }

    if down {
        if SPACE_HELD.swap(true, Ordering::Relaxed) {
            // Повтор от удержания — глотаем, но ничего не делаем.
            return LRESULT(1);
        }
        // Все три модификатора — не разговор, а переключатель ожидания
        // обращения. Проверяется первым: иначе сочетание, в котором Alt тоже
        // зажат, считалось бы обычным «Alt с пробелом».
        if toggle {
            send(Event::ToggleWake);
        } else if alt {
            RECORDING.store(true, Ordering::Relaxed);
            send(Event::TalkStart);
        } else {
            send(Event::Speak);
        }
        return LRESULT(1);
    }

    if up {
        SPACE_HELD.store(false, Ordering::Relaxed);
        if RECORDING.swap(false, Ordering::Relaxed) {
            send(Event::TalkStop);
        }
        return LRESULT(1);
    }

    pass(())
}

/// Окно, которое сейчас впереди, числом. Ноль — такого нет.
#[cfg(target_os = "windows")]
fn foreground_window() -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: только читает состояние системы.
    unsafe { GetForegroundWindow().0 as isize }
}

/// Принадлежит ли окно, которое сейчас впереди, нам самим.
#[cfg(target_os = "windows")]
fn foreground_is_ours() -> bool {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // SAFETY: обе функции только читают состояние системы и ничего не меняют.
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        pid != 0 && pid == GetCurrentProcessId()
    }
}

/// Индикатор ли сейчас впереди.
///
/// Отличаем по заголовку окна: обращаться из хука к состоянию приложения нельзя
/// — обработчик обязан быть мгновенным, — а заголовок читается парой системных
/// вызовов и у индикатора свой.
#[cfg(target_os = "windows")]
fn foreground_is_hud() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

    // SAFETY: обе функции только читают состояние окна.
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return false;
        }
        let mut title = [0u16; 64];
        let length = GetWindowTextW(window, &mut title);
        if length <= 0 {
            return false;
        }
        String::from_utf16_lossy(&title[..length as usize]) == crate::overlay::HUD_TITLE
    }
}

#[cfg(not(target_os = "windows"))]
fn foreground_window() -> isize {
    0
}

#[cfg(not(target_os = "windows"))]
fn foreground_is_ours() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn foreground_is_hud() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn windows_loop() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_punctuation_are_typing() {
        assert!(typing_key(0x41), "A");
        assert!(typing_key(0x5A), "Z");
        assert!(typing_key(0x35), "5");
        assert!(typing_key(0xBC), "запятая");
        assert!(typing_key(0xDB), "скобка — на русской раскладке это «х»");
        assert!(typing_key(0x08), "Backspace — правка набранного");
    }

    #[test]
    fn navigation_is_not_typing() {
        // Листать и переключаться — не печатать: пробел за попапом остаётся.
        for vk in [0x1B, 0x25, 0x26, 0x27, 0x28, 0x70, 0x10, 0x11, 0x12, 0x09, 0x20] {
            assert!(!typing_key(vk), "клавиша {vk:#x}");
        }
    }
}
