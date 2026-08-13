//! Команды, доступные фронтенду попапа.

use std::time::Duration;

use tauri::{AppHandle, Manager, State};

use crate::ai_client::{AiError, Explanation, ThreadItem};
use crate::config::{RuntimeConfig, TriggerConfig};
use crate::overlay;
use crate::selection::{Capability, Diagnostics};
use crate::state::AppState;

/// Записывает текущую конфигурацию на диск. Вынесено отдельно: настройки правятся
/// из двух мест окна (модель и триггер), а файл должен быть один и всегда целиком.
pub fn persist(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let path = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("не удалось определить каталог настроек: {err}"))?;
    std::fs::create_dir_all(&path).map_err(|err| err.to_string())?;

    // Ключ шифруется прямо перед записью: в памяти он нужен обычной строкой,
    // а на диске лежит файлом с обычными правами — его читает всё, что запущено
    // от имени пользователя, а на общей машине и соседняя учётная запись.
    let mut config = state.config().clone();
    config.ai.api_key = crate::secret::protect(&config.ai.api_key);

    let json = serde_json::to_string_pretty(&config).map_err(|err| err.to_string())?;
    std::fs::write(path.join("config.json"), json).map_err(|err| err.to_string())
}

/// Предел ожидания провайдера. Заведомо больше любого внутреннего таймаута
/// (у Википедии 8 секунд, у моделей 12 плюс повтор), потому что это не второй
/// таймаут, а рубеж на случай, когда внутренний почему-то не сработал.
const CALL_LIMIT: Duration = Duration::from_secs(25);

/// Выполняет обращение к провайдеру так, чтобы окно получило ответ при любом исходе.
///
/// Прямой `await` в команде оставлял окно с вечным индикатором сразу в двух случаях.
/// Паника внутри задачи не роняет приложение и никуда не печатается — она просто не
/// отвечает, а обещание в браузере остаётся висеть навсегда. Зависшая задача ведёт
/// себя точно так же. Отдельная задача превращает панику в `JoinError`, а внешний
/// предел — зависание в ошибку; и то и другое попадает в журнал с указанием, что
/// именно случилось, и на экран внятной фразой.
async fn guarded<T>(
    what: &str,
    task: impl std::future::Future<Output = Result<T, AiError>> + Send + 'static,
    fallback: String,
) -> Result<T, String>
where
    T: Send + 'static,
{
    let handle = tauri::async_runtime::spawn(task);
    match tokio::time::timeout(CALL_LIMIT, handle).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(err))) => {
            log::warn!("{what}: {err}");
            Err(err.user_text(&fallback))
        }
        Ok(Err(err)) => {
            log::error!("{what}: обработчик оборвался ({err})");
            Err("Внутренняя ошибка — подробности в журнале".into())
        }
        Err(_) => {
            log::error!("{what}: ответа нет дольше {} с", CALL_LIMIT.as_secs());
            Err("Ответ не пришёл. Проверьте соединение, а если нужен VPN — впишите прокси в настройке.".into())
        }
    }
}

/// Настройки, нужные окну при старте: тема и текст ошибки по умолчанию.
#[tauri::command]
pub fn runtime_config(state: State<'_, AppState>) -> RuntimeConfig {
    RuntimeConfig::from(&*state.config())
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
    // Отметка о самом факте вызова. Без неё по журналу нельзя отличить «запрос ушёл
    // и не вернулся» от «попап открылся, но до запроса дело не дошло», а это разные
    // поломки в разных местах.
    log::info!("запрошено объяснение «{term}»");

    let provider = state.provider();
    let fallback = state.error_text();
    let what = format!("объяснение «{term}»");
    guarded(
        &what,
        async move { provider.explain(&term, &context).await },
        fallback,
    )
    .await
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
    let what = format!("вопрос про «{term}»");
    guarded(
        &what,
        async move { provider.ask(&term, &context, &thread, &question).await },
        fallback,
    )
    .await
}

/// Настройки, которые пользователь может менять из окна: провайдер и доступ к модели.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub provider: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub proxy: String,
}

/// Текущие настройки для окна. Ключ отдаём замаскированным: показывать его целиком
/// незачем, а понять «ключ сохранён» пользователю нужно.
#[tauri::command]
pub fn ai_settings(state: State<'_, AppState>) -> AiSettings {
    let config = state.config();
    AiSettings {
        provider: config.ai.provider.clone(),
        endpoint: config.ai.endpoint.clone(),
        api_key: if config.ai.api_key.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        model: config.ai.model.clone(),
        proxy: config.ai.proxy.clone(),
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
        let mut config = state.config_mut();
        config.ai.provider = settings.provider;
        config.ai.endpoint = settings.endpoint;
        config.ai.model = settings.model;
        config.ai.proxy = settings.proxy;
        // Пустое поле ключа означает «не менять»: в окно он приходит замаскированным,
        // и сохранять маску вместо настоящего ключа нельзя.
        if !settings.api_key.is_empty() && !settings.api_key.starts_with('•') {
            config.ai.api_key = settings.api_key;
        }
    }

    persist(&app, &state)?;

    let config = state.config();
    state.rebuild_provider(&config.ai, &config.ui.language);
    Ok(())
}

/// Вид приложения: тема и язык. Отдельно от настроек модели — это разные
/// решения, и менять их человек может независимо.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: String,
    pub language: String,
}

#[tauri::command]
pub fn appearance(state: State<'_, AppState>) -> Appearance {
    let config = state.config();
    Appearance {
        theme: config.ui.theme.clone(),
        language: config.ui.language.clone(),
    }
}

/// Сохраняет тему и язык.
///
/// Провайдера пересобираем: от языка зависит подсказка модели, иначе после
/// переключения на английский объяснения продолжали бы приходить по-русски.
#[tauri::command]
pub fn save_appearance(
    app: AppHandle,
    state: State<'_, AppState>,
    appearance: Appearance,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.ui.theme = appearance.theme;
        config.ui.language = appearance.language;
        // Текст ошибки, оставшийся от прежнего языка, сбрасываем: пусть его
        // подставит перевод, иначе в английском окне висела бы русская фраза.
        config.ui.error_text = String::new();
    }

    persist(&app, &state)?;

    let config = state.config();
    state.rebuild_provider(&config.ai, &config.ui.language);
    Ok(())
}

/// Настройки жеста для окна: чем открывается попап и разрешён ли запасной способ.
#[tauri::command]
pub fn trigger_settings(state: State<'_, AppState>) -> TriggerConfig {
    state.config().trigger.clone()
}

#[tauri::command]
pub fn save_trigger_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: TriggerConfig,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.trigger = settings;
    }
    persist(&app, &state)
}

/// Что наблюдатель видел в последнее время — живая сводка для окна настройки.
///
/// try_state, а не state: наблюдателя кладут в состояние только на десктопе, а
/// state() при отсутствии значения не возвращает ошибку, а паникует. Окно
/// опрашивает эту команду раз в секунду, так что на телефоне обычный state()
/// ронял бы приложение через секунду после запуска.
#[tauri::command]
pub fn capture_diagnostics(app: AppHandle) -> Diagnostics {
    app.try_state::<crate::watcher::Integration>()
        .map(|integration| integration.diagnostics())
        .unwrap_or_default()
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

/// Какие модели стоят на этом компьютере и отвечает ли вообще Ollama.
///
/// Адрес берём из настроек, а не из воздуха: человек мог поднять Ollama на
/// другом порту. Держатель конфигурации живёт в своей области видимости —
/// иначе он поехал бы через `await`, а этого делать нельзя.
#[tauri::command]
pub async fn local_models(app: AppHandle) -> crate::ollama::Status {
    let host = {
        let state = app.state::<AppState>();
        let config = state.config();
        crate::ollama::host_from(&config.ai.endpoint)
    };
    crate::ollama::status(&host).await
}

/// Запускает Ollama, если она стоит, но не поднята.
///
/// После перезагрузки Windows Ollama не всегда стартует сама — записи в
/// автозапуске у неё может не быть вовсе. Со стороны это выглядит так, будто
/// из программы пропали все скачанные модели, хотя они лежат на диске.
#[tauri::command]
pub fn start_ollama() -> Result<(), String> {
    log::info!("запускаю Ollama по просьбе из окна");
    crate::ollama::start()
}

/// Сколько весит установщик Ollama. Окно показывает это на кнопке: полтора
/// гигабайта — не та цифра, которую человек должен узнать уже после нажатия.
#[tauri::command]
pub async fn ollama_install_size() -> Option<f64> {
    crate::ollama::install_size_gb().await
}

/// Скачивает и ставит Ollama. Ход установки уходит событиями `ollama:install`.
#[tauri::command]
pub async fn install_ollama(app: AppHandle) -> Result<(), String> {
    log::info!("устанавливаю Ollama по просьбе из окна");
    crate::ollama::install(app).await
}

/// Скачивает модель. Ход загрузки уходит событиями `model:pull` — команда
/// возвращается только когда всё скачано, поэтому окно её не ждёт.
#[tauri::command]
pub async fn pull_model(app: AppHandle, model: String) -> Result<(), String> {
    let host = {
        let state = app.state::<AppState>();
        let config = state.config();
        crate::ollama::host_from(&config.ai.endpoint)
    };
    log::info!("скачиваю модель {model}");
    crate::ollama::pull(app.clone(), host, model).await
}

/// Пробный запрос: пользователь должен увидеть, что ключ рабочий, до того как
/// начнёт выделять текст и получать «Сбой сети».
#[tauri::command]
pub async fn test_ai(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state.provider();
    let fallback = state.error_text();
    log::info!("проверка провайдера");
    guarded(
        "проверка провайдера",
        async move { provider.explain("альбедо", "").await },
        fallback,
    )
    .await
    .map(|explanation| explanation.def)
}

/// Состояние системной интеграции — для окна онбординга.
#[tauri::command]
pub fn integration_status(app: AppHandle) -> Capability {
    // На мобильных наблюдателя нет вовсе — там вход через системное меню
    // «Объяснить», разрешений он не требует. Отвечаем «всё доступно»: окно по
    // этому ответу прячет верхний блок про доступ, и на телефоне остаётся
    // ровно то, что там осмысленно, — выбор источника и модели.
    app.try_state::<crate::watcher::Integration>()
        .map(|integration| integration.capability())
        .unwrap_or(Capability::Ready)
}

/// Открыть системные настройки с нужным разрешением (macOS Accessibility и т.п.).
#[tauri::command]
pub fn open_permission_settings(app: AppHandle) -> bool {
    app.try_state::<crate::watcher::Integration>()
        .map(|integration| integration.open_permission_settings())
        .unwrap_or(false)
}
