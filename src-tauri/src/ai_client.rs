//! AI-клиент бэкенда (SPEC §10).
//!
//! Сеть живёт здесь, а не в webview: CSP окна попапа запрещает внешние запросы,
//! и ключ API не должен попадать во фронтенд вообще.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::AiConfig;

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
            AiError::Http(status) => *status == 408 || *status >= 500,
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
pub fn build_provider(config: &AiConfig, language: &str) -> Box<dyn AiProvider> {
    match config.provider.as_str() {
        "wikipedia" => match WikipediaProvider::new(config) {
            Ok(provider) => Box::new(provider),
            Err(err) => {
                log::error!("провайдер Википедии не собрался ({err}) — работаем на mock");
                Box::new(MockProvider::default())
            }
        },
        "http" => match HttpProvider::new(config, language) {
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
        // ответы не связаны с его вопросом, и как включить настоящие (SPEC §10).
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
    "обычным текстом, по-русски — даже если сам термин на другом языке. Только объяснение: ",
    "без вступлений, без списков, без разметки и без JSON."
);

const SYSTEM_PROMPT_EN: &str = concat!(
    "You explain a term the user selected. Answer in one or two plain-text sentences ",
    "in English, even if the term itself is in another language. The explanation only: ",
    "no preamble, no lists, no markup, no JSON."
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
/// Без него модель пишет, пока не выговорится. В попапе это лишние секунды
/// ожидания ради текста, который туда всё равно не поместится, а мелкие модели
/// на длинной дистанции ещё и уходят в повторы и бессвязицу.
const ANSWER_LIMIT: u32 = 220;

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
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

impl HttpProvider {
    pub fn new(config: &AiConfig, language: &str) -> Result<Self, AiError> {
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
        })
    }

    /// Один заход за ответом на вопрос: отправить и достать текст.
    async fn answer_once(&self, messages: Vec<Message<'_>>) -> Result<String, AiError> {
        let value = self.send(messages).await?;
        let text = extract_text(&value)
            .map(|text| strip_reasoning(&text))
            .map(|text| strip_code_fence(&text).to_string())
            .ok_or(AiError::Parse)?;

        if is_deliberation(&text) {
            log::warn!("модель прислала рассуждение вместо ответа — считаем это отказом");
            return Err(AiError::Parse);
        }
        Ok(text)
    }

    async fn send(&self, messages: Vec<Message<'_>>) -> Result<serde_json::Value, AiError> {
        let mut body = serde_json::json!({
            "model": (!self.model.is_empty()).then(|| self.model.clone()),
            "messages": messages,
            // Ollama по умолчанию отвечает потоком построчного JSON — разобрать его
            // как один объект нельзя. Для OpenAI-совместимых API поле безвредно.
            "stream": false,
            "max_tokens": ANSWER_LIMIT,
            "temperature": TEMPERATURE,
        });

        // Родной API Ollama живёт по своим именам: max_tokens он не знает, зато
        // понимает options.num_predict. Отправляем эти поля только ему — чужим
        // сервисам лишние ключи ни к чему.
        if self.endpoint.contains("/api/chat") {
            body["options"] = serde_json::json!({
                "num_predict": ANSWER_LIMIT,
                "temperature": TEMPERATURE,
            });
            // Иначе Ollama выгружает модель из памяти после пяти минут простоя,
            // и первое же выделение после паузы ждёт её загрузки — десятки секунд.
            // Человек видит «нет ответа» ровно там, где программа работает
            // правильно. Срок один на всё приложение: тем же значением модель
            // прогревается при запуске (см. ollama::preload).
            body["keep_alive"] = serde_json::json!(crate::ollama::KEEP_ALIVE);
        }

        // OpenRouter умеет не присылать размышление — просим его об этом.
        //
        // Дело не в лишнем трафике, а в том, куда уходит лимит ответа. Модель
        // сначала думает вслух, и думает много; лимит у нас короткий, потому что
        // объяснение должно быть в два-три предложения. Размышление съедает его
        // целиком, и на сам ответ не остаётся ничего — сервис возвращает пустой
        // текст. Поле нестандартное, поэтому отправляем его только тем, кто его
        // понимает: чужие API на неизвестный ключ отвечают ошибкой.
        if self.endpoint.contains("openrouter.ai") {
            body["reasoning"] = serde_json::json!({ "exclude": true });
        }

        let mut last = AiError::Network;
        for attempt in 0..=self.retries {
            match self.send_once(&body).await {
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
        let mut request = self.client.post(&self.endpoint).json(body);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        log::info!("запрос к модели: {}", self.endpoint);
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
            return Err(AiError::Http(status.as_u16()));
        }
        response.json().await.map_err(|_| AiError::Parse)
    }
}

/// Вычищает чужие письменности и следит, чтобы что-то осталось.
///
/// Пустой результат — это не «ответ без иероглифов», а «ответа не было».
/// Показать в окне пустоту хуже, чем честную ошибку: человек будет ждать.
fn purge(text: &str) -> Result<String, AiError> {
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

fn has_foreign_script(text: &str) -> bool {
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
    async fn interpret(&self, rules: &str, said: &str) -> Result<String, AiError> {
        self.answer_once(vec![
            Message {
                role: "system",
                content: rules.to_string(),
            },
            Message {
                role: "user",
                content: said.to_string(),
            },
        ])
        .await
    }

    async fn explain(&self, term: &str, context: &str) -> Result<Explanation, AiError> {
        let messages = || {
            vec![
                Message {
                    role: "system",
                    content: system_prompt(&self.language).to_string(),
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
                    content: match self.language.as_str() {
                        "en" if term.trim().is_empty() => format!(
                            "Your name is Noa, you are a voice assistant. Answer the question \
                             itself, briefly — two or three sentences, like in conversation. \
                             If it calls for an estimate, estimate and name a number. \
                             Do not restate the question. Plain text, no lists, in English."
                        ),
                        "en" => format!(
                            "Your name is Noa. The user is asking a follow-up about the term \
                             “{term}”. Answer briefly, in plain text, no JSON. \
                             Answer in English, even if the term itself is in another language."
                        ),
                        // Пустой термин означает вопрос с чистого места: его
                        // задали голосом, ничего не выделяя. Отвечать на такой
                        // определением — не то, что просили: на «сколько раз
                        // отжаться, чтобы устать» ждут прикидку, а не толкование
                        // самого вопроса.
                        _ if term.trim().is_empty() => format!(
                            "Тебя зовут Ноа, ты голосовой помощник. Отвечай на вопрос по \
                             существу и коротко — двумя-тремя предложениями, как в разговоре. \
                             Если вопрос требует прикидки, прикидывай и называй число. \
                             Не пересказывай вопрос и не объясняй, что он означает. \
                             Обычный текст, без списков и разметки, по-русски."
                        ),
                        _ => format!(
                            "Тебя зовут Ноа. Пользователь уточняет ранее объяснённый термин \
                             «{term}». Отвечай коротко, обычным текстом, без JSON. \
                             Отвечай по-русски, даже если сам термин на другом языке."
                        ),
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
