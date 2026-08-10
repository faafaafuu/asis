//! Команды, доступные фронтенду попапа.

use tauri::{AppHandle, Manager, State};

use crate::ai_client::{Explanation, ThreadItem};
use crate::config::{RuntimeConfig, TriggerConfig};
use crate::overlay;
use crate::selection::{Capability, Diagnostics};
use crate::state::AppState;

/// Записывает текущую конфигурацию на диск. Вынесено отдельно: настройки правятся
/// из двух мест окна (модель и триггер), а файл должен быть один и всегда целиком.
fn persist(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let path = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("не удалось определить каталог настроек: {err}"))?;
    std::fs::create_dir_all(&path).map_err(|err| err.to_string())?;

    let config = state.config.read().expect("config poisoned");
    let json = serde_json::to_string_pretty(&*config).map_err(|err| err.to_string())?;
    std::fs::write(path.join("config.json"), json).map_err(|err| err.to_string())
}

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

/// Настройки, которые пользователь может менять из окна: провайдер и доступ к модели.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub provider: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

/// Текущие настройки для окна. Ключ отдаём замаскированным: показывать его целиком
/// незачем, а понять «ключ сохранён» пользователю нужно.
#[tauri::command]
pub fn ai_settings(state: State<'_, AppState>) -> AiSettings {
    let config = state.config.read().expect("config poisoned");
    AiSettings {
        provider: config.ai.provider.clone(),
        endpoint: config.ai.endpoint.clone(),
        api_key: if config.ai.api_key.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        model: config.ai.model.clone(),
    }
}

/// Сохраняет настройки и сразу пересобирает провайдера — без перезапуска приложения.
#[tauri::command]
pub fn save_ai_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AiSettings,
) -> Result<(), String> {
    {
        let mut config = state.config.write().expect("config poisoned");
        config.ai.provider = settings.provider;
        config.ai.endpoint = settings.endpoint;
        config.ai.model = settings.model;
        // Пустое поле ключа означает «не менять»: в окно он приходит замаскированным,
        // и сохранять маску вместо настоящего ключа нельзя.
        if !settings.api_key.is_empty() && !settings.api_key.starts_with('•') {
            config.ai.api_key = settings.api_key;
        }
    }

    persist(&app, &state)?;

    let config = state.config.read().expect("config poisoned");
    state.rebuild_provider(&config.ai);
    Ok(())
}

/// Настройки жеста для окна: чем открывается попап и разрешён ли запасной способ.
#[tauri::command]
pub fn trigger_settings(state: State<'_, AppState>) -> TriggerConfig {
    state.config.read().expect("config poisoned").trigger.clone()
}

#[tauri::command]
pub fn save_trigger_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: TriggerConfig,
) -> Result<(), String> {
    {
        let mut config = state.config.write().expect("config poisoned");
        config.trigger = settings;
    }
    persist(&app, &state)
}

/// Что наблюдатель видел в последнее время — живая сводка для окна настройки.
#[tauri::command]
pub fn capture_diagnostics(app: AppHandle) -> Diagnostics {
    app.state::<crate::watcher::Integration>().diagnostics()
}

/// Открывает каталог с журналом в проводнике. Когда попап не появляется, журнал —
/// единственное место, где написано почему; заставлять пользователя искать путь
/// вида `%LOCALAPPDATA%\app.sufler.popup\logs` бессмысленно.
#[tauri::command]
pub fn open_logs(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|err| format!("каталог журнала не определён: {err}"))?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("не удалось открыть {}: {err}", dir.display()))
}

/// Пробный запрос: пользователь должен увидеть, что ключ рабочий, до того как
/// начнёт выделять текст и получать «Сбой сети».
#[tauri::command]
pub async fn test_ai(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state.provider();
    let fallback = state.error_text();
    provider
        .explain("альбедо", "")
        .await
        .map(|explanation| explanation.def)
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
