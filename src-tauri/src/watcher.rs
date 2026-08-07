//! Фоновый наблюдатель за системным выделением.
//!
//! Отдельный поток опрашивает платформенную интеграцию и, поймав жест
//! «отпустил левую кнопку мыши с зажатым левым Ctrl», просит показать попап.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::selection::{create, Capability, PlatformIntegration, POLL_INTERVAL_MS};
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
}

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
        loop {
            let config = {
                let state = app.state::<AppState>();
                let config = state.config.read().expect("config poisoned");
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

            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
    });

    Integration(integration)
}
