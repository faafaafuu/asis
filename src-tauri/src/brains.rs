//! Модели, между которыми переключаются голосом.
//!
//! Ноа запоминает каждую модель, на которой работала: облачную с её ключом,
//! мост, свою в Ollama. Голосом: «какая модель сейчас», «какие модели есть»,
//! «переключись на мистраль», «поставь свою модель», «давай облачную».

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::config::{is_local, AiConfig, Config};
use crate::state::AppState;

/// Запомненная модель. Ключ на диске — зашифрованным, как ключ в `ai`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Brain {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub proxy: String,
}

const KEEP: usize = 12;

impl Brain {
    fn of(ai: &AiConfig) -> Self {
        Self {
            endpoint: ai.endpoint.clone(),
            api_key: ai.api_key.clone(),
            model: ai.model.clone(),
            proxy: ai.proxy.clone(),
        }
    }

    fn same(&self, other: &Brain) -> bool {
        self.endpoint.trim_end_matches('/') == other.endpoint.trim_end_matches('/') && self.model == other.model
    }

    fn local(&self) -> bool {
        is_local(&self.endpoint)
    }
}

/// Запоминает текущую модель первой в списке.
pub fn remember(config: &mut Config) {
    if config.ai.provider != "http" || config.ai.model.trim().is_empty() {
        return;
    }
    let current = Brain::of(&config.ai);
    config.brains.retain(|brain| !brain.same(&current));
    config.brains.insert(0, current);
    config.brains.truncate(KEEP);
}

/// Ключ, который Ноа помнит для этого адреса: у каждого сервиса свой.
///
/// Нужен, когда источник меняют в окне, не вводя ключ заново. Без этого
/// оставался ключ прежнего сервиса: после своей модели — пустой, и OpenRouter
/// отвечал «Missing Authentication header»; после моста — чужой.
pub fn key_for(config: &Config, endpoint: &str) -> String {
    let wanted = endpoint.trim().trim_end_matches('/');
    config
        .brains
        .iter()
        .find(|brain| brain.endpoint.trim().trim_end_matches('/') == wanted && !brain.api_key.is_empty())
        .map(|brain| brain.api_key.clone())
        .unwrap_or_default()
}

/// Модель в списке окна: одна строка, выбирается щелчком.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub endpoint: String,
    pub model: String,
    /// «мост», «облако» или «своя».
    pub kind: String,
    pub current: bool,
}

/// Все модели, на которые можно переключиться: запомненные и те, что стоят в
/// Ollama. Имени наизусть помнить не нужно — оно здесь.
pub async fn choices(app: &AppHandle) -> Vec<Choice> {
    let current = Brain::of(&app.state::<AppState>().config().ai);
    candidates(app)
        .await
        .into_iter()
        .map(|brain| Choice {
            current: brain.same(&current),
            kind: kind(&brain).into(),
            endpoint: brain.endpoint,
            model: brain.model,
        })
        .collect()
}

/// Переключает на модель из списка окна.
pub async fn choose(app: &AppHandle, endpoint: &str, model: &str) -> Result<String, String> {
    let wanted = Brain { endpoint: endpoint.into(), model: model.into(), ..Default::default() };
    let brain = candidates(app)
        .await
        .into_iter()
        .find(|brain| brain.same(&wanted))
        .ok_or("Такой модели в списке уже нет.")?;
    Ok(switch(app, brain))
}

/// Как назвать модель вслух: без владельца и служебных хвостов.
pub fn spoken_name(model: &str) -> String {
    let short = model.rsplit('/').next().unwrap_or(model);
    short.trim_end_matches(":free").replace(['-', '_'], " ")
}

fn kind(brain: &Brain) -> &'static str {
    if crate::local_cli::is_cli(&brain.endpoint) {
        "подписка"
    } else if brain.endpoint.contains("127.0.0.2") || brain.model.contains("bridge") {
        "мост"
    } else if brain.local() {
        "своя"
    } else {
        "облако"
    }
}

/// Русские названия моделей — латиницей, как они записаны у сервисов.
const ALIASES: &[(&str, &str)] = &[
    ("мистрал", "mistral"),
    ("квен", "qwen"),
    ("квин", "qwen"),
    ("гемм", "gemma"),
    ("джемм", "gemma"),
    ("гемин", "gemini"),
    ("джемин", "gemini"),
    ("клод", "claude"),
    ("клауд", "claude"),
    ("дипсик", "deepseek"),
    ("дип сик", "deepseek"),
    ("ламу", "llama"),
    ("лама", "llama"),
    ("ламм", "llama"),
    ("гпт", "gpt"),
    ("грок", "grok"),
    ("фи", "phi"),
    ("мост", "bridge"),
    ("кими", "kimi"),
    ("кодекс", "codex"),
    ("чатгпт", "codex"),
    ("чатжпт", "codex"),
    ("опенаи", "codex"),
    ("глм", "glm"),
];

const NUMBERS: &[(&str, &str)] = &[
    ("один", "1"),
    ("два", "2"),
    ("три", "3"),
    ("четыр", "4"),
    ("пять", "5"),
    ("шест", "6"),
    ("сем", "7"),
    ("восем", "8"),
    ("девят", "9"),
];

fn words(said: &str) -> Vec<String> {
    said.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Про модели ли речь и что просят.
enum Ask {
    Which,
    List,
    Switch(Vec<String>),
}

fn parse(said: &str) -> Option<Ask> {
    const NOUNS: &[&str] = &["модел", "мозг", "нейросет", "нейронк", "ллм", "llm"];
    const VERBS: &[&str] = &[
        "переключ", "постав", "смени", "смена", "поменя", "выбер", "включ", "используй", "перейди",
        "давай", "вруби", "верни",
    ];
    let words = words(said);
    let noun = words.iter().position(|w| NOUNS.iter().any(|n| w.starts_with(n)));
    let verb = words.iter().any(|w| VERBS.iter().any(|v| w.starts_with(v)));
    let named = words.iter().any(|w| ALIASES.iter().any(|(ru, _)| w.starts_with(ru)));

    // «Переключись на мистраль» — модель можно и не называть словом «модель».
    if noun.is_none() && !(verb && named && words.iter().any(|w| w.starts_with("переключ"))) {
        return None;
    }
    if !verb {
        let question = words.iter().any(|w| ["какая", "какой", "какую", "что", "чем"].contains(&w.as_str()));
        let plural = words.iter().any(|w| ["какие", "список", "есть", "доступн"].iter().any(|q| w.starts_with(q)));
        return if plural && words.iter().any(|w| w.starts_with("модел")) && !words.iter().any(|w| w == "сейчас") {
            Some(Ask::List)
        } else if question || words.iter().any(|w| w == "сейчас") {
            Some(Ask::Which)
        } else {
            None
        };
    }
    const FILLER: &[&str] = &[
        "ноа", "на", "в", "с", "мне", "пожалуйста", "модель", "модели", "моделью", "мозг", "мозги",
        "нейросеть", "нейронку", "давай", "другую", "эту", "ту", "обратно",
    ];
    let rest: Vec<String> = words
        .into_iter()
        .filter(|w| !VERBS.iter().any(|v| w.starts_with(v)) && !FILLER.contains(&w.as_str()))
        .collect();
    Some(Ask::Switch(rest))
}

/// Насколько модель похожа на названное: сколько слов нашлось в её имени.
fn score(brain: &Brain, wanted: &[String]) -> usize {
    let name = brain.model.to_lowercase();
    wanted
        .iter()
        .map(|word| {
            let latin = ALIASES
                .iter()
                .find(|(ru, _)| word.starts_with(ru))
                .map(|(_, en)| en.to_string())
                .or_else(|| NUMBERS.iter().find(|(ru, _)| word.starts_with(ru)).map(|(_, n)| n.to_string()))
                .unwrap_or_else(|| word.clone());
            usize::from(latin.len() >= 1 && name.contains(&latin))
        })
        .sum()
}

async fn candidates(app: &AppHandle) -> Vec<Brain> {
    let (mut list, base) = {
        let state = app.state::<AppState>();
        let config = state.config();
        (config.brains.clone(), config.ai.clone())
    };
    // Мост умеет несколько подписок (Claude, ChatGPT через Codex, Gemini, Qwen):
    // что у него есть, он сам скажет списком. Вариант «:free» — тот, при
    // котором разбор реплик делает своя модель, а мост только отвечает.
    // Подписки на этом компьютере: Qwen Code, Gemini CLI, Codex, Claude Code —
    // те, что установлены. «:free» — разбор реплик делает своя модель.
    let installed = tauri::async_runtime::spawn_blocking(crate::local_cli::installed).await.unwrap_or_default();
    for (endpoint, _) in installed {
        let id = endpoint.trim_start_matches("cli:");
        let brain = Brain { endpoint: endpoint.clone(), model: format!("{id}-cli:free"), ..Default::default() };
        if !list.iter().any(|known| known.same(&brain)) {
            list.push(brain);
        }
    }
    let bridges: Vec<Brain> = list.iter().filter(|b| kind(b) == "мост").cloned().collect();
    let mut seen = std::collections::HashSet::new();
    for bridge in bridges {
        if !seen.insert(bridge.endpoint.clone()) {
            continue;
        }
        let mut ai = base.clone();
        ai.endpoint = bridge.endpoint.clone();
        ai.api_key = bridge.api_key.clone();
        ai.proxy = bridge.proxy.clone();
        let Ok(models) = crate::ai_client::catalog(&ai).await else { continue };
        for model in models.into_iter().filter(|m| m.id.ends_with(":free")) {
            let brain = Brain { model: model.id, ..bridge.clone() };
            if !list.iter().any(|known| known.same(&brain)) {
                list.push(brain);
            }
        }
    }
    let status = crate::ollama::status(crate::ollama::DEFAULT_HOST).await;
    for model in status.installed.iter().filter(|m| !m.name.contains("embed")) {
        let brain = Brain {
            endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
            model: model.name.clone(),
            ..Default::default()
        };
        if !list.iter().any(|known| known.same(&brain)) {
            list.push(brain);
        }
    }
    list
}

/// Названия моделей и уровни рассуждения, которые понимает мост к Claude Code.
const BRIDGE_WORDS: &[&str] = &[
    "опус", "соннет", "сонет", "хайку", "хаику", "фейбл", "фэйбл", "opus", "sonnet", "haiku", "fable",
    "уровень", "уровня", "effort", "эффорт", "рассужд",
];

/// Уровни рассуждения. Сами по себе слова частые, поэтому считаются только в
/// короткой фразе: «включи medium», «поставь очень высокий».
const EFFORT_WORDS: &[&str] = &[
    "low", "medium", "high", "xhigh", "max", "лоу", "медиум", "хай", "экстра", "extra", "низк", "средн",
    "высок", "максимал", "макс",
];

fn bridge_control(said: &str) -> bool {
    let words = words(said);
    words.iter().any(|w| BRIDGE_WORDS.iter().any(|b| w.starts_with(b)))
        || (words.len() <= 6
            && words.iter().any(|w| EFFORT_WORDS.iter().any(|e| w == e || (e.chars().count() > 3 && w.starts_with(e)))))
}

/// Спрашивает мост напрямую: он сам меняет свою модель и уровень и отвечает,
/// не тратя на это модель.
async fn ask_bridge(app: &AppHandle, said: &str) -> String {
    let provider = app.state::<AppState>().provider();
    match provider.ask("", "", &[], said).await {
        Ok(reply) if !reply.trim().is_empty() => reply.trim().to_string(),
        Ok(_) => "Мост ничего не ответил.".into(),
        Err(err) => format!("Мост не ответил: {err}"),
    }
}

/// Ответ на просьбу про модели, если это она.
pub async fn voice(app: &AppHandle, said: &str) -> Option<String> {
    let current = Brain::of(&app.state::<AppState>().config().ai);
    // Через мост модель и уровень меняет сам мост: «переключи на хайку»,
    // «какой уровень», «поставь medium».
    if kind(&current) == "мост" && bridge_control(said) {
        return Some(ask_bridge(app, said).await);
    }
    let ask = parse(said)?;
    match ask {
        Ask::Which if kind(&current) == "мост" => {
            Some(format!("Работаю через мост. {}", ask_bridge(app, "какая модель сейчас").await))
        }
        Ask::Which => Some(format!(
            "Сейчас отвечает {} — {}.",
            spoken_name(&current.model),
            kind(&current)
        )),
        Ask::List => {
            let list = candidates(app).await;
            let names: Vec<String> = list
                .iter()
                .take(8)
                .map(|b| format!("{} ({})", spoken_name(&b.model), kind(b)))
                .collect();
            Some(format!("Могу переключиться на: {}.", names.join(", ")))
        }
        Ask::Switch(wanted) => {
            let list = candidates(app).await;
            let others: Vec<&Brain> = list.iter().filter(|b| !b.same(&current)).collect();
            let has = |stems: &[&str]| wanted.iter().any(|w| stems.iter().any(|s| w.starts_with(s)));
            let pick = if has(&["локальн", "свою", "своя", "свой", "офлайн", "оллам"]) {
                let preferred = tauri::async_runtime::spawn_blocking(|| crate::ollama::pick(&crate::ollama::hardware()))
                    .await
                    .ok();
                others
                    .iter()
                    .filter(|b| b.local())
                    .max_by_key(|b| preferred.is_some_and(|p| b.model == p))
                    .copied()
            } else if has(&["облач", "облако", "онлайн"]) {
                others.iter().find(|b| !b.local()).copied()
            } else if wanted.is_empty() || has(&["предыдущ", "прошл", "прежн"]) {
                others.first().copied()
            } else {
                others
                    .iter()
                    .map(|b| (score(b, &wanted), *b))
                    .filter(|(s, _)| *s > 0)
                    .max_by_key(|(s, _)| *s)
                    .map(|(_, b)| b)
            };
            let Some(brain) = pick else {
                if list.iter().any(|b| b.same(&current) && score(b, &wanted) > 0) {
                    return Some(format!("Уже работаю на {}.", spoken_name(&current.model)));
                }
                return Some(format!(
                    "Такой модели не знаю. Могу переключиться на: {}.",
                    others.iter().take(6).map(|b| spoken_name(&b.model)).collect::<Vec<_>>().join(", ")
                ));
            };
            Some(switch(app, brain.clone()))
        }
    }
}

fn switch(app: &AppHandle, brain: Brain) -> String {
    let state = app.state::<AppState>();
    {
        let mut config = state.config_mut();
        config.ai.provider = "http".into();
        config.ai.endpoint = brain.endpoint.clone();
        config.ai.model = brain.model.clone();
        config.ai.proxy = brain.proxy.clone();
        if !brain.local() || !brain.api_key.is_empty() {
            config.ai.api_key = brain.api_key.clone();
        }
        remember(&mut config);
    }
    if let Err(err) = crate::commands::persist(app, &state) {
        log::warn!("новая модель не сохранилась: {err}");
    }
    {
        let config = state.config();
        state.rebuild_provider(&config.ai, &config.ui.language, &config.voice.wake_name);
    }
    #[cfg(desktop)]
    crate::wake_local_model(app);
    log::info!("модель переключена голосом: {}", brain.model);
    format!("Переключился на {} — {}.", spoken_name(&brain.model), kind(&brain))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain(endpoint: &str, model: &str) -> Brain {
        Brain { endpoint: endpoint.into(), model: model.into(), ..Default::default() }
    }

    #[test]
    fn phrases_are_recognized() {
        assert!(matches!(parse("какая модель сейчас"), Some(Ask::Which)));
        assert!(matches!(parse("Ноа, какие модели есть"), Some(Ask::List)));
        assert!(matches!(parse("переключись на мистраль"), Some(Ask::Switch(_))));
        assert!(matches!(parse("поставь свою модель"), Some(Ask::Switch(_))));
        assert!(parse("поставь таймер на пять минут").is_none());
        assert!(parse("что такое модель данных").is_none() || matches!(parse("что такое модель данных"), Some(Ask::Which)));
    }

    #[test]
    fn spoken_names_find_models() {
        let mistral = brain("https://openrouter.ai/api/v1/chat/completions", "mistralai/mistral-small-3.2-24b-instruct");
        let qwen = brain("http://127.0.0.1:11434/api/chat", "qwen3.5:9b");
        let Some(Ask::Switch(wanted)) = parse("переключись на мистраль") else { panic!() };
        assert!(score(&mistral, &wanted) > score(&qwen, &wanted));
        let Some(Ask::Switch(wanted)) = parse("поставь модель квен девять") else { panic!() };
        assert_eq!(score(&qwen, &wanted), 2);
    }

    #[test]
    fn remembered_list_keeps_latest_first() {
        let mut config = Config::default();
        config.ai.model = "a".into();
        remember(&mut config);
        config.ai.model = "b".into();
        remember(&mut config);
        config.ai.model = "a".into();
        remember(&mut config);
        assert_eq!(config.brains.iter().map(|b| b.model.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn bridge_phrases_are_detected() {
        assert!(bridge_control("переключи на хайку"));
        assert!(bridge_control("какой уровень сейчас"));
        assert!(bridge_control("включи medium"));
        assert!(bridge_control("поставь очень высокий"));
        assert!(!bridge_control("поставь будильник на семь утра"));
        assert!(!bridge_control("какая погода в Казани"));
    }

    #[test]
    fn names_are_short() {
        assert_eq!(spoken_name("mistralai/mistral-small-3.2-24b-instruct"), "mistral small 3.2 24b instruct");
        assert_eq!(spoken_name("claude-code-bridge:free"), "claude code bridge");
    }
}
