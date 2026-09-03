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
    }

    persist(&app, &state)?;

    {
        let config = state.config();
        state.rebuild_provider(&config.ai, &config.ui.language);
    }

    // Новую модель греем, старую отпускаем: иначе в видеопамяти копятся все, что
    // человек успел попробовать, и места не остаётся ни одной.
    #[cfg(desktop)]
    crate::wake_local_model(&app);
    Ok(())
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
    pub input_device: String,
    pub rate: f32,
    pub speak_answers: bool,
    /// Скачан ли выбранный голос. Окну нужно, чтобы показать кнопку загрузки
    /// вместо обещания, что всё готово.
    #[serde(default)]
    pub ready: bool,
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
        input_device: config.voice.input_device.clone(),
        rate: config.voice.rate,
        speak_answers: config.voice.speak_answers,
        ready: crate::voice::assets::ready(&app, &config.voice.voice),
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
        config.voice.input_device = settings.input_device;
        config.voice.rate = settings.rate;
        config.voice.speak_answers = settings.speak_answers;
    }
    persist(&app, &state)?;
    // Пробуждение включили или выключили — перестраиваем слушателя сразу,
    // а не со следующего запуска.
    crate::restart_wake(&app);
    Ok(())
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
        "edge": to_json(crate::voice::edge_voices()),
    })
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
    log::info!("озвучиваю {} символов голосом {}", text.chars().count(), config.voice);
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

/// Отдаёт ссылку системе — пусть открывает тем, чем человек обычно читает.
fn open_externally(target: &str) -> Result<(), String> {
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

/// Пробный запрос: пользователь должен увидеть, что ключ рабочий, до того как
/// начнёт выделять текст и получать «Сбой сети».
#[tauri::command]
pub async fn test_ai(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state.provider();
    let fallback = state.error_text();
    let limit = state.config().ai.call_limit();
    log::info!("проверка провайдера");
    guarded(
        "проверка провайдера",
        async move { provider.explain("альбедо", "").await },
        fallback,
        limit,
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

/* ── Задачи ──────────────────────────────────────────────────────────────── */

/// Открывает окно со списком задач.
#[tauri::command]
pub fn open_tasks(app: AppHandle) -> Result<(), String> {
    crate::overlay::show_tasks(&app).map_err(|err| err.to_string())
}

/// Закрывает окно со списком задач.
#[tauri::command]
pub fn close_tasks(app: AppHandle) {
    crate::overlay::hide_tasks(&app);
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
}

fn view(task: &crate::tasks::Task, now: chrono::DateTime<chrono::Local>) -> TaskView {
    TaskView {
        id: task.id.clone(),
        title: task.title.clone(),
        due: task.due.map(|due| due.to_rfc3339()),
        done: task.done_at.is_some(),
        overdue: task.overdue(now),
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
