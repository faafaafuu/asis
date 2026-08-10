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
}

/// Создаёт окно попапа, если его ещё нет. Окно рождается скрытым: первым делом его
/// надо измерить и поставить на место, иначе пользователь увидит прыжок (SPEC §4).
pub fn ensure_popup_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        return Ok(window);
    }

    let window = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("popup.html".into()))
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

    Ok(window)
}

/// Показывает попап для нового выделения: сохраняет якорь и отдаёт фронтенду термин.
/// Само окно появится в `apply_geometry`, когда фронтенд сообщит свой размер.
pub fn show_for_selection(app: &AppHandle, selection: Selection) -> tauri::Result<()> {
    let window = ensure_popup_window(app)?;

    let state = app.state::<AppState>();
    let (theme, error_text) = {
        let config = state.config();
        (config.ui.theme.clone(), config.ui.error_text.clone())
    };

    let payload = OpenPayload {
        term: selection.text.clone(),
        context: selection.context.clone(),
        theme,
        error_text,
    };
    state.set_selection(selection);

    // Окно прячем перед сменой якоря: переоткрытие идёт без анимации закрытия (SPEC §8).
    let _ = window.hide();
    window.emit_to(POPUP_LABEL, "popup:open", payload)?;
    Ok(())
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

pub fn hide_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        let _ = window.hide();
    }
    app.state::<AppState>().clear_selection();
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
