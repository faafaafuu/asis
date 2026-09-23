//! Оверлей-окно попапа: создание, показ, позиционирование.
//!
//! Позиционирование продублировано из `src/js/position.js` — там оно работает в
//! координатах вьюпорта, здесь в экранных координатах монитора. Константы обязаны
//! совпадать: при правке одного места правьте оба.

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::selection::{ScreenRect, Selection};
use crate::state::AppState;

pub const POPUP_LABEL: &str = "popup";
pub const ONBOARDING_LABEL: &str = "onboarding";

/// Зазор между окном и выделением, логические пиксели.
const GAP: f64 = 12.0;
/// Отступ от краёв экрана, логические пиксели.
const INSET: f64 = 12.0;

/// Полезная нагрузка события открытия попапа.
///
/// `Clone` — потому что её иногда приходится придержать: см. `PENDING`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPayload {
    pub term: String,
    pub context: String,
    pub theme: String,
    pub error_text: String,
    /// См. `RuntimeConfig::dialogue`. Приходит с каждым открытием, а не только
    /// при старте окна: источник меняют в настройках, а окно попапа живёт до
    /// конца сеанса и иначе показывало бы «?» по вчерашним сведениям.
    pub dialogue: bool,
    /// Прочитать ответ вслух, как только он придёт.
    ///
    /// Нужно, когда окно открыли голосом: человек спросил вслух и ответа ждёт
    /// тоже вслух, а не глазами.
    speak: bool,
    /// Готовый текст вместо вопроса к модели.
    ///
    /// Так приходит напоминание о задаче: спрашивать про него нечего, оно уже
    /// написано. Пустое поле означает обычное открытие с вопросом.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// Создаёт окно попапа, если его ещё нет. Окно рождается скрытым: первым делом его
/// надо измерить и поставить на место, иначе пользователь увидит прыжок.
pub fn ensure_popup_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        return Ok(window);
    }

    // Десктоп и мобильные здесь не делят одну цепочку методов: decorations,
    // always_on_top, skip_taskbar, shadow и focused существуют только в настольном
    // API Tauri — рамка окна, задачная панель, «поверх других окон», фокус мимо
    // приложения — на телефоне у этих понятий просто нет прообраза, экран и так
    // один на всё и модальный. Разводить цепочку по одному методу за раз не
    // выйдет: rustc глушит однотипные ошибки в одной цепочке после первой, и
    // каждая починка вскрывала бы следующую только на очередном запуске CI.
    //
    // Мобильная ветка ничем не рискует в рантайме: ensure_popup_window вызывает
    // только show_for_selection, а его — только watcher, который запускается
    // исключительно на десктопе. На телефоне вход другой —
    // пункт меню «Объяснить» из нативного плагина. Здесь достаточно, чтобы
    // функция типобезопасно существовала и собиралась.
    #[cfg(desktop)]
    let window = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("popup.html".into()))
        // Тема до первого кадра: попап появляется мгновенно поверх чужого окна,
        // и вспышка чужой темы здесь заметнее, чем где-либо ещё.
        .initialization_script(&theme_script(app))
        .title("Суфлёр")
        .inner_size(400.0, 160.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // Рамки у окна нет, но тянуть его за края можно: без этого система
        // откажется менять размер, за что бы мы её ни просили.
        .resizable(true)
        .shadow(false)
        // Попап не должен забирать фокус клавиатуры у приложения, из которого
        // пользователь выделил текст.
        .focused(false)
        .visible(false)
        .build()?;

    #[cfg(not(desktop))]
    let window = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("popup.html".into()))
        .visible(false)
        .build()?;

    Ok(window)
}

/// Показывает попап для нового выделения: сохраняет якорь и отдаёт фронтенду термин.
/// Само окно появится в `apply_geometry`, когда фронтенд сообщит свой размер.
pub fn show_for_selection(app: &AppHandle, selection: Selection) -> tauri::Result<()> {
    // Окно появилось — отсчёт бездействия начинается отсюда.
    touch_popup();
    #[cfg(desktop)]
    release_control();
    #[cfg(desktop)]
    OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    #[cfg(desktop)]
    BY_VOICE.store(false, std::sync::atomic::Ordering::Relaxed);

    // Узнаём до создания: окна ещё нет — значит слушателя событий тоже.
    let fresh = app.get_webview_window(POPUP_LABEL).is_none();
    let window = ensure_popup_window(app)?;

    let state = app.state::<AppState>();
    let (theme, error_text, dialogue) = {
        let config = state.config();
        (
            config.ui.theme.clone(),
            config.ui.resolved_error_text(),
            config.ai.provider != "wikipedia",
        )
    };

    let payload = OpenPayload {
        term: selection.text.clone(),
        context: selection.context.clone(),
        theme,
        error_text,
        dialogue,
        speak: false,
        answer: None,
    };
    state.set_selection(selection);

    // Окно прячем перед сменой якоря: переоткрытие идёт без анимации закрытия.
    let _ = window.hide();
    if fresh {
        *PENDING.lock().unwrap_or_else(|err| err.into_inner()) = Some(payload);
    } else {
        window.emit_to(POPUP_LABEL, "popup:open", payload)?;
    }

    // Пока попап на экране, пробел принадлежит ему. В остальное время хук
    // пропускает клавиши насквозь и ничего не трогает.
    #[cfg(desktop)]
    crate::voice::hotkey::arm(true);
    Ok(())
}

/// Вопрос, который окно попапа не успело получить.
///
/// Событие, отправленное только что созданному окну, до него не доходит:
/// страница ещё не загрузилась и слушателя на нём нет. Окно при этом
/// показывается — в своём начальном состоянии, то есть с вечным «Анализирую…»,
/// потому что открывать его никто так и не попросил.
///
/// Поэтому первому открытию вопрос не отправляют, а кладут сюда: окно забирает
/// его само, когда загрузится.
static PENDING: std::sync::Mutex<Option<OpenPayload>> = std::sync::Mutex::new(None);

/// Отдаёт придержанный вопрос ровно один раз.
pub fn take_pending() -> Option<OpenPayload> {
    PENDING.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Окно индикатора голосового режима.
pub const HUD_LABEL: &str = "hud";

/// Заголовок этого окна. По нему хук отличает индикатор от остальных наших
/// окон: у окна настройки и попапа пробел отбирать нельзя, у индикатора можно.
pub const HUD_TITLE: &str = "Суфлёр — голос";

/// Состояние, в котором индикатор сейчас находится.
///
/// Нужно потому, что окно создаётся не мгновенно: страница ещё не загрузилась,
/// а состояние уже отправлено — и первое событие пропадает. Окно спрашивает его
/// само, когда будет готово.
#[cfg(desktop)]
static HUD_MODE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// С какого момента индикатор в нынешнем состоянии. По этому видно, что
/// «думаю» висит дольше, чем модель вообще имеет право думать.
#[cfg(desktop)]
static HUD_SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Висит ли «думаю» дольше предела.
#[cfg(desktop)]
pub fn hud_stuck_thinking(limit: std::time::Duration) -> bool {
    // «Загрузка» тоже в счёт: распознавание могло так и не подняться.
    let thinking = HUD_MODE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_deref()
        .is_some_and(|mode| mode == "thinking" || mode == "loading");
    let since = *HUD_SINCE.lock().unwrap_or_else(|err| err.into_inner());
    thinking && since.is_some_and(|at| at.elapsed() > limit)
}

/// В каком состоянии индикатор. Для окна, которое только что загрузилось.
#[cfg(desktop)]
pub fn hud_mode() -> String {
    HUD_MODE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone()
        .unwrap_or_else(|| "idle".into())
}

/// Показывает индикатор в нужном состоянии.
///
/// Окно отдельное, а не часть попапа, по двум причинам. Попапа может не быть
/// вовсе — вопрос задают с закрытым окном. И место у них разное: попап стоит
/// у выделенного слова, индикатор — всегда сверху по центру экрана, где его
/// видно, куда бы ни смотрел человек.
#[cfg(desktop)]
pub fn show_hud(app: &AppHandle, mode: &str) {
    let window = match ensure_hud_window(app) {
        Ok(window) => window,
        Err(err) => {
            log::warn!("индикатор голоса не создался: {err}");
            return;
        }
    };
    // Появление, а не смена состояния: звук нужен один раз, когда помощник
    // возник на экране, а не на каждом переходе «слушаю — думаю — говорю».
    let appearing = {
        let mut current = HUD_MODE.lock().unwrap_or_else(|err| err.into_inner());
        let appearing = current.is_none();
        if current.as_deref() != Some(mode) {
            log::info!("индикатор: {mode}");
            *HUD_SINCE.lock().unwrap_or_else(|err| err.into_inner()) =
                Some(std::time::Instant::now());
        }
        *current = Some(mode.to_string());
        appearing
    };
    if appearing {
        crate::voice::chime_open();
        // Окно показывается мгновенно, а проявляется само — рисованием.
        // Событие только сообщает, что отсчёт пошёл.
        let _ = window.emit_to(HUD_LABEL, "hud:appear", ());
    }
    let _ = window.emit_to(HUD_LABEL, "hud:mode", mode.to_string());
    // Свёрнутый индикатор (например, после «Свернуть все окна») показ не
    // разворачивает: запись шла, а на экране ничего не было.
    let _ = window.unminimize();
    let _ = window.show();
    #[cfg(target_os = "windows")]
    make_passive(&window);
    // Пока индикатор на экране, голос занят: Esc его остановит.
    crate::voice::hotkey::voice_active(true);
}

#[cfg(desktop)]
pub fn hide_hud(app: &AppHandle) {
    *HUD_MODE.lock().unwrap_or_else(|err| err.into_inner()) = None;
    *HUD_SINCE.lock().unwrap_or_else(|err| err.into_inner()) = None;
    crate::voice::hotkey::voice_active(false);
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.hide();
    }
}

#[cfg(desktop)]
fn ensure_hud_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        return Ok(window);
    }

    let window = WebviewWindowBuilder::new(app, HUD_LABEL, WebviewUrl::App("hud.html".into()))
        .title(HUD_TITLE)
        .inner_size(HUD_WIDTH, HUD_HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        // Не забирать фокус: индикатор появляется поверх чужой работы, и увести
        // у человека курсор с текста, который он читает, было бы дурным тоном.
        .focused(false)
        .visible(false)
        .build()?;

    // Сквозь него можно щёлкать: это картинка, а не орган управления.
    let _ = window.set_ignore_cursor_events(true);
    #[cfg(target_os = "windows")]
    make_passive(&window);
    place_hud(&window);
    Ok(window)
}

/// Делает индикатор картинкой поверх экрана, а не окном приложения.
///
/// `skip_taskbar` у Tauri держится не всегда: после скрытия и показа индикатор
/// снова числился обычным окном в группе Суфлёра на панели задач, и щелчок по
/// группе сворачивал его вместо окна настройки. Стиль «окно-инструмент» убирает
/// его с панели задач и из Alt+Tab насовсем, «не активируется» не даёт ему
/// забирать фокус, а без кнопок «свернуть» и «развернуть» свернуть его нечем.
#[cfg(target_os = "windows")]
fn make_passive(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    };

    let Ok(raw) = window.hwnd() else { return };
    let hwnd = HWND(raw.0 as _);
    // SAFETY: окно наше и живое; меняются только биты стиля.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE)
            & !((WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        // Новые стили вступают в силу только после пересчёта рамки.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Голосовые из Telegram, ждущие разбора: номер и куда отдать результат.
#[cfg(desktop)]
type Decoded = std::sync::mpsc::Sender<Result<Vec<u8>, String>>;
#[cfg(desktop)]
static DECODES: std::sync::Mutex<Vec<(u64, Decoded)>> = std::sync::Mutex::new(Vec::new());

/// Разбирает голосовое сообщение (OGG/Opus) в WAV на 16 кГц — движком
/// браузера в окне индикатора.
///
/// Своего декодера Opus в программе нет, а тянуть его ради одной задачи —
/// лишняя библиотека на C. Движок браузера Opus понимает и так, а окно
/// индикатора есть всегда, скрыто и фокус не берёт.
#[cfg(desktop)]
pub fn decode_audio(app: &AppHandle, data: &[u8]) -> Result<Vec<u8>, String> {
    transcode(app, "audio:decode", data)
}

/// Обратное: WAV в OGG/Opus — голосовой ответ в Telegram.
#[cfg(desktop)]
pub fn encode_voice(app: &AppHandle, wav: &[u8]) -> Result<Vec<u8>, String> {
    transcode(app, "audio:encode", wav)
}

/// Отдаёт звук странице индикатора и ждёт, что она вернёт.
#[cfg(desktop)]
fn transcode(app: &AppHandle, event: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::RecvTimeoutError;
    static NEXT: AtomicU64 = AtomicU64::new(1);

    let window =
        ensure_hud_window(app).map_err(|err| format!("окно разбора не создалось: {err}"))?;
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = std::sync::mpsc::channel();
    DECODES
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .push((id, tx));
    let payload = serde_json::json!({ "id": id, "data": crate::secret::base64(data) });

    // Только что созданное окно ещё грузит страницу, и первое событие до неё
    // может не дойти: просьба повторяется раз в секунду, пока не ответят.
    let mut result = Err("звук не обработался: окно индикатора не ответило".to_string());
    for _ in 0..30 {
        let _ = window.emit_to(HUD_LABEL, event, payload.clone());
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(done) => {
                result = done;
                break;
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    DECODES
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .retain(|(known, _)| *known != id);
    result
}

/// Результат разбора из окна индикатора.
#[cfg(desktop)]
pub fn decoded_audio(id: u64, result: Result<Vec<u8>, String>) {
    let decodes = DECODES.lock().unwrap_or_else(|err| err.into_inner());
    if let Some((_, tx)) = decodes.iter().find(|(known, _)| *known == id) {
        let _ = tx.send(result);
    }
}

/// Ставит индикатор сверху по центру того монитора, где сейчас работают.
#[cfg(desktop)]
fn place_hud(window: &WebviewWindow) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };

    let scale = monitor.scale_factor();
    let area = monitor.size();
    let origin = monitor.position();

    let width = (HUD_WIDTH * scale) as i32;
    let x = origin.x + (area.width as i32 - width) / 2;
    let y = origin.y + (HUD_TOP * scale) as i32;

    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Размер индикатора в логических пикселях — как в макете.
#[cfg(desktop)]
const HUD_WIDTH: f64 = 360.0;
#[cfg(desktop)]
const HUD_HEIGHT: f64 = 180.0;
/// Отступ сверху: индикатор не должен налезать на строку заголовка чужого окна.
#[cfg(desktop)]
const HUD_TOP: f64 = 16.0;

/// Открывает попап на вопрос, заданный голосом с чистого места.
///
/// Отличий от обычного открытия два: якоря выделения нет — окно встаёт у
/// курсора, — и ответ читается вслух, потому что и вопрос был голосом.
pub fn show_for_voice(app: &AppHandle, question: String) -> tauri::Result<()> {
    // Окно появилось — отсчёт бездействия начинается отсюда.
    touch_popup();
    #[cfg(desktop)]
    release_control();
    #[cfg(desktop)]
    OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    #[cfg(desktop)]
    BY_VOICE.store(true, std::sync::atomic::Ordering::Relaxed);

    let fresh = app.get_webview_window(POPUP_LABEL).is_none();
    let window = ensure_popup_window(app)?;

    let state = app.state::<AppState>();
    let (theme, error_text, dialogue) = {
        let config = state.config();
        (
            config.ui.theme.clone(),
            config.ui.resolved_error_text(),
            config.ai.provider != "wikipedia",
        )
    };

    let payload = OpenPayload {
        term: question.clone(),
        context: String::new(),
        theme,
        error_text,
        dialogue,
        speak: true,
        answer: None,
    };
    state.set_selection(Selection {
        text: question,
        rect: None,
        cursor: cursor_position(),
        context: String::new(),
    });

    let _ = window.hide();
    if fresh {
        *PENDING.lock().unwrap_or_else(|err| err.into_inner()) = Some(payload);
    } else {
        window.emit_to(POPUP_LABEL, "popup:open", payload)?;
    }

    #[cfg(desktop)]
    crate::voice::hotkey::arm(true);
    Ok(())
}

/// Показывает напоминание о задаче.
///
/// Это тот же попап, что и у голосового вопроса: посреди экрана, с текстом
/// вместо ответа. Отдельного окна ему не нужно — напоминание живёт секунды,
/// а человек уже знает, как это окно закрывается.
pub fn show_for_reminder(app: &AppHandle, text: String) -> tauri::Result<()> {
    touch_popup();
    #[cfg(desktop)]
    {
        BY_VOICE.store(true, std::sync::atomic::Ordering::Relaxed);
        release_control();
        OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    let fresh = app.get_webview_window(POPUP_LABEL).is_none();
    let window = ensure_popup_window(app)?;

    let state = app.state::<AppState>();
    let (theme, error_text) = {
        let config = state.config();
        (config.ui.theme.clone(), config.ui.resolved_error_text())
    };

    let payload = OpenPayload {
        term: String::new(),
        context: String::new(),
        theme,
        error_text,
        dialogue: false,
        speak: false,
        // Напоминание — готовый текст: спрашивать про него модель нечего.
        answer: Some(text.clone()),
    };
    state.set_selection(Selection {
        text,
        rect: None,
        cursor: cursor_position(),
        context: String::new(),
    });

    let _ = window.hide();
    if fresh {
        *PENDING.lock().unwrap_or_else(|err| err.into_inner()) = Some(payload);
    } else {
        window.emit_to(POPUP_LABEL, "popup:open", payload)?;
    }
    Ok(())
}

/// Где сейчас указатель мыши. Якорь для окна, открытого без выделения.
fn cursor_position() -> (f64, f64) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut point = POINT::default();
        // SAFETY: функция только читает положение указателя.
        if unsafe { GetCursorPos(&mut point) }.is_ok() {
            return (f64::from(point.x), f64::from(point.y));
        }
    }
    (0.0, 0.0)
}

/// Ставит окно по якорю и показывает его. Размер приходит из фронтенда в логических
/// пикселях — он единственный, кто знает реальную высоту контента.
///
/// `shadow_inset` — поле вокруг попапа внутри окна. Тень по выходит за границы
/// самого попапа на несколько десятков пикселей; без запаса окно обрезало бы её.
/// Поэтому окно всегда больше попапа на `2 × shadow_inset`, а позиция сдвигается
/// на `shadow_inset` обратно, чтобы визуальная рамка попапа встала точно по расчёту.
pub fn apply_geometry(
    app: &AppHandle,
    width: f64,
    height: f64,
    shadow_inset: f64,
) -> tauri::Result<()> {
    let window = ensure_popup_window(app)?;
    let state = app.state::<AppState>();
    let Some(selection) = state.selection() else {
        return Ok(());
    };

    let scale = window.scale_factor().unwrap_or(1.0);
    let (moved, sized) = {
        #[cfg(desktop)]
        {
            use std::sync::atomic::Ordering;
            (
                USER_MOVED.load(Ordering::Relaxed),
                USER_SIZED.load(Ordering::Relaxed),
            )
        }
        #[cfg(not(desktop))]
        {
            (false, false)
        }
    };

    if !sized {
        window.set_size(LogicalSize::new(
            width + shadow_inset * 2.0,
            height + shadow_inset * 2.0,
        ))?;
    }

    let anchor = selection.anchor();
    let size_physical = (width * scale, height * scale);

    // Границы того монитора, на котором находится выделение.
    let (mx, my, mw, mh) = monitor_bounds(&window, anchor.left(), anchor.top());
    let gap = GAP * scale;
    let inset = INSET * scale;

    let by_voice = {
        #[cfg(desktop)]
        {
            BY_VOICE.load(std::sync::atomic::Ordering::Relaxed)
        }
        #[cfg(not(desktop))]
        {
            false
        }
    };

    let (x, y) = if by_voice {
        centre(size_physical, (mx, my, mw, mh), inset)
    } else {
        place(anchor, size_physical, (mx, my, mw, mh), gap, inset)
    };

    if !moved {
        let pad = (shadow_inset * scale).round() as i32;
        window.set_position(PhysicalPosition::new(x - pad, y - pad))?;
    }
    window.show()?;
    // На части оконных менеджеров always-on-top «слетает» после show — подтверждаем.
    // Понятие «поверх других окон» — тоже настольное: на телефоне окна не делят
    // экран, там ему просто не с чем конкурировать. Как и apply_geometry в целом,
    // на мобильных этот путь не выполняется, но обязан собираться.
    #[cfg(desktop)]
    window.set_always_on_top(true)?;

    // На Linux окно приходится сфокусировать, иначе его нечем закрыть: глобального
    // состояния клавиш и кнопок мыши там прочитать нельзя, поэтому Esc
    // ловится только фронтендом, а он получает события лишь при фокусе. Это
    // сознательный размен в пользу работоспособности: без фокуса попап оставался бы
    // висеть на экране навсегда. На Windows и macOS фокус не трогаем — там закрытие
    // отслеживает watcher, и правило «не отбирать фокус» соблюдается.
    #[cfg(target_os = "linux")]
    window.set_focus()?;

    Ok(())
}

/// Чистая функция позиционирования — вынесена отдельно ради тестов.
/// Всё в физических пикселях; `monitor` — (x, y, width, height) рабочей области.
/// Середина экрана, чуть выше геометрического центра.
///
/// «Чуть выше» — не украшение. Ровно посередине окно кажется съехавшим вниз:
/// глаз считает центром точку выше настоящей, и любое сообщение, поставленное
/// по геометрическому центру, читается как провалившееся. Треть высоты — та
/// самая оптическая середина.
fn centre(size: (f64, f64), monitor: (f64, f64, f64, f64), inset: f64) -> (i32, i32) {
    let (mx, my, mw, mh) = monitor;
    let (w, h) = size;

    let x = mx + (mw - w) / 2.0;
    let y = my + (mh - h) * 0.32;

    // Порядок важен: сначала не даём уйти вниз, потом — вверх. Окно, которое
    // на экран не влезает целиком, должно потерять низ, а не заголовок.
    let y = y.min(my + mh - h - inset).max(my + inset);
    (x.round() as i32, y.round() as i32)
}

fn place(
    anchor: ScreenRect,
    size: (f64, f64),
    monitor: (f64, f64, f64, f64),
    gap: f64,
    inset: f64,
) -> (i32, i32) {
    let (mx, my, mw, mh) = monitor;
    let (w, h) = size;

    // По горизонтали — центр выделения, прижатый к inset.
    let mut left = anchor.x + anchor.width / 2.0 - w / 2.0;
    left = left.clamp(mx + inset, (mx + mw - w - inset).max(mx + inset));

    // По вертикали — сначала над выделением.
    let mut top = anchor.top() - gap - h;
    if top < my + inset {
        // Не помещается сверху — зеркалим под выделение.
        top = anchor.bottom() + gap;
    }

    // Финальный зажим в границы экрана. Нужен не только когда окно не помещается:
    // выделение может целиком уехать за край (прокрутили документ, сменился
    // монитор), и тогда «под выделением» — это далеко за пределами экрана.
    let max_top = (my + mh - h - inset).max(my + inset);
    top = top.clamp(my + inset, max_top);

    (left.round() as i32, top.round() as i32)
}

/// Рабочая область монитора под точкой. Если монитор определить не удалось —
/// берём первый доступный, лишь бы окно не улетело в никуда.
fn monitor_bounds(window: &WebviewWindow, x: f64, y: f64) -> (f64, f64, f64, f64) {
    let monitors = window.available_monitors().unwrap_or_default();
    let containing = monitors.iter().find(|m| {
        let pos = m.position();
        let size = m.size();
        x >= pos.x as f64
            && x < (pos.x as f64 + size.width as f64)
            && y >= pos.y as f64
            && y < (pos.y as f64 + size.height as f64)
    });

    let monitor = containing
        .or_else(|| monitors.first())
        .map(|m| (m.position().x as f64, m.position().y as f64, m.size().width as f64, m.size().height as f64));

    monitor.unwrap_or((0.0, 0.0, 1920.0, 1080.0))
}

/// Когда с попапом последний раз что-то происходило.
///
/// `None` — попапа нет. Время сдвигают и действия человека (навёл мышь, набрал
/// букву, прокрутил), и работа программы (пришёл ответ, читается вслух).
#[cfg(desktop)]
static ALIVE_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Отмечает, что попап не заброшен.
#[cfg(desktop)]
pub fn touch_popup() {
    *ALIVE_AT.lock().unwrap_or_else(|err| err.into_inner()) = Some(std::time::Instant::now());
}

/// Сколько попап стоит без единого события. `None` — попапа нет.
#[cfg(desktop)]
pub fn popup_idle() -> Option<std::time::Duration> {
    ALIVE_AT
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .map(|at| at.elapsed())
}

/// Спросили голосом, а не выделением.
///
/// От этого зависит, где встанет окно. Выделение — это место на экране, куда
/// человек смотрит, и окно должно оказаться рядом с ним. У вопроса, заданного
/// голосом, такого места нет: человек мог смотреть куда угодно, а курсор стоит
/// там, где его бросили. Ставить окно у случайной точки — значит заставлять
/// искать его глазами.
#[cfg(desktop)]
static BY_VOICE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Сколько раз попап открывали. Служит окну именем: по нему видно, то ли это
/// окно, о котором шла речь, или его успели закрыть и открыть заново.
#[cfg(desktop)]
static OPENS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Какой попап открыт сейчас.
#[cfg(desktop)]
pub fn popup_generation() -> u64 {
    OPENS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Закрывает попап, если это всё ещё тот самый.
///
/// Отложенные закрытия — по концу разговора, по бездействию — принимают решение
/// заранее, а выполняют его спустя время. За это время человек мог выделить
/// новое слово, и закрывать пришлось бы уже чужое окно.
#[cfg(desktop)]
pub fn hide_popup_if(app: &AppHandle, generation: u64) {
    if popup_generation() == generation {
        hide_popup(app);
    }
}

/// Человек передвинул окно сам — больше его не двигаем.
#[cfg(desktop)]
static USER_MOVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Человек задал размер сам — больше под содержимое не подгоняем.
#[cfg(desktop)]
static USER_SIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Отмечает, что окно взяли в руки: за заголовок или за край.
///
/// Дальше геометрия — забота человека, а не программы. Иначе первое же
/// изменение содержимого (пришёл ответ, раскрыли «простыми словами») вернуло бы
/// окно на прежнее место прежнего размера, и растянуть его было бы невозможно.
#[cfg(desktop)]
pub fn take_over(moved: bool, sized: bool) {
    use std::sync::atomic::Ordering;

    if moved {
        USER_MOVED.store(true, Ordering::Relaxed);
    }
    if sized {
        USER_SIZED.store(true, Ordering::Relaxed);
    }
}

/// Возвращает окно под управление программы. Зовётся, когда попап открывается
/// заново: у нового вопроса своё место у нового выделения.
#[cfg(desktop)]
fn release_control() {
    use std::sync::atomic::Ordering;

    USER_MOVED.store(false, Ordering::Relaxed);
    USER_SIZED.store(false, Ordering::Relaxed);
}

/// Виден ли сейчас попап.
pub fn is_popup_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POPUP_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Закрывает окно попапа, чтобы оно создалось заново при следующем выделении.
///
/// Нужно после сна компьютера: окно у нас прозрачное и поверх остальных, а
/// такое рисуется через подсистему композиции, связь с которой сон разрывает.
/// Починить существующее окно нечем — только построить новое, благо стоит это
/// доли секунды и происходит незаметно, пока человек ничего не выделял.
pub fn rebuild_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        // destroy, а не hide: спрятанное окно осталось бы тем же самым, с той же
        // разорванной связью, и попап так и не появился бы.
        if let Err(err) = window.destroy() {
            log::warn!("не удалось закрыть окно попапа: {err}");
            return;
        }
    }
    app.state::<AppState>().clear_selection();
}

pub fn hide_popup(app: &AppHandle) {
    #[cfg(desktop)]
    crate::voice::hotkey::arm(false);
    #[cfg(desktop)]
    crate::voice::stop();
    // Окно закрыли — разговор окончен, даже если «спасибо» не прозвучало.
    // Открытый микрофон при закрытом окне — не то, чего от программы ждут.
    #[cfg(desktop)]
    crate::stop_conversation(app);
    // И индикатор убираем всегда, а не только когда шёл разговор: он мог
    // остаться от чтения вслух, а окна, к которому он относится, уже нет.
    #[cfg(desktop)]
    hide_hud(app);

    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        use tauri::Emitter;
        // Окну — весть, что его спрятали: незаконченные запросы отменяются,
        // иначе ответ, пришедший после Esc, прочитался бы вслух.
        let _ = window.emit_to(POPUP_LABEL, "popup:closed", ());
        let _ = window.hide();
    }
    *ALIVE_AT.lock().unwrap_or_else(|err| err.into_inner()) = None;
    app.state::<AppState>().clear_selection();
}

/// Скрипт, проставляющий тему и язык до первого кадра страницы.
///
/// Выполняется при создании документа, когда `<html>` может ещё не
/// существовать, — поэтому пробуем сразу, а если элемента нет, ждём готовности
/// разметки. Значения подставляются как строки JSON: тема приходит из файла
/// настроек, который человек вправе править руками, и кавычка в нём иначе
/// сломала бы весь скрипт.
fn theme_script(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let config = state.config();
    let theme = serde_json::to_string(&config.ui.theme).unwrap_or_else(|_| "\"system\"".into());
    let language = serde_json::to_string(&config.ui.language).unwrap_or_else(|_| "\"ru\"".into());

    format!(
        r#"(function () {{
  var view = {{ theme: {theme}, language: {language} }};
  window.__SUFLER_VIEW__ = view;
  var apply = function () {{
    if (!document.documentElement) return false;
    document.documentElement.dataset.theme = view.theme;
    document.documentElement.lang = view.language;
    return true;
  }};
  if (!apply()) {{
    document.addEventListener('readystatechange', apply);
  }}
}})();"#
    )
}

/// Окно онбординга: объясняет, какого разрешения не хватает, и как его выдать.
/// Окно списка задач.
pub const TASKS_LABEL: &str = "tasks";

/// Показывает список задач. Если окно уже есть — поднимает его наверх.
pub fn show_tasks(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(TASKS_LABEL) {
        bring_forward(&window);
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(app, TASKS_LABEL, WebviewUrl::App("tasks.html".into()))
            // Тема — до первого кадра, как и у остальных окон.
            .initialization_script(&theme_script(app))
            .title("Суфлёр — задачи")
            .inner_size(400.0, 560.0)
            .min_inner_size(320.0, 320.0)
            .resizable(true)
            // Рамки нет: окно должно читаться как карточка, а не как программа.
            // Двигают его за заголовок — см. tasks.js.
            .decorations(false)
            // На панели задач: свёрнутое окно возвращают оттуда.
            .skip_taskbar(false)
            .build()?;

    // У правого края экрана, ближе к верху: список задач смотрят краем глаза,
    // не отрываясь от работы, и середина экрана ему не место.
    let (x, y) = tasks_corner(&window);
    window.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

/// Правый верхний угол рабочего стола с отступом.
fn tasks_corner(window: &WebviewWindow) -> (i32, i32) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return (120, 120);
    };
    let scale = monitor.scale_factor();
    let area = monitor.size();
    let at = monitor.position();
    let size = window.outer_size().unwrap_or_default();
    let margin = (INSET * 3.0 * scale).round() as i32;

    (
        at.x + area.width as i32 - size.width as i32 - margin,
        at.y + margin,
    )
}

/// Окно заказа продуктов.
pub const ORDER_LABEL: &str = "order";

/// Показывает, что сейчас с заказом.
pub fn show_order(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ORDER_LABEL) {
        bring_forward(&window);
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(app, ORDER_LABEL, WebviewUrl::App("order.html".into()))
            .initialization_script(&theme_script(app))
            .title("Суфлёр — заказ")
            .inner_size(400.0, 520.0)
            .min_inner_size(320.0, 300.0)
            .resizable(true)
            .decorations(false)
            // На панели задач: свёрнутое окно возвращают оттуда.
            .skip_taskbar(false)
            .build()?;

    // Там же, где список задач: у правого края, ближе к верху. Оба окна —
    // про то, что человек попросил, и искать их логично в одном месте.
    let (x, y) = tasks_corner(&window);
    window.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

pub const LEARN_LABEL: &str = "learning";

/// Показывает окно обучения. Если окно уже есть — поднимает его наверх.
pub fn show_learning(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(LEARN_LABEL) {
        bring_forward(&window);
        return Ok(());
    }
    // Посреди экрана и крупнее прочих окон: здесь читают уроки и пишут ответы.
    WebviewWindowBuilder::new(app, LEARN_LABEL, WebviewUrl::App("learning.html".into()))
        .initialization_script(theme_script(app))
        .title("Суфлёр — обучение")
        .inner_size(1000.0, 700.0)
        .min_inner_size(720.0, 480.0)
        .resizable(true)
        .decorations(false)
        .center()
        .build()?;
    Ok(())
}

/// Прячет окно обучения.
pub fn hide_learning(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LEARN_LABEL) {
        let _ = window.hide();
    }
}

pub const WATCH_LABEL: &str = "watchlist";

/// Показывает список активов. Если окно уже есть — поднимает его наверх.
pub fn show_watchlist(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(WATCH_LABEL) {
        bring_forward(&window);
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(app, WATCH_LABEL, WebviewUrl::App("watchlist.html".into()))
            .initialization_script(&theme_script(app))
            .title("Суфлёр — активы")
            .inner_size(640.0, 460.0)
            .min_inner_size(480.0, 280.0)
            .resizable(true)
            .decorations(false)
            // На панели задач: свёрнутое окно возвращают оттуда.
            .skip_taskbar(false)
            .build()?;

    // Там же, где задачи и заказ: у правого края, ближе к верху.
    let (x, y) = tasks_corner(&window);
    window.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

pub const USAGE_LABEL: &str = "usage";

/// Окно модуля: у каждого модуля своё, метка — `module-<id>`.
///
/// Модуль описывает окно разметкой, а рамку, заголовок и оформление даёт Ноа:
/// окно модуля — это окно Ноа, а не страница в браузере.
pub fn show_module(app: &AppHandle, manifest: &crate::module_kit::Manifest) -> tauri::Result<()> {
    let label = format!("module-{}", manifest.id.replace(|c: char| !c.is_alphanumeric(), "-"));
    if let Some(window) = app.get_webview_window(&label) {
        bring_forward(&window);
        return Ok(());
    }

    let spec = &manifest.window;
    let title = if spec.title.trim().is_empty() { manifest.title.clone() } else { spec.title.clone() };
    let width = if spec.width == 0 { 460 } else { spec.width } as f64;
    let height = if spec.height == 0 { 520 } else { spec.height } as f64;

    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("module.html?id={}", manifest.id).into()),
    )
    .initialization_script(&theme_script(app))
    .title(format!("Суфлёр — {title}"))
    .inner_size(width, height)
    .min_inner_size(320.0, 240.0)
    .resizable(true)
    .decorations(false)
    .skip_taskbar(false)
    .build()?;

    let (x, y) = tasks_corner(&window);
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    Ok(())
}
const USAGE_WIDTH: f64 = 330.0;
const USAGE_HEIGHT: f64 = 30.0;

/// Виджет расхода: маленькая карточка на рабочем столе, живёт, пока работает
/// программа. Перетаскивается за любое место; место запоминается.
pub fn show_usage_widget(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(USAGE_LABEL) {
        window.show()?;
        #[cfg(target_os = "windows")]
        make_passive(&window);
        return Ok(());
    }
    let widget = app.state::<AppState>().config().widget.clone();
    let window = WebviewWindowBuilder::new(app, USAGE_LABEL, WebviewUrl::App("usage.html".into()))
        .initialization_script(&theme_script(app))
        .title("Суфлёр — расход")
        .inner_size(USAGE_WIDTH, USAGE_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(widget.on_top)
        .always_on_bottom(!widget.on_top)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(target_os = "windows")]
    make_passive(&window);

    // Сохранённое место — только если оно на одном из экранов: монитор могли
    // отключить, а в старых настройках могли остаться координаты свёрнутого окна.
    let visible = |x: i32, y: i32| {
        window
            .available_monitors()
            .map(|monitors| {
                monitors.iter().any(|monitor| {
                    let (at, size) = (monitor.position(), monitor.size());
                    x >= at.x - 50
                        && y >= at.y - 50
                        && x < at.x + size.width as i32 - 20
                        && y < at.y + size.height as i32 - 20
                })
            })
            .unwrap_or(false)
    };
    let position = match (widget.x, widget.y) {
        (Some(x), Some(y)) if visible(x, y) => PhysicalPosition::new(x, y),
        // Впервые — в правом верхнем углу основного экрана.
        _ => match window.primary_monitor()? {
            Some(monitor) => {
                let scale = monitor.scale_factor();
                PhysicalPosition::new(
                    monitor.position().x + monitor.size().width as i32 - ((USAGE_WIDTH + 24.0) * scale) as i32,
                    monitor.position().y + (64.0 * scale) as i32,
                )
            }
            None => PhysicalPosition::new(40, 40),
        },
    };
    window.set_position(position)?;
    window.show()?;
    // Показ возвращает окну стиль «окно приложения»: без повтора виджет
    // оказывался в группе Суфлёра на панели задач и в доке, рядом с настоящими
    // окнами, и сворачивался вместо них.
    #[cfg(target_os = "windows")]
    make_passive(&window);

    let handle = app.clone();
    let widget_window = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(to) = event {
            // Свёрнут («Свернуть все окна», Win+D) — виджет рабочего стола
            // должен остаться на рабочем столе.
            if to.x <= -30000 || to.y <= -30000 {
                let widget = widget_window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let _ = widget.unminimize();
                    #[cfg(target_os = "windows")]
                    make_passive(&widget);
                });
                return;
            }
            remember_widget_position(&handle, to.x, to.y);
        }
    });
    Ok(())
}

pub fn hide_usage_widget(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(USAGE_LABEL) {
        let _ = window.close();
    }
}

/// Поверх окон или на рабочем столе.
pub fn pin_usage_widget(app: &AppHandle, on_top: bool) {
    if let Some(window) = app.get_webview_window(USAGE_LABEL) {
        let _ = window.set_always_on_bottom(!on_top);
        let _ = window.set_always_on_top(on_top);
        #[cfg(target_os = "windows")]
        make_passive(&window);
    }
}

static WIDGET_MOVE: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);
static WIDGET_SAVING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Запоминает, куда перетащили виджет. На диск — один раз, когда движение
/// улеглось: событие приходит на каждый пиксель пути.
fn remember_widget_position(app: &AppHandle, x: i32, y: i32) {
    use std::sync::atomic::Ordering;
    // −32000 — так Windows ставит свёрнутое окно («Свернуть все окна»). Это не
    // место, куда виджет перетащили, и запоминать его нельзя.
    if x <= -30000 || y <= -30000 {
        return;
    }
    *WIDGET_MOVE.lock().unwrap_or_else(|err| err.into_inner()) = Some((x, y));
    if WIDGET_SAVING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        WIDGET_SAVING.store(false, Ordering::SeqCst);
        let Some((x, y)) = WIDGET_MOVE.lock().unwrap_or_else(|err| err.into_inner()).take() else {
            return;
        };
        let state = app.state::<AppState>();
        {
            let mut config = state.config_mut();
            config.widget.x = Some(x);
            config.widget.y = Some(y);
        }
        if let Err(err) = crate::commands::persist(&app, &state) {
            log::warn!("место виджета не сохранилось: {err}");
        }
    });
}

/// Прячет окно активов.
pub fn hide_watchlist(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WATCH_LABEL) {
        let _ = window.hide();
    }
}

/// Прячет окно заказа.
pub fn hide_order(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ORDER_LABEL) {
        let _ = window.hide();
    }
}

/// Прячет окно задач.
pub fn hide_tasks(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TASKS_LABEL) {
        let _ = window.hide();
    }
}

pub fn show_onboarding(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ONBOARDING_LABEL) {
        bring_forward(&window);
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        ONBOARDING_LABEL,
        WebviewUrl::App("onboarding.html".into()),
    )
    // Тема проставляется до того, как страница начнёт рисоваться.
    //
    // Раньше в разметке стояла тема по умолчанию, а настоящую окно узнавало
    // запросом к Rust уже после загрузки — и на секунду показывало чужую.
    // Здесь скрипт выполняется в момент создания документа, до первого кадра.
    .initialization_script(&theme_script(app))
    .title("Суфлёр — настройка и проверка")
    .inner_size(560.0, 720.0)
    // Окно выросло: кроме разрешений в нём теперь живая проверка перехвата и выбор
    // источника объяснений. На ноутбуке с невысоким экраном фиксированная высота
    // обрезала бы нижнюю половину, поэтому размер отдан пользователю.
    .resizable(true)
    .min_inner_size(460.0, 420.0)
    .build()?;

    window.set_position(LogicalPosition::new(120.0, 120.0))?;
    bring_forward(&window);
    Ok(())
}

/// Разворачивает свёрнутые окна Суфлёра. `false` — свёрнутых нет.
pub fn restore_minimized(app: &AppHandle) -> bool {
    let mut restored = false;
    for label in [LEARN_LABEL, TASKS_LABEL, WATCH_LABEL, ORDER_LABEL, ONBOARDING_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            if window.is_minimized().unwrap_or(false) {
                bring_forward(&window);
                restored = true;
            }
        }
    }
    restored
}

/// Выводит окно наверх: разворачивает, показывает и отдаёт ему фокус.
///
/// Одного `set_focus` мало. Windows не отдаёт передний план программе, которая
/// сейчас не на нём: окно остаётся под остальными, а на панели задач лишь
/// мигает кнопка — щелчок по трею выглядел так, будто ничего не произошло.
/// Короткое «поверх всех» поднимает окно в любом случае.
pub fn bring_forward(window: &WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.set_always_on_top(false);
}

/// Раздел настроек, который окно должно показать, когда загрузится.
static SETTINGS_SECTION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Открывает настройки сразу на нужном разделе: «Ноа, открой настройки заказов».
///
/// Окну, которое уже открыто, раздел приходит событием. Только что созданному
/// событие не дойдёт — страница ещё грузится, слушателя нет, — поэтому раздел
/// придерживается здесь, и окно забирает его само: см. `take_settings_section`.
pub fn show_settings_section(app: &AppHandle, section: &str) -> tauri::Result<()> {
    use tauri::Emitter;

    let existed = app.get_webview_window(ONBOARDING_LABEL).is_some();
    if !existed {
        *SETTINGS_SECTION.lock().unwrap_or_else(|err| err.into_inner()) = Some(section.to_string());
    }
    show_onboarding(app)?;
    if existed {
        let _ = app.emit_to(ONBOARDING_LABEL, "onboarding:section", section.to_string());
    }
    Ok(())
}

/// Отдаёт придержанный раздел ровно один раз.
pub fn take_settings_section() -> Option<String> {
    SETTINGS_SECTION
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    #[test]
    fn voice_window_sits_mid_screen_a_bit_high() {
        let monitor = (0.0, 0.0, 1920.0, 1080.0);
        let (x, y) = centre((400.0, 300.0), monitor, 12.0);

        // По горизонтали — ровно посередине.
        assert_eq!(x, (1920 - 400) / 2);
        // По вертикали — выше середины, но не у самого края.
        assert!(y < (1080 - 300) / 2, "окно должно стоять выше центра");
        assert!(y > 100, "и всё же не под самой кромкой экрана");
    }

    #[test]
    fn a_tall_window_stays_on_screen() {
        let monitor = (0.0, 0.0, 1920.0, 1080.0);
        let (_, y) = centre((400.0, 1060.0), monitor, 12.0);
        assert_eq!(y, 12, "окно выше экрана прижимается к верхнему краю");
    }

    use super::*;

    const MONITOR: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1080.0);

    fn rect(x: f64, y: f64) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width: 100.0,
            height: 20.0,
        }
    }

    #[test]
    fn stands_above_selection_and_centers_on_it() {
        let (x, y) = place(rect(800.0, 500.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(x, 650, "центр окна совпадает с центром выделения");
        assert_eq!(y, 288, "окно над выделением с зазором 12px");
    }

    #[test]
    fn flips_below_when_no_room_above() {
        let (_, y) = place(rect(800.0, 40.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(y, 72, "зеркалится под выделение: bottom + gap");
    }

    #[test]
    fn clamps_to_screen_inset() {
        let (x, _) = place(rect(10.0, 500.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(x, 12, "прижато к левому краю минус inset");

        let (x, _) = place(rect(1900.0, 500.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(x, 1508, "прижато к правому краю минус inset");
    }

    #[test]
    fn falls_back_to_bottom_edge_when_it_fits_nowhere() {
        // Выделение вверху, а окно выше монитора: и сверху, и снизу не влезает.
        let (_, y) = place(rect(800.0, 30.0), (400.0, 1060.0), MONITOR, 12.0, 12.0);
        assert_eq!(y, 12, "прижимаем к верхнему inset, ниже уже некуда");
    }

    #[test]
    fn keeps_window_on_screen_when_selection_scrolled_away() {
        // Выделение уехало выше экрана: «под выделением» оказалось бы за краем.
        let (_, y) = place(rect(800.0, -400.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(y, 12, "окно прижато к верхнему краю, а не улетело за экран");

        // И то же самое снизу.
        let (_, y) = place(rect(800.0, 2000.0), (400.0, 200.0), MONITOR, 12.0, 12.0);
        assert_eq!(y, 868, "окно прижато к нижнему краю минус inset");
    }

    #[test]
    fn respects_secondary_monitor_origin() {
        let monitor = (1920.0, 0.0, 1920.0, 1080.0);
        let (x, _) = place(rect(1930.0, 500.0), (400.0, 200.0), monitor, 12.0, 12.0);
        assert_eq!(x, 1932, "inset считается от края второго монитора, а не от нуля");
    }
}
