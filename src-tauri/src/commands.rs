//! Команды, доступные фронтенду попапа.

use tauri::{AppHandle, Manager, State};

use crate::ai_client::{Explanation, ThreadItem};
use crate::config::RuntimeConfig;
use crate::overlay;
use crate::selection::Capability;
use crate::state::AppState;

/// Настройки, нужные окну при старте: тема и текст ошибки по умолчанию.
#[tauri::command]
pub fn runtime_config(state: State<'_, AppState>) -> RuntimeConfig {
    RuntimeConfig::from(&*state.config.read().expect("config poisoned"))
}

/// Фронтенд посчитал свой размер — можно ставить окно на место и показывать.
/// Вызывается и при первом рендере, и после каждого изменения высоты
/// (раскрытие, новый ответ в треде).
#[tauri::command]
pub fn popup_ready(
    app: AppHandle,
    width: f64,
    height: f64,
    shadow_inset: f64,
) -> Result<(), String> {
    overlay::apply_geometry(&app, width, height, shadow_inset).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn close_popup(app: AppHandle) {
    overlay::hide_popup(&app);
}

#[tauri::command]
pub async fn ai_explain(
    state: State<'_, AppState>,
    term: String,
    context: String,
) -> Result<Explanation, String> {
    let provider = state.provider();
    let fallback = state.error_text();
    provider
        .explain(&term, &context)
        .await
        .map_err(|err| err.user_text(&fallback))
}

#[tauri::command]
pub async fn ai_ask(
    state: State<'_, AppState>,
    term: String,
    context: String,
    thread: Vec<ThreadItem>,
    question: String,
) -> Result<String, String> {
    let provider = state.provider();
    let fallback = state.error_text();
    provider
        .ask(&term, &context, &thread, &question)
        .await
        .map_err(|err| err.user_text(&fallback))
}

/// Состояние системной интеграции — для окна онбординга.
#[tauri::command]
pub fn integration_status(app: AppHandle) -> Capability {
    let integration = app.state::<crate::watcher::Integration>();
    integration.capability()
}

/// Открыть системные настройки с нужным разрешением (macOS Accessibility и т.п.).
#[tauri::command]
pub fn open_permission_settings(app: AppHandle) -> bool {
    let integration = app.state::<crate::watcher::Integration>();
    integration.open_permission_settings()
}
