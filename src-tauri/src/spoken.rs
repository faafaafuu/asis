//! Текст для голоса без кода: блок — «код на экране», команда — «команду на
//! экране», путь — последней частью. Общий для голоса на компьютере и для
//! практики, которая есть и на телефоне.

/// Ответ модели пришёл JSON-ом вместо текста — «{ "answer": [ "…" ] }» — и
/// звучал вслух со скобками и кавычками. Отсюда берутся только слова: строки
/// из него через пробел. Не JSON — текст как есть.
pub fn plain(text: &str) -> String {
    let trimmed = text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return text.to_string();
    }
    let Some(value) = crate::practice::lenient_json(trimmed) else {
        return text.to_string();
    };
    fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if !text.trim().is_empty() => out.push(text.trim().to_string()),
            serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
            serde_json::Value::Object(map) => {
                // Сначала то, что предназначено человеку.
                for key in ["say", "text", "answer", "reply", "message", "response_text"] {
                    if let Some(found) = map.get(key) {
                        let before = out.len();
                        strings(found, out);
                        if out.len() > before {
                            return;
                        }
                    }
                }
                map.values().for_each(|item| strings(item, out));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    strings(&value, &mut out);
    if out.is_empty() {
        text.to_string()
    } else {
        out.join(" ")
    }
}

/// Код вслух не читается. Синтезатор проговаривал команды по буквам — «эс-эс-аш
/// минус и» — и пути через «слэш»: на слух это не понять, а на экране они и
/// так есть. Модель просят объяснять словами, что делает команда; это —
/// страховка, если код всё же попал в речь:
/// - блок кода — «код на экране»;
/// - `команда` с пробелами и знаками — «команду на экране»; `nginx`, `kubectl`
///   — просто слово;
/// - путь — последней частью: /etc/nginx/nginx.conf — «nginx.conf»; ссылка — «ссылка»;
/// - выделили и читают сам код — вместо него подсказка про «Объяснить».
pub fn without_code(text: &str) -> String {
    if looks_like_code(text) {
        return "Это код. Чтобы я рассказала, что он делает, нажмите «Объяснить».".into();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("```") {
        out.push_str(&rest[..at]);
        match rest[at + 3..].find("```") {
            Some(end) => {
                out.push_str(" Код — на экране. ");
                rest = &rest[at + 3 + end + 3..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);

    // Встроенный код: `…`.
    let mut spoken = String::with_capacity(out.len());
    let mut parts = out.split('`');
    if let Some(first) = parts.next() {
        spoken.push_str(first);
    }
    let mut inside = true;
    for part in parts {
        if inside {
            let simple = part.chars().count() <= 24 && part.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c));
            spoken.push_str(if simple { part } else { "команду на экране" });
        } else {
            spoken.push_str(part);
        }
        inside = !inside;
    }

    spoken.split(' ').map(spoken_word).collect::<Vec<_>>().join(" ")
}

/// Путь — последней частью, ссылка — словом.
fn spoken_word(word: &str) -> String {
    let core = word.trim_matches(|c: char| ",.;:!?()«»\"'".contains(c));
    if core.starts_with("http://") || core.starts_with("https://") {
        return word.replace(core, "ссылка");
    }
    if core.matches('/').count() >= 2 || (core.starts_with('/') && core.len() > 1) || core.starts_with("~/") {
        let last = core.trim_end_matches('/').rsplit('/').next().unwrap_or(core);
        return word.replace(core, last);
    }
    word.to_string()
}

/// Сам текст — код: много скобок, точек с запятой, присваиваний и отступов.
fn looks_like_code(text: &str) -> bool {
    let total = text.chars().filter(|c| !c.is_whitespace()).count();
    if total < 20 {
        return false;
    }
    let marks = text.chars().filter(|c| "{}();=<>$|&[]\\#".contains(*c)).count();
    let letters_ru = text.chars().filter(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(' '))).count();
    marks * 100 / total >= 8 && letters_ru * 100 / total < 20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_not_read_aloud() {
        assert_eq!(
            without_code("Перезапустите `systemctl restart nginx`, и сервер подхватит конфиг."),
            "Перезапустите команду на экране, и сервер подхватит конфиг."
        );
        assert_eq!(without_code("Это делает `kubectl`."), "Это делает kubectl.");
        assert_eq!(without_code("Смотрите:\n```bash\napt update\n```\nГотово."), "Смотрите:\n Код — на экране. \nГотово.");
        assert_eq!(without_code("Конфиг лежит в /etc/nginx/nginx.conf."), "Конфиг лежит в nginx.conf.");
        assert_eq!(without_code("Подробнее — https://k3s.io/docs"), "Подробнее — ссылка");
        assert!(without_code("server {\n  listen 443 ssl;\n  root /var/www;\n  location / { try_files $uri =404; }\n}").starts_with("Это код"));
        assert_eq!(without_code("Да, это так: 2 + 2 = 4."), "Да, это так: 2 + 2 = 4.");
    }
}
