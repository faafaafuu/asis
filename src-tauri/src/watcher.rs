//! Фоновый наблюдатель за системным выделением.
//!
//! Отдельный поток опрашивает платформенную интеграцию и, поймав жест
//! «отпустил левую кнопку мыши с зажатым левым Ctrl», просит показать попап.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::selection::{create, Capability, Diagnostics, PlatformIntegration, POLL_INTERVAL_MS};
use crate::state::AppState;
use crate::overlay;

/// Обёртка над платформенной интеграцией: кладётся в managed state, чтобы команды
/// могли спросить у неё статус разрешений.
pub struct Integration(Arc<dyn PlatformIntegration>);

impl Integration {
    pub fn capability(&self) -> Capability {
        self.0.capability()
    }

    pub fn open_permission_settings(&self) -> bool {
        self.0.open_permission_settings()
    }

    pub fn diagnostics(&self) -> Diagnostics {
        self.0.diagnostics()
    }
}

/// Насколько быстрый повтор того же выделения считаем одним жестом.
///
/// Порог человеческий, а не технический: интервал двойного щелчка в Windows
/// по умолчанию 500 мс и настраивается пользователем в бо́льшую сторону. Брать
/// сильно больше нельзя — осмысленное повторное выделение того же слова
/// (например, после того как попап закрыли) перестанет открывать окно.
const REPEAT_WINDOW: Duration = Duration::from_millis(700);

/// Разрыв между кругами опроса, после которого считаем, что компьютер спал.
///
/// Круг занимает единицы миллисекунд, так что полминуты — заведомо не задержка
/// от загруженности системы. Меньше брать нельзя: под нагрузкой поток может
/// простоять и несколько секунд, а пересоздавать окно на ровном месте незачем.
const SLEEP_GAP: Duration = Duration::from_secs(30);

/// Запускает наблюдателя. Возвращает интеграцию, чтобы вызывающий положил её в state.
pub fn spawn(app: &AppHandle) -> Integration {
    let integration: Arc<dyn PlatformIntegration> = Arc::from(create());
    let capability = integration.capability();

    if !capability.is_ready() {
        // Не молчим: без разрешения или без системного API продукт не работает,
        // и пользователь должен узнать об этом сразу, а не по отсутствию реакции
        // (SPEC §14).
        log::warn!("системная интеграция недоступна: {capability:?}");
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = overlay::show_onboarding(&app) {
                log::error!("не удалось показать онбординг: {err}");
            }
        });
    }

    let worker = integration.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        log::info!("наблюдатель за выделением запущен");
        // Последнее показанное выделение — чтобы отличить новый жест от повтора.
        let mut last_shown: Option<(String, Instant)> = None;
        // Отметка предыдущего круга — по ней виден сон компьютера, см. ниже.
        let mut last_tick = Instant::now();
        loop {
            // Компьютер просыпается после сна — пересоздаём окно попапа.
            //
            // Попап прозрачный и висит поверх других окон; такое окно рисуется
            // через подсистему композиции Windows, и после сна связь с ней
            // теряется. Снаружи это выглядит так, будто программа сломалась
            // насовсем: жест ловится, запрос уходит, ответ приходит — а окна нет,
            // и в журнале сыплется «WebView2 error 0x8007139F». Лечилось только
            // перезапуском, о котором человек догадаться не может.
            //
            // Сон определяем по себе: круг занимает миллисекунды, и если между
            // двумя кругами прошло больше полуминуты, значит нас усыпили.
            // Отдельного системного сообщения о пробуждении не ждём — для него
            // нужно своё окно и очередь сообщений, а этот способ ничего не стоит.
            if last_tick.elapsed() > SLEEP_GAP {
                log::info!("похоже, компьютер просыпался — пересоздаём окно попапа");
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || overlay::rebuild_popup(&handle));
            }
            last_tick = Instant::now();

            let config = {
                let state = app.state::<AppState>();
                let config = state.config();
                config.trigger.clone()
            };

            // Закрытие попапа по Esc и по клику вне окна. Оба события ловятся здесь,
            // а не во фронтенде: окно не забирает фокус, и клавиатурные события до
            // него не доходят (SPEC §8).
            if overlay::is_popup_visible(&app) {
                let escape = worker.is_escape_pressed();
                let outside_click = worker.is_primary_mouse_down()
                    && worker
                        .cursor_position()
                        .map(|(x, y)| !overlay::is_point_inside_popup(&app, x, y))
                        .unwrap_or(false);

                if escape || outside_click {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || overlay::hide_popup(&handle));
                    std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                    continue;
                }
            }

            if let Some(selection) = worker.poll_trigger(&config) {
                if !selection.text.trim().is_empty() {
                    // Двойной щелчок — самый естественный способ выделить одно слово,
                    // но для системы это два нажатия и два отпускания, то есть два
                    // жеста подряд с одним и тем же текстом. Человек считает, что
                    // выделил один раз, а платит за это ожиданием: локальная модель
                    // считает оба запроса по очереди, и ответ приходит вдвое позже.
                    let repeat = last_shown
                        .as_ref()
                        .is_some_and(|(text, at)| {
                            *text == selection.text && at.elapsed() < REPEAT_WINDOW
                        });

                    if repeat {
                        log::debug!("тот же текст сразу следом — считаем повтором жеста");
                    } else {
                        last_shown = Some((selection.text.clone(), Instant::now()));
                        // Показ окна — только в главном потоке: этого требуют оконные API
                        // всех трёх платформ.
                        let handle = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if let Err(err) = overlay::show_for_selection(&handle, selection) {
                                log::error!("не удалось показать попап: {err}");
                            }
                        });
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });

    Integration(integration)
}
