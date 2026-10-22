//! Текст для чтения вслух: то, что синтезатор сам прочесть не может.
//!
//! Silero молча выбрасывает цифры («порт 443 открыт» звучит как «порт
//! открыт»), а латиницу проговаривает по буквам — «kubectl» выходил
//! «кубектл», «CPU» — «кпу». Здесь всё это становится словами, как сказал бы
//! инженер: числа — по-русски, с процентами и единицами; термины — по словарю
//! (`assets/pronounce.json`, общему с голосовым сервером); аббревиатуры — по
//! английским буквам; пути и имена команд — по частям; незнакомое слово — по
//! звучанию. Тот же разбор — в `assets/silero_server.py` для голоса с сайта.

use std::collections::HashMap;
use std::sync::OnceLock;

struct Dictionary {
    words: HashMap<String, String>,
    letters: HashMap<char, String>,
}

fn dictionary() -> &'static Dictionary {
    static DICT: OnceLock<Dictionary> = OnceLock::new();
    DICT.get_or_init(|| {
        let value: serde_json::Value = serde_json::from_str(include_str!("../assets/pronounce.json")).unwrap_or_default();
        let words = value["words"]
            .as_object()
            .map(|map| map.iter().filter_map(|(k, v)| Some((k.to_lowercase(), v.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        let letters = value["letters"]
            .as_object()
            .map(|map| map.iter().filter_map(|(k, v)| Some((k.chars().next()?, v.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        Dictionary { words, letters }
    })
}

/// Слова, выученные сервером (`/api/noa/pronounce`): новые термины, которых
/// нет в словаре программы.
static LEARNED: std::sync::RwLock<Option<HashMap<String, String>>> = std::sync::RwLock::new(None);

pub fn learn(words: HashMap<String, String>) {
    let words = words.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    *LEARNED.write().unwrap_or_else(|err| err.into_inner()) = Some(words);
}

fn known(word: &str) -> Option<String> {
    let lower = word.to_lowercase();
    if let Some(found) = dictionary().words.get(&lower) {
        return Some(found.clone());
    }
    LEARNED.read().ok()?.as_ref()?.get(&lower).cloned()
}

/// Прочитанные по звучанию слова — ждут отправки серверу на разучивание.
static PENDING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Что отправить серверу и забыть.
pub fn take_unknown() -> Vec<String> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|err| err.into_inner()))
}

fn note_unknown(text: &str) {
    let words = unknown_words(text);
    if words.is_empty() {
        return;
    }
    let mut pending = PENDING.lock().unwrap_or_else(|err| err.into_inner());
    for word in words {
        if pending.len() < 300 && !pending.contains(&word) {
            pending.push(word);
        }
    }
}

/// Латинские слова, которых нет ни в словаре, ни в выученном, — серверу на
/// разучивание.
pub fn unknown_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let letters = word.chars().filter(char::is_ascii_alphabetic).count();
        if letters < 3 || word.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) || known(word).is_some() {
            continue;
        }
        let lower = word.to_lowercase();
        if !out.contains(&lower) {
            out.push(lower);
        }
    }
    out
}

/* ── Числа ──────────────────────────────────────────────────────────────── */

const ONES: [&str; 20] = [
    "ноль", "один", "два", "три", "четыре", "пять", "шесть", "семь", "восемь", "девять", "десять", "одиннадцать", "двенадцать",
    "тринадцать", "четырнадцать", "пятнадцать", "шестнадцать", "семнадцать", "восемнадцать", "девятнадцать",
];
const TENS: [&str; 10] = ["", "", "двадцать", "тридцать", "сорок", "пятьдесят", "шестьдесят", "семьдесят", "восемьдесят", "девяносто"];
const HUNDREDS: [&str; 10] = ["", "сто", "двести", "триста", "четыреста", "пятьсот", "шестьсот", "семьсот", "восемьсот", "девятьсот"];

/// Форма слова по числу: 1 процент, 2 процента, 5 процентов.
pub fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (tail, last) = (n % 100, n % 10);
    if (11..=14).contains(&tail) {
        many
    } else if last == 1 {
        one
    } else if (2..=4).contains(&last) {
        few
    } else {
        many
    }
}

fn hundreds(n: u64, feminine: bool, out: &mut Vec<String>) {
    let (h, rest) = (n / 100, n % 100);
    if h > 0 {
        out.push(HUNDREDS[h as usize].into());
    }
    if rest >= 20 {
        out.push(TENS[(rest / 10) as usize].into());
        if rest % 10 > 0 {
            out.push(unit(rest % 10, feminine));
        }
    } else if rest > 0 {
        out.push(unit(rest, feminine));
    }
}

fn unit(n: u64, feminine: bool) -> String {
    match (n, feminine) {
        (1, true) => "одна".into(),
        (2, true) => "две".into(),
        _ => ONES[n as usize].into(),
    }
}

/// Целое число словами: 443 — «четыреста сорок три».
pub fn number(n: u64) -> String {
    if n == 0 {
        return ONES[0].into();
    }
    let mut out = Vec::new();
    let groups = [
        (1_000_000_000_000, "триллион", "триллиона", "триллионов", false),
        (1_000_000_000, "миллиард", "миллиарда", "миллиардов", false),
        (1_000_000, "миллион", "миллиона", "миллионов", false),
        (1_000, "тысяча", "тысячи", "тысяч", true),
    ];
    let mut rest = n;
    for (size, one, few, many, feminine) in groups {
        let count = rest / size;
        if count > 0 {
            hundreds(count % 1000, feminine, &mut out);
            out.push(plural(count, one, few, many).into());
            rest %= size;
        }
    }
    hundreds(rest, false, &mut out);
    out.join(" ")
}

/// Единица после числа: словами и в нужной форме.
fn unit_after(word: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match word {
        "%" => ("процент", "процента", "процентов"),
        "ГБ" | "Гб" | "GB" | "Gb" | "G" | "Gi" | "GiB" => ("гигабайт", "гигабайта", "гигабайт"),
        "МБ" | "Мб" | "MB" | "Mb" | "M" | "Mi" | "MiB" => ("мегабайт", "мегабайта", "мегабайт"),
        "КБ" | "Кб" | "KB" | "Kb" | "K" | "Ki" | "KiB" => ("килобайт", "килобайта", "килобайт"),
        "ТБ" | "Тб" | "TB" | "Tb" => ("терабайт", "терабайта", "терабайт"),
        "мс" | "ms" => ("миллисекунда", "миллисекунды", "миллисекунд"),
        "ГГц" | "GHz" => ("гигагерц", "гигагерца", "гигагерц"),
        "МГц" | "MHz" => ("мегагерц", "мегагерца", "мегагерц"),
        "°C" | "°С" | "°" => ("градус", "градуса", "градусов"),
        "₽" | "руб" | "руб." => ("рубль", "рубля", "рублей"),
        "€" => ("евро", "евро", "евро"),
        "vCPU" | "CPU" => ("ядро", "ядра", "ядер"),
        _ => return None,
    })
}

/* ── Латиница ───────────────────────────────────────────────────────────── */

const PAIRS: [(&str, &str); 19] = [
    ("sch", "ш"), ("tch", "ч"), ("sh", "ш"), ("ch", "ч"), ("zh", "ж"), ("th", "т"), ("ph", "ф"), ("kh", "х"), ("ts", "ц"),
    ("ck", "к"), ("oo", "у"), ("ee", "и"), ("ea", "и"), ("ou", "ау"), ("qu", "кв"), ("ya", "я"), ("yu", "ю"), ("yo", "ё"), ("wh", "в"),
];

fn latin_letter(c: char) -> &'static str {
    match c {
        'a' => "а", 'b' => "б", 'c' => "к", 'd' => "д", 'e' => "е", 'f' => "ф", 'g' => "г", 'h' => "х", 'i' => "и",
        'j' => "дж", 'k' => "к", 'l' => "л", 'm' => "м", 'n' => "н", 'o' => "о", 'p' => "п", 'q' => "к", 'r' => "р",
        's' => "с", 't' => "т", 'u' => "у", 'v' => "в", 'w' => "в", 'x' => "кс", 'y' => "и", 'z' => "з",
        _ => "",
    }
}

/// Незнакомое слово — по звучанию: «grafana» — «графана».
fn transliterate(word: &str) -> String {
    let mut lower = word.to_lowercase();
    // Немая «e» на конце: «replace» — «реплак», а не «реплаце».
    if lower.len() > 3 && lower.ends_with('e') && !lower.ends_with("ee") {
        let before = lower.chars().rev().nth(1).unwrap_or('a');
        if !"aeiouy".contains(before) {
            lower.pop();
        }
    }
    let bytes = lower.as_str();
    let mut out = String::new();
    let mut i = 0;
    // «e» в начале слова звучит «э»: «etc» — «этк».
    if bytes.starts_with('e') && !bytes.starts_with("ee") && !bytes.starts_with("ea") {
        out.push('э');
        i = 1;
    }
    'outer: while i < bytes.len() {
        for (latin, cyr) in PAIRS {
            if bytes[i..].starts_with(latin) {
                out.push_str(cyr);
                i += latin.len();
                continue 'outer;
            }
        }
        let ch = bytes[i..].chars().next().unwrap_or(' ');
        if ch == 'c' && matches!(bytes[i + 1..].chars().next(), Some('e' | 'i' | 'y')) {
            out.push('с');
        } else {
            out.push_str(latin_letter(ch));
        }
        i += ch.len_utf8();
    }
    out
}

/// Аббревиатура — по английским буквам: «SSH» — «эс эс эйч».
fn spell(word: &str) -> String {
    let letters = &dictionary().letters;
    word.chars()
        .filter_map(|c| letters.get(&c.to_ascii_lowercase()).cloned())
        .collect::<Vec<_>>()
        .join("-")
}

/// Части слова: «getPods» — get, Pods; «ec2» — ec, 2.
fn parts(word: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut last: Option<char> = None;
    for c in word.chars() {
        let boundary = match last {
            Some(prev) => {
                (prev.is_ascii_digit() != c.is_ascii_digit())
                    || (prev.is_ascii_lowercase() && c.is_ascii_uppercase())
            }
            None => false,
        };
        if boundary && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push(c);
        last = Some(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Одно слово без разделителей.
fn read_word(word: &str) -> String {
    if let Some(found) = known(word) {
        return found;
    }
    if word.chars().all(|c| c.is_ascii_digit()) {
        return word.parse::<u64>().map(number).unwrap_or_default();
    }
    let pieces = parts(word);
    if pieces.len() > 1 {
        return pieces.iter().map(|p| read_word(p)).collect::<Vec<_>>().join(" ");
    }
    let letters = word.chars().filter(char::is_ascii_alphabetic).count();
    let upper = word.chars().filter(char::is_ascii_uppercase).count();
    if letters == 1 || (upper == letters && letters <= 6) {
        return spell(word);
    }
    // Без гласных слово не прочесть — только по буквам: «ls», «pwd».
    if !word.to_lowercase().chars().any(|c| "aeiouy".contains(c)) {
        return spell(word);
    }
    transliterate(word)
}

/// Слитный кусок латиницы с разделителями: путь, имя файла, команда.
fn read_chunk(chunk: &str) -> String {
    if let Some(found) = known(chunk) {
        return found;
    }
    let plus = chunk.replace("++", " плюс плюс").replace('+', " плюс ").replace('#', " шарп");
    let alpha_sides = chunk.split('/').count() == 2 && !chunk.starts_with('/') && chunk.split('/').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphabetic()));
    if alpha_sides {
        // «TCP/UDP» — «ти-си-пи или ю-ди-пи».
        return chunk.split('/').map(read_word).collect::<Vec<_>>().join(" или ");
    }
    plus.split(|c: char| matches!(c, '/' | '.' | '_' | '-' | '\\' | ':' | '~' | '@' | '=') || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| if p.chars().any(|c| c.is_ascii_alphanumeric()) { read_word(p) } else { p.to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

/* ── Разбор ─────────────────────────────────────────────────────────────── */

fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn chunk_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '.' | '_' | '-' | '/' | '\\' | ':' | '~' | '@' | '=')
}

/// Текст для голоса: без цифр и латиницы, всё — словами.
pub fn readable(text: &str) -> String {
    note_unknown(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 32);
    let mut i = 0;
    let push_word = |out: &mut String, word: &str| {
        if !out.is_empty() && !out.ends_with(char::is_whitespace) && !out.ends_with(['(', '«', '"']) {
            out.push(' ');
        }
        out.push_str(word);
    };
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        // Число: целое, дробь, адрес или версия — и единица после него.
        let minus = c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && (i == 0 || prev.is_whitespace() || prev == '(');
        if c.is_ascii_digit() && !is_latin(prev) || minus {
            let start = if minus { i + 1 } else { i };
            let mut j = start;
            let mut groups: Vec<(String, char)> = Vec::new();
            let mut digits = String::new();
            while j < chars.len() {
                let d = chars[j];
                if d.is_ascii_digit() {
                    digits.push(d);
                    j += 1;
                } else if matches!(d, '.' | ',' | ':') && chars.get(j + 1).is_some_and(char::is_ascii_digit) && !digits.is_empty() {
                    groups.push((std::mem::take(&mut digits), d));
                    j += 1;
                } else {
                    break;
                }
            }
            // Число, приклеенное к латинице, — часть имени: «k8s», «ec2».
            if chars.get(j).is_some_and(|c| is_latin(*c)) && unit_word(&chars, j).is_none() {
                let mut end = j;
                while end < chars.len() && chunk_char(chars[end]) {
                    end += 1;
                }
                while end > j && matches!(chars[end - 1], '.' | ',' | ':' | '-' | '/' | '=') {
                    end -= 1;
                }
                let chunk: String = chars[i..end].iter().collect();
                push_word(&mut out, &read_chunk(&chunk));
                i = end;
                continue;
            }
            groups.push((digits, ' '));
            let mut spoken: Vec<String> = Vec::new();
            if minus {
                spoken.push("минус".into());
            }
            let joiner = |sep: char| match sep {
                ',' => "запятая",
                ':' => "",
                _ => "точка",
            };
            for (at, (group, sep)) in groups.iter().enumerate() {
                // Ведущие нули — по цифрам: «08» — «ноль восемь».
                if group.len() > 1 && group.starts_with('0') {
                    spoken.extend(group.chars().map(|d| ONES[d.to_digit(10).unwrap_or(0) as usize].to_string()));
                } else if group.len() > 15 {
                    spoken.extend(group.chars().map(|d| ONES[d.to_digit(10).unwrap_or(0) as usize].to_string()));
                } else {
                    spoken.push(number(group.parse().unwrap_or(0)));
                }
                if at + 1 < groups.len() && !joiner(*sep).is_empty() {
                    spoken.push(joiner(*sep).into());
                }
            }
            let last: u64 = if groups.len() == 1 { groups[0].0.parse().unwrap_or(0) } else { 5 };
            // Знак денег перед числом: «$5» — «пять долларов».
            let dollars = i > 0 && chars[i - 1] == '$';
            if dollars {
                out.pop();
            }
            push_word(&mut out, &spoken.join(" "));
            if dollars {
                out.push(' ');
                out.push_str(plural(last, "доллар", "доллара", "долларов"));
            }
            i = j;
            if let Some((len, forms)) = unit_word(&chars, i) {
                out.push(' ');
                out.push_str(plural(last, forms.0, forms.1, forms.2));
                i += len;
            }
            continue;
        }
        // Латиница: слово, путь, имя файла, команда.
        if is_latin(c) || (matches!(c, '/' | '~') && chars.get(i + 1).is_some_and(|n| is_latin(*n) || *n == '/')) {
            let mut end = i;
            while end < chars.len() && chunk_char(chars[end]) {
                end += 1;
            }
            // Точка, запятая, двоеточие и дефис на конце — знаки фразы, не имени.
            while end > i && matches!(chars[end - 1], '.' | ',' | ':' | '-' | '/' | '=') {
                end -= 1;
            }
            if end == i {
                out.push(c);
                i += 1;
                continue;
            }
            let chunk: String = chars[i..end].iter().collect();
            if chunk.chars().any(is_latin) {
                push_word(&mut out, &read_chunk(&chunk));
            } else {
                out.push_str(&chunk);
            }
            i = end;
            continue;
        }
        match c {
            '&' => push_word(&mut out, "и"),
            '=' if chars.get(i + 1) != Some(&'=') => push_word(&mut out, "равно"),
            '→' | '⇒' => out.push_str(" — "),
            '№' => push_word(&mut out, "номер"),
            '~' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => push_word(&mut out, "около"),
            '-' if chars.get(i + 1) == Some(&'>') => {
                out.push_str(" — ");
                i += 1;
            }
            '$' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => out.push(' '),
            '|' | '{' | '}' | '[' | ']' | '<' | '>' | '\\' | '^' => out.push(' '),
            other => out.push(other),
        }
        i += 1;
    }
    // Двойные пробелы после замен.
    let mut tidy = String::with_capacity(out.len());
    let mut space = false;
    for c in out.chars() {
        if c == ' ' {
            if !space {
                tidy.push(' ');
            }
            space = true;
        } else {
            space = false;
            tidy.push(c);
        }
    }
    tidy.trim().to_string()
}

/// Единица сразу после числа (с пробелом или без): длина и формы слова.
fn unit_word(chars: &[char], at: usize) -> Option<(usize, (&'static str, &'static str, &'static str))> {
    let mut start = at;
    if chars.get(start) == Some(&' ') {
        start += 1;
    }
    if chars.get(start) == Some(&'%') {
        return Some((start + 1 - at, unit_after("%")?));
    }
    let mut end = start;
    while end < chars.len() && (chars[end].is_alphabetic() || matches!(chars[end], '°' | '₽' | '€')) {
        end += 1;
    }
    if end == start {
        return None;
    }
    let word: String = chars[start..end].iter().collect();
    // «руб.» — с точкой; остальные — словом целиком.
    unit_after(&word).map(|forms| (end - at, forms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_become_words() {
        assert_eq!(number(443), "четыреста сорок три");
        assert_eq!(number(2026), "две тысячи двадцать шесть");
        assert_eq!(number(1_000_001), "один миллион один");
        assert_eq!(number(21_000), "двадцать одна тысяча");
        assert_eq!(readable("Порт 443 открыт."), "Порт четыреста сорок три открыт.");
        assert_eq!(readable("Нагрузка 75%."), "Нагрузка семьдесят пять процентов.");
        assert_eq!(readable("Осталось 2 ГБ, а нужно 1 ГБ"), "Осталось два гигабайта, а нужно один гигабайт");
        assert_eq!(readable("адрес 203.0.113.5"), "адрес двести три точка ноль точка сто тринадцать точка пять");
        assert_eq!(readable("было -5 градусов"), "было минус пять градусов");
        assert_eq!(readable("стоит $20"), "стоит двадцать долларов");
    }

    #[test]
    fn terms_sound_like_engineers_say_them() {
        assert_eq!(readable("kubectl get pods"), "кубкатл гет поды");
        assert_eq!(readable("Загрузка CPU выросла"), "Загрузка си-пи-ю выросла");
        assert_eq!(readable("протоколы TCP/UDP"), "протоколы ти-си-пи или ю-ди-пи");
        assert_eq!(readable("конфиг /etc/nginx/nginx.conf"), "конфиг этк энджинкс энджинкс конф");
        assert_eq!(readable("Под в k8s."), "Под в кубернетис.");
        assert_eq!(readable("Флаг OOMKilled"), "Флаг оу-оу-эм килд");
        assert_eq!(readable("команда ls"), "команда эл-эс");
    }

    #[test]
    fn unknown_words_are_spelled_or_read_by_sound() {
        assert_eq!(readable("RBAC"), "ар-би-эй-си");
        assert_eq!(readable("grafana"), "графана");
        assert_eq!(read_word("replace"), "реплак");
        assert!(unknown_words("Используем Velero и kubectl").contains(&"velero".to_string()));
        assert!(!unknown_words("kubectl и RBAC").iter().any(|w| w == "kubectl" || w == "rbac"));
    }

    #[test]
    fn plain_russian_stays_as_is() {
        let text = "Контейнер — обычный процесс на общем ядре.";
        assert_eq!(readable(text), text);
    }
}
