//! Оверлей-окно попапа: создание, показ, позиционирование (SPEC §4).
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

/// Зазор между окном и выделением, логические пиксели (SPEC §11).
const GAP: f64 = 12.0;
/// Отступ от краёв экрана, логические пиксели (SPEC §11).
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
}

/// Создаёт окно попапа, если его ещё нет. Окно рождается скрытым: первым делом его
/// надо измерить и поставить на место, иначе пользователь увидит прыжок (SPEC §4).
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
    // исключительно на десктопе (SPEC §9.4, §9.5). На телефоне вход другой —
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
        // пользователь выделил текст (SPEC §8).
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
    };
    state.set_selection(selection);

    // Окно прячем перед сменой якоря: переоткрытие идёт без анимации закрытия (SPEC §8).
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
    let _ = window.show();
}

#[cfg(desktop)]
pub fn hide_hud(app: &AppHandle) {
    *HUD_MODE.lock().unwrap_or_else(|err| err.into_inner()) = None;
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
    place_hud(&window);
    Ok(window)
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
/// `shadow_inset` — поле вокруг попапа внутри окна. Тень по SPEC §5 выходит за границы
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
    // состояния клавиш и кнопок мыши там прочитать нельзя (SPEC §9.3), поэтому Esc
    // ловится только фронтендом, а он получает события лишь при фокусе. Это
    // сознательный размен в пользу работоспособности: без фокуса попап оставался бы
    // висеть на экране навсегда. На Windows и macOS фокус не трогаем — там закрытие
    // отслеживает watcher, и правило «не отбирать фокус» (SPEC §8) соблюдается.
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

/// Окно онбординга: объясняет, какого разрешения не хватает, и как его выдать
/// (SPEC §9.2, §14 — «понятный экран с инструкцией, а не тихий отказ»).
pub fn show_onboarding(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ONBOARDING_LABEL) {
        window.show()?;
        window.set_focus()?;
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
    Ok(())
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
