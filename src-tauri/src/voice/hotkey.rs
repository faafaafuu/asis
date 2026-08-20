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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;

/// Открыт ли попап. Пока false — хук не трогает ни одной клавиши.
static ARMED: AtomicBool = AtomicBool::new(false);

/// Пробел уже нажат и удерживается. Windows шлёт нажатие снова и снова, пока
/// клавишу держат; без этого флага одно нажатие читало бы текст десятки раз.
static SPACE_HELD: AtomicBool = AtomicBool::new(false);

/// Идёт запись голоса. Нужен, чтобы отпускание пробела остановило именно запись,
/// а не сработало как что-то ещё.
static RECORDING: AtomicBool = AtomicBool::new(false);

static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Пробел: прочитать вслух то, что сейчас в окне.
    Speak,
    /// Левый Alt с пробелом зажаты: пишем голос.
    TalkStart,
    /// Отпустили: расшифровываем и отправляем вопросом.
    TalkStop,
}

/// Включает и выключает перехват. Зовётся, когда попап появляется и исчезает.
pub fn arm(on: bool) {
    ARMED.store(on, Ordering::Relaxed);
    if !on {
        // Попап закрыли с зажатым пробелом — отпускания мы уже не увидим,
        // и без сброса следующий пробел посчитался бы повтором.
        SPACE_HELD.store(false, Ordering::Relaxed);
        if RECORDING.swap(false, Ordering::Relaxed) {
            send(Event::TalkStop);
        }
    }
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
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LMENU, VK_SPACE};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    let pass = |_| unsafe { CallNextHookEx(None, code, wparam, lparam) };

    if code != HC_ACTION as i32 || !ARMED.load(Ordering::Relaxed) {
        return pass(());
    }

    // Программно посланные нажатия (флаг LLKHF_INJECTED) не отсеиваем намеренно:
    // для человека с переназначенными клавишами — AutoHotkey и прочее — его
    // пробел приходит именно таким, и отличать его от «настоящего» значило бы
    // молча не работать у части людей.
    let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if info.vkCode != VK_SPACE.0 as u32 {
        return pass(());
    }

    let message = wparam.0 as u32;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let up = message == WM_KEYUP || message == WM_SYSKEYUP;

    // Именно левый Alt: правый оставляем системе и раскладкам, где он AltGr.
    let alt = unsafe { (GetAsyncKeyState(VK_LMENU.0 as i32) as u16 & 0x8000) != 0 };

    if down {
        if SPACE_HELD.swap(true, Ordering::Relaxed) {
            // Повтор от удержания — глотаем, но ничего не делаем.
            return LRESULT(1);
        }
        if alt {
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

#[cfg(not(target_os = "windows"))]
fn windows_loop() {}
