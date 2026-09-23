//! AI-клиент бэкенда.
//!
//! Сеть живёт здесь, а не в webview: CSP окна попапа запрещает внешние запросы,
//! и ключ API не должен попадать во фронтенд вообще.

use std::time::Duration;

use async_trait::async_trait;
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::config::AiConfig;

/// Дата, день недели и время строкой для системного промпта.
///
/// Без неё модель не знает, какой сейчас год, и отвечает исходя из того, на
/// каких данных её обучили, — а это может быть год-два-три назад. Прежде здесь
/// была одна дата, и на «который час» модель выдумывала время; теперь — то же,
/// что при разборе распоряжений (`planner::now_line`).
fn today_line(language: &str) -> String {
    match language {
        "en" => format!("Now: {}.", Local::now().format("%A, %Y-%m-%d %H:%M")),
        _ => crate::planner::now_line(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub def: String,
    #[serde(default)]
    pub simple: String,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadItem {
    pub q: String,
    pub a: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("Сбой сети — нет ответа")]
    Network,
    #[error("Таймаут запроса")]
    Timeout,
    #[error("Сервис ответил ошибкой {0}")]
    Http(u16),
    /// Отказ с объяснением сервиса — его текст из тела ответа.
    #[error("Сервис ответил ошибкой {0}: {1}")]
    Refused(u16, String),
    #[error("Не удалось разобрать ответ модели")]
    Parse,
    #[error("{0}")]
    Config(String),
}

impl AiError {
    /// Повторяем только то, что имеет шанс пройти со второй попытки.
    ///
    /// 429 сюда не входит намеренно. Ограничение частоты снимается через минуты
    /// или сутки, а не через полсекунды: повтор гарантированно упрётся в тот же
    /// отказ и потратит вторую попытку из дневной квоты. У бесплатных тарифов,
    /// где эта квота и так мала, так сгорает вдвое больше запросов.
    fn retryable(&self) -> bool {
        match self {
            AiError::Network | AiError::Timeout => true,
            AiError::Http(status) | AiError::Refused(status, _) => *status == 408 || *status >= 500,
            _ => false,
        }
    }

    /// Текст, который увидит пользователь в состоянии Error.
    pub fn user_text(&self, default_text: &str) -> String {
        match self {
            AiError::Network | AiError::Timeout => default_text.to_string(),
            // Голое «ошибка 429» человеку ничего не говорит и выглядит как поломка,
            // хотя чинится ожиданием.
            AiError::Http(429) => {
                "Сервис ограничил частоту запросов — попробуйте позже".to_string()
            }
            // 402 — «требуется оплата»: у сервиса кончились деньги на счету.
            // Числом это не читается никак, а чинится не ожиданием и не
            // повтором, а пополнением счёта или переходом на свою модель.
            AiError::Http(402) => {
                "У сервиса модели кончились деньги на счету. Пополните счёт \
                 или выберите модель на этом компьютере в настройках"
                    .to_string()
            }
            // 401 и 403 — про ключ, а не про деньги и не про сеть.
            AiError::Http(401) | AiError::Http(403) => {
                "Сервис не принял ключ — проверьте его в настройках".to_string()
            }
            // У бесплатных моделей OpenRouter — около двадцати запросов в минуту
            // и полсотни в сутки; ожидание снимает первое, но не второе.
            AiError::Refused(429, _) => {
                "Сервис ограничил частоту запросов: у бесплатных моделей лимит в минуту \
                 и в сутки. Подождите или выберите модель на этом компьютере"
                    .to_string()
            }
            AiError::Refused(402, _) => {
                "У сервиса модели кончились деньги на счету. Пополните счёт \
                 или выберите модель на этом компьютере в настройках"
                    .to_string()
            }
            AiError::Refused(401 | 403, _) => {
                "Сервис не принял ключ — проверьте его в настройках".to_string()
            }
            // OpenRouter отвечает 404, когда модели с таким именем нет или
            // бесплатная модель закрыта настройками приватности аккаунта.
            AiError::Refused(404, message) => format!(
                "Сервис не нашёл модель ({message}). Проверьте имя модели в настройках; \
                 для бесплатных моделей OpenRouter включите бесплатные эндпоинты на \
                 openrouter.ai/settings/privacy"
            ),
            // Модель есть, но она не для разговора: у OpenRouter так отвечают
            // модели решений и прочие, у которых свой адрес. Выписанное с
            // сайта имя человек читать в оригинале не должен — ему надо знать,
            // что делать.
            AiError::Refused(400, message)
                if message.contains("cannot be used with the chat/completions")
                    || message.contains("is not a chat model") =>
            {
                "Эта модель не отвечает в чате — выберите другую в настройках,                  в списке моделей"
                    .to_string()
            }
            other => other.to_string(),
        }
    }
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn explain(&self, term: &str, context: &str) -> Result<Explanation, AiError>;
    async fn ask(
        &self,
        term: &str,
        context: &str,
        thread: &[ThreadItem],
        question: &str,
    ) -> Result<String, AiError>;

    /// Разбирает сказанное по заданным правилам и отдаёт ответ как есть.
    ///
    /// Нужно там, где от модели ждут не объяснение человеку, а разобранные
    /// данные: во что превратить «напомни завтра в три позвонить в банк».
    /// Обычные `explain` и `ask` для этого не годятся — они обязаны отвечать
    /// связным текстом по-русски, и любая просьба ответить иначе спорит с их
    /// собственными указаниями.
    ///
    /// Умеют не все источники: Википедия ничего не разбирает. Поэтому у метода
    /// есть общий отказ, а переопределяют его те, кто может.
    async fn interpret(&self, _rules: &str, _said: &str) -> Result<String, AiError> {
        Err(AiError::Parse)
    }

    /// Отвечает человеку по заданным указаниям обычным текстом — как
    /// `interpret`, но без требования JSON: диагностике нужен связный ответ.
    async fn advise(&self, rules: &str, said: &str) -> Result<String, AiError> {
        self.interpret(rules, said).await
    }

    /// Занятие с репетитором: свои указания, история разговора и новая реплика.
    ///
    /// Не `ask`: у того указания про выделенный термин и ответ в пару фраз, а
    /// репетитору нужно объяснять — по шагам, с примером, столько, сколько
    /// требует непонятое. `long` — подробный разбор раздела урока, ему нужен
    /// потолок длины в несколько раз выше обычного.
    ///
    /// По умолчанию история складывается в одну реплику и уходит в `advise`.
    async fn tutor(
        &self,
        rules: &str,
        thread: &[ThreadItem],
        said: &str,
        _long: bool,
    ) -> Result<String, AiError> {
        let mut text = String::new();
        for item in thread {
            text.push_str(&format!("Ученик: {}
Репетитор: {}

", item.q, item.a));
        }
        text.push_str(said);
        self.advise(rules, &text).await
    }
}

/// Навешивает прокси на клиент, если он задан.
///
/// Кривой адрес не считаем поводом отказать в работе: клиент собирается дальше, но
/// уже без прокси, и в журнал уходит внятная строка. Иначе одна опечатка в поле
/// оставила бы человека вообще без объяснений — включая те, что прекрасно доходят
/// напрямую, вроде Википедии.
fn with_proxy(builder: reqwest::ClientBuilder, proxy: &str) -> reqwest::ClientBuilder {
    let proxy = proxy.trim();
    if proxy.is_empty() {
        return builder;
    }
    match reqwest::Proxy::all(proxy) {
        Ok(p) => builder.proxy(p),
        Err(err) => {
            log::warn!("прокси «{proxy}» не понят ({err}) — работаем напрямую");
            builder
        }
    }
}

/// Собирает провайдера по конфигурации. Неизвестное имя — это mock, а не паника:
/// приложение уже запущено, ронять его из-за опечатки в конфиге нельзя.
pub fn build_provider(config: &AiConfig, language: &str, wake_name: &str) -> Box<dyn AiProvider> {
    match config.provider.as_str() {
        "wikipedia" => match WikipediaProvider::new(config) {
            Ok(provider) => Box::new(provider),
            Err(err) => {
                log::error!("провайдер Википедии не собрался ({err}) — работаем на mock");
                Box::new(MockProvider::default())
            }
        },
        "http" => match HttpProvider::new(config, language, wake_name) {
            Ok(provider) => Box::new(provider),
            Err(err) => {
                log::error!("HTTP-провайдер не собрался ({err}) — работаем на mock");
                Box::new(MockProvider::default())
            }
        },
        other => {
            if other != "mock" {
                log::warn!("неизвестный провайдер «{other}» — работаем на mock");
            }
            Box::new(MockProvider::default())
        }
    }
}

/* ─────────────────────────────── Mock ─────────────────────────────────── */

/// Детерминированные ответы для разработки и демо-режима. Словарь — тот же, что во
/// фронтенде (`src/js/ai-client.js`), чтобы демо и приложение вели себя одинаково.
#[derive(Debug, Default)]
pub struct MockProvider;

const MOCK_ANSWERS: &[(&str, &str, &str, &[&str])] = &[
    (
        "альбедо",
        "Отражательная способность поверхности: доля падающего света, которая уходит обратно.",
        "Насколько поверхность «светлая» для солнца. Светлая отражает и остаётся холодной, тёмная поглощает и греется.",
        &["свежий снег — 0.8–0.9", "открытый океан — около 0.06", "белая крыша летом прохладнее чёрной"],
    ),
    (
        "криоконит",
        "Тёмный осадок из минеральной пыли, сажи и микроорганизмов на поверхности ледника.",
        "Грязь на льду. Она темнее льда, потому сильнее нагревается и проплавляет себе ямку.",
        &["криоконитовые колодцы глубиной в несколько сантиметров", "пыль от лесных пожаров, осевшая на ледник"],
    ),
    (
        "абляция",
        "Убыль массы льда: таяние, испарение, сублимация и механический отрыв.",
        "Всё, из-за чего ледник теряет лёд. Противоположность накоплению снега.",
        &["стаявший за лето слой на поверхности", "откол айсбергов от языка ледника"],
    ),
    (
        "изостазия",
        "Равновесие литосферы на пластичной мантии: снимите нагрузку — кора поднимется.",
        "Земная кора плавает, как плот. Убрали с него груз льда — плот всплывает, но очень медленно.",
        &["Скандинавия поднимается ~8 мм в год после последнего оледенения", "прогиб коры под Гренландским щитом"],
    ),
    (
        "криосфера",
        "Все формы льда в системе Земли: морской лёд, ледники и щиты, снежный покров, мерзлота.",
        "Вся замёрзшая часть планеты, вместе взятая.",
        &["арктический морской лёд", "мерзлота Сибири", "сезонный снежный покров"],
    ),
    (
        "литосфера",
        "Жёсткая внешняя оболочка Земли: кора и верхняя часть мантии.",
        "Твёрдая «скорлупа» планеты, которая лежит на более вязком слое под ней.",
        &["толщина под океаном — около 70 км", "континентальная литосфера — до 150 км"],
    ),
];

/// Грубый подбор по основе слова: «альбедо», «альбедо,», «альбедой» — одно и то же.
/// Резать строку байтовым срезом здесь нельзя: у кириллицы 2 байта на символ,
/// и `&key[..5]` попадает в середину буквы (тест `mock_finds_term_by_prefix`).
fn mock_lookup(term: &str) -> Option<Explanation> {
    let needle: String = term
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic() || *c == '-')
        .collect();

    MOCK_ANSWERS
        .iter()
        .find(|(key, ..)| {
            let stem: String = key.chars().take(5).collect();
            needle == **key || needle.starts_with(&stem)
        })
        .map(|(_, def, simple, examples)| Explanation {
            def: (*def).to_string(),
            simple: (*simple).to_string(),
            examples: examples.iter().map(|s| (*s).to_string()).collect(),
        })
}

#[async_trait]
impl AiProvider for MockProvider {
    async fn explain(&self, term: &str, _context: &str) -> Result<Explanation, AiError> {
        // Небольшая задержка нужна не для красоты: без неё состояние Loading
        // не успевает отрисоваться, и мы не увидим, что оно вообще работает.
        tokio::time::sleep(Duration::from_millis(900)).await;
        Ok(mock_lookup(term).unwrap_or_else(|| Explanation {
            def: format!("Определения для «{term}» нет — выделите одно слово или термин."),
            simple: String::new(),
            examples: Vec::new(),
        }))
    }

    async fn ask(
        &self,
        term: &str,
        _context: &str,
        thread: &[ThreadItem],
        question: &str,
    ) -> Result<String, AiError> {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let data = mock_lookup(term);

        // Заглушка не притворяется моделью: пользователь должен понимать, почему
        // ответы не связаны с его вопросом, и как включить настоящие.
        if thread.is_empty() {
            return Ok(format!(
                "Это демонстрационный режим: отвечает заглушка, а не модель. Настоящие ответы \
                 включаются в config.json — там указываются адрес и ключ API. По существу \
                 «{term}» — {}.",
                data.as_ref()
                    .map(|d| d.def.trim_end_matches('.').to_string())
                    .unwrap_or_else(|| "определения в демо-словаре нет".into())
            ));
        }
        let variants = [
            format!(
                "Если совсем коротко: {}.",
                data.as_ref()
                    .map(|d| d.def.trim_end_matches('.').to_string())
                    .unwrap_or_default()
            ),
            format!(
                "Иначе говоря: {}",
                data.as_ref()
                    .map(|d| d.simple.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "определение выше — самое короткое, что тут есть.".into())
            ),
            format!(
                "Пример по делу: {}.",
                data.as_ref()
                    .and_then(|d| d.examples.first().cloned())
                    .unwrap_or_else(|| "—".into())
            ),
        ];
        Ok(format!(
            "{} (демо-режим: вопрос «{}» модели не отправлялся)",
            variants[(thread.len() - 1) % variants.len()],
            question.trim()
        ))
    }
}

/* ─────────────────────────────── HTTP ─────────────────────────────────── */

const SYSTEM_PROMPT_RU: &str = concat!(
    "Ты объясняешь термин, который выделил пользователь. Ответь одним-двумя предложениями ",
    "обычным текстом, по-русски — даже если сам термин на другом языке. Объясняй в том смысле, ",
    "в каком термин стоит в контексте (окно и текст вокруг): «ядро» в статье про Linux — ядро ",
    "системы, а не ядро ореха. Если выделена фраза или предложение — объясни её смысл в этом ",
    "тексте. Только объяснение: без вступлений, без списков, без разметки и без JSON."
);

const SYSTEM_PROMPT_EN: &str = concat!(
    "You explain a term the user selected. Answer in one or two plain-text sentences ",
    "in English, even if the term itself is in another language. Explain it in the sense it has ",
    "in the given context (window and surrounding text); if a phrase or sentence is selected, ",
    "explain what it means in that text. The explanation only: no preamble, no lists, no markup, no JSON."
);

/// Подсказка модели на языке интерфейса.
///
/// Язык ответа и язык интерфейса обязаны совпадать: русское объяснение в
/// английском окне читается как поломка, а не как забота.
fn system_prompt(language: &str) -> &'static str {
    match language {
        "en" => SYSTEM_PROMPT_EN,
        _ => SYSTEM_PROMPT_RU,
    }
}

/// Потолок длины ответа.
///
/// Без него модель пишет, пока не выговорится, а мелкие модели на длинной
/// дистанции ещё и уходят в повторы и бессвязицу. Но и тесный потолок вреден:
/// при 220 токенах — это полтысячи русских знаков — ответ на живой вопрос в
/// разговоре обрывался на полуслове, и человек видел половину мысли. Краткость
/// объяснению задаёт подсказка («одним-двумя предложениями»), а потолок лишь
/// страховка от бесконечного текста; где он всё же срабатывает, ответ
/// дорезается до конца предложения — см. `finish_at_sentence`.
const ANSWER_LIMIT: u32 = 700;

/// Насколько модели позволено выбирать неочевидные слова.
///
/// По умолчанию у Ollama 0.8 — это настройка для сочинительства. Здесь же нужен
/// словарь: на одно и то же слово ожидается один и тот же ответ, а не новая
/// формулировка каждый раз. Заодно реже случаются срывы на чужой язык.
const TEMPERATURE: f32 = 0.2;

/// Заготовка реального провайдера: нейтральный chat-подобный формат.
/// Под конкретный API подгоняется правкой `request_body`/`extract_text`.
pub struct HttpProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    retries: u32,
    retry_backoff_ms: u64,
    /// Язык, на котором модель обязана отвечать. См. `system_prompt`.
    language: String,
    /// Как модель представляется человеку — своё у каждого, по умолчанию «Ноа».
    wake_name: String,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

impl HttpProvider {
    pub fn new(config: &AiConfig, language: &str, wake_name: &str) -> Result<Self, AiError> {
        if config.endpoint.is_empty() {
            return Err(AiError::Config("не задан endpoint AI-провайдера".into()));
        }
        let client = with_proxy(
            crate::net::client_builder().timeout(Duration::from_millis(config.timeout_ms)),
            &config.proxy,
        )
        .build()
        .map_err(|_| AiError::Config("не удалось создать HTTP-клиент".into()))?;
        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            retries: config.retries,
            retry_backoff_ms: config.retry_backoff_ms,
            language: language.to_string(),
            wake_name: if wake_name.trim().is_empty() {
                crate::config::DEFAULT_WAKE_NAME.to_string()
            } else {
                wake_name.to_string()
            },
        })
    }

    /// Один заход за ответом на вопрос: отправить и достать текст.
    async fn answer_once(&self, messages: Vec<Message<'_>>) -> Result<String, AiError> {
        self.answer_as(messages, false).await
    }

    /// `json` — ответ обязан быть объектом JSON (разбор реплик).
    async fn answer_as(&self, messages: Vec<Message<'_>>, json: bool) -> Result<String, AiError> {
        self.answer_limited(messages, json, answer_limit(&self.endpoint)).await
    }

    /// То же, что `answer_as`, но со своим потолком длины ответа.
    async fn answer_limited(
        &self,
        messages: Vec<Message<'_>>,
        json: bool,
        limit: u32,
    ) -> Result<String, AiError> {
        let value = self.send_limited(messages, json, limit).await?;
        let text = extract_text(&value)
            .map(|text| strip_reasoning(&text))
            .map(|text| strip_code_fence(&text).to_string())
            .ok_or(AiError::Parse)?;
        let text = if cut_short(&value) {
            log::info!("ответ упёрся в потолок длины — дорезаю до конца предложения");
            finish_at_sentence(&text)
        } else {
            text
        };

        if is_deliberation(&text) {
            log::warn!("модель прислала рассуждение вместо ответа — считаем это отказом");
            return Err(AiError::Parse);
        }
        Ok(text)
    }

    async fn send(&self, messages: Vec<Message<'_>>) -> Result<serde_json::Value, AiError> {
        self.send_as(messages, false).await
    }

    async fn send_as(&self, messages: Vec<Message<'_>>, json: bool) -> Result<serde_json::Value, AiError> {
        self.send_limited(messages, json, answer_limit(&self.endpoint)).await
    }

    async fn send_limited(
        &self,
        messages: Vec<Message<'_>>,
        json: bool,
        limit: u32,
    ) -> Result<serde_json::Value, AiError> {
        let mut body = serde_json::json!({
            "model": (!self.model.is_empty()).then(|| self.model.clone()),
            "messages": messages,
            // Ollama по умолчанию отвечает потоком построчного JSON — разобрать его
            // как один объект нельзя. Для OpenAI-совместимых API поле безвредно.
            "stream": false,
            "max_tokens": limit,
            "temperature": TEMPERATURE,
        });

        // Родной API Ollama живёт по своим именам: max_tokens он не знает, зато
        // понимает options.num_predict. Отправляем эти поля только ему — чужим
        // сервисам лишние ключи ни к чему.
        if self.endpoint.contains("/api/chat") {
            body["options"] = serde_json::json!({
                "num_predict": limit,
                "temperature": TEMPERATURE,
            });
            // Иначе Ollama выгружает модель из памяти после пяти минут простоя,
            // и первое же выделение после паузы ждёт её загрузки — десятки секунд.
            // Человек видит «нет ответа» ровно там, где программа работает
            // правильно. Срок один на всё приложение: тем же значением модель
            // прогревается при запуске (см. ollama::preload).
            body["keep_alive"] = serde_json::json!(crate::ollama::KEEP_ALIVE);
            // Qwen 3 и новее сначала рассуждают, а потом отвечают: для голоса это
            // лишние секунды. Модели без размышлений этот ключ просто не замечают.
            body["think"] = serde_json::json!(false);
        }

        // OpenRouter умеет не присылать размышление — просим его об этом.
        //
        // Дело не в лишнем трафике, а в том, куда уходит лимит ответа. Модель
        // сначала думает вслух, и думает много; лимит у нас короткий, потому что
        // объяснение должно быть в два-три предложения. Размышление съедает его
        // целиком, и на сам ответ не остаётся ничего — сервис возвращает пустой
        // текст. Поле нестандартное, поэтому отправляем его только тем, кто его
        // понимает: чужие API на неизвестный ключ отвечают ошибкой.
        //
        // Скрыть размышление мало: модель всё равно думает — это и секунды
        // ожидания, и оплаченные токены. Поэтому думать просим как можно меньше.
        // Не `none`: модели, которые без размышления не работают, отвечают на
        // него ошибкой, а `minimal` понимают все.
        if self.endpoint.contains("openrouter.ai") {
            body["reasoning"] = serde_json::json!({ "effort": "minimal", "exclude": true });
            // Стоимость запроса — в самом ответе: по ней считает виджет расхода.
            body["usage"] = serde_json::json!({ "include": true });
        }
        // Gemini через совместимый интерфейс Google думает по умолчанию.
        if self.endpoint.contains("googleapis.com") {
            body["reasoning_effort"] = serde_json::json!("low");
        }

        // Разбор реплик обязан вернуть JSON, а просьба в тексте правил для
        // облачных моделей — не гарантия: Mistral, увидев подходящий модуль,
        // отвечал прозой и сам выдумывал результат. Режим JSON сервиса это
        // исключает.
        let ollama = self.endpoint.contains("/api/chat");
        if json {
            if ollama {
                body["format"] = serde_json::json!("json");
            } else {
                body["response_format"] = serde_json::json!({ "type": "json_object" });
            }
        }
        match self.send_retrying(&body).await {
            // Сервис без режима JSON отвечает отказом — тогда как раньше, без него.
            Err(err @ (AiError::Http(400 | 422) | AiError::Refused(400 | 422, _))) if json && !ollama => {
                log::info!("режим JSON сервис не принял ({err}) — повторяю без него");
                body.as_object_mut().map(|fields| fields.remove("response_format"));
                self.send_retrying(&body).await
            }
            other => other,
        }
    }

    async fn send_retrying(&self, body: &serde_json::Value) -> Result<serde_json::Value, AiError> {
        let mut last = AiError::Network;
        for attempt in 0..=self.retries {
            match self.send_once(body).await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let retryable = err.retryable();
                    last = err;
                    if !retryable || attempt == self.retries {
                        break;
                    }
                    let backoff = self.retry_backoff_ms * 2u64.pow(attempt);
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
            }
        }
        Err(last)
    }

    async fn send_once(&self, body: &serde_json::Value) -> Result<serde_json::Value, AiError> {
        // Подписка на этом компьютере — её программа, а не сетевой сервис.
        if crate::local_cli::is_cli(&self.endpoint) {
            return crate::local_cli::ask(&self.endpoint, body).await.map_err(|err| {
                log::warn!("запрос к модели не удался: {err}");
                AiError::Refused(502, err)
            });
        }
        let mut request = self.client.post(&self.endpoint).json(body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        log::info!("запрос к модели: {}", self.endpoint);
        let started = std::time::Instant::now();
        let response = request.send().await.map_err(|err| {
            // Подробность нужна именно здесь: «Сбой сети» на экране одинаково выглядит
            // и при отказе TLS, и при недоступном хосте, и при таймауте, а чинятся они
            // по-разному — сертификат, прокси, терпение.
            log::warn!("запрос к модели не удался: {err}");
            if err.is_timeout() {
                AiError::Timeout
            } else {
                AiError::Network
            }
        })?;

        let status = response.status();
        log::info!("модель ответила {status}");
        if !status.is_success() {
            // Сервис объясняет отказ в теле ответа: «модели нет», «ключ не тот»,
            // «закрыто настройками приватности». Без этого в журнале оставалась
            // одна цифра, и понять, что чинить, было нельзя.
            let body = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value["error"]["message"]
                        .as_str()
                        .or(value["error"].as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| body.chars().take(200).collect());
            let message = message.trim().to_string();
            log::warn!("отказ сервиса {status}: {message}");
            return Err(AiError::Refused(status.as_u16(), message));
        }
        // Заголовки приходят сразу, а текст — когда модель договорит: время
        // ответа видно только здесь.
        let value: serde_json::Value = response.json().await.map_err(|_| AiError::Parse)?;
        log::info!("ответ модели получен за {} мс", started.elapsed().as_millis());
        crate::usage::record(&value);
        Ok(value)
    }
}

/// Лимит ответа. У облачных моделей — вчетверо больше: новые Gemini и многие
/// модели OpenRouter сначала думают, и размышление тратит тот же лимит. С
/// лимитом своей модели на сам ответ не оставалось ничего — сервис
/// возвращал пустой текст. Краткость ответа держит подсказка, а не лимит.
fn answer_limit(endpoint: &str) -> u32 {
    if endpoint.contains("googleapis.com") || endpoint.contains("openrouter.ai") {
        ANSWER_LIMIT * 4
    } else {
        ANSWER_LIMIT
    }
}

/// Модели, которые есть у облачного сервиса: его список по ключу.
///
/// Адрес списка — рядом с адресом ответов: `…/chat/completions` → `…/models`.
/// Так устроены OpenRouter, Groq и OpenAI-совместимый вход Google.
pub async fn list_models(config: &AiConfig) -> Result<Vec<String>, String> {
    Ok(fetch_models(config)
        .await?
        .iter()
        .filter_map(|model| model["id"].as_str())
        // Google отдаёт имена с приставкой `models/`, а принимает без неё.
        .map(|id| id.trim_start_matches("models/").to_string())
        .collect())
}

/// Модель из каталога сервиса — для выбора в окне настройки.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Длина контекста в токенах; 0 — сервис не сообщил.
    pub context: u64,
    /// Бесплатна ли модель; `None` — сервис цен не публикует (Groq, Google).
    pub free: Option<bool>,
    /// Цена за миллион токенов запроса, доллары. `None` — цен нет.
    pub prompt_price: Option<f64>,
    /// Цена за миллион токенов ответа, доллары.
    pub completion_price: Option<f64>,
}

/// Каталог разговорных моделей сервиса в его собственном порядке (у OpenRouter —
/// новые первыми). Картинки, речь, эмбеддинги и фильтры модерации отброшены:
/// на вопрос они не ответят.
pub async fn catalog(config: &AiConfig) -> Result<Vec<ModelInfo>, String> {
    const NOT_CHAT: &[&str] = &[
        "embed", "whisper", "tts", "imagen", "veo", "guard", "moderation", "rerank", "aqa",
        "content-safety", "image-generation",
    ];
    Ok(fetch_models(config)
        .await?
        .iter()
        .filter_map(|model| {
            let id = model["id"].as_str()?.trim_start_matches("models/").to_string();
            let lower = id.to_lowercase();
            if NOT_CHAT.iter().any(|word| lower.contains(word)) {
                return None;
            }
            let pricing = &model["pricing"];
            // Цена приходит за один токен и строкой: «0.0000025». Человеку
            // такое число ни о чём не говорит, поэтому пересчитываем на
            // миллион — в этом виде цены и печатают сами сервисы.
            let per_million = |key: &str| {
                pricing[key]
                    .as_str()
                    .and_then(|p| p.parse::<f64>().ok())
                    .map(|price| price * 1_000_000.0)
            };
            let free = if id.ends_with(":free") {
                Some(true)
            } else if pricing.is_object() {
                let zero = |key: &str| {
                    pricing[key].as_str().and_then(|p| p.parse::<f64>().ok()) == Some(0.0)
                };
                Some(zero("prompt") && zero("completion"))
            } else {
                None
            };
            let name = model["name"]
                .as_str()
                .or_else(|| model["display_name"].as_str())
                .unwrap_or(&id)
                .to_string();
            let context = model["context_length"]
                .as_u64()
                .or_else(|| model["context_window"].as_u64())
                .unwrap_or(0);
            Some(ModelInfo {
                id,
                name,
                context,
                free,
                prompt_price: per_million("prompt"),
                completion_price: per_million("completion"),
            })
        })
        .collect())
}

/// Сырой список `data` из `{base}/models` OpenAI-совместимого сервиса.
async fn fetch_models(config: &AiConfig) -> Result<Vec<serde_json::Value>, String> {
    let base = config
        .endpoint
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions");
    let client = with_proxy(crate::net::client_builder(), &config.proxy)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))?;
    let mut request = client.get(format!("{base}/models"));
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("список моделей не пришёл: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("список моделей: ошибка {}", response.status()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "список моделей не разобрался".to_string())?;
    Ok(body["data"].as_array().cloned().unwrap_or_default())
}

/// Живая замена модели, которой у сервиса больше нет. `None` — выбрать не из чего.
///
/// Бесплатную меняем на бесплатную: человек, выбравший `:free`, платить не
/// собирался. Среди прочих берём разговорные модели, а не картинки, речь и
/// «эмбеддинги» — у них те же имена рядом, но на вопрос они не ответят.
pub fn pick_model(endpoint: &str, current: &str, available: &[String]) -> Option<String> {
    let has = |id: &str| available.iter().any(|known| known == id);
    let chat = |id: &str| {
        let id = id.to_lowercase();
        ![
            "image", "tts", "embed", "audio", "vision", "-vl", "safety", "guard", "live", "batch",
            "code",
        ]
        .iter()
        .any(|word| id.contains(word))
    };
    let first = |wanted: &[&str], keep: &dyn Fn(&str) -> bool| {
        wanted
            .iter()
            .find(|id| has(id))
            .map(|id| id.to_string())
            .or_else(|| available.iter().find(|id| keep(id) && chat(id)).cloned())
    };
    if endpoint.contains("openrouter.ai") {
        // Пустое имя — тоже бесплатная: платить никто не соглашался. Gemma —
        // последней: у OpenRouter её часто обслуживает Google, а тот для Gemma
        // не принимает системную инструкцию и отвечает 400.
        if current.is_empty() || current.ends_with(":free") {
            return first(
                &[
                    "z-ai/glm-5.2:free",
                    "nvidia/nemotron-3-super-120b-a12b:free",
                    "google/gemma-4-31b-it:free",
                ],
                &|id| id.ends_with(":free"),
            );
        }
        return first(&["google/gemini-3.5-flash"], &|id| id.contains("flash"));
    }
    if endpoint.contains("googleapis.com") {
        if has("gemini-flash-latest") {
            return Some("gemini-flash-latest".into());
        }
        // Самая новая Flash — по имени: версии растут, и последняя по
        // алфавиту — последняя по времени.
        return available
            .iter()
            .filter(|id| id.starts_with("gemini") && id.contains("flash") && !id.contains("lite") && chat(id))
            .max()
            .cloned();
    }
    if endpoint.contains("groq.com") {
        return first(&["llama-3.3-70b-versatile"], &|id| id.contains("llama"));
    }
    available.iter().find(|id| chat(id)).cloned()
}

#[cfg(test)]
mod live_explain {
    use super::*;

    /// Какая модель толковее объясняет термины — на одних и тех же словах.
    ///
    /// `cargo test --lib ai_client::live_explain -- --ignored --nocapture`;
    /// модели — через `SUFLER_MODELS=qwen2.5:7b,qwen3.5:9b`.
    #[test]
    #[ignore = "ходит в локальную модель"]
    fn compare_explanations() {
        let terms = [
            "альбедо",
            "литосфера",
            "Kubernetes",
            "инфляция",
            "квантовая запутанность",
            "эмбеддинг",
        ];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let models: Vec<String> = std::env::var("SUFLER_MODELS")
            .map(|list| list.split(',').map(str::to_string).collect())
            .unwrap_or_else(|_| vec!["qwen2.5:7b".into()]);

        for model in models {
            let config = AiConfig {
                endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
                model: model.clone(),
                ..Default::default()
            };
            let provider = HttpProvider::new(&config, "ru", "Ноа").expect("провайдер");
            // Первый запрос грузит модель в память — его время не в счёт.
            let _ = runtime.block_on(provider.explain("прогрев", ""));
            let started = std::time::Instant::now();
            for term in terms {
                match runtime.block_on(provider.explain(term, "")) {
                    Ok(found) => println!("{model:>11} {term}: {} | проще: {}", found.def, found.simple),
                    Err(err) => println!("{model:>11} {term}: ошибка — {err}"),
                }
            }
            println!(
                "{model}: в среднем {} мс на термин\n",
                started.elapsed().as_millis() / terms.len() as u128
            );
            runtime.block_on(crate::ollama::unload(crate::ollama::DEFAULT_HOST, &model));
        }
    }
}

#[cfg(test)]
mod model_pick_tests {
    use super::pick_model;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn a_free_model_is_replaced_by_a_free_one() {
        let available = ids(&["google/gemini-3.5-flash", "z-ai/glm-5.2:free", "liquid/lfm-2.5-2.6b:free"]);
        let pick = pick_model("https://openrouter.ai/api/v1/chat/completions", "openai/gpt-oss-20b:free", &available);
        assert_eq!(pick.as_deref(), Some("z-ai/glm-5.2:free"));
    }

    #[test]
    fn google_gets_the_newest_flash() {
        let endpoint = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
        let available = ids(&[
            "gemini-3.5-flash",
            "gemini-3.7-flash",
            "gemini-3.7-flash-image",
            "gemini-3.5-flash-lite",
            "text-embedding-004",
        ]);
        assert_eq!(pick_model(endpoint, "gemini-2.0-flash", &available).as_deref(), Some("gemini-3.7-flash"));
        let with_alias = ids(&["gemini-flash-latest", "gemini-3.7-flash"]);
        assert_eq!(
            pick_model(endpoint, "gemini-2.0-flash", &with_alias).as_deref(),
            Some("gemini-flash-latest")
        );
    }
}

/// Вычищает чужие письменности и следит, чтобы что-то осталось.
///
/// Пустой результат — это не «ответ без иероглифов», а «ответа не было».
/// Показать в окне пустоту хуже, чем честную ошибку: человек будет ждать.
pub(crate) fn purge(text: &str) -> Result<String, AiError> {
    let cleaned = strip_foreign(text);
    if cleaned.is_empty() {
        log::warn!("после вычистки чужого письма от ответа ничего не осталось");
        return Err(AiError::Parse);
    }
    Ok(cleaned)
}

/// Строка по указателю — если она там есть и не пуста.
///
/// Пустую строку приравниваем к отсутствию ответа. Без этого перебор вариантов
/// ниже останавливался бы на первом же поле: рассуждающие модели кладут в
/// `content` именно `""`, и дальше искать было уже негде.
fn text_at(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Вытаскивает текст ответа из типовых обёрток chat-completions.
fn extract_text(value: &serde_json::Value) -> Option<String> {
    // OpenAI-совместимые API (в том числе /v1 у Ollama и LM Studio)
    text_at(value.pointer("/choices/0/message/content"))
        // Родной формат Ollama: POST /api/chat
        .or_else(|| text_at(value.pointer("/message/content")))
        // Anthropic
        .or_else(|| text_at(value.pointer("/content/0/text")))
        .or_else(|| text_at(value.pointer("/response")))
        .or_else(|| text_at(value.pointer("/text")))
        // Рассуждающие модели (gpt-oss и родня) отдают видимый ответ отдельным
        // полем, а content оставляют пустым. Для нас это тот же ответ: сервис
        // вернул 200 и текст — молчать из-за формы обёртки нельзя.
        .or_else(|| text_at(value.pointer("/choices/0/message/reasoning")))
        .or_else(|| text_at(value.pointer("/choices/0/message/reasoning_content")))
}

/// Оборвала ли модель ответ из-за потолка длины, а не потому, что договорила.
///
/// OpenAI-совместимые API сообщают это в `finish_reason`, родной API Ollama —
/// в `done_reason`, Anthropic — в `stop_reason`.
fn cut_short(value: &serde_json::Value) -> bool {
    [
        "/choices/0/finish_reason",
        "/done_reason",
        "/stop_reason",
    ]
    .iter()
    .filter_map(|path| value.pointer(path).and_then(|reason| reason.as_str()))
    .any(|reason| reason == "length" || reason == "max_tokens")
}

/// Дорезает оборванный текст до последнего законченного предложения.
///
/// Оборванная на полуслове фраза читается как поломка, и вслух это слышно
/// особенно: голос замолкает посреди слова. Законченных предложений почти всегда
/// хватает, чтобы ответить; если же точки нет вовсе или она у самого начала —
/// режем по последнему слову и честно ставим многоточие.
fn finish_at_sentence(text: &str) -> String {
    let text = text.trim_end();
    let ends = text
        .char_indices()
        .rev()
        .find(|(at, ch)| {
            matches!(ch, '.' | '!' | '?' | '…')
                && text[at + ch.len_utf8()..]
                    .chars()
                    .next()
                    .map_or(true, char::is_whitespace)
        })
        .map(|(at, ch)| at + ch.len_utf8());

    match ends {
        // Меньше трети текста — значит, выбросили бы почти всё сказанное.
        Some(end) if end * 3 >= text.len() => text[..end].to_string(),
        _ => match text.rfind(char::is_whitespace) {
            Some(space) => format!("{}…", text[..space].trim_end()),
            None => format!("{text}…"),
        },
    }
}

/// Убирает размышление, оставленное моделью прямо в тексте ответа.
///
/// Рассуждающие модели отделяют мысли от ответа разметкой: одни — тегом
/// `<think>`, другие — служебными каналами формата harmony. Показывать это
/// человеку нельзя: он спросил, что такое альбедо, а получает страницу
/// английских рассуждений о том, как бы ему ответить.
fn strip_reasoning(text: &str) -> String {
    let mut out = text.to_string();

    // Парные теги — вырезаем вместе с содержимым.
    for (open, close) in [("<think>", "</think>"), ("<thinking>", "</thinking>")] {
        while let (Some(from), Some(to)) = (out.find(open), out.find(close)) {
            if from >= to {
                break;
            }
            out.replace_range(from..to + close.len(), "");
        }
    }

    // Формат harmony: всё до последнего «final» — черновик.
    if let Some(at) = out.rfind("<|channel|>final<|message|>") {
        out = out[at + "<|channel|>final<|message|>".len()..].to_string();
    }
    for mark in ["<|start|>", "<|end|>", "<|message|>", "<|channel|>analysis"] {
        out = out.replace(mark, "");
    }

    out.trim().to_string()
}

/// Похоже ли на размышление модели, а не на ответ человеку.
///
/// Случай не выдуманный: сервис вернул 200, поле ответа пустое, а в поле
/// размышления — «We have a conversation. The user asks…». Прежде это уходило
/// в окно как есть. Честное «не ответила» здесь лучше: человек переспросит или
/// сменит модель, а не будет разбирать чужой черновик.
///
/// Признак — обращение к себе в третьем лице на английском в самом начале.
/// Проверяем только начало: ответ по-английски бывает законным (термин
/// английский, разговор английский), а вот начинаться с «The user asks» он
/// не может.
fn is_deliberation(text: &str) -> bool {
    const MARKS: &[&str] = &[
        "we have a conversation",
        "the user asks",
        "the user wants",
        "the user is asking",
        "the system instructions",
        "the developer instruction",
        "we need to answer",
        "let me think",
        "i need to respond",
        "пользователь спрашивает,",
    ];

    let head: String = text.trim().to_lowercase().chars().take(200).collect();
    MARKS.iter().any(|mark| head.contains(mark))
}

/// Снимает обёртку ```json … ```, в которую модели любят заворачивать ответ.
///
/// Просим мы обычный текст, но модель — не подчинённый, а угадыватель: разметка
/// проскакивает регулярно, особенно у мелких. Показать человеку определение
/// вместе с тремя обратными кавычками — мелочь, из-за которой окно выглядит
/// сломанным.
fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    // Сразу за кавычками модели пишут имя языка — до конца строки нам не нужно.
    let Some((_, body)) = rest.split_once('\n') else {
        return trimmed;
    };
    let body = body.trim_end();
    let body = body.strip_suffix("```").unwrap_or(body).trim();
    // Пустота означает, что разметка оказалась не тем, чем мы её посчитали.
    if body.is_empty() {
        trimmed
    } else {
        body
    }
}

/// Ответу модели не доверяем: приводим к контракту {def, simple, examples}.
///
/// Просим обычный текст, но JSON от больших моделей продолжаем понимать: они
/// его присылают и без просьбы, а в нём есть «простыми словами» и примеры.
fn normalize_explanation(value: &serde_json::Value) -> Result<Explanation, AiError> {
    let candidate = match extract_text(value).map(|text| strip_code_fence(&text).to_string()) {
        Some(text) => serde_json::from_str::<serde_json::Value>(&text).unwrap_or_else(|_| {
            // Модель ответила обычным текстом вместо JSON — это всё ещё определение.
            serde_json::json!({ "def": text })
        }),
        None => value.clone(),
    };

    let def = candidate
        .get("def")
        .or_else(|| candidate.get("definition"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(AiError::Parse)?;

    Ok(Explanation {
        def: def.to_string(),
        simple: candidate
            .get("simple")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string(),
        examples: candidate
            .get("examples")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_string)
                    .take(3)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Есть ли в тексте иероглифы и кана.
///
/// Модели, обученной на китайском, случается сорваться на него посреди русского
/// ответа. Бывает редко и на повторе не воспроизводится — значит это не
/// непонимание задачи, а разовый промах при выборе очередного слова.
fn is_foreign(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF   // основные иероглифы
        | 0x3400..=0x4DBF // редкие иероглифы
        | 0x3040..=0x30FF // японские каны
        | 0xAC00..=0xD7AF // корейский хангыль
        | 0x3000..=0x303F // китайская пунктуация: 。、「」
        | 0xFF00..=0xFFEF // «широкие» латиница и знаки
        | 0x0590..=0x05FF // иврит
        | 0x0600..=0x06FF // арабица
        | 0x0900..=0x097F // деванагари
        | 0x0E00..=0x0E7F // тайское письмо
    )
}

pub(crate) fn has_foreign_script(text: &str) -> bool {
    text.chars().any(is_foreign)
}

/// Убирает из ответа всё, чего в нём быть не может.
///
/// Переспросить модель — половина решения: мелкие модели срываются на другой
/// язык и во второй раз, и тогда человек видел иероглифы в конце объяснения.
/// Вторая половина — вычистить их самим, независимо от того, какая модель
/// подключена: правило одно и то же для своей Ollama, для облачного сервиса
/// и для любого, который появится потом.
///
/// Латиницу и кириллицу не трогаем никогда: термин вполне может быть на любом
/// из этих алфавитов, и вычищать их означало бы портить правильные ответы.
fn strip_foreign(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if is_foreign(c) { ' ' } else { c })
        .collect();

    // Схлопываем пробелы, оставшиеся на месте вырезанного, и подчищаем хвост:
    // после удаления часто остаётся висящая запятая или тире.
    let mut out = String::with_capacity(cleaned.len());
    let mut space = true;
    for c in cleaned.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(c);
            space = false;
        }
    }
    out.trim()
        .trim_end_matches([' ', ',', ';', ':', '-', '—', '–'])
        .trim()
        .to_string()
}

#[async_trait]
impl AiProvider for HttpProvider {
    async fn tutor(
        &self,
        rules: &str,
        thread: &[ThreadItem],
        said: &str,
        long: bool,
    ) -> Result<String, AiError> {
        let mut messages = vec![Message {
            role: "system",
            content: rules.to_string(),
        }];
        for item in thread {
            messages.push(Message {
                role: "user",
                content: item.q.clone(),
            });
            messages.push(Message {
                role: "assistant",
                content: item.a.clone(),
            });
        }
        messages.push(Message {
            role: "user",
            content: said.to_string(),
        });
        let limit = if long { ANSWER_LIMIT * 4 } else { answer_limit(&self.endpoint) };
        self.answer_limited(messages, false, limit).await
    }

    async fn advise(&self, rules: &str, said: &str) -> Result<String, AiError> {
        self.answer_as(vec![
            Message {
                role: "system",
                content: rules.to_string(),
            },
            Message {
                role: "user",
                content: said.to_string(),
            },
        ], false)
        .await
    }

    async fn interpret(&self, rules: &str, said: &str) -> Result<String, AiError> {
        self.answer_as(vec![
            Message {
                role: "system",
                content: rules.to_string(),
            },
            Message {
                role: "user",
                content: said.to_string(),
            },
        ], true)
        .await
    }

    async fn explain(&self, term: &str, context: &str) -> Result<Explanation, AiError> {
        let messages = || {
            vec![
                Message {
                    role: "system",
                    content: format!("{} {}", system_prompt(&self.language), today_line(&self.language)),
                },
                Message {
                    role: "user",
                    content: match self.language.as_str() {
                        "en" => format!("Term: “{term}”.\nContext: {context}"),
                        _ => format!("Термин: «{term}».\nКонтекст: {context}"),
                    },
                },
            ]
        };

        let parsed = normalize_explanation(&self.send(messages()).await?)?;
        if !has_foreign_script(&parsed.def) {
            return Ok(parsed);
        }

        // Переспрашиваем ровно один раз: повтор почти всегда даёт чистый ответ,
        // а бесконечно бороться с моделью за язык — не наше дело. Если и второй
        // раз с иероглифами, отдаём как есть: объяснение по существу всё же
        // лучше, чем пустое окно.
        log::warn!("ответ сорвался на другой язык — переспрашиваем");
        let second = normalize_explanation(&self.send(messages()).await?).ok();

        // Берём тот из двух, что чище. Если чистого нет — вычищаем сами:
        // объяснение с обрезанным хвостом полезнее, чем с иероглифами.
        let best = match second {
            Some(second) if !has_foreign_script(&second.def) => second,
            Some(second) => second,
            None => parsed,
        };
        Ok(Explanation {
            def: purge(&best.def)?,
            simple: strip_foreign(&best.simple),
            examples: best.examples.iter().map(|e| strip_foreign(e)).collect(),
        })
    }

    async fn ask(
        &self,
        term: &str,
        context: &str,
        thread: &[ThreadItem],
        question: &str,
    ) -> Result<String, AiError> {
        let messages = || {
            let mut messages = vec![
                Message {
                    role: "system",
                    content: {
                        let name = self.wake_name.as_str();
                        let persona = match self.language.as_str() {
                        "en" if term.trim().is_empty() => format!(
                            "Your name is {name}, you are a voice assistant talking out loud. \
                             Get to the point: usually one or two short sentences, casual \
                             and relaxed, like a friend. Never offer further help or ask \
                             what else you can do, no intros, caveats, summaries or \
                             lectures. If it calls for an estimate, estimate and name a \
                             number. Do not restate the question. Plain text, no lists, \
                             in English."
                        ),
                        "en" => format!(
                            "Your name is {name}. The user is asking a follow-up about \
                             “{term}”. Answer in one or two short casual sentences, plain \
                             text, no JSON, no offers of further help. \
                             Answer in English, even if the term itself is in another language."
                        ),
                        // Пустой термин означает вопрос с чистого места: его
                        // задали голосом, ничего не выделяя. Отвечать на такой
                        // определением — не то, что просили: на «сколько раз
                        // отжаться, чтобы устать» ждут прикидку, а не толкование
                        // самого вопроса.
                        // Ответ звучит вслух, поэтому короче, чем текст: одна-две
                        // фразы. «Чем ещё помочь?» и вступления в голосе раздражают —
                        // запрет на них назван прямо, иначе модели добавляют их сами.
                        _ if term.trim().is_empty() => format!(
                            "Тебя зовут {name}, ты голосовой помощник программы {name} на \
                             компьютере человека (Windows); говоришь с ним голосом или в \
                             Telegram. Действия на компьютере — открыть и закрыть программы, \
                             найти и прислать файл или снимок экрана, напомнить, заказать — \
                             выполняет сама программа по командам, до тебя доходят только \
                             разговорные вопросы. Никогда не говори, что у тебя нет доступа к \
                             компьютеру или что ты на сервере: ты — часть программы на его \
                             компьютере и управляешь им через неё; спросят про доступ — отвечай, что да, \
                             и что умеешь. Если просьба похожа на действие, скажи коротко, что \
                             не разобрал её как команду, и предложи сказать проще, например \
                             «пришли последний скриншот» или «найди файл договор». \
                             Отвечай по делу и коротко — обычно одной-двумя фразами, \
                             живым разговорным языком, легко и непринуждённо, как друг. \
                             Не предлагай помощь и не спрашивай, чем ещё помочь; без \
                             вступлений, оговорок, выводов и нравоучений, не спорь. \
                             Если вопрос требует прикидки, прикинь и назови число. \
                             Не пересказывай вопрос. Обычный текст, без списков и разметки, \
                             по-русски."
                        ),
                        _ => format!(
                            "Тебя зовут {name}. Пользователь уточняет то, о чём шла речь, — \
                             «{term}». Отвечай одной-двумя короткими фразами, разговорно, \
                             обычным текстом, без JSON и без предложений помочь ещё. \
                             Отвечай по-русски, даже если сам термин на другом языке."
                        ),
                        };
                        format!("{persona} {} {}", today_line(&self.language), crate::profile::prompt_line())
                    },
                },
            ];

            // Контекст — только когда он есть. Вопрос, заданный голосом с чистого
            // места, приходит без выделенного текста, и раньше модель получала
            // пустую реплику «Исходный контекст:» перед самим вопросом. Модели
            // покрупнее её просто игнорировали, а те, что поменьше, принимали за
            // начало разговора и отвечали «Есть вопрос?» вместо ответа — вживую
            // так и было, на каждую фразу подряд.
            if !context.trim().is_empty() {
                messages.push(Message {
                    role: "user",
                    content: match self.language.as_str() {
                        "en" => format!("Original context: {context}"),
                        _ => format!("Исходный контекст: {context}"),
                    },
                });
            }
            // Сколько обменов брать, решает вызывающий: своей маленькой модели
            // — три, облачной — больше (см. `thread_depth`).
            for item in thread {
                messages.push(Message {
                    role: "user",
                    content: item.q.clone(),
                });
                messages.push(Message {
                    role: "assistant",
                    content: item.a.clone(),
                });
            }
            messages.push(Message {
                role: "user",
                content: question.to_string(),
            });
            messages
        };

        let answer = self.answer_once(messages()).await?;
        if !has_foreign_script(&answer) {
            return Ok(answer);
        }

        // То же, что и у объяснения: срыв на чужой язык случаен и на повторе
        // почти всегда проходит. В длинном ответе на вопрос он заметнее — там
        // модели есть где разогнаться, — так что защита нужна и здесь.
        log::warn!("ответ на вопрос сорвался на другой язык — переспрашиваем");
        let best = match self.answer_once(messages()).await {
            Ok(second) => second,
            Err(_) => answer,
        };
        purge(&best)
    }
}

/* ───────────────────────────── Википедия ──────────────────────────────── */

/// Определения без ключей и регистрации. Для «что это за слово» энциклопедия
/// точнее генерации: она не выдумывает. Взамен не умеет ни примеров, ни диалога —
/// это работа модели.
pub struct WikipediaProvider {
    client: reqwest::Client,
}

impl WikipediaProvider {
    pub fn new(config: &AiConfig) -> Result<Self, AiError> {
        let client = with_proxy(
            crate::net::client_builder()
                .timeout(Duration::from_millis(config.timeout_ms.min(8_000)))
                // Википедия отвечает 403 на запросы без внятного User-Agent —
                // это её правило для автоматических клиентов.
                .user_agent("Sufler/0.1 (https://github.com/faafaafuu/asis)"),
            &config.proxy,
        )
        .build()
            .map_err(|_| AiError::Config("не удалось создать HTTP-клиент".into()))?;
        Ok(Self { client })
    }

    /// Латиница — английская Википедия, кириллица — русская.
    fn host(term: &str) -> &'static str {
        if term.chars().any(|c| ('а'..='я').contains(&c.to_ascii_lowercase()) || c == 'ё') {
            "ru.wikipedia.org"
        } else {
            "en.wikipedia.org"
        }
    }

    async fn summary(&self, term: &str) -> Option<serde_json::Value> {
        let url = format!(
            "https://{}/api/rest_v1/page/summary/{}",
            Self::host(term),
            urlencoding(term)
        );
        // Журнал вокруг самого сетевого вызова: без этих двух строк «запрос ушёл и
        // не вернулся» неотличимо от «запрос даже не начался».
        log::info!("запрос к {}", Self::host(term));
        let response = match self.client.get(&url).send().await {
            Ok(response) => response,
            Err(err) => {
                log::warn!("сеть не ответила: {err}");
                return None;
            }
        };
        log::info!("ответ {} от {}", response.status(), Self::host(term));
        if !response.status().is_success() {
            return None;
        }
        let value: serde_json::Value = response.json().await.ok()?;
        // Страница-разрешение неоднозначностей определением не является.
        if value.get("type").and_then(|v| v.as_str()) == Some("disambiguation") {
            return None;
        }
        Some(value)
    }

    async fn via_search(&self, term: &str) -> Option<serde_json::Value> {
        let url = format!(
            "https://{}/w/api.php?action=query&list=search&srlimit=1&srsearch={}&format=json",
            Self::host(term),
            urlencoding(term)
        );
        let value: serde_json::Value = self.client.get(&url).send().await.ok()?.json().await.ok()?;
        let title = value.pointer("/query/search/0/title")?.as_str()?.to_string();
        self.summary(&title).await
    }
}

/// Процентное кодирование без лишней зависимости: кодируем всё, кроме безопасного.
fn urlencoding(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('_'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Первое-второе предложение: в шапке нужен короткий ответ, а не абзац.
fn first_sentences(text: &str, count: usize) -> String {
    let mut result = String::new();
    let mut taken = 0;
    for ch in text.chars() {
        result.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            taken += 1;
            if taken >= count {
                break;
            }
        }
    }
    result.trim().to_string()
}

#[async_trait]
impl AiProvider for WikipediaProvider {
    async fn explain(&self, term: &str, _context: &str) -> Result<Explanation, AiError> {
        let page = match self.summary(term).await {
            Some(page) => page,
            None => self.via_search(term).await.ok_or(AiError::Parse)?,
        };

        let extract = page
            .get("extract")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(AiError::Parse)?;

        let def = first_sentences(extract, 2);
        Ok(Explanation {
            // Полная выжимка попадает под «?», если она содержательнее первой фразы.
            simple: if extract.len() > def.len() + 40 {
                extract.to_string()
            } else {
                String::new()
            },
            def,
            examples: Vec::new(),
        })
    }

    async fn ask(
        &self,
        term: &str,
        _context: &str,
        _thread: &[ThreadItem],
        _question: &str,
    ) -> Result<String, AiError> {
        // Не притворяемся, что умеем вести диалог.
        Ok(format!(
            "Уточняющие вопросы умеет только языковая модель — сейчас определения берутся из \
             Википедии. Подключить модель можно в config.json. Статья целиком: https://{}/wiki/{}",
            WikipediaProvider::host(term),
            urlencoding(term)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_answer_ends_on_a_whole_sentence() {
        let cut = "Отжиманий хватит двадцати. Потом отдых минуту. Дальше повторить три под";
        assert_eq!(
            finish_at_sentence(cut),
            "Отжиманий хватит двадцати. Потом отдых минуту."
        );
    }

    #[test]
    fn without_a_sentence_end_the_cut_is_marked() {
        // Точки нет — режем по слову и не притворяемся, что мысль закончена.
        assert_eq!(finish_at_sentence("один два три четы"), "один два три…");
        // Точка у самого начала выбросила бы почти всё — лучше многоточие.
        assert_eq!(
            finish_at_sentence("Да. а дальше очень длинное рассуждение без конца и кра"),
            "Да. а дальше очень длинное рассуждение без конца и…"
        );
    }

    #[test]
    fn numbers_with_dots_are_not_sentence_ends() {
        // «2.5» — не конец предложения: после точки нет пробела.
        assert_eq!(
            finish_at_sentence("Жирность 2.5 процента. Объём девятьсот милли"),
            "Жирность 2.5 процента."
        );
    }

    #[test]
    fn only_the_length_limit_counts_as_cut() {
        assert!(cut_short(&serde_json::json!({ "choices": [{ "finish_reason": "length" }] })));
        assert!(cut_short(&serde_json::json!({ "done_reason": "length" })));
        assert!(cut_short(&serde_json::json!({ "stop_reason": "max_tokens" })));
        assert!(!cut_short(&serde_json::json!({ "choices": [{ "finish_reason": "stop" }] })));
        assert!(!cut_short(&serde_json::json!({ "done_reason": "stop" })));
    }

    #[test]
    fn mock_finds_term_by_prefix() {
        assert!(mock_lookup("Альбедо").is_some());
        assert!(mock_lookup("альбедо,").is_some());
        assert!(mock_lookup("совершенно другое слово").is_none());
    }

    #[test]
    fn normalize_accepts_plain_text_answer() {
        let value = serde_json::json!({ "choices": [{ "message": { "content": "просто текст" } }] });
        let parsed = normalize_explanation(&value).unwrap();
        assert_eq!(parsed.def, "просто текст");
        assert!(parsed.examples.is_empty());
    }

    #[test]
    fn normalize_reads_answer_of_reasoning_model() {
        // gpt-oss и родня: content пустой, ответ лежит в reasoning.
        let value = serde_json::json!({
            "choices": [{ "message": { "content": "", "reasoning": "Альбедо — доля отражённого света." } }]
        });
        let parsed = normalize_explanation(&value).unwrap();
        assert_eq!(parsed.def, "Альбедо — доля отражённого света.");
    }

    #[test]
    fn code_fence_does_not_reach_the_window() {
        let value = serde_json::json!({
            "choices": [{ "message": { "content": "```json\n{\"def\": \"краткое определение\"}\n```" } }]
        });
        let parsed = normalize_explanation(&value).unwrap();
        assert_eq!(parsed.def, "краткое определение");
    }

    #[test]
    fn plain_answer_without_fence_survives_untouched() {
        assert_eq!(
            strip_code_fence("  Альбедо — это отражение.  "),
            "Альбедо — это отражение."
        );
        assert_eq!(
            strip_code_fence("```"),
            "```",
            "обрывок разметки не должен съедать ответ"
        );
    }

    #[test]
    fn foreign_script_is_noticed_only_when_it_is_there() {
        assert!(has_foreign_script("«Failed» означает 失败 в этом контексте"));
        assert!(has_foreign_script("Ответ готов。"));
        assert!(!has_foreign_script("«Failed» означает неудачу или сбой."));
        assert!(
            !has_foreign_script("Throttling — это ограничение скорости"),
            "латиница в термине — не повод переспрашивать"
        );
    }

    #[test]
    fn empty_content_does_not_shadow_later_fields() {
        let value = serde_json::json!({ "choices": [{ "message": { "content": "   " } }] });
        assert!(extract_text(&value).is_none(), "пробелы — это не ответ");
    }

    #[test]
    fn normalize_reads_json_answer() {
        let inner = r#"{"def":"опр","simple":"проще","examples":["a","b","c","d"]}"#;
        let value = serde_json::json!({ "choices": [{ "message": { "content": inner } }] });
        let parsed = normalize_explanation(&value).unwrap();
        assert_eq!(parsed.simple, "проще");
        assert_eq!(parsed.examples.len(), 3, "примеров берём не больше трёх");
    }

    #[test]
    fn thinking_never_reaches_the_window() {
        assert_eq!(
            strip_reasoning("<think>надо ответить коротко</think>Альбедо — доля отражённого света."),
            "Альбедо — доля отражённого света."
        );
        assert_eq!(
            strip_reasoning("<|channel|>analysis<|message|>думаю<|channel|>final<|message|>Ответ."),
            "Ответ."
        );
        // Обычный текст не трогаем.
        assert_eq!(strip_reasoning("  Просто ответ.  "), "Просто ответ.");
    }

    #[test]
    fn a_draft_is_not_an_answer() {
        // Ровно то, что пришло из OpenRouter вживую.
        assert!(is_deliberation(
            "We have a conversation. The user asks: «Эта модель бесплатная?»              The system instructions: You are ChatGPT"
        ));
        assert!(!is_deliberation("Альбедо — доля света, которую отражает поверхность."));
        // Английский ответ по существу размышлением не считается.
        assert!(!is_deliberation("Albedo is the share of light a surface reflects."));
    }

    #[test]
    fn empty_answer_is_parse_error() {
        let value = serde_json::json!({ "choices": [{ "message": { "content": "{}" } }] });
        assert!(matches!(normalize_explanation(&value), Err(AiError::Parse)));
    }

    #[test]
    fn wikipedia_picks_language_by_script() {
        assert_eq!(WikipediaProvider::host("альбедо"), "ru.wikipedia.org");
        assert_eq!(WikipediaProvider::host("albedo"), "en.wikipedia.org");
    }

    #[test]
    fn foreign_tail_is_cut_off() {
        // Ровно то, на что жаловались: хвост из иероглифов в конце ответа.
        assert_eq!(
            strip_foreign("Альбедо — доля отражённого света. 这是一个解释"),
            "Альбедо — доля отражённого света."
        );
        // Висящие знаки после вырезанного тоже убираем.
        assert_eq!(strip_foreign("Это ответ — 说明"), "Это ответ");
        // Латиница и кириллица неприкосновенны: термин может быть любым.
        assert_eq!(
            strip_foreign("TCP — protocol передачи данных"),
            "TCP — protocol передачи данных"
        );
    }

    #[test]
    fn nothing_left_is_an_error_not_an_empty_answer() {
        assert!(purge("这是一个解释").is_err());
        assert!(purge("  ").is_err());
        assert!(purge("Нормальный ответ").is_ok());
    }

    #[test]
    fn url_encoding_survives_cyrillic_and_spaces() {
        assert_eq!(urlencoding("albedo"), "albedo");
        assert_eq!(urlencoding("ледниковый щит"), "%D0%BB%D0%B5%D0%B4%D0%BD%D0%B8%D0%BA%D0%BE%D0%B2%D1%8B%D0%B9_%D1%89%D0%B8%D1%82");
    }

    #[test]
    fn takes_first_sentences_only() {
        let text = "Альбедо — характеристика отражения. Измеряется долей. Третье предложение.";
        assert_eq!(first_sentences(text, 2), "Альбедо — характеристика отражения. Измеряется долей.");
    }

    /// Отказы, которые чинятся не повтором, должны объяснять себя словами.
    ///
    /// Числа кода человеку не говорят ничего, а починка у них разная: 402 —
    /// пополнить счёт или уйти на свою модель, 401 — проверить ключ, 429 —
    /// подождать. Один и тот же «сбой сети» на всё это уводил бы в сторону.
    #[test]
    fn money_and_key_failures_explain_themselves() {
        let default = "Сбой сети — нет ответа";

        let out_of_money = AiError::Http(402).user_text(default);
        assert!(out_of_money.contains("деньги"), "{out_of_money}");
        assert!(
            out_of_money.contains("на этом компьютере"),
            "надо подсказать выход: своя модель"
        );

        assert!(AiError::Http(401).user_text(default).contains("ключ"));
        assert!(AiError::Http(403).user_text(default).contains("ключ"));
        assert!(AiError::Http(429).user_text(default).contains("частоту"));

        // Сеть и таймаут остаются обычным текстом: там и правда нечего добавить.
        assert_eq!(AiError::Network.user_text(default), default);

        // И ни один из них не повторяется: повтор не добавит денег и не
        // исправит ключ.
        assert!(!AiError::Http(402).retryable());
        assert!(!AiError::Http(403).retryable());
    }

    #[test]
    fn retry_policy_covers_only_transient_failures() {
        assert!(AiError::Timeout.retryable());
        assert!(AiError::Http(503).retryable());
        assert!(!AiError::Http(401).retryable());
        assert!(!AiError::Parse.retryable());
        // 429 раньше повторяли, теперь нет: ограничение частоты снимается через
        // минуты или сутки, так что вторая попытка упирается в тот же отказ
        // и лишь вдвое быстрее сжигает дневную квоту.
        assert!(!AiError::Http(429).retryable());
    }
}
