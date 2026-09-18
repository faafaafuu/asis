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
    for brain in &mut config.brains {
        brain.api_key = crate::secret::protect(&brain.api_key);
    }

    let json = serde_json::to_string_pretty(&config).map_err(|err| err.to_string())?;
    std::fs::write(path.join("config.json"), json).map_err(|err| err.to_string())
}

// Предел ожидания провайдера больше не задан числом: он считается из настроек
// в `AiConfig::call_limit()`. Здесь стояло 25 секунд, и это молча обесценивало
// любой таймаут больше двадцати пяти — запрос обрывался снаружи ровно тогда же,
// сколько бы ни было выставлено внутри.

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
    limit: Duration,
) -> Result<T, String>
where
    T: Send + 'static,
{
    let handle = tauri::async_runtime::spawn(task);
    match tokio::time::timeout(limit, handle).await {
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
            log::error!("{what}: ответа нет дольше {} с", limit.as_secs());
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
/// Вопрос, который окно не успело получить, пока загружалось.
///
/// Спрашивает само окно попапа при старте. Событие, отправленное только что
/// созданному окну, до него не доходит — слушателя ещё нет, — и без этого
/// первое открытие оставалось бы вечным «Анализирую…».
#[tauri::command]
pub fn pending_open() -> Option<crate::overlay::OpenPayload> {
    crate::overlay::take_pending()
}

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
    // Пришёл запрос из окна — окно живо, отсчёт бездействия начинается заново.
    #[cfg(desktop)]
    crate::overlay::touch_popup();

    // Отметка о самом факте вызова. Без неё по журналу нельзя отличить «запрос ушёл
    // и не вернулся» от «попап открылся, но до запроса дело не дошло», а это разные
    // поломки в разных местах.
    log::info!("запрошено объяснение «{term}»");

    let provider = state.provider();
    let fallback = state.error_text();
    let limit = state.config().ai.call_limit();
    let what = format!("объяснение «{term}»");
    let answer = guarded(
        &what,
        async move { provider.explain(&term, &context).await },
        fallback,
        limit,
    )
    .await;

    // И по возвращении тоже: модель имеет право думать дольше минуты, а окно
    // всё это время не должно считаться заброшенным.
    #[cfg(desktop)]
    crate::overlay::touch_popup();
    answer
}

#[tauri::command]
pub async fn ai_ask(
    state: State<'_, AppState>,
    term: String,
    context: String,
    thread: Vec<ThreadItem>,
    question: String,
) -> Result<String, String> {
    // Пришёл запрос из окна — окно живо, отсчёт бездействия начинается заново.
    #[cfg(desktop)]
    crate::overlay::touch_popup();

    let provider = state.provider();
    let fallback = state.error_text();
    let limit = state.config().ai.call_limit();
    let what = format!("вопрос про «{term}»");
    let answer = guarded(
        &what,
        async move { provider.ask(&term, &context, &thread, &question).await },
        fallback,
        limit,
    )
    .await;

    #[cfg(desktop)]
    crate::overlay::touch_popup();
    answer
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
        crate::brains::remember(&mut config);
    }

    persist(&app, &state)?;

    {
        let config = state.config();
        state.rebuild_provider(&config.ai, &config.ui.language, &config.voice.wake_name);
    }

    // Новую модель греем, старую отпускаем: иначе в видеопамяти копятся все, что
    // человек успел попробовать, и места не остаётся ни одной.
    #[cfg(desktop)]
    crate::wake_local_model(&app);
    Ok(())
}

/// Свежий каталог моделей облачного сервиса — по тому, что сейчас введено в окне,
/// ещё до сохранения.
///
/// Сохранённый ключ подставляется, только если адрес сервиса тот же: иначе ключ
/// одного сервиса ушёл бы другому.
#[tauri::command]
pub async fn cloud_models(
    state: State<'_, AppState>,
    endpoint: String,
    api_key: String,
    proxy: String,
) -> Result<Vec<crate::ai_client::ModelInfo>, String> {
    let mut ai = state.config().ai.clone();
    if ai.endpoint.trim() != endpoint.trim() {
        ai.api_key = String::new();
    }
    if !api_key.is_empty() && !api_key.starts_with('•') {
        ai.api_key = api_key;
    }
    ai.endpoint = endpoint;
    ai.proxy = proxy;
    if ai.endpoint.trim().is_empty() {
        return Err("Сначала укажите адрес сервиса".into());
    }
    let models = crate::ai_client::catalog(&ai).await?;
    log::info!("каталог моделей: {} шт.", models.len());
    Ok(models)
}

/* ── Голос ───────────────────────────────────────────────────────────────── */

/// Настройки голоса для окна.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    pub enabled: bool,
    pub engine: String,
    pub voice: String,
    pub edge_voice: String,
    pub wake_word: bool,
    /// Пустая строка в окне — значит «имя по умолчанию»; настоящее имя для
    /// этого случая окно берёт отдельным вызовом (`default_wake_name`), не
    /// отсюда, чтобы поле оставалось пустым и подсказка в нём не терялась.
    #[serde(default)]
    pub wake_name: String,
    pub input_device: String,
    pub rate: f32,
    pub speak_answers: bool,
    /// Скачан ли выбранный голос. Окну нужно, чтобы показать кнопку загрузки
    /// вместо обещания, что всё готово.
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub silero_voice: String,
    /// Ключ Azure: в окно — маской, из окна — новый ключ или пусто («не менять»).
    #[serde(default)]
    pub azure_key: String,
    #[serde(default)]
    pub azure_region: String,
}

#[cfg(desktop)]
#[tauri::command]
pub fn voice_settings(app: AppHandle, state: State<'_, AppState>) -> VoiceSettings {
    let config = state.config();
    VoiceSettings {
        enabled: config.voice.enabled,
        engine: config.voice.engine.clone(),
        voice: config.voice.voice.clone(),
        edge_voice: config.voice.edge_voice.clone(),
        wake_word: config.voice.wake_word,
        // Пусто, если ещё не переименовывали: своё имя пользователь видит,
        // введённое им самим, а не «Ноа» из общего умолчания.
        wake_name: config.voice.wake_name.clone(),
        input_device: config.voice.input_device.clone(),
        rate: config.voice.rate,
        speak_answers: config.voice.speak_answers,
        // Готовность — того способа, что выбран: кнопка «Скачать» относится к нему.
        ready: match config.voice.engine.as_str() {
            "silero" => crate::voice::silero_ready(&app),
            "azure" => true,
            _ => crate::voice::assets::ready(&app, &config.voice.voice),
        },
        silero_voice: config.voice.silero_voice.clone(),
        azure_key: if config.voice.azure_key.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        azure_region: config.voice.azure_region.clone(),
    }
}

#[cfg(desktop)]
#[tauri::command]
pub fn save_voice_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: VoiceSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.voice.enabled = settings.enabled;
        config.voice.engine = settings.engine;
        config.voice.voice = settings.voice;
        config.voice.edge_voice = settings.edge_voice;
        config.voice.wake_word = settings.wake_word;
        config.voice.wake_name = settings.wake_name.trim().to_string();
        config.voice.input_device = settings.input_device;
        config.voice.rate = settings.rate;
        config.voice.speak_answers = settings.speak_answers;
        if !settings.silero_voice.trim().is_empty() {
            config.voice.silero_voice = settings.silero_voice.trim().to_string();
        }
        // Пустое поле и маска означают «не менять»: сохранить маску вместо
        // ключа значило бы потерять ключ.
        let key = settings.azure_key.trim();
        if !key.is_empty() && !key.starts_with('•') {
            config.voice.azure_key = crate::secret::protect(key);
        }
        let region = settings.azure_region.trim().to_lowercase();
        if !region.is_empty() {
            config.voice.azure_region = region;
        }
    }
    persist(&app, &state)?;
    // Имя поменялось — модель должна представляться им сразу, а не после
    // перезапуска: та же причина, что и для смены языка в save_appearance.
    {
        let config = state.config();
        state.rebuild_provider(&config.ai, &config.ui.language, &config.voice.wake_name);
    }
    // Пробуждение включили, выключили или переименовали — перестраиваем
    // слушателя сразу, а не со следующего запуска.
    crate::restart_wake(&app);
    Ok(())
}

/// Имя по умолчанию — для подсказки в пустом поле окна.
#[tauri::command]
pub fn default_wake_name() -> &'static str {
    crate::config::DEFAULT_WAKE_NAME
}

/// Голоса, между которыми можно выбирать. Оба списка сразу: окно показывает
/// подходящий по выбранному способу и не ходит за вторым отдельно.
#[cfg(desktop)]
#[tauri::command]
pub fn voice_list() -> serde_json::Value {
    let to_json = |list: &[(&str, &str)]| -> Vec<serde_json::Value> {
        list.iter()
            .map(|(id, label)| serde_json::json!({ "id": id, "label": label }))
            .collect()
    };
    serde_json::json!({
        "piper": to_json(crate::voice::assets::VOICES),
        "azure": to_json(crate::voice::azure_voices()),
        "silero": to_json(crate::voice::silero_voices()),
    })
}

/// Скачивает Python, PyTorch и голоса Silero. Ход — событием `voice:install`.
#[cfg(desktop)]
#[tauri::command]
pub async fn silero_install(app: AppHandle) -> Result<(), String> {
    crate::voice::silero_install(app).await
}

/// Расход модели для виджета.
#[cfg(desktop)]
#[tauri::command]
pub fn usage_summary(app: AppHandle) -> crate::usage::Summary {
    crate::usage::summary(&app)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSettings {
    pub enabled: bool,
    pub on_top: bool,
}

#[cfg(desktop)]
#[tauri::command]
pub fn widget_settings(state: State<'_, AppState>) -> WidgetSettings {
    let config = state.config();
    WidgetSettings {
        enabled: config.widget.enabled,
        on_top: config.widget.on_top,
    }
}

/// Включает или выключает виджет и закрепляет его поверх окон.
#[cfg(desktop)]
// Асинхронная: окно, созданное из синхронной команды, на Windows вешает
// программу — команда ждёт главный поток, а он ждёт команду.
#[tauri::command]
pub async fn save_widget_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: WidgetSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.widget.enabled = settings.enabled;
        config.widget.on_top = settings.on_top;
    }
    persist(&app, &state)?;
    if settings.enabled {
        crate::overlay::show_usage_widget(&app).map_err(|err| err.to_string())?;
        crate::overlay::pin_usage_widget(&app, settings.on_top);
    } else {
        crate::overlay::hide_usage_widget(&app);
    }
    Ok(())
}

/// Имя папки модуля из команды: пакет `@scope/server-memory` → `server-memory`.
fn module_id(title: &str, parts: &[String]) -> String {
    let source = parts
        .iter()
        .rev()
        .find(|part| !part.starts_with('-') && (part.contains('/') || part.contains("mcp") || part.contains("server")))
        .map(|part| part.rsplit('/').next().unwrap_or(part).to_string())
        .unwrap_or_else(|| title.to_string());
    let id: String = source
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() {
        format!("module-{}", chrono::Local::now().format("%H%M%S"))
    } else {
        id.chars().take(40).collect()
    }
}

/// Подключает MCP-сервер как свой модуль: название, командная строка, пример фразы.
#[cfg(desktop)]
#[tauri::command]
pub fn plugins_connect(
    app: AppHandle,
    title: String,
    command: String,
    about: String,
    voice: String,
) -> Result<(), String> {
    let parts = crate::plugins::split_command(&command);
    let (program, args) = parts.split_first().ok_or("укажите команду запуска MCP-сервера")?;
    let manifest = crate::plugins::Manifest {
        id: module_id(&title, &parts),
        title: title.trim().to_string(),
        icon: "✦".into(),
        about: about.trim().to_string(),
        voice: voice.trim().to_string(),
        mcp: crate::plugins::McpSpec {
            command: program.clone(),
            args: args.to_vec(),
            env: Default::default(),
        },
        ..Default::default()
    };
    crate::plugins::install(&app, manifest)
}

/// Ставит модуль из библиотеки.
#[cfg(desktop)]
#[tauri::command]
pub async fn plugins_install(app: AppHandle, manifest: crate::plugins::Manifest) -> Result<(), String> {
    // С площадки — вместе с файлами сервера; без площадки — описание из
    // вшитой библиотеки (там только пакеты npx).
    match crate::platform::package(&manifest.id).await {
        Ok((manifest, files)) => crate::plugins::install_package(&app, manifest, files),
        Err(err) => {
            log::info!("модуль «{}» не взят с площадки ({err}) — ставлю из вшитой библиотеки", manifest.id);
            crate::plugins::install(&app, manifest)
        }
    }
}

#[cfg(desktop)]
#[tauri::command]
pub fn plugins_remove(app: AppHandle, id: String) -> Result<(), String> {
    crate::plugins::uninstall(&app, &id)
}

/// Ключи модуля: названия и есть ли значение. Самих значений окно не видит.
#[cfg(desktop)]
#[tauri::command]
pub fn plugins_secrets(app: AppHandle, id: String) -> Result<Vec<crate::plugins::SecretField>, String> {
    crate::plugins::secret_fields(&app, &id)
}

#[cfg(desktop)]
#[tauri::command]
pub fn plugins_save_secrets(
    app: AppHandle,
    id: String,
    values: std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    crate::plugins::save_secret_values(&app, &id, values)
}

/// Проверить модуль заново.
#[cfg(desktop)]
#[tauri::command]
pub fn plugins_recheck(app: AppHandle, id: String) -> Result<(), String> {
    crate::plugins::recheck(&app, &id)
}

/// Библиотека модулей из репозитория.
#[cfg(desktop)]
#[tauri::command]
pub async fn plugins_library() -> Result<Vec<crate::plugins::Manifest>, String> {
    match crate::platform::list("").await {
        Ok(found) if !found.is_empty() => Ok(found
            .into_iter()
            .filter_map(|card| {
                Some(crate::plugins::Manifest {
                    id: card["id"].as_str()?.to_string(),
                    title: card["title"].as_str()?.to_string(),
                    icon: card["icon"].as_str().unwrap_or("✦").to_string(),
                    about: card["about"].as_str().unwrap_or_default().to_string(),
                    voice: card["voice"].as_str().unwrap_or_default().to_string(),
                    ..Default::default()
                })
            })
            .collect()),
        _ => crate::plugins::library().await,
    }
}

/// Площадка модулей: адрес и есть ли ключ. Сам ключ окно не видит.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub has_token: bool,
}

#[cfg(desktop)]
#[tauri::command]
pub fn platform_settings(state: State<'_, AppState>) -> PlatformSettings {
    let config = state.config();
    PlatformSettings {
        url: config.platform.url.clone(),
        token: String::new(),
        has_token: !config.platform.token.is_empty(),
    }
}

#[cfg(desktop)]
#[tauri::command]
pub async fn save_platform_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: PlatformSettings,
) -> Result<String, String> {
    let url = settings.url.trim().trim_end_matches('/').to_string();
    let url = if url.is_empty() { crate::config::DEFAULT_PLATFORM_URL.to_string() } else { url };
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Адрес площадки начинается с https://".into());
    }
    let token = settings.token.trim().to_string();
    // Новый ключ сначала проверяем: сохранённый нерабочий ключ сломал бы публикацию молча.
    let owner = if token.is_empty() {
        None
    } else {
        if !token.starts_with("noah_") {
            return Err("Ключ площадки начинается с noah_ — скопируйте его из кабинета целиком.".into());
        }
        Some(crate::platform::whoami(&url, &token).await?)
    };
    {
        let mut config = state.config_mut();
        config.platform.url = url;
        if !token.is_empty() {
            config.platform.token = crate::secret::protect(&token);
        }
    }
    persist(&app, &state)?;
    Ok(match owner {
        Some(name) => format!("Ключ подходит: автор {name}."),
        None => "Сохранено.".into(),
    })
}

/// Модули и их состояние — для главного окна.
#[cfg(desktop)]
#[tauri::command]
pub fn modules_overview(app: AppHandle) -> Vec<crate::modules::ModuleCard> {
    crate::modules::overview(&app)
}

/// Открывает окно модуля.
#[cfg(desktop)]
// Асинхронная: окно, созданное из синхронной команды, на Windows вешает
// программу — команда ждёт главный поток, а он ждёт команду.
#[tauri::command]
pub async fn open_module(app: AppHandle, id: String) -> Result<(), String> {
    crate::modules::open(&app, &id)
}

/// Пробный запрос к Azure — для кнопки «Послушать»: озвучивание при неудаче
/// тихо переходит на свой голос, а человеку нужна причина.
#[cfg(desktop)]
#[tauri::command]
pub async fn azure_check(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config().voice.clone();
    crate::voice::check_azure(&config).await
}

/// Скачивает синтезатор и выбранный голос.
#[cfg(desktop)]
#[tauri::command]
pub async fn voice_install(app: AppHandle, voice: String) -> Result<(), String> {
    crate::voice::assets::install(app, voice).await
}

/// Окно взяли в руки: передвинули за заголовок или потянули за край.
///
/// Само перетаскивание делает система — окно просит её об этом само, средствами
/// Tauri. Сюда приходит только весть о том, что случилось: дальше геометрия
/// принадлежит человеку, и подгонять окно под содержимое мы перестаём.
#[cfg(desktop)]
#[tauri::command]
pub fn popup_taken_over(moved: bool, sized: bool) {
    crate::overlay::take_over(moved, sized);
}

/// Окно сообщает, что с ним работают.
///
/// Зовётся из попапа на движение мыши, нажатие клавиши и прокрутку — с большим
/// запасом по частоте, не на каждое событие. Нужно, чтобы окно, в котором
/// человек читает длинный ответ, не закрылось у него на глазах.
#[cfg(desktop)]
#[tauri::command]
pub fn popup_active() {
    crate::overlay::touch_popup();
}

/// Пробел в самом окне попапа при пустом поле ввода.
///
/// Хук такой пробел не забирает — окно наше, — поэтому окно передаёт его само,
/// и нажатие значит то же, что пробел поверх чужой программы.
#[cfg(desktop)]
#[tauri::command]
pub fn popup_space() {
    crate::overlay::touch_popup();
    crate::voice::hotkey::press_speak();
}

/* ── Заказы: настройки, вход, оплата кнопкой ────────────────────────────── */

/// Магазины, между которыми можно выбирать, — их коды.
const FOOD_STORES: &[&str] = &["vkusvill", "magnit", "metro", "pyaterochka"];

/// Настройки заказов для окна.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoodSettings {
    pub enabled: bool,
    pub endpoint: String,
    /// Включённые магазины. В настройках хранится обратное — выключенные: так
    /// новый магазин, добавленный в программу, включён сразу.
    pub stores: Vec<String>,
    pub auto_pay: bool,
    /// Предел одного заказа без подтверждения, ₽.
    pub per_order: u32,
    /// Предел за сутки, ₽.
    pub per_day: u32,
    pub free_delivery_from: u32,
    /// Ключ parse.bot. Наружу уходит замаскированным, как ключ от модели.
    #[serde(default)]
    pub parse_key: String,
    /// Сколько оплачено без подтверждения за последние сутки. Только для показа.
    #[serde(default)]
    pub spent_today: u32,
    /// Открывался ли уже браузер для входа. Только для показа.
    #[serde(default)]
    pub signed_in: bool,
}

#[tauri::command]
pub fn food_settings(state: State<'_, AppState>) -> FoodSettings {
    let food = state.config().food.clone();
    FoodSettings {
        enabled: food.enabled,
        endpoint: food.endpoint.clone(),
        stores: FOOD_STORES
            .iter()
            .filter(|code| !food.disabled_stores.iter().any(|off| off == *code))
            .map(|code| (*code).to_string())
            .collect(),
        auto_pay: food.auto_pay,
        per_order: food.max_order,
        per_day: food.daily_limit,
        free_delivery_from: food.free_delivery_from,
        parse_key: if food.parse_key.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        spent_today: crate::spend::spent_today(),
        signed_in: !food.session_id.trim().is_empty(),
    }
}

#[tauri::command]
pub fn save_food_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: FoodSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        let food = &mut config.food;
        food.enabled = settings.enabled;
        // Пустой адрес — не «выключить», а опечатка: оставляем прежний.
        if !settings.endpoint.trim().is_empty() {
            food.endpoint = settings.endpoint.trim().to_string();
        }
        food.disabled_stores = FOOD_STORES
            .iter()
            .filter(|code| !settings.stores.iter().any(|on| on == *code))
            .map(|code| (*code).to_string())
            .collect();
        food.auto_pay = settings.auto_pay;
        food.max_order = settings.per_order;
        food.daily_limit = settings.per_day;
        food.free_delivery_from = settings.free_delivery_from;
        // Точки означают «не менять»: наружу ключ уходил замаскированным.
        if !settings.parse_key.starts_with('•') {
            food.parse_key = settings.parse_key.trim().to_string();
        }
    }
    persist(&app, &state)
}

/// Открывает браузер Ноа на ВкусВилле, чтобы человек вошёл в первый раз.
#[cfg(desktop)]
#[tauri::command]
pub async fn food_login(app: AppHandle) -> Result<(), String> {
    crate::food::open_session(&app, false).await.map(|_| ())
}

/// Оплачивает заказ, который ждёт подтверждения: человек нажал «Оплатить».
#[cfg(desktop)]
#[tauri::command]
pub async fn order_pay(app: AppHandle) -> Result<(), String> {
    crate::planner::pay_confirmed(&app).await
}

/// Раздел настроек, который окно должно показать при загрузке.
#[tauri::command]
pub fn settings_section() -> Option<String> {
    crate::overlay::take_settings_section()
}

/// Открывает в браузере корзину текущего заказа.
///
/// Адрес берётся из состояния заказа, а не из окна: окно просит «открой
/// корзину», и открыть по этой просьбе можно только то, что собрал сам Ноа.
#[cfg(desktop)]
#[tauri::command]
pub fn open_order_link() -> Result<(), String> {
    let link = crate::order::current()
        .and_then(|order| order.link)
        .ok_or("корзины со ссылкой нет")?;
    crate::pc::open(&link)
}

/// Произнести текст. Возвращается сразу: речь идёт своим чередом.
#[cfg(desktop)]
#[tauri::command]
pub async fn voice_speak(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    let config = state.config().voice.clone();
    if !config.enabled {
        log::info!("просили озвучить, но голос выключен в настройках");
        return Err("голос выключен в настройках".into());
    }
    // Отметка о самом факте: без неё по журналу не отличить «пробел не дошёл»
    // от «дошёл, но озвучивать нечем», а чинится это в разных местах.
    log::info!(
        "озвучиваю {} символов голосом {}: «{}»",
        text.chars().count(),
        config.voice,
        text.chars().take(90).collect::<String>()
    );
    let result = crate::voice::speak(&app, &config, &text).await;
    if let Err(err) = &result {
        log::warn!("озвучить не вышло: {err}");
    }
    result
}

/// Замолчать.
///
/// `async` здесь не для ожидания, а ради потока: Tauri выполняет обычные
/// команды в главном потоке, а эта снимает процесс синтезатора — работа хоть
/// и короткая, но с ожиданием чужого процесса, и в главном потоке ей не место.
#[cfg(desktop)]
#[tauri::command]
pub async fn voice_stop() {
    crate::voice::stop();
}

/// Готово ли распознавание речи и чем оно будет считать.
#[cfg(desktop)]
#[tauri::command]
pub fn speech_status(app: AppHandle) -> serde_json::Value {
    let vram = crate::ollama::hardware().vram_gb;
    serde_json::json!({
        "ready": crate::voice::whisper::ready(&app),
        // Размер загрузки зависит от того, есть ли видеокарта: со сборкой под
        // CUDA это два гигабайта, без неё — полтора. Человеку честнее знать
        // заранее, а не по ходу загрузки.
        "sizeGb": if vram >= 2.0 { 2.1 } else { 1.5 },
        "gpu": vram >= 2.0,
    })
}

/// В каком состоянии индикатор голоса. Спрашивает само окно индикатора,
/// когда загрузилось: событие, отправленное до загрузки, до него не дошло бы.
#[cfg(desktop)]
#[tauri::command]
pub fn hud_mode() -> String {
    crate::overlay::hud_mode()
}

/// Микрофоны, которые видит система.
#[cfg(desktop)]
#[tauri::command]
pub fn input_devices() -> Vec<String> {
    crate::voice::stt::devices()
}

/// Скачивает распознавание речи.
#[cfg(desktop)]
#[tauri::command]
pub async fn speech_install(app: AppHandle) -> Result<(), String> {
    crate::voice::whisper::install(app).await
}

/// Какую модель стоит поставить на этой машине.
///
/// Нужна окну: когда человек сам выбирает «модель на этом устройстве», поле
/// модели не должно быть пустым или заполненным наугад. То же решение, что
/// принимает программа при первом запуске, — но показанное заранее.
#[tauri::command]
pub fn recommended_model() -> String {
    crate::ollama::pick(&crate::ollama::hardware()).to_string()
}

/// Настройки запуска. Галочка одна, но раздел свой: это решение про поведение
/// программы в системе, а не про модель и не про внешний вид.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSettings {
    pub launch_at_login: bool,
}

#[tauri::command]
pub fn startup_settings(state: State<'_, AppState>) -> StartupSettings {
    StartupSettings {
        launch_at_login: state.config().startup.launch_at_login,
    }
}

#[tauri::command]
pub fn save_startup_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: StartupSettings,
) -> Result<(), String> {
    {
        state.config_mut().startup.launch_at_login = settings.launch_at_login;
    }
    persist(&app, &state)?;
    // Запись в автозагрузке правится сразу: галочка, которая начнёт действовать
    // «со следующего раза», ничем не отличается от неработающей.
    crate::apply_autostart(&app);
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
    state.rebuild_provider(&config.ai, &config.ui.language, &config.voice.wake_name);
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
/// Открывает страницу, где у выбранного сервиса берут ключ.
///
/// Адреса заданы здесь, а не приходят из окна: команда открывает что-то во
/// внешнем браузере, и принимать для этого произвольную строку — значит дать
/// любому, кто доберётся до окна, открывать что угодно. Список закрытый.
#[tauri::command]
pub fn open_key_page(provider: String) -> Result<(), String> {
    let url = match provider.as_str() {
        "groq" => "https://console.groq.com/keys",
        "google" => "https://aistudio.google.com/app/apikey",
        "openrouter" => "https://openrouter.ai/keys",
        other => return Err(format!("для «{other}» страницы ключей нет")),
    };
    open_externally(url)
}

/// Открывает площадку модулей — по сохранённому адресу, не по строке из окна.
#[cfg(desktop)]
#[tauri::command]
pub fn open_platform(state: State<'_, AppState>) -> Result<(), String> {
    let url = state.config().platform.url.clone();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("адрес площадки не задан".into());
    }
    open_externally(&url)
}

/// Отдаёт ссылку системе — пусть открывает тем, чем человек обычно читает.
pub(crate) fn open_externally(target: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };

    std::process::Command::new(opener)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("не удалось открыть {target}: {err}"))
}

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

/// Удаляет скачанную модель с диска. Выбранную — нельзя: ей сейчас отвечают.
#[tauri::command]
pub async fn delete_model(app: AppHandle, model: String) -> Result<(), String> {
    let (host, chosen) = {
        let state = app.state::<AppState>();
        let config = state.config();
        (crate::ollama::host_from(&config.ai.endpoint), config.ai.model.clone())
    };
    if model == chosen {
        return Err("Эта модель сейчас выбрана — сначала выберите другую.".into());
    }
    crate::ollama::delete(&host, &model).await
}

/// Пробный запрос: пользователь должен увидеть, что ключ рабочий, до того как
/// начнёт выделять текст и получать «Сбой сети».
#[tauri::command]
pub async fn test_ai(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    const NO_ANSWER: &str =
        "Ответ не пришёл. Проверьте соединение, а если нужен VPN — впишите прокси в настройке.";
    let provider = state.provider();
    let fallback = state.error_text();
    let limit = state.config().ai.call_limit();
    log::info!("проверка провайдера");
    // 404 — модели с таким именем нет; 400 «про модель» — пустое или неверное имя.
    let about_model = |err: &AiError| match err {
        AiError::Refused(404, _) | AiError::Http(404) => true,
        AiError::Refused(400, message) => message.to_lowercase().contains("model"),
        _ => false,
    };
    let refused = match tokio::time::timeout(limit, provider.explain("альбедо", "")).await {
        Ok(Ok(explanation)) => return Ok(explanation.def),
        Ok(Err(err)) if about_model(&err) => err,
        Ok(Err(err)) => {
            log::warn!("проверка провайдера: {err}");
            return Err(err.user_text(&fallback));
        }
        Err(_) => return Err(NO_ANSWER.into()),
    };

    // Облачные сервисы закрывают модели, и имя из пресета через год уже не
    // отвечает. Берём у сервиса его список и переходим на живую модель сами:
    // искать новое имя руками человек не обязан.
    let ai = state.config().ai.clone();
    let available = crate::ai_client::list_models(&ai).await.map_err(|err| {
        format!("Модели «{}» у сервиса нет, а список моделей он не отдал: {err}", ai.model)
    })?;
    // Модель в списке есть — значит, дело не в имени. У OpenRouter так отвечают
    // бесплатные модели, пока в настройках приватности аккаунта они не разрешены;
    // подменять модель тогда нельзя, нужна настоящая причина.
    if available.iter().any(|name| *name == ai.model) {
        log::warn!("проверка провайдера: модель «{}» у сервиса есть, отказ: {refused}", ai.model);
        return Err(refused.user_text(&fallback));
    }
    let Some(pick) = crate::ai_client::pick_model(&ai.endpoint, &ai.model, &available) else {
        let some = available.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
        return Err(format!("Модели «{}» у сервиса нет. Есть, например: {some}.", ai.model));
    };
    log::info!("модели «{}» у сервиса нет — переключаюсь на «{pick}»", ai.model);
    state.config_mut().ai.model = pick.clone();
    persist(&app, &state)?;
    {
        let config = state.config();
        state.rebuild_provider(&config.ai, &config.ui.language, &config.voice.wake_name);
    }
    let provider = state.provider();
    match tokio::time::timeout(limit, provider.explain("альбедо", "")).await {
        Ok(Ok(explanation)) => Ok(format!(
            "Модели «{}» у сервиса больше нет — переключил на «{pick}». {}",
            ai.model, explanation.def
        )),
        Ok(Err(err)) => Err(format!(
            "Модели «{}» у сервиса нет; переключил на «{pick}», но и она не ответила: {}",
            ai.model,
            err.user_text(&fallback)
        )),
        Err(_) => Err(NO_ANSWER.into()),
    }
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

/* ── Задачи ──────────────────────────────────────────────────────────────── */

/// Открывает окно со списком задач.
// Асинхронная: окно, созданное из синхронной команды, на Windows вешает
// программу — команда ждёт главный поток, а он ждёт команду.
#[tauri::command]
pub async fn open_tasks(app: AppHandle) -> Result<(), String> {
    crate::overlay::show_tasks(&app).map_err(|err| err.to_string())
}

/// Закрывает окно со списком задач.
#[tauri::command]
pub fn close_tasks(app: AppHandle) {
    crate::overlay::hide_tasks(&app);
}

/// Открывает окно заказа.
// Асинхронная: окно, созданное из синхронной команды, на Windows вешает
// программу — команда ждёт главный поток, а он ждёт команду.
#[tauri::command]
pub async fn open_order(app: AppHandle) -> Result<(), String> {
    crate::overlay::show_order(&app).map_err(|err| err.to_string())
}

/// Закрывает окно заказа.
#[tauri::command]
pub fn close_order(app: AppHandle) {
    crate::overlay::hide_order(&app);
}

/// Строки окна активов: цены и изменения.
#[tauri::command]
pub async fn watch_rows() -> Vec<crate::watchlist::Row> {
    crate::watchlist::rows().await
}

/// Добавляет актив по тикеру или названию — во вкладку, если она открыта;
/// отдаёт, как актив называется.
#[tauri::command]
pub async fn watch_add(query: String, tab: Option<String>) -> Result<String, String> {
    crate::watchlist::add(&query, tab.as_deref())
        .await
        .map(|asset| asset.name)
}

/// Убирает актив: из вкладки, если она открыта, иначе из списка совсем.
#[tauri::command]
pub fn watch_remove(id: String, tab: Option<String>) -> bool {
    crate::watchlist::remove(&id, tab.as_deref()).is_some()
}

/// Вкладки списка активов.
#[tauri::command]
pub fn watch_tabs() -> Vec<String> {
    crate::watchlist::tabs()
}

/// Заводит вкладку.
#[tauri::command]
pub fn watch_tab_add(name: String) -> Result<String, String> {
    crate::watchlist::add_tab(&name)
}

/// Убирает вкладку; активы остаются.
#[tauri::command]
pub fn watch_tab_remove(name: String) -> bool {
    crate::watchlist::remove_tab(&name)
}

/// Вкладка, которую голосом попросили открыть, пока окна не было.
#[tauri::command]
pub fn watch_open_tab() -> Option<String> {
    crate::watchlist::take_open_tab()
}

/// Новый порядок активов — как перетащили в окне.
#[tauri::command]
pub fn watch_reorder(ids: Vec<String>) {
    crate::watchlist::reorder(&ids);
}

/// Кладёт актив во вкладку или вынимает из неё.
#[tauri::command]
pub fn watch_set_tab(id: String, tab: String, on: bool) -> bool {
    crate::watchlist::set_tab(&id, &tab, on)
}

/// Ставит оповещение о цене; отдаёт цену словами.
#[tauri::command]
pub async fn watch_alert_add(id: String, price: f64) -> Result<String, String> {
    crate::watchlist::add_alert(&id, price)
        .await
        .map(|(_, money)| money)
}

/// Убирает оповещение.
#[tauri::command]
pub fn watch_alert_remove(id: String, index: usize) -> bool {
    crate::watchlist::remove_alert(&id, index)
}

/// Подключён ли Telegram — окно активов подсказывает, куда придёт оповещение.
#[tauri::command]
pub fn watch_telegram_ready(app: AppHandle) -> bool {
    crate::telegram::ready(&app)
}

/// Настройки Telegram для окна настройки. Токен наружу — только точками.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramSettings {
    pub token: String,
    #[serde(default)]
    pub chat_id: String,
}

#[tauri::command]
pub fn telegram_settings(state: State<'_, AppState>) -> TelegramSettings {
    let config = state.config();
    TelegramSettings {
        token: if config.telegram.bot_token.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        chat_id: config.telegram.chat_id.clone(),
    }
}

#[tauri::command]
pub fn save_telegram_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: TelegramSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        let telegram = &mut config.telegram;
        // Точки означают «не менять»: наружу токен уходил замаскированным.
        if !settings.token.starts_with('•') {
            let token = settings.token.trim();
            // Другой бот — другой чат: прежний ему не принадлежит.
            if crate::secret::reveal(&telegram.bot_token) != token {
                telegram.chat_id.clear();
            }
            telegram.bot_token = crate::secret::protect(token);
        }
        if !settings.chat_id.trim().is_empty() {
            telegram.chat_id = settings.chat_id.trim().to_string();
        }
    }
    persist(&app, &state)
}

/// Разобранное голосовое из окна индикатора — см. `overlay::decode_audio`.
#[cfg(desktop)]
#[tauri::command]
pub fn audio_decoded(id: u64, data: Option<String>, error: Option<String>) {
    let result = match (data, error) {
        (Some(data), _) => {
            crate::secret::unbase64(&data).ok_or_else(|| "звук пришёл испорченным".to_string())
        }
        (None, Some(error)) => Err(error),
        (None, None) => Err("звук не пришёл".into()),
    };
    crate::overlay::decoded_audio(id, result);
}

/// Проверка из окна настройки: найти чат, если его ещё нет, и написать туда.
#[tauri::command]
pub async fn telegram_test(app: AppHandle) -> Result<String, String> {
    let (token, chat) = crate::telegram::credentials(&app);
    if token.is_empty() {
        return Err("Сначала вставьте токен бота.".into());
    }
    let (chat, name) = if chat.is_empty() {
        let found = crate::telegram::find_chat(&token).await?;
        {
            let state = app.state::<AppState>();
            state.config_mut().telegram.chat_id = found.0.clone();
            persist(&app, &state)?;
        }
        found
    } else {
        (chat, String::new())
    };
    crate::telegram::send(
        &token,
        &chat,
        "Ноа на связи. Сюда будут приходить оповещения о ценах из окна «Активы».",
    )
    .await?;
    Ok(if name.is_empty() {
        "Готово: проверочное сообщение отправлено.".into()
    } else {
        format!("Готово: нашёл чат «{name}» и отправил туда проверочное сообщение.")
    })
}

/// Открывает график актива в TradingView.
///
/// Адрес собирается здесь, по списку: окно передаёт только, какой актив, и
/// открыть по его просьбе можно лишь то, что в списке есть.
#[tauri::command]
pub fn watch_chart(id: String) -> Result<(), String> {
    let url = crate::watchlist::chart_url(&id).ok_or("такого актива в списке нет")?;
    crate::pc::open(&url)
}

/// Закрывает окно активов.
#[tauri::command]
pub fn close_watchlist(app: AppHandle) {
    crate::overlay::hide_watchlist(&app);
}

/// Курсы и прогресс по ним.
#[tauri::command]
pub fn learn_overview() -> Vec<crate::learning::CourseCard> {
    crate::learning::overview()
}

/// Тема: урок, задачи, прежние баллы и ошибки.
#[tauri::command]
pub fn learn_topic(course: String, topic: String) -> Result<crate::learning::TopicView, String> {
    crate::learning::topic_view(&course, &topic)
}

/// Урок прочитан.
#[tauri::command]
pub fn learn_read(course: String, topic: String) -> Result<(), String> {
    crate::learning::mark_read(&course, &topic)
}

/// Проверяет ответ на задачу или вопрос.
#[tauri::command]
pub async fn learn_check(
    app: AppHandle,
    course: String,
    question: String,
    answer: serde_json::Value,
) -> Result<crate::learning::Verdict, String> {
    crate::learning::check(&app, &course, &question, &answer).await
}

/// Оценка себя самим, когда модель не ответила.
#[tauri::command]
pub fn learn_self_grade(course: String, question: String, knew: bool) -> Result<(), String> {
    crate::learning::self_grade(&course, &question, knew)
}

/// Экзамен: мини по теме или финальный (`scope` = `final`).
#[tauri::command]
pub fn learn_exam(course: String, scope: String) -> Result<crate::learning::Exam, String> {
    crate::learning::exam(&course, &scope)
}

/// Сдаёт экзамен.
#[tauri::command]
pub async fn learn_submit(
    app: AppHandle,
    course: String,
    scope: String,
    answers: std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<crate::learning::ExamResult, String> {
    crate::learning::submit(&app, &course, &scope, &answers).await
}

/// Закрывает окно обучения.
#[tauri::command]
pub fn close_learning(app: AppHandle) {
    crate::overlay::hide_learning(&app);
}

/// Начинает запись ответа голосом — кнопка «Надиктовать» в окне обучения.
#[cfg(desktop)]
#[tauri::command]
pub fn learn_dictate_start(app: AppHandle) -> Result<(), String> {
    // Микрофон один: ожидание имени уступает записи.
    crate::stop_wake();
    crate::voice::whisper::warm(&app);
    crate::voice::stt::start(&crate::input_device(&app));
    Ok(())
}

/// Останавливает запись и отдаёт расшифровку.
#[cfg(desktop)]
#[tauri::command]
pub async fn learn_dictate_stop(app: AppHandle) -> Result<String, String> {
    let wav = crate::voice::stt::stop();
    crate::start_wake(&app);
    let wav = wav.ok_or("Ничего не записалось — проверьте микрофон в настройках.")?;
    let text = crate::voice::whisper::transcribe(&app, wav, "ru", "").await?;
    Ok(text.trim().to_string())
}

/// Устный зачёт: Ноа задаёт вопросы темы вслух и слушает ответы.
#[cfg(desktop)]
#[tauri::command]
pub fn learn_oral(app: AppHandle, course: String, topic: Option<String>) -> Result<(), String> {
    let text = crate::learning::oral(&course, topic.as_deref())?;
    crate::say_then_listen(&app, text);
    Ok(())
}

/// Обсуждение темы голосом: модель отвечает, зная урок.
#[cfg(desktop)]
#[tauri::command]
pub fn learn_discuss(app: AppHandle, course: String, topic: String) -> Result<(), String> {
    let text = crate::learning::discuss(&course, &topic)?;
    crate::say_then_listen(&app, text);
    Ok(())
}

/// Обсудить один вопрос: после проверки, прочитав эталон.
#[tauri::command]
pub fn learn_discuss_question(
    app: AppHandle,
    course: String,
    question: String,
    answer: String,
) -> Result<(), String> {
    let text = crate::learning::discuss_question(&course, &question, &answer)?;
    crate::say_then_listen(&app, text);
    Ok(())
}

/// Что сейчас с заказом. `null` — заказа ещё не было.
#[tauri::command]
pub fn order_state() -> Option<crate::order::Order> {
    crate::order::current()
}

/// Задача в том виде, в каком её показывает окно.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub title: String,
    /// Срок в ISO 8601 с часовым поясом. Форматирует его окно: там знают язык
    /// интерфейса и умеют писать «сегодня в 15:00» вместо полной даты.
    pub due: Option<String>,
    pub done: bool,
    pub overdue: bool,
    pub steps: Vec<StepView>,
    pub advice: Option<String>,
    /// Сколько раз переносили. Три и больше — окно показывает это отдельно.
    pub postponed: u32,
    /// Уехало ли дело в календарь.
    pub in_calendar: bool,
}

#[derive(serde::Serialize)]
pub struct StepView {
    pub title: String,
    pub done: bool,
}

fn view(task: &crate::tasks::Task, now: chrono::DateTime<chrono::Local>) -> TaskView {
    TaskView {
        id: task.id.clone(),
        title: task.title.clone(),
        due: task.due.map(|due| due.to_rfc3339()),
        done: task.done_at.is_some(),
        overdue: task.overdue(now),
        steps: task
            .steps
            .iter()
            .map(|step| StepView {
                title: step.title.clone(),
                done: step.done,
            })
            .collect(),
        advice: task.advice.clone(),
        postponed: task.postponed,
        in_calendar: task.event_id.is_some(),
    }
}

/// Весь список задач для окна.
#[tauri::command]
pub fn task_list() -> Vec<TaskView> {
    let now = chrono::Local::now();
    crate::tasks::all().iter().map(|task| view(task, now)).collect()
}

/// Добавляет задачу. `due` — ISO 8601 или пусто.
#[tauri::command]
pub fn task_add(app: AppHandle, title: String, due: Option<String>) -> Result<TaskView, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("у задачи должно быть название".into());
    }

    let due = parse_due(due.as_deref())?;
    let task = crate::tasks::add(title, due, None);
    log::info!("задача добавлена: «{}»", task.title);
    changed(&app);
    Ok(view(&task, chrono::Local::now()))
}

/// Отмечает сделанной или возвращает в работу.
#[tauri::command]
pub fn task_done(app: AppHandle, id: String, done: bool) -> Option<TaskView> {
    let task = crate::tasks::set_done(&id, done)?;
    changed(&app);
    Some(view(&task, chrono::Local::now()))
}

/// Меняет название и срок.
#[tauri::command]
pub fn task_edit(
    app: AppHandle,
    id: String,
    title: Option<String>,
    due: Option<String>,
) -> Result<Option<TaskView>, String> {
    // Пустая строка в сроке означает «убрать срок», отсутствие поля — «не трогать».
    let due = match due {
        Some(raw) if raw.trim().is_empty() => Some(None),
        Some(raw) => Some(parse_due(Some(&raw))?),
        None => None,
    };
    let task = crate::tasks::edit(&id, title, due);
    if task.is_some() {
        changed(&app);
    }
    Ok(task.map(|task| view(&task, chrono::Local::now())))
}

/// Удаляет задачу.
#[tauri::command]
pub fn task_remove(app: AppHandle, id: String) -> bool {
    let removed = crate::tasks::remove(&id).is_some();
    if removed {
        changed(&app);
    }
    removed
}

/// Разбирает срок, присланный окном.
fn parse_due(raw: Option<&str>) -> Result<Option<chrono::DateTime<chrono::Local>>, String> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|parsed| Some(parsed.with_timezone(&chrono::Local)))
        .map_err(|err| format!("не понял срок «{raw}»: {err}"))
}

/// Сообщает всем окнам, что список изменился.
fn changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("tasks:changed", ());
}

/* ── Календарь и разбор дня ──────────────────────────────────────────────── */

/// Настройки календаря для окна. Секрет и ключ наружу не отдаются целиком.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSettings {
    pub client_id: String,
    pub client_secret: String,
    pub calendar_id: String,
    pub enabled: bool,
    /// Подключён ли аккаунт. Само значение ключа окну незачем.
    pub connected: bool,
}

#[tauri::command]
pub fn calendar_settings(state: State<'_, AppState>) -> CalendarSettings {
    let config = state.config();
    CalendarSettings {
        client_id: config.calendar.client_id.clone(),
        // Секрет показываем замаскированным — как и ключ от модели.
        client_secret: if config.calendar.client_secret.is_empty() {
            String::new()
        } else {
            "••••••••".into()
        },
        calendar_id: config.calendar.calendar_id.clone(),
        enabled: config.calendar.enabled,
        connected: !config.calendar.refresh_token.is_empty(),
    }
}

#[tauri::command]
pub fn save_calendar_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: CalendarSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.calendar.client_id = settings.client_id.trim().to_string();
        // Точки означают «не менять»: наружу секрет уходил замаскированным.
        if !settings.client_secret.is_empty() && !settings.client_secret.starts_with('•') {
            config.calendar.client_secret = settings.client_secret.trim().to_string();
        }
        config.calendar.calendar_id = match settings.calendar_id.trim() {
            "" => "primary".into(),
            named => named.to_string(),
        };
        config.calendar.enabled = settings.enabled;
    }
    persist(&app, &state)
}

/// Проводит через согласие Google и запоминает ключ.
#[cfg(desktop)]
#[tauri::command]
pub async fn calendar_connect(app: AppHandle) -> Result<(), String> {
    let token = crate::calendar::connect(&app).await?;

    let state = app.state::<AppState>();
    {
        let mut config = state.config_mut();
        config.calendar.refresh_token = token;
        // Подключили — значит, собирались пользоваться.
        config.calendar.enabled = true;
    }
    persist(&app, &state)?;
    log::info!("календарь подключён");
    Ok(())
}

/// Отключает календарь: ключ забывается.
#[cfg(desktop)]
#[tauri::command]
pub fn calendar_forget(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.calendar.refresh_token.clear();
        config.calendar.enabled = false;
    }
    persist(&app, &state)
}

/// Настройки вечернего разбора.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSettings {
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
}

#[tauri::command]
pub fn review_settings(state: State<'_, AppState>) -> ReviewSettings {
    let config = state.config();
    ReviewSettings {
        enabled: config.review.enabled,
        hour: config.review.hour,
        minute: config.review.minute,
    }
}

#[tauri::command]
pub fn save_review_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: ReviewSettings,
) -> Result<(), String> {
    {
        let mut config = state.config_mut();
        config.review.enabled = settings.enabled;
        // Часы и минуты приходят из поля времени, но файл настроек правят и
        // руками: значение вне суток превратило бы разбор в никогда.
        config.review.hour = settings.hour.min(23);
        config.review.minute = settings.minute.min(59);
    }
    persist(&app, &state)
}

/// Отмечает шаг задачи сделанным.
#[tauri::command]
pub fn task_step(app: AppHandle, id: String, at: usize, done: bool) -> Option<TaskView> {
    let task = crate::tasks::set_step_done(&id, at, done)?;
    changed(&app);
    Some(view(&task, chrono::Local::now()))
}

/// Просит модель разбить задачу на шаги.
#[cfg(desktop)]
#[tauri::command]
pub async fn task_plan(app: AppHandle, id: String) -> Result<Option<TaskView>, String> {
    let task = crate::planner::plan_task(&app, &id).await?;
    changed(&app);
    Ok(task.map(|task| view(&task, chrono::Local::now())))
}

/// Переносит задачу на новый срок. `due` — ISO 8601.
///
/// Перенос считается — так же, как голосом: дело, которое переносят раз за
/// разом, окно отмечает отдельно.
#[tauri::command]
pub fn task_postpone(app: AppHandle, id: String, due: String) -> Result<Option<TaskView>, String> {
    let Some(due) = parse_due(Some(&due))? else {
        return Err("срок не указан".into());
    };
    let task = crate::tasks::postpone(&id, due);
    if task.is_some() {
        changed(&app);
    }
    Ok(task.map(|task| view(&task, chrono::Local::now())))
}

/// Убирает шаг задачи.
#[tauri::command]
pub fn task_step_remove(app: AppHandle, id: String, at: usize) -> Option<TaskView> {
    let task = crate::tasks::remove_step(&id, at)?;
    changed(&app);
    Some(view(&task, chrono::Local::now()))
}

/// Добавляет шаг задаче.
#[tauri::command]
pub fn task_step_add(app: AppHandle, id: String, title: String) -> Result<Option<TaskView>, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("у шага должно быть название".into());
    }
    let task = crate::tasks::add_step(&id, title);
    if task.is_some() {
        changed(&app);
    }
    Ok(task.map(|task| view(&task, chrono::Local::now())))
}

/// Убирает все сделанные задачи. Отдаёт, сколько убрано.
#[tauri::command]
pub fn task_clear_done(app: AppHandle) -> usize {
    let removed = crate::tasks::clear_done().len();
    if removed > 0 {
        changed(&app);
    }
    removed
}
