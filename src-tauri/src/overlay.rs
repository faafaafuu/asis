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
        .resizable(false)
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
    window.emit_to(POPUP_LABEL, "popup:open", payload)?;

    // Пока попап на экране, пробел принадлежит ему. В остальное время хук
    // пропускает клавиши насквозь и ничего не трогает.
    #[cfg(desktop)]
    crate::voice::hotkey::arm(true);
    Ok(())
}

/// Открывает попап на вопрос, заданный голосом с чистого места.
///
/// Отличий от обычного открытия два: якоря выделения нет — окно встаёт у
/// курсора, — и ответ читается вслух, потому что и вопрос был голосом.
pub fn show_for_voice(app: &AppHandle, question: String) -> tauri::Result<()> {
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
    window.emit_to(POPUP_LABEL, "popup:open", payload)?;

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
    window.set_size(LogicalSize::new(
        width + shadow_inset * 2.0,
        height + shadow_inset * 2.0,
    ))?;

    let anchor = selection.anchor();
    let size_physical = (width * scale, height * scale);

    // Границы того монитора, на котором находится выделение.
    let (mx, my, mw, mh) = monitor_bounds(&window, anchor.left(), anchor.top());
    let gap = GAP * scale;
    let inset = INSET * scale;

    let (x, y) = place(anchor, size_physical, (mx, my, mw, mh), gap, inset);

    let pad = (shadow_inset * scale).round() as i32;
    window.set_position(PhysicalPosition::new(x - pad, y - pad))?;
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

/// Виден ли сейчас попап.
pub fn is_popup_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POPUP_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Попала ли точка в окно попапа. По этому решается, закрывать ли его при клике:
/// клик внутри — это работа с попапом, клик снаружи — закрытие (SPEC §8).
pub fn is_point_inside_popup(app: &AppHandle, x: f64, y: f64) -> bool {
    let Some(window) = app.get_webview_window(POPUP_LABEL) else {
        return false;
    };
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return false;
    };
    x >= pos.x as f64
        && x <= pos.x as f64 + size.width as f64
        && y >= pos.y as f64
        && y <= pos.y as f64 + size.height as f64
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

    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        let _ = window.hide();
    }
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
