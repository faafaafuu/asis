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
    pub startup: StartupConfig,
    pub voice: VoiceConfig,
    pub calendar: CalendarConfig,
    pub review: ReviewConfig,
    pub food: FoodConfig,
}

/// Заказ продуктов через FoodPilot.
///
/// Своего понимания еды у Ноа нет: что из чего готовится, сколько в этом
/// калорий и чего человек не ест — знает FoodPilot, и спрашивать об этом
/// нужно его, а не модель.
///
/// `max_order` — предохранитель вместо подтверждения человеком. FoodPilot
/// по своей архитектуре требует подтверждать каждый заказ вручную; здесь это
/// подтверждение снято ради голосового заказа, и единственное, что стоит
/// между оговоркой и деньгами, — этот потолок.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FoodConfig {
    /// Адрес API FoodPilot. Пусто — заказ выключен.
    pub endpoint: String,
    /// Чей профиль спрашивать: вкусы, нелюбимые продукты, лимит калорий.
    pub user_id: String,
    /// Код магазина в FoodPilot. `mock-store` — тренировочный, без денег.
    pub store_code: String,
    /// Потолок суммы одного заказа в рублях. Дороже — Ноа не оформляет сам.
    pub max_order: u32,
    /// С какой суммы магазин везёт бесплатно. 0 — порога нет.
    ///
    /// Нужно, чтобы Ноа говорил «до бесплатной доставки не хватает трёхсот
    /// рублей»: человек у плиты не считает это в уме, а разница ощутимая.
    pub free_delivery_from: u32,
    /// Заказывать ли вообще. Выключено — Ноа отвечает, что не умеет.
    pub enabled: bool,
}

impl Default for FoodConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:3001".into(),
            user_id: String::new(),
            // Тренировочный магазин, а не боевой: сначала проверяется вся
            // цепочка целиком, и только потом решается, пускать ли её к
            // настоящим деньгам.
            store_code: "mock-store".into(),
            max_order: 3000,
            // Порог ВкусВилла на осень 2026. Магазины его меняют, поэтому
            // значение вынесено в настройки, а не зашито в код.
            free_delivery_from: 2000,
            enabled: false,
        }
    }
}

impl FoodConfig {
    /// Готов ли заказ работать.
    pub fn ready(&self) -> bool {
        self.enabled && !self.endpoint.trim().is_empty() && !self.user_id.trim().is_empty()
    }
}

/// Связь с Google-календарём.
///
/// Ключ заводит сам человек: Google пускает к календарю только программы,
/// зарегистрированные у него в облаке, а регистрация от нашего имени означала
/// бы для каждого пользователя экран «приложение не проверено» и потолок в сто
/// человек. Свой ключ снимает и то и другое: человек в этом случае сам себе
/// разработчик.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CalendarConfig {
    /// Идентификатор клиента из Google Cloud.
    pub client_id: String,
    /// Секрет клиента. Для настольных программ Google не считает его тайной,
    /// но хранится он так же, как ключ от модели.
    pub client_secret: String,
    /// Долгоживущий ключ, полученный после согласия. Пусто — не подключён.
    pub refresh_token: String,
    /// В какой календарь писать. `primary` — основной календарь человека.
    pub calendar_id: String,
    /// Отправлять ли дела в календарь. Выключено — работает только список.
    pub enabled: bool,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            refresh_token: String::new(),
            calendar_id: "primary".into(),
            enabled: false,
        }
    }
}

impl CalendarConfig {
    /// Готов ли календарь принимать записи.
    pub fn ready(&self) -> bool {
        self.enabled
            && !self.client_id.trim().is_empty()
            && !self.refresh_token.trim().is_empty()
    }
}

/// Вечерний разбор дня.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReviewConfig {
    /// Спрашивать ли вечером, что сделано.
    pub enabled: bool,
    /// Во сколько спрашивать: часы и минуты по местному времени.
    pub hour: u32,
    pub minute: u32,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Вечер, но не ночь: в девять человек ещё за компьютером, и
            // перенести несделанное на завтра ещё имеет смысл.
            hour: 20,
            minute: 30,
        }
    }
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
            // Из коробки — своя модель на этом же компьютере: она отвечает по сути,
            // а не первым абзацем энциклопедии, работает без ключей, регистрации и
            // интернета и ничего не отправляет наружу. Всё остальное — Википедия,
            // облачные сервисы по ключу — остаётся рядом как запасной вариант,
            // выбираемый в окне настройки.
            //
            // Модель здесь намеренно не названа: подходящая зависит от того, сколько
            // на машине видеопамяти, и выбирается при первом запуске (`ollama::pick`).
            provider: "http".into(),
            endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
            api_key: String::new(),
            model: String::new(),
            proxy: String::new(),
            // Девяносто секунд, а не двенадцать, как было.
            //
            // Двенадцати хватало на ответ уже загруженной модели, но не на её
            // загрузку: холодный старт четырёхмиллиардной модели с быстрого диска
            // занимает около тринадцати секунд, и ровно на этом первый же запрос
            // после включения компьютера — или после паузы, за которую Ollama
            // успела выгрузить веса, — обрывался таймаутом. Со стороны это выглядело
            // так: «первый раз всегда ошибка, со второго работает».
            //
            // Само ожидание при этом никуда не делось, поэтому лечится оно не только
            // здесь: модель прогревается при запуске программы (см. `lib.rs`) и
            // держится в памяти дольше (см. `ai_client.rs`). Этот запас — на случай,
            // когда прогрев не успел или модель всё же выгрузили.
            timeout_ms: 90_000,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StartupConfig {
    /// Запускаться вместе с системой и молча уходить в трей.
    ///
    /// Включено по умолчанию, и это осознанно. Программа нужна не сама по себе,
    /// а в тот момент, когда человек читает чужой текст и споткнулся о термин.
    /// Если её в этот момент нет, он не пойдёт её искать — он просто не станет
    /// ничего спрашивать, и инструмента как будто не существует.
    ///
    /// Выключается одной галочкой в окне настройки.
    pub launch_at_login: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            launch_at_login: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceConfig {
    /// Озвучивать ли вообще. Выключенный голос не качает ни синтезатор, ни голоса.
    pub enabled: bool,
    /// `piper` — на своём компьютере, `edge` — нейроголоса Microsoft по сети.
    pub engine: String,
    /// Голос Piper, например `ru_RU-irina-medium`.
    pub voice: String,
    /// Голос Edge, например `ru-RU-SvetlanaNeural`. Отдельным полем: наборы
    /// голосов у способов разные, и переключение туда-обратно не должно
    /// каждый раз сбрасывать выбор.
    pub edge_voice: String,
    /// Просыпаться на обращение «хэй, ноа», не дожидаясь клавиш.
    ///
    /// Означает постоянно открытый микрофон: программа режет поток на фразы
    /// и прогоняет каждую через распознавание, чтобы услышать обращение. Наружу
    /// при этом ничего не уходит — распознавание своё, на этом же компьютере, —
    /// но комната слушается всё время работы программы, и человек должен иметь
    /// возможность это выключить.
    pub wake_word: bool,
    /// Микрофон, с которого писать. Пусто — тот, что система считает основным.
    ///
    /// Отдельная настройка нужна потому, что «основной» и «тот, в который
    /// говорят» совпадают не всегда: виртуальные микрофоны сторонних программ
    /// и микрофоны геймпадов охотно занимают первое место и пишут тишину.
    pub input_device: String,
    /// Скорость речи. 1.0 — как задумано голосом, 1.5 — в полтора раза быстрее,
    /// 0.8 — медленнее. Разумные пределы 0.5..2.0: за ними речь либо тянется,
    /// либо перестаёт разбираться на слух.
    pub rate: f32,
    /// Читать ответ сразу, как только он пришёл, не дожидаясь пробела.
    /// Нужно для голосового разговора: спросил голосом — услышал ответ.
    pub speak_answers: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: "piper".into(),
            // Строкой, а не ссылкой на voice::assets: модуль голоса собирается
            // только для настольных систем, а настройки общие для всех.
            voice: "ru_RU-irina-medium".into(),
            edge_voice: "ru-RU-SvetlanaNeural".into(),
            wake_word: true,
            input_device: String::new(),
            rate: 1.0,
            // По умолчанию выключено: озвучивать каждое объяснение, которое
            // человек и так читает глазами, — навязчиво. Включается вместе
            // с голосовым разговором.
            speak_answers: false,
        }
    }
}

impl AiConfig {
    /// Сколько ждать провайдера снаружи — с учётом повторов.
    ///
    /// Это не второй таймаут, а рубеж на случай, когда внутренний почему-то не
    /// сработал (зависшая задача, паника). Считается от настроек, а не задан
    /// числом: раньше здесь стояло 25 секунд, и любое увеличение таймаута
    /// упиралось в этот предел — запрос обрывался снаружи ровно тогда же.
    pub fn call_limit(&self) -> std::time::Duration {
        let attempts = u64::from(self.retries) + 1;
        let backoff: u64 = (0..self.retries).map(|n| self.retry_backoff_ms << n).sum();
        std::time::Duration::from_millis(self.timeout_ms * attempts + backoff + 5_000)
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
        config.normalize();
        config
    }

    /// Правит настройки, которые остались от прежних версий и теперь мешают.
    ///
    /// Умолчание меняется только для тех, у кого файла настроек ещё нет. У всех
    /// остальных в `config.json` записано прежнее значение, и оно сильнее — то
    /// есть исправление до них не доедет вовсе, а именно они и столкнулись
    /// с ошибкой.
    fn normalize(&mut self) {
        // Двенадцать секунд — прежнее умолчание. Своей модели их не хватает даже
        // на загрузку в память (около тринадцати), и первый запрос обрывался
        // всегда. Поднимаем только заведомо непригодные значения и только для
        // своего компьютера: у облачного сервиса короткий таймаут осмыслен —
        // там нечему грузиться, и долгое молчание значит, что сервис недоступен.
        const LOCAL_FLOOR_MS: u64 = 90_000;
        if is_local(&self.ai.endpoint) && self.ai.timeout_ms < LOCAL_FLOOR_MS {
            log::info!(
                "таймаут {} мс мал для своей модели — поднят до {LOCAL_FLOOR_MS} мс",
                self.ai.timeout_ms
            );
            self.ai.timeout_ms = LOCAL_FLOOR_MS;
        }
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

/// Свой ли это компьютер. Одно место на всё приложение: тот же вопрос задаётся
/// перед тем, как поднимать Ollama и прогревать модель.
pub fn is_local(endpoint: &str) -> bool {
    endpoint.contains("localhost") || endpoint.contains("127.0.0.1")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_short_timeout_is_raised_only_for_own_computer() {
        let mut config = Config::default();
        config.ai.endpoint = "http://localhost:11434/api/chat".into();
        config.ai.timeout_ms = 12_000;
        config.normalize();
        assert_eq!(config.ai.timeout_ms, 90_000);

        // У облачного сервиса короткий таймаут осмыслен — не трогаем.
        let mut cloud = Config::default();
        cloud.ai.endpoint = "https://api.groq.com/openai/v1/chat/completions".into();
        cloud.ai.timeout_ms = 12_000;
        cloud.normalize();
        assert_eq!(cloud.ai.timeout_ms, 12_000);
    }

    #[test]
    fn call_limit_covers_every_attempt() {
        let mut ai = AiConfig::default();
        ai.timeout_ms = 10_000;
        ai.retries = 1;
        ai.retry_backoff_ms = 400;
        // Две попытки по 10 с, пауза 400 мс и запас 5 с.
        assert_eq!(ai.call_limit().as_millis(), 25_400);
    }
}
