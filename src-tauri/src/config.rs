//! Конфигурация приложения (SPEC §10): endpoint и ключ AI-провайдера не хардкодятся.
//!
//! Порядок источников, от слабого к сильному:
//!   1. значения по умолчанию (провайдер `mock` — приложение работает сразу после сборки);
//!   2. `config.json` в каталоге конфигурации приложения;
//!   3. `config.json` рядом с исполняемым файлом (удобно для портативной сборки);
//!   4. переменные окружения `SUFLER_*`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ai: AiConfig,
    pub ui: UiConfig,
    pub trigger: TriggerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AiConfig {
    /// `mock` | `http`
    pub provider: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    /// Прокси для запросов к модели: `http://host:port` или `socks5://host:port`.
    /// Пусто — идём напрямую (и всё равно уважаем системные HTTP_PROXY/HTTPS_PROXY).
    ///
    /// Нужен потому, что почти все бесплатные сервисы моделей недоступны из части
    /// стран: ключ у человека есть, интернет есть, а запрос не доходит. Своё поле
    /// в настройках честнее, чем совет «настройте системный прокси» — VPN-клиенты
    /// поднимают локальный socks5 и системные настройки при этом не трогают.
    pub proxy: String,
    pub timeout_ms: u64,
    pub retries: u32,
    pub retry_backoff_ms: u64,

    /// Признак «ключ на диске лежит незашифрованным».
    ///
    /// В файл не пишется — это заметка на время работы. Нужна, чтобы перевести
    /// настройки прежних версий на шифрование самим, а не ждать, пока человек
    /// случайно зайдёт в окно и что-нибудь сохранит: до тех пор ключ так и
    /// лежал бы открытым, а обещание защиты было бы пустым.
    #[serde(skip)]
    pub key_stored_plain: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            // Из коробки — Википедия: определения без ключей и регистрации, для любого
            // слова, а не для семи терминов демо-словаря. Заглушка (`mock`) остаётся
            // для разработки и показа состояний интерфейса.
            provider: "wikipedia".into(),
            endpoint: String::new(),
            api_key: String::new(),
            model: String::new(),
            proxy: String::new(),
            timeout_ms: 12_000,
            retries: 1,
            retry_backoff_ms: 400,
            key_stored_plain: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiConfig {
    /// `system` | `dark` | `light` | `neon` | `synthwave`
    pub theme: String,
    /// `ru` | `en`. Задаёт и язык интерфейса, и язык, на котором отвечает модель:
    /// объяснение по-русски в английском интерфейсе выглядело бы поломкой.
    pub language: String,
    /// Текст ошибки по умолчанию. Пустой — берём из перевода по языку: иначе
    /// при смене языка здесь навсегда осталась бы фраза на прежнем.
    pub error_text: String,
}

impl UiConfig {
    /// Текст ошибки для окна: своё значение из настроек главнее, пустое
    /// означает «возьми по языку». Одно место на всё приложение — иначе при
    /// добавлении языка часть окон осталась бы на прежнем.
    pub fn resolved_error_text(&self) -> String {
        if !self.error_text.is_empty() {
            return self.error_text.clone();
        }
        match self.language.as_str() {
            "en" => "Network error — no response".into(),
            _ => "Сбой сети — нет ответа".into(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: "ru".into(),
            error_text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TriggerConfig {
    /// Открывать попап только если во время выделения был зажат ИМЕННО левый Ctrl.
    /// Выключать осознанно: без этого условия попап начнёт мешать обычному copy/paste
    /// (SPEC §3).
    pub require_left_ctrl: bool,
    /// Разрешить фолбэк через буфер обмена там, где системный API не отдал выделение
    /// (Windows, приложения без поддержки UI Automation — SPEC §9.1, §12.3).
    pub clipboard_fallback: bool,
    /// Linux: открывать попап по изменению PRIMARY selection, хотя факт нажатия Ctrl
    /// на этом окружении определить нельзя (SPEC §9.3). По умолчанию выключено —
    /// безопасное поведение из SPEC §12.5: лучше не открыть, чем открыть не вовремя.
    pub linux_primary_without_ctrl: bool,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            require_left_ctrl: true,
            // На Windows запасной путь включён сразу, и это осознанный выбор в пользу
            // работоспособности. UI Automation молчит в самых ходовых программах —
            // всё, что построено на Chromium: браузеры, Telegram, Discord, VS Code.
            // С выключенным фолбэком продукт там просто не отвечает на жест, и
            // пользователю неоткуда узнать почему. Согласие, которого требует SPEC §12.3,
            // спрашивается не молчанием, а окном первого запуска, где этот пункт описан
            // словами и выключается одной галочкой.
            clipboard_fallback: cfg!(target_os = "windows"),
            linux_primary_without_ctrl: false,
        }
    }
}

impl Config {
    /// Читает конфигурацию из всех источников. Ошибки чтения не фатальны:
    /// приложение обязано запуститься даже с битым config.json, иначе пользователь
    /// останется без единственного способа его починить.
    pub fn load(app_config_dir: Option<PathBuf>) -> Self {
        let mut config = Config::default();

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(dir) = app_config_dir {
            candidates.push(dir.join("config.json"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("config.json"));
            }
        }

        for path in candidates {
            match std::fs::read_to_string(&path) {
                Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                    Ok(parsed) => {
                        log::info!("конфигурация прочитана из {}", path.display());
                        config = parsed;
                    }
                    Err(err) => log::warn!("{} не разобрался: {err}", path.display()),
                },
                Err(_) => continue,
            }
        }

        // На диске ключ лежит зашифрованным, внутри программы — обычной строкой:
        // расшифровываем один раз здесь, чтобы всё остальное про это не знало.
        // Значение без метки — ключ от прежней версии; помечаем его, чтобы
        // приложение перевело файл на шифрование само, при ближайшем запуске.
        config.ai.key_stored_plain =
            !config.ai.api_key.is_empty() && !crate::secret::is_protected(&config.ai.api_key);
        config.ai.api_key = crate::secret::reveal(&config.ai.api_key);

        config.apply_env();
        config
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("SUFLER_AI_PROVIDER") {
            self.ai.provider = v;
        }
        if let Ok(v) = std::env::var("SUFLER_AI_ENDPOINT") {
            self.ai.endpoint = v;
        }
        if let Ok(v) = std::env::var("SUFLER_AI_KEY") {
            self.ai.api_key = v;
        }
        if let Ok(v) = std::env::var("SUFLER_AI_MODEL") {
            self.ai.model = v;
        }
        if let Ok(v) = std::env::var("SUFLER_AI_PROXY") {
            self.ai.proxy = v;
        }
        if let Ok(v) = std::env::var("SUFLER_THEME") {
            self.ui.theme = v;
        }

        // Провайдер `http` без endpoint работать не может — откатываемся на Википедию,
        // иначе пользователь получит бесконечную ошибку сети без объяснения причины.
        if self.ai.provider == "http" && self.ai.endpoint.is_empty() {
            log::warn!("provider=http без endpoint — включён провайдер Википедии");
            self.ai.provider = "wikipedia".into();
        }
    }
}

/// То, что нужно знать фронтенду попапа при старте окна.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
    pub theme: String,
    pub error_text: String,
    /// Умеет ли источник отвечать на вопросы, а не только выдавать определение.
    ///
    /// От этого зависит, показывать ли «?»: у модели «простыми словами» и примеры
    /// спрашиваются отдельным запросом, когда человек нажал кнопку, а Википедия
    /// разговаривать не умеет — ей нечего ответить, кроме той же статьи.
    pub dialogue: bool,
    /// Телефон это или компьютер.
    ///
    /// Окно настройки общее для всех систем, но часть его текста верна только
    /// на компьютере: ни трея, ни левого Ctrl на телефоне нет, и обещать их
    /// там — врать человеку в лицо. Определяем в Rust, а не по строке браузера:
    /// здесь это известно достоверно, на этапе сборки.
    pub mobile: bool,
    /// Язык интерфейса: `ru` | `en`.
    pub language: String,
}

impl From<&Config> for RuntimeConfig {
    fn from(config: &Config) -> Self {
        Self {
            theme: config.ui.theme.clone(),
            error_text: config.ui.resolved_error_text(),
            dialogue: config.ai.provider != "wikipedia",
            mobile: cfg!(mobile),
            language: config.ui.language.clone(),
        }
    }
}
