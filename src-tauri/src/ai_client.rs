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
pub fn build_provider(config: &AiConfig) -> Box<dyn AiProvider> {
    match config.provider.as_str() {
        "wikipedia" => match WikipediaProvider::new(config) {
            Ok(provider) => Box::new(provider),
            Err(err) => {
                log::error!("провайдер Википедии не собрался ({err}) — работаем на mock");
                Box::new(MockProvider::default())
            }
        },
        "http" => match HttpProvider::new(config) {
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

const SYSTEM_PROMPT: &str = concat!(
    "Ты объясняешь термин, который выделил пользователь. Ответь одним-двумя предложениями ",
    "обычным текстом, по-русски — даже если сам термин на другом языке. Только объяснение: ",
    "без вступлений, без списков, без разметки и без JSON."
);

/// Потолок длины ответа.
///
/// Без него модель пишет, пока не выговорится. В попапе это лишние секунды
/// ожидания ради текста, который туда всё равно не поместится, а мелкие модели
/// на длинной дистанции ещё и уходят в повторы и бессвязицу.
const ANSWER_LIMIT: u32 = 220;

/// Заготовка реального провайдера: нейтральный chat-подобный формат.
/// Под конкретный API подгоняется правкой `request_body`/`extract_text`.
pub struct HttpProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    retries: u32,
    retry_backoff_ms: u64,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

impl HttpProvider {
    pub fn new(config: &AiConfig) -> Result<Self, AiError> {
        if config.endpoint.is_empty() {
            return Err(AiError::Config("не задан endpoint AI-провайдера".into()));
        }
        let client = with_proxy(
            reqwest::Client::builder().timeout(Duration::from_millis(config.timeout_ms)),
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
        })
    }

    async fn send(&self, messages: Vec<Message<'_>>) -> Result<serde_json::Value, AiError> {
        let mut body = serde_json::json!({
            "model": (!self.model.is_empty()).then(|| self.model.clone()),
            "messages": messages,
            // Ollama по умолчанию отвечает потоком построчного JSON — разобрать его
            // как один объект нельзя. Для OpenAI-совместимых API поле безвредно.
            "stream": false,
            "max_tokens": ANSWER_LIMIT,
        });

        // Родной API Ollama живёт по своим именам: max_tokens он не знает, зато
        // понимает options.num_predict. Отправляем эти поля только ему — чужим
        // сервисам лишние ключи ни к чему.
        if self.endpoint.contains("/api/chat") {
            body["options"] = serde_json::json!({ "num_predict": ANSWER_LIMIT });
            // Иначе Ollama выгружает модель из памяти после пяти минут простоя,
            // и первое же выделение после паузы ждёт её загрузки — десятки секунд
            // против двенадцати, которые терпит таймаут. Человек видит «нет
            // ответа» ровно там, где программа работает правильно.
            body["keep_alive"] = serde_json::json!("30m");
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

#[async_trait]
impl AiProvider for HttpProvider {
    async fn explain(&self, term: &str, context: &str) -> Result<Explanation, AiError> {
        let value = self
            .send(vec![
                Message {
                    role: "system",
                    content: SYSTEM_PROMPT.to_string(),
                },
                Message {
                    role: "user",
                    content: format!("Термин: «{term}».\nКонтекст: {context}"),
                },
            ])
            .await?;
        normalize_explanation(&value)
    }

    async fn ask(
        &self,
        term: &str,
        context: &str,
        thread: &[ThreadItem],
        question: &str,
    ) -> Result<String, AiError> {
        let mut messages = vec![
            Message {
                role: "system",
                content: format!(
                    "Пользователь уточняет ранее объяснённый термин «{term}». \
                     Отвечай коротко, обычным текстом, без JSON. \
                     Отвечай по-русски, даже если сам термин на другом языке."
                ),
            },
            Message {
                role: "user",
                content: format!("Исходный контекст: {context}"),
            },
        ];
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

        let value = self.send(messages).await?;
        extract_text(&value)
            .map(|text| strip_code_fence(&text).to_string())
            .ok_or(AiError::Parse)
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
            reqwest::Client::builder()
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
