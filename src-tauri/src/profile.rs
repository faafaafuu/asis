//! Долгая память о человеке: то, что он просил запомнить.
//!
//! Факты лежат в `memory.md` в папке данных Ноа, по одному на строку
//! («- живу в Казани»). Файл можно открыть и поправить руками. Все факты
//! подмешиваются к каждому вопросу модели, поэтому «закажи домой» и «какая
//! погода у меня» понятны без повторного объяснения.
//!
//! Запомнить, перечислить и забыть можно голосом, без модели: «запомни, что
//! я живу в Казани», «что ты обо мне знаешь», «забудь про Казань».

use std::path::PathBuf;

/// Больше фактов в подсказку не кладём: длинный список модель читает хуже.
const MAX_FACTS: usize = 60;

fn path() -> Option<PathBuf> {
    crate::module_kit::data_dir().map(|dir| dir.join("memory.md"))
}

/// Все запомненные факты по порядку.
pub fn facts() -> Vec<String> {
    let Some(path) = path() else { return Vec::new() };
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|fact| fact.trim().to_string())
        .filter(|fact| !fact.is_empty())
        .collect()
}

fn save(facts: &[String]) -> Result<(), String> {
    let path = path().ok_or("папка данных не найдена")?;
    let mut text = String::from("# Что Ноа знает о человеке\n\n");
    for fact in facts {
        text.push_str(&format!("- {fact}\n"));
    }
    std::fs::write(&path, text).map_err(|err| format!("память не сохранилась: {err}"))
}

/// Строка для подсказки модели. Пустая, если ничего не запомнено.
pub fn prompt_line() -> String {
    let facts = facts();
    if facts.is_empty() {
        return String::new();
    }
    let start = facts.len().saturating_sub(MAX_FACTS);
    format!(
        "Что ты знаешь о человеке — он сам просил это запомнить; учитывай, когда \
         это к месту, и не пересказывай без повода: {}.",
        facts[start..].join("; ")
    )
}

/// Ответ на просьбу о памяти, если это она.
pub fn voice(said: &str) -> Option<String> {
    let lower = said.trim().to_lowercase().replace('ё', "е");
    let clean = |text: &str| {
        text.trim()
            .trim_start_matches([',', ':', ' '])
            .trim_start_matches("что ")
            .trim_start_matches("про ")
            .trim_start_matches("о ")
            .trim_end_matches(['.', '!', ' '])
            .trim()
            .to_string()
    };

    if [
        "что ты обо мне знаешь",
        "что ты про меня знаешь",
        "что ты обо мне помнишь",
        "что ты про меня помнишь",
        "что ты запомнил",
        "что ты запомнила",
        "что у тебя в памяти",
    ]
    .iter()
    .any(|ask| lower.contains(ask))
    {
        let facts = facts();
        return Some(if facts.is_empty() {
            "Пока ничего. Скажи «запомни, что…» — и запомню.".into()
        } else {
            format!("Помню: {}.", facts.join("; "))
        });
    }

    if let Some(rest) = ["запомни", "запомните"]
        .iter()
        .find_map(|verb| lower.strip_prefix(verb))
    {
        let fact = clean(rest);
        if fact.chars().count() < 3 {
            return Some("Что запомнить?".into());
        }
        let mut facts = facts();
        if !facts.iter().any(|known| known.to_lowercase() == fact) {
            facts.push(fact.clone());
        }
        return Some(match save(&facts) {
            Ok(()) => format!("Запомнила: {fact}."),
            Err(err) => format!("Не запомнила: {err}."),
        });
    }

    if let Some(rest) = ["забудь", "забудьте"]
        .iter()
        .find_map(|verb| lower.strip_prefix(verb))
    {
        let what = clean(rest);
        if what.chars().count() < 3 {
            return None;
        }
        if ["все", "всё", "все обо мне", "всё обо мне"].contains(&what.as_str()) {
            return Some(match save(&[]) {
                Ok(()) => "Всё забыла.".into(),
                Err(err) => format!("Не вышло: {err}."),
            });
        }
        let facts = facts();
        // «Забудь про Казань» должно найти «живу в Казани»: сравниваем по
        // началу слов, без окончаний.
        let stems: Vec<String> = what
            .split_whitespace()
            .filter(|w| w.chars().count() > 2)
            .map(|w| w.chars().take(5).collect())
            .collect();
        let (gone, kept): (Vec<String>, Vec<String>) = facts.into_iter().partition(|fact| {
            let fact = fact.to_lowercase().replace('ё', "е");
            fact.contains(&what) || (!stems.is_empty() && stems.iter().all(|stem| fact.contains(stem.as_str())))
        });
        if gone.is_empty() {
            return Some(format!("Про «{what}» ничего не помню."));
        }
        return Some(match save(&kept) {
            Ok(()) => format!("Забыла: {}.", gone.join("; ")),
            Err(err) => format!("Не вышло: {err}."),
        });
    }
    None
}
