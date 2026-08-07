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
    fn retryable(&self) -> bool {
        match self {
            AiError::Network | AiError::Timeout => true,
            AiError::Http(status) => *status == 408 || *status == 429 || *status >= 500,
            _ => false,
        }
    }

    /// Текст, который увидит пользователь в состоянии Error.
    pub fn user_text(&self, default_text: &str) -> String {
        match self {
            AiError::Network | AiError::Timeout => default_text.to_string(),
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

/// Собирает провайдера по конфигурации. Неизвестное имя — это mock, а не паника:
/// приложение уже запущено, ронять его из-за опечатки в конфиге нельзя.
pub fn build_provider(config: &AiConfig) -> Box<dyn AiProvider> {
    match config.provider.as_str() {
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
        _question: &str,
    ) -> Result<String, AiError> {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let data = mock_lookup(term);
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
        Ok(variants[thread.len() % variants.len()].clone())
    }
}

/* ─────────────────────────────── HTTP ─────────────────────────────────── */

const SYSTEM_PROMPT: &str = concat!(
    "Ты объясняешь выделенный пользователем термин. Отвечай по-русски, по сути, без служебных ",
    "фраз вроде «это фрагмент из абзаца». Верни строгий JSON: ",
    r#"{"def": "одно-два предложения", "simple": "то же максимально просто", "examples": ["2–3 коротких примера"]}."#
);

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
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
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
        let body = serde_json::json!({
            "model": (!self.model.is_empty()).then(|| self.model.clone()),
            "messages": messages,
        });

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
        let response = request.send().await.map_err(|err| {
            if err.is_timeout() {
                AiError::Timeout
            } else {
                AiError::Network
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AiError::Http(status.as_u16()));
        }
        response.json().await.map_err(|_| AiError::Parse)
    }
}

/// Вытаскивает текст ответа из типовых обёрток chat-completions.
fn extract_text(value: &serde_json::Value) -> Option<String> {
    value
        .pointer("/choices/0/message/content")
        .or_else(|| value.pointer("/content/0/text"))
        .or_else(|| value.pointer("/text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Ответу модели не доверяем: приводим к контракту {def, simple, examples}.
fn normalize_explanation(value: &serde_json::Value) -> Result<Explanation, AiError> {
    let candidate = match extract_text(value) {
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
                     Отвечай коротко, обычным текстом, без JSON."
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
        extract_text(&value).ok_or(AiError::Parse)
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
    fn retry_policy_covers_only_transient_failures() {
        assert!(AiError::Timeout.retryable());
        assert!(AiError::Http(503).retryable());
        assert!(AiError::Http(429).retryable());
        assert!(!AiError::Http(401).retryable());
        assert!(!AiError::Parse.retryable());
    }
}
