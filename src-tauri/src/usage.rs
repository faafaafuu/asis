//! Расход модели — токены и деньги — для виджета на рабочем столе.
//!
//! Токены приходят в каждом ответе модели: у OpenAI-совместимых сервисов —
//! `usage.prompt_tokens` и `usage.completion_tokens`, у Ollama —
//! `prompt_eval_count` и `eval_count`. Стоимость присылает OpenRouter
//! (`usage.cost`), когда её просят в запросе. Остаток на счёте OpenRouter —
//! отдельным запросом раз в несколько минут.
//!
//! Итоги хранятся по дням в `usage.json` рядом с настройками.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Сколько дней помнить.
const KEEP_DAYS: usize = 400;
/// Как часто спрашивать остаток на счёте и лимиты подписки.
const BALANCE_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tally {
    pub requests: u64,
    pub prompt: u64,
    pub completion: u64,
    /// Доллары.
    pub cost: f64,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.requests += other.requests;
        self.prompt += other.prompt;
        self.completion += other.completion;
        self.cost += other.cost;
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    days: BTreeMap<String, Tally>,
    /// Модель, которая отвечала последней.
    ///
    /// В настройках может стоять «claude» или адрес моста, а отвечает
    /// `claude-opus-5`: имя приходит в ответе. Человек, который смотрит на
    /// виджет расхода, спрашивает «какая модель сейчас стоит» — и ответом
    /// должна быть та, что тратит его деньги, а не запись в поле.
    #[serde(default)]
    model: String,
}

static STORE: Mutex<Option<(PathBuf, Store)>> = Mutex::new(None);
/// Счёт OpenRouter: остаток и сколько потрачено за всё время — долларами.
static ACCOUNT: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// Лимиты того, чем сейчас отвечает Ноа: подписка через мост или бесплатные
/// модели OpenRouter.
static LIMITS: Mutex<Vec<Limit>> = Mutex::new(Vec::new());

/// Один лимит: «5 ч — 66%», «сегодня — 12 из 50».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    /// Какое окно: «5 ч», «неделя», «месяц», «сегодня».
    pub name: String,
    /// Сколько израсходовано, в процентах.
    pub used: f64,
    /// Сколько осталось — словами для подсказки.
    pub left: String,
    /// Когда обнулится, если известно: RFC 3339.
    pub resets: Option<String>,
}

pub fn load(dir: PathBuf) {
    let path = dir.join("usage.json");
    let store = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    *STORE.lock().unwrap_or_else(|err| err.into_inner()) = Some((path, store));
}

/// Расход одного ответа. `None` — сервис его не сообщил.
fn tally(value: &serde_json::Value) -> Option<Tally> {
    let usage = &value["usage"];
    let prompt = usage["prompt_tokens"]
        .as_u64()
        .or_else(|| value["prompt_eval_count"].as_u64())
        .unwrap_or(0);
    let completion = usage["completion_tokens"]
        .as_u64()
        .or_else(|| value["eval_count"].as_u64())
        .unwrap_or(0);
    if prompt == 0 && completion == 0 {
        return None;
    }
    Some(Tally {
        requests: 1,
        prompt,
        completion,
        cost: usage["cost"].as_f64().unwrap_or(0.0),
    })
}

/// Прибавляет расход ответа модели к сегодняшнему.
pub fn record(value: &serde_json::Value) {
    let Some(add) = tally(value) else { return };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
    let Some((path, store)) = guard.as_mut() else { return };
    if let Some(model) = value["model"].as_str().filter(|name| !name.trim().is_empty()) {
        store.model = model.to_string();
    }
    store.days.entry(today).or_default().add(&add);
    while store.days.len() > KEEP_DAYS {
        let oldest = store.days.keys().next().cloned();
        if let Some(oldest) = oldest {
            store.days.remove(&oldest);
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(path, text);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub today: Tally,
    pub month: Tally,
    /// За всё время, что ведётся учёт.
    pub total: Tally,
    /// Остаток на счёте OpenRouter, доллары. `None` — не OpenRouter или не узнали.
    pub balance: Option<f64>,
    /// Сколько потрачено на счёте OpenRouter за всё время — всеми программами.
    pub spent: Option<f64>,
    pub model: String,
    /// Лимиты подписки или бесплатного тарифа: сколько использовано и осталось.
    pub limits: Vec<Limit>,
    /// Облачная ли модель: у своей деньги не считаются.
    pub cloud: bool,
    pub service: String,
}

pub fn summary(app: &AppHandle) -> Summary {
    let (model, endpoint) = {
        let state = app.state::<crate::state::AppState>();
        let config = state.config();
        (config.ai.model.clone(), config.ai.endpoint.clone())
    };
    let now = chrono::Local::now();
    let today_key = now.format("%Y-%m-%d").to_string();
    let month_key = now.format("%Y-%m").to_string();
    let (today, month, total, answered) = {
        let guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
        let mut month = Tally::default();
        let mut today = Tally::default();
        let mut total = Tally::default();
        if let Some((_, store)) = guard.as_ref() {
            for (day, tally) in &store.days {
                total.add(tally);
                if day.starts_with(&month_key) {
                    month.add(tally);
                }
                if *day == today_key {
                    today = *tally;
                }
            }
        }
        let answered = guard.as_ref().map(|(_, store)| store.model.clone()).unwrap_or_default();
        (today, month, total, answered)
    };
    let account = *ACCOUNT.lock().unwrap_or_else(|err| err.into_inner());
    Summary {
        today,
        month,
        total,
        balance: account.map(|(balance, _)| balance),
        spent: account.map(|(_, spent)| spent),
        // Имя из ответа вернее записи в настройках: через мост в поле стоит
        // одно, а отвечает то, что выбрано на той стороне.
        model: if answered.trim().is_empty() { model } else { answered },
        limits: LIMITS.lock().unwrap_or_else(|err| err.into_inner()).clone(),
        cloud: !crate::config::is_local(&endpoint),
        service: service_name(&endpoint).into(),
    }
}

fn service_name(endpoint: &str) -> &'static str {
    if endpoint.contains("8791") || endpoint.contains("127.0.0.2") {
        "мост"
    } else if endpoint.contains("openrouter.ai") {
        "OpenRouter"
    } else if endpoint.contains("googleapis.com") {
        "Google"
    } else if endpoint.contains("groq.com") {
        "Groq"
    } else if crate::config::is_local(endpoint) {
        "Ollama"
    } else {
        "облако"
    }
}

/// Счёт OpenRouter: остаток (купленное минус потраченное) и потраченное.
async fn openrouter_account(key: &str, proxy: &str) -> Option<(f64, f64)> {
    let mut builder = crate::net::client_builder().timeout(Duration::from_secs(15));
    if !proxy.trim().is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(proxy.trim()) {
            builder = builder.proxy(proxy);
        }
    }
    let body: serde_json::Value = builder
        .build()
        .ok()?
        .get("https://openrouter.ai/api/v1/credits")
        .bearer_auth(key)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let data = &body["data"];
    let credits = data["total_credits"].as_f64()?;
    let spent = data["total_usage"].as_f64()?;
    Some((credits - spent, spent))
}

/// Лимиты подписки через мост: мост сам спрашивает Claude или Codex.
async fn bridge_limits(endpoint: &str, key: &str, model: &str) -> Vec<Limit> {
    let base = endpoint.trim_end_matches('/').trim_end_matches("/chat/completions").trim_end_matches("/v1");
    let Ok(client) = crate::net::client_builder().timeout(Duration::from_secs(20)).build() else { return Vec::new() };
    let Ok(response) = client
        .get(format!("{base}/v1/limits?model={}", model.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != ':' && c != '.', "")))
        .bearer_auth(key)
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(body) = response.json::<serde_json::Value>().await else { return Vec::new() };
    body["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|window| {
            let used: f64 = window["used"].as_f64()?;
            Some(Limit {
                name: window["name"].as_str().unwrap_or("лимит").to_string(),
                used,
                left: format!("осталось {}%", (100.0 - used).max(0.0).round()),
                resets: window["resets"].as_str().map(str::to_string),
            })
        })
        .collect()
}

/// Бесплатные модели OpenRouter: дневной лимит запросов — 50, а у тех, кто
/// хоть раз покупал кредиты, — 1000. Сколько сегодня потрачено, считаем сами.
async fn openrouter_free_limit(key: &str, proxy: &str) -> Vec<Limit> {
    let mut builder = crate::net::client_builder().timeout(Duration::from_secs(15));
    if !proxy.trim().is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(proxy.trim()) {
            builder = builder.proxy(proxy);
        }
    }
    let Some(client) = builder.build().ok() else { return Vec::new() };
    let body: Option<serde_json::Value> = async {
        client.get("https://openrouter.ai/api/v1/key").bearer_auth(key).send().await.ok()?.json().await.ok()
    }
    .await;
    let Some(body) = body else { return Vec::new() };
    let per_day = if body["data"]["is_free_tier"].as_bool().unwrap_or(true) { 50.0 } else { 1000.0 };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let used = STORE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .and_then(|(_, store)| store.days.get(&today).map(|tally| tally.requests as f64))
        .unwrap_or(0.0);
    vec![Limit {
        name: "сегодня".into(),
        used: (used / per_day * 100.0).min(100.0),
        left: format!("бесплатных запросов {used:.0} из {per_day:.0}"),
        resets: None,
    }]
}

/// Следит за остатком на счёте, пока программа работает.
pub fn watch(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("sufler-usage".into())
        .spawn(move || loop {
            let (endpoint, key, proxy, model) = {
                let state = app.state::<crate::state::AppState>();
                let config = state.config();
                (config.ai.endpoint.clone(), config.ai.api_key.clone(), config.ai.proxy.clone(), config.ai.model.clone())
            };
            let openrouter = endpoint.contains("openrouter.ai") && !key.is_empty();
            let account = if openrouter {
                tauri::async_runtime::block_on(openrouter_account(&key, &proxy))
            } else {
                None
            };
            *ACCOUNT.lock().unwrap_or_else(|err| err.into_inner()) = account;
            let limits = if service_name(&endpoint) == "мост" && !key.is_empty() {
                tauri::async_runtime::block_on(bridge_limits(&endpoint, &key, &model))
            } else if openrouter && model.ends_with(":free") {
                tauri::async_runtime::block_on(openrouter_free_limit(&key, &proxy))
            } else {
                Vec::new()
            };
            *LIMITS.lock().unwrap_or_else(|err| err.into_inner()) = limits;
            std::thread::sleep(BALANCE_EVERY);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_style_usage_is_read() {
        let value = serde_json::json!({
            "usage": { "prompt_tokens": 1200, "completion_tokens": 80, "cost": 0.00042 }
        });
        assert_eq!(
            tally(&value),
            Some(Tally { requests: 1, prompt: 1200, completion: 80, cost: 0.00042 })
        );
    }

    #[test]
    fn ollama_counts_are_read() {
        let value = serde_json::json!({ "prompt_eval_count": 900, "eval_count": 40 });
        let got = tally(&value).unwrap();
        assert_eq!((got.prompt, got.completion, got.cost), (900, 40, 0.0));
    }

    #[test]
    fn answer_without_usage_is_skipped() {
        assert_eq!(tally(&serde_json::json!({ "choices": [] })), None);
    }
}
