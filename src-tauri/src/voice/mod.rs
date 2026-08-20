//! Голос: озвучивание объяснений.
//!
//! Два способа сказать одно и то же, как и с самими объяснениями:
//!
//!   `piper` — на этом же компьютере. Ничего не уходит наружу, работает без
//!     интернета. Голос живой, но слышно, что синтезированный.
//!   `edge`  — нейроголоса Microsoft. Звучат почти неотличимо от человека, но
//!     это сеть, чужой сервер и недокументированный протокол (см. edge.rs).
//!
//! Умолчание — `piper`, по той же причине, по какой объяснения по умолчанию даёт
//! своя модель: работает всегда и ни от кого не зависит.

pub mod assets;
mod audio;
pub mod hotkey;
mod edge;
mod piper;
pub mod stt;
pub mod whisper;

use tauri::AppHandle;

use crate::config::VoiceConfig;

/// Говорит текст выбранным способом. Возвращается сразу, не дожидаясь конца речи.
pub async fn speak(app: &AppHandle, config: &VoiceConfig, text: &str) -> Result<(), String> {
    let text = clean(text);
    if text.is_empty() {
        return Ok(());
    }

    match config.engine.as_str() {
        "edge" => edge::speak(&config.edge_voice, &text).await,
        _ => piper::speak(app, &config.voice, config.rate, &text),
    }
}

/// Список онлайн-голосов. Через обёртку: сам модуль edge закрытый, наружу
/// торчит только то, что нужно окну настройки.
pub fn edge_voices() -> &'static [(&'static str, &'static str)] {
    edge::VOICES
}

/// Идёт ли сейчас речь из колонок.
pub fn speaking() -> bool {
    audio::speaking()
}

/// Замолчать: и звук, и работу, которая его готовит.
pub fn stop() {
    piper::stop();
    edge::stop();
}

/// Готовит текст к произнесению.
///
/// Модель отвечает текстом для глаз: там встречаются и списки, и кавычки-ёлочки,
/// и длинные тире. Часть этого синтезатор проговаривает буквально («тире»,
/// «звёздочка»), часть просто спотыкает интонацию.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = true;

    for ch in text.chars() {
        let ch = match ch {
            // Маркеры списков и оформление — на паузу: перечисление должно
            // звучать перечислением, а не сплошной строкой.
            '·' | '•' | '—' | '–' | '*' | '`' => ' ',
            '«' | '»' | '"' => ' ',
            '\n' | '\r' | '\t' => ' ',
            other => other,
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoration_does_not_reach_the_voice() {
        assert_eq!(
            clean("Альбедо — доля отражённого света"),
            "Альбедо доля отражённого света"
        );
        assert_eq!(clean("· первый\n· второй"), "первый второй");
        assert_eq!(clean("он сказал «да»"), "он сказал да");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(clean("   \n\t "), "");
        assert_eq!(clean("— — —"), "");
    }
}
