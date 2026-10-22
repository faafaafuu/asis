//! Быстрые команды — то, что понятно без модели: «открой Телеграм», «закрой
//! браузер», «сверни всё», «громче», «таймер на две минуты», «закройся».
//!
//! Зачем отдельно. Раньше каждая такая фраза шла к модели на разбор: 6–20
//! секунд ожидания, а когда распознавание писало «открою» вместо «открой» или
//! «Но» вместо «Ноа», модель то отвечала долго, то не разбирала вовсе, то
//! путала «закройся» с «закрой обучение». Одна и та же команда работала через
//! раз. Здесь разбор — по словам, с допуском на ослышки, мгновенно; модель
//! остаётся для разговора и сложных просьб.
//!
//! Правило осторожности: не уверены — не делаем, отдаём модели. Лучше
//! медленно, чем не то; выключить компьютер или закрыть всё отсюда нельзя
//! случайно — только прямыми словами.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Что сделать.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Open(Thing),
    Close(Thing),
    /// «Сними задачу» — завершить процессы сразу.
    Kill(Thing),
    /// «Закройся», «уйди» — спрятать окна Ноа и закончить разговор.
    CloseSelf,
    /// «Замолчи», «хватит», «стоп» — перестать говорить.
    Hush,
    MinimizeAll,
    RestoreAll,
    Volume(Volume),
    Media(Media),
    /// Таймер на столько секунд.
    Timer(u64),
    /// «Найди на ютубе ролики про секреты в кубере» — поиск на сайте в браузере.
    Search { site: &'static str, query: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Thing {
    /// Своё окно: tasks, watchlist, learning, practice, order, usage,
    /// settings, guides, answer.
    Own(&'static str),
    /// Модуль с окном — по его id.
    Module(String),
    /// Вид программы: browser, messenger, mail, music, terminal.
    Kind(&'static str),
    /// Программа или окно по названию — как его сказали.
    Program(String),
    /// Сайт из знакомых: «ютуб», «вайлдберриз».
    Site(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Volume {
    Up,
    Down,
    Set(u8),
    Mute,
    Unmute,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Media {
    PlayPause,
    Next,
    Previous,
}

/// Слова, которые ничего не меняют в команде: оклики, вежливость, связки,
/// ругань. Имя помощника в разных ослышках — тоже.
const FILLER: &[&str] = &[
    "ну", "ка", "а", "так", "короче", "давай", "давайте", "пожалуйста", "плиз", "мне", "же", "уже", "сейчас",
    "быстро", "просто", "вообще", "слушай", "слухай", "глухай", "эй", "хэй", "чел", "чувак", "вак", "ноа", "нова",
    "noa", "noah", "блядь", "блять", "бля", "сука", "нахер", "нахуй", "нахрен", "нафиг", "епт", "говорю", "можешь",
    "можно", "ты", "сделай", "будь", "добр", "добра", "так", "пжлст", "мать", "твою", "ещё", "еще", "тогда",
    "блин", "пипец", "черт", "чёрт", "ладно", "окей", "ок", "хорошо", "сразу", "скорее",
];

/// Ответы, которые не являются командами, даже если похожи: «ты не закрыл».
fn past_or_state(word: &str) -> bool {
    // «открыт», «открыта», «закрыл», «закрылось» — состояние или прошлое, не просьба.
    let stems = ["открыт", "открыл", "закрыт", "закрыл", "выключил", "выключен", "включил", "включен"];
    stems.iter().any(|stem| word.starts_with(stem))
}

/// Слова фразы: строчные, «ё» — «е», без знаков.
pub fn words(said: &str) -> Vec<String> {
    said.to_lowercase()
        .replace('ё', "е")
        .split(|c: char| !(c.is_alphanumeric() || c == '%'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Verb {
    Open,
    Close,
    Kill,
    Minimize,
    Restore,
}

/// Глагол команды — только повелительные формы и их ослышки. По началу
/// слова («закр…») цеплялись «закругление» и «открываешь» из пересказа.
fn verb_of(word: &str) -> Option<Verb> {
    const OPEN: &[&str] = &["открой", "открою", "откроем", "открыть", "открывай", "откройте", "открывайте", "запусти", "запустите", "запускай", "запустить"];
    const CLOSE: &[&str] = &["закрой", "закрою", "закроем", "закрыть", "закрывай", "закройте", "укрой", "выключи", "выруби", "вырубай", "убери"];
    const KILL: &[&str] = &["сними", "снять", "снимай", "убей", "прибей"];
    const MINIMIZE: &[&str] = &["сверни", "сверните", "свернуть", "сворачивай"];
    const RESTORE: &[&str] = &["верни", "разверни", "покажи"];
    if OPEN.contains(&word) {
        Some(Verb::Open)
    } else if CLOSE.contains(&word) {
        Some(Verb::Close)
    } else if KILL.contains(&word) {
        Some(Verb::Kill)
    } else if MINIMIZE.contains(&word) {
        Some(Verb::Minimize)
    } else if RESTORE.contains(&word) {
        Some(Verb::Restore)
    } else {
        None
    }
}

/// Себя закрыть просят возвратным глаголом: «закройся», «закрыться», «да
/// себя закрой».
fn wants_self_closed(words: &[&str]) -> bool {
    let reflexive = words.iter().any(|w| matches!(*w, "закройся" | "закрыться" | "закрывайся" | "закройтесь"));
    let yourself = words.contains(&"себя") && words.iter().any(|w| verb_of(w) == Some(Verb::Close));
    let negated = words.windows(2).any(|pair| pair[0] == "не" && pair[1].starts_with("закр"));
    (reflexive || yourself) && !negated
}

/// Адрес сайта прямо в речи: «mail.ru», «noahlab.ru».
fn domain_in(said: &str) -> Option<String> {
    const TLD: &[&str] = &["ru", "com", "org", "net", "io", "рф", "su", "dev", "app", "me", "tv", "ai"];
    said.split(|c: char| c.is_whitespace() || matches!(c, ',' | '«' | '»' | '"' | '(' | ')'))
        .map(|token| token.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .find(|token| {
            let mut parts = token.rsplitn(2, '.');
            let tld = parts.next().unwrap_or_default();
            let host = parts.next().unwrap_or_default();
            !host.is_empty() && TLD.contains(&tld) && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        })
}

/// Что сделать по фразе. Пусто — это не быстрая команда, пусть разбирает
/// модель. `modules` — свои модули с окнами: (id, название).
pub fn understand(said: &str, modules: &[(String, String)]) -> Vec<Act> {
    let all = words(said);
    let meaningful: Vec<&str> = all.iter().map(String::as_str).filter(|w| !FILLER.contains(w)).collect();
    // Команда — это коротко. Длинный рассказ, где команда процитирована («я
    // говорю: „закройся“, а он…»), — жалоба или пересказ, не просьба.
    if meaningful.is_empty() || meaningful.len() > 14 {
        return Vec::new();
    }
    // «Да себя закрой, нахер ты обучение закрыл» — о самой Ноа, где бы ни стояло.
    if wants_self_closed(&meaningful) {
        return vec![Act::CloseSelf];
    }
    if let Some(act) = search_request(&all) {
        return vec![act];
    }
    // Команда — короткая фраза. Разбор по предложениям: «Открой телеграм.
    // Открой телеграм.» — одна команда, а не название из пяти слов.
    for sentence in said.split(|c: char| matches!(c, '!' | '?' | ';' | '…')) {
        for piece in split_sentences(sentence) {
            let acts = sentence_acts(&piece, modules);
            if !acts.is_empty() {
                return acts;
            }
        }
    }
    Vec::new()
}

/// Где искать: слово в просьбе — сайт и адрес поиска.
const SEARCH_SITES: &[(&[&str], &str)] = &[
    (&["ютуб", "ютубе", "ютуба", "ютюб", "ютюбе", "ютьюб", "ютьюбе", "youtube", "ютубчик"], "youtube"),
    (&["гугл", "гугле", "google", "гуглу"], "google"),
    (&["яндекс", "яндексе", "yandex"], "yandex"),
    (&["интернете", "сети", "браузере", "инете"], "yandex"),
];
const SEARCH_VERBS: &[&str] = &[
    "найди", "найти", "найдешь", "поищи", "поискать", "ищи", "покажи", "показать", "включи", "открой", "загугли", "глянь",
    "посмотреть", "поставь", "запусти", "хочу",
];
/// Что выбросить из запроса: глагол, сайт, предлоги перед сайтом и слова-обёртки.
const SEARCH_NOISE: &[&str] = &[
    "мне", "пожалуйста", "ка", "плиз", "давай", "ролики", "ролик", "ролике", "роликов", "видео", "видос", "видосы", "видосик",
    "клип", "клипы", "в", "на", "по", "запросу", "там", "нибудь", "можешь", "можно",
];

/// «Найди на ютубе ролики про секреты в кубере», «загугли что такое etcd»,
/// «включи на YouTube лофай». Сайт назван словом; без сайта, но с «загугли» —
/// поиск в интернете. Запрос — то, что осталось без глагола и обёрток.
fn search_request(all: &[String]) -> Option<Act> {
    let site = SEARCH_SITES.iter().find(|(names, _)| all.iter().any(|w| names.contains(&w.as_str()))).map(|(_, site)| *site);
    let verb = all.iter().any(|w| SEARCH_VERBS.contains(&w.as_str()));
    let video = all.iter().any(|w| w.starts_with("ролик") || w.starts_with("видео") || w.starts_with("видос"));
    let googled = all.iter().any(|w| w.starts_with("загугл") || w.starts_with("погугл"));
    let site = match site {
        Some("yandex") if !verb => return None,
        Some(site) if verb || video => site,
        None if googled => "yandex",
        _ => return None,
    };
    let site_words: Vec<&str> = SEARCH_SITES.iter().flat_map(|(names, _)| names.iter().copied()).collect();
    let mut query: Vec<&str> = all
        .iter()
        .map(String::as_str)
        .filter(|w| !SEARCH_VERBS.contains(w) && !site_words.contains(w) && !w.starts_with("загугл") && !w.starts_with("погугл"))
        .collect();
    // Обёртки — только в начале запроса: «секреты в кубере» своё «в» сохраняет.
    while query.first().is_some_and(|w| SEARCH_NOISE.contains(w) || *w == "про" || *w == "о" || *w == "об") {
        query.remove(0);
    }
    while query.last().is_some_and(|w| SEARCH_NOISE.contains(w)) {
        query.pop();
    }
    if query.is_empty() {
        return None;
    }
    Some(Act::Search { site, query: query.join(" ") })
}

/// Адрес поиска на сайте.
fn search_url(site: &str, query: &str) -> String {
    let q: String = query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect();
    match site {
        "youtube" => format!("https://www.youtube.com/results?search_query={q}"),
        "google" => format!("https://www.google.com/search?q={q}"),
        _ => format!("https://yandex.ru/search/?text={q}"),
    }
}

/// Предложения по точке — но не по точке внутри адреса: «mail.ru».
fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    for (i, c) in chars.iter().enumerate() {
        let inside_word = *c == '.' && i > 0 && i + 1 < chars.len() && chars[i - 1].is_alphanumeric() && chars[i + 1].is_alphanumeric();
        if *c == '.' && !inside_word {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(*c);
        }
    }
    out.push(current);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

fn sentence_acts(said: &str, modules: &[(String, String)]) -> Vec<Act> {
    let all = words(said);
    let meaningful: Vec<&str> = all.iter().map(String::as_str).filter(|w| !FILLER.contains(w)).collect();
    if meaningful.is_empty() || meaningful.len() > 10 {
        return Vec::new();
    }
    if let Some(act) = about_self(&meaningful) {
        return vec![act];
    }
    if let Some(act) = timer(&meaningful) {
        return vec![act];
    }
    if let Some(act) = volume(&meaningful) {
        return vec![act];
    }
    if meaningful.len() <= 4 {
        if let Some(act) = media(&meaningful) {
            return vec![act];
        }
    }

    // Глагол — в первых трёх значимых словах: «а вот закрой браузер». Или
    // в конце короткой части фразы: «модуль задачи закрой».
    let Some((at, verb)) = meaningful.iter().take(3).enumerate().find_map(|(at, w)| verb_of(w).map(|v| (at, v))) else {
        return verb_last(said, modules);
    };
    // «Не закрывай», «не открывай» — просьба ничего не делать.
    if at > 0 && meaningful[at - 1] == "не" {
        return Vec::new();
    }
    // Предмет — до следующего глагола или союза: «открой, а не закрой»,
    // «открой окно диалоговое, а то я не понимаю».
    let verb_word = meaningful[at];
    let after = all.iter().position(|w| w == verb_word).map_or(all.len(), |i| i + 1);
    let rest: Vec<&str> = all[after..]
        .iter()
        .map(String::as_str)
        .take_while(|w| verb_of(w).is_none() && !matches!(*w, "не" | "а" | "но" | "потому" | "чтобы" | "если" | "когда" | "что" | "типа"))
        .filter(|w| !FILLER.contains(w))
        .collect();

    // «Открой диспетчер устройств или что там…» — человек сам не уверен.
    if all.iter().any(|w| w == "или") {
        return Vec::new();
    }
    // «Открой mail.ru», «mail.ru в браузере» — сайт по адресу.
    if verb == Verb::Open {
        if let Some(domain) = domain_in(said) {
            return vec![Act::Open(Thing::Site(domain))];
        }
    }

    match verb {
        Verb::Minimize if rest.is_empty() || mentions_everything(&rest) => return vec![Act::MinimizeAll],
        Verb::Minimize => return Vec::new(),
        Verb::Restore if mentions_everything(&rest) => return vec![Act::RestoreAll],
        Verb::Restore => {
            // «Покажи активы», «разверни погодыч» — своё окно; остальное — модели.
            return thing_of(&rest, modules, Verb::Open).map(Act::Open).filter(|a| matches!(a, Act::Open(Thing::Own(_) | Thing::Module(_)))).into_iter().collect();
        }
        _ => {}
    }
    // «Закрой все окна», «закрой все модули суфлёра».
    if verb == Verb::Close && rest.iter().any(|w| matches!(*w, "все" | "всё")) {
        if rest.iter().any(|w| w.starts_with("модул") || w.starts_with("суфл") || w.starts_with("сафл") || w.starts_with("свои")) {
            return vec![Act::CloseSelf];
        }
        if rest.iter().any(|w| w.starts_with("окн") || w.starts_with("программ") || w.starts_with("приложен")) {
            return vec![Act::Close(Thing::Own("everything"))];
        }
    }

    // «Снять задачу Chrome» — «задачу» тут часть глагола.
    let rest: Vec<&str> = if verb == Verb::Kill {
        rest.into_iter().skip_while(|w| w.starts_with("задач") || *w == "процесс").collect()
    } else {
        rest
    };
    // «Открой, разверни окно погодыч» — глаголы подряд, предмет за последним.
    if rest.is_empty() {
        if let Some(next) = meaningful.get(at + 1).filter(|w| verb_of(w).is_some()) {
            let tail: Vec<&str> = meaningful[at + 2..].iter().copied().take_while(|w| verb_of(w).is_none() && *w != "не").collect();
            if let (false, Some(thing)) = (tail.is_empty(), thing_of(&tail, modules, if verb == Verb::Open { Verb::Open } else { verb_of(next).unwrap_or(verb) })) {
                return vec![act(verb, thing)];
            }
        }
    }
    if rest.is_empty() {
        // «Сними задачу», «открой, а не закрой» без предмета — о том, о чём шла речь.
        return match (verb, last()) {
            (Verb::Kill, Some(thing)) => vec![Act::Kill(thing)],
            (Verb::Open, Some(thing)) => vec![Act::Open(thing)],
            _ => Vec::new(),
        };
    }
    // Себя — «сними задачу Суфлёр» — не снимаем: закрываемся.
    if rest.iter().any(|w| w.starts_with("суфл") || w.starts_with("сафл")) {
        return vec![Act::CloseSelf];
    }

    // «Открой обучение и практику» — несколько предметов через «и». Только
    // если каждый понятен сам по себе: «установку и удаление программ» — одно.
    let parts: Vec<Vec<&str>> = rest.split(|w| *w == "и").map(<[&str]>::to_vec).filter(|p| !p.is_empty()).collect();
    if parts.len() > 1 {
        let things: Vec<Thing> = parts.iter().filter_map(|part| thing_of(part, modules, verb)).collect();
        if things.len() == parts.len() && things.iter().all(|t| !matches!(t, Thing::Program(_))) {
            return things.into_iter().map(|thing| act(verb, thing)).collect();
        }
    }
    thing_of(&rest, modules, verb).map(|thing| vec![act(verb, thing)]).unwrap_or_default()
}

/// Глагол последним в короткой части фразы: «я про раздел задачи, модуль
/// задачи закрой» — «модуль задачи закрой».
fn verb_last(said: &str, modules: &[(String, String)]) -> Vec<Act> {
    let clause = said.rsplit(',').next().unwrap_or(said);
    let all = words(clause);
    let meaningful: Vec<&str> = all.iter().map(String::as_str).filter(|w| !FILLER.contains(w)).collect();
    let Some((last, before)) = meaningful.split_last() else {
        return Vec::new();
    };
    let Some(verb) = verb_of(last).filter(|v| matches!(v, Verb::Open | Verb::Close)) else {
        return Vec::new();
    };
    if before.is_empty() || before.len() > 3 || before.contains(&"не") {
        return Vec::new();
    }
    thing_of(before, modules, verb).filter(|thing| !matches!(thing, Thing::Program(_))).map(|thing| vec![act(verb, thing)]).unwrap_or_default()
}

fn act(verb: Verb, thing: Thing) -> Act {
    match verb {
        Verb::Close => Act::Close(thing),
        Verb::Kill => Act::Kill(thing),
        _ => Act::Open(thing),
    }
}

fn mentions_everything(rest: &[&str]) -> bool {
    rest.iter().any(|w| matches!(*w, "все" | "всё" | "всю" | "вся" | "окна" | "программы" | "программа" | "программу" | "приложения" | "рабочий" | "стол"))
}

/// «Закройся», «уйди», «замолчи», «хватит» — о самой Ноа.
fn about_self(words: &[&str]) -> Option<Act> {
    // «Всё, закройся», «я говорю, закройся», «ладно, хватит».
    let words: Vec<&str> = words.iter().copied().filter(|w| !matches!(*w, "все" | "всё" | "я" | "ладно" | "всем" | "тебе")).collect();
    let words = words.as_slice();
    let hush = ["замолчи", "помолчи", "заткнись", "хватит", "стоп", "тихо", "достаточно", "харэ", "тсс"];
    let leave = ["закройся", "уйди", "спрячься", "отключись", "отвали", "свали"];
    match words {
        [w] if leave.contains(w) => Some(Act::CloseSelf),
        [w] if hush.contains(w) => Some(Act::Hush),
        [w, "пока"] | ["пока", w] if leave.contains(w) => Some(Act::CloseSelf),
        [w, "себя"] | ["себя", w] if w.starts_with("закр") || *w == "убери" || *w == "выключи" => Some(Act::CloseSelf),
        [w, "говорить"] if hush.contains(w) || *w == "перестань" || *w == "прекрати" => Some(Act::Hush),
        _ => None,
    }
}

/// «Таймер на две минуты», «засеки пять минут», «поставь таймер на полчаса».
fn timer(words: &[&str]) -> Option<Act> {
    let asked = words.iter().any(|w| w.starts_with("таймер") || w.starts_with("засеки") || *w == "засечь");
    if !asked {
        return None;
    }
    let mut seconds = 0u64;
    let mut number: Option<u64> = None;
    for w in words {
        if let Some(n) = number_word(w) {
            number = Some(number.unwrap_or(0) + n);
            continue;
        }
        let unit = if w.starts_with("сек") {
            1
        } else if w.starts_with("мин") {
            60
        } else if w.starts_with("час") {
            3600
        } else if *w == "полчаса" {
            seconds += 1800;
            continue;
        } else if *w == "полминуты" {
            seconds += 30;
            continue;
        } else {
            continue;
        };
        seconds += number.take().unwrap_or(1) * unit;
    }
    (seconds > 0 && seconds <= 24 * 3600).then_some(Act::Timer(seconds))
}

fn number_word(w: &str) -> Option<u64> {
    if let Ok(n) = w.parse::<u64>() {
        return Some(n);
    }
    Some(match w {
        "одну" | "один" | "одна" => 1,
        "две" | "два" => 2,
        "три" => 3,
        "четыре" => 4,
        "пять" => 5,
        "шесть" => 6,
        "семь" => 7,
        "восемь" => 8,
        "девять" => 9,
        "десять" => 10,
        "пятнадцать" => 15,
        "двадцать" => 20,
        "тридцать" => 30,
        "сорок" => 40,
        "пятьдесят" => 50,
        "полтора" | "полторы" => return None,
        _ => return None,
    })
}

/// «Громче», «потише», «громкость 80», «выключи звук».
fn volume(words: &[&str]) -> Option<Act> {
    let sound = words.iter().any(|w| w.starts_with("громк") || w.starts_with("звук"));
    if let Some(n) = words.iter().find_map(|w| w.trim_end_matches('%').parse::<u8>().ok()) {
        if sound && n <= 100 {
            return Some(Act::Volume(Volume::Set(n)));
        }
    }
    if words.iter().any(|w| matches!(*w, "громче" | "погромче" | "прибавь")) {
        return Some(Act::Volume(Volume::Up));
    }
    if words.iter().any(|w| matches!(*w, "тише" | "потише" | "убавь")) {
        return Some(Act::Volume(Volume::Down));
    }
    if sound && words.iter().any(|w| w.starts_with("выключ") || w.starts_with("отключ") || w.starts_with("убер")) {
        return Some(Act::Volume(Volume::Mute));
    }
    if sound && words.iter().any(|w| w.starts_with("включ") || w.starts_with("верни")) {
        return Some(Act::Volume(Volume::Unmute));
    }
    if words.iter().any(|w| matches!(*w, "мьют" | "замьють" | "замьюти")) {
        return Some(Act::Volume(Volume::Mute));
    }
    None
}

/// «Пауза», «следующий трек», «выключи музыку».
fn media(words: &[&str]) -> Option<Act> {
    let music = words.iter().any(|w| w.starts_with("музык") || w.starts_with("трек") || w.starts_with("песн") || w.starts_with("видео"));
    if words == ["пауза"] || words.iter().any(|w| w.starts_with("пауз")) {
        return Some(Act::Media(Media::PlayPause));
    }
    let song = words.iter().any(|w| w.starts_with("трек") || w.starts_with("песн"));
    if song && words.iter().any(|w| w.starts_with("следующ") || *w == "дальше" || w.starts_with("переключ")) {
        return Some(Act::Media(Media::Next));
    }
    if song && words.iter().any(|w| w.starts_with("предыдущ") || w.starts_with("прошл")) {
        return Some(Act::Media(Media::Previous));
    }
    if music && words.iter().any(|w| w.starts_with("выключ") || w.starts_with("останов") || *w == "стоп" || w.starts_with("включ") || w.starts_with("продолж")) {
        return Some(Act::Media(Media::PlayPause));
    }
    None
}

/// Предмет команды: своё окно, модуль, вид программы, сайт или программа.
fn thing_of(rest: &[&str], modules: &[(String, String)], verb: Verb) -> Option<Thing> {
    // «Окно, в котором мы разговариваем», «это окошко» — окно ответа Ноа.
    let joined = rest.join(" ");
    if (joined.contains("окн") || joined.contains("окош")) && (joined.contains("разговар") || joined.contains("ответ") || joined.contains("диалог")) {
        return Some(Thing::Own("answer"));
    }
    // «Закрой это окно», «закрой раздел этот» — то, что впереди.
    if !rest.is_empty() && rest.iter().all(|w| matches!(*w, "это" | "этот" | "эту" | "окно" | "окошко" | "раздел" | "вкладку")) && rest.iter().any(|w| *w != "это") {
        return Some(Thing::Own("front"));
    }
    // «Закрой эти задачи» — о самих делах, «открой меня» — непонятно: модели.
    if rest.iter().any(|w| matches!(*w, "эти" | "этих" | "меня")) {
        return None;
    }
    // «Свой терминал», «твой терминал» — терминал Ноа, а не Windows.
    if rest.iter().any(|w| matches!(*w, "свой" | "твой" | "свою" | "твою")) && rest.iter().any(|w| w.starts_with("терминал")) {
        return Some(Thing::Own("practice"));
    }
    // Компьютер выключают только прямыми словами — это другая команда.
    if rest.iter().any(|w| matches!(*w, "компьютер" | "комп" | "пк" | "ноутбук" | "ноут")) {
        return None;
    }
    // «Его», «её», «это» — то, о чём только что шла речь.
    if rest.len() == 1 && matches!(rest[0], "его" | "ее" | "её" | "это" | "него" | "нее" | "эту" | "этот") {
        return last();
    }
    let rest: Vec<&str> = rest
        .iter()
        .copied()
        .filter(|w| !matches!(*w, "мой" | "мои" | "моих" | "мою" | "модуль" | "модули" | "раздел" | "окно" | "окошко" | "вкладку" | "приложение" | "программу" | "программа" | "это" | "этот" | "эту" | "свой" | "твой" | "свою"))
        .collect();
    if rest.is_empty() || rest.len() > 4 {
        return None;
    }
    let joined = rest.join(" ");
    let has = |stem: &str| rest.iter().any(|w| w.starts_with(stem));

    // Свои окна. «Терминал» — свой, только если сказано «свой/твой/ноа»: это
    // уже отфильтровано выше, поэтому смотрим на исходную фразу через has.
    if has("обучен") || has("курс") || has("урок") || has("учеб") {
        return Some(Thing::Own("learning"));
    }
    if has("практик") {
        return Some(Thing::Own("practice"));
    }
    if has("путеводител") {
        return Some(Thing::Own("guides"));
    }
    // «Задачи», «дела» — окно; «задачу купить корм» — это про само дело,
    // его разбирает модуль задач.
    if rest.iter().any(|w| matches!(*w, "задачи" | "задач" | "дела" | "дел")) && rest.len() == 1 {
        return Some(Thing::Own("tasks"));
    }
    if rest.first().is_some_and(|w| w.starts_with("задач") || *w == "дело") {
        return None;
    }
    if has("актив") || has("портфел") || has("котиров") {
        return Some(Thing::Own("watchlist"));
    }
    if (has("заказ") || has("корзин")) && rest.len() <= 2 {
        return Some(Thing::Own("order"));
    }
    if has("расход") {
        return Some(Thing::Own("usage"));
    }
    if has("настройк") && !has("виндовс") && !has("windows") && !has("систем") {
        return Some(Thing::Own("settings"));
    }

    // Модули — по названию, с допуском: «погодыч», «погодач», «погоды».
    let spoken: Vec<String> = rest.iter().map(|w| cyrillic(w)).collect();
    for (id, title) in modules {
        let title = title.to_lowercase().replace('ё', "е");
        let stem: String = title.chars().take(5).collect();
        if stem.chars().count() >= 4 && spoken.iter().any(|w| w.starts_with(&stem) || title.starts_with(w.as_str()) && w.chars().count() >= 5) {
            return Some(Thing::Module(id.clone()));
        }
    }

    // Виды программ.
    // «Браузер» — тот, что есть; «Brave браузер», «яндекс браузер» — названный.
    if (has("браузер") || joined == "browser") && rest.len() == 1 {
        return Some(Thing::Kind("browser"));
    }
    if has("браузер") {
        let named: Vec<&str> = rest.iter().copied().filter(|w| !w.starts_with("браузер")).collect();
        if named.iter().any(|w| w.starts_with("яндекс")) {
            return Some(Thing::Program("яндекс браузер".into()));
        }
        return Some(Thing::Program(named.join(" ")));
    }
    if has("мессенджер") {
        return Some(Thing::Kind("messenger"));
    }
    if has("почт") && rest.len() == 1 {
        return Some(Thing::Kind("mail"));
    }
    if (has("терминал") || has("консол") || joined.contains("командную строку")) && rest.len() <= 2 {
        return Some(Thing::Kind("terminal"));
    }
    if (has("музык") || has("плеер")) && verb == Verb::Open && rest.len() == 1 {
        return Some(Thing::Kind("music"));
    }

    // «Открой сайт TradingView», «сайт лента» — сайт по имени.
    if verb == Verb::Open && rest.first().is_some_and(|w| w.starts_with("сайт")) && (2..=3).contains(&rest.len()) {
        return Some(Thing::Site(rest[1..].join(" ")));
    }
    // Знакомые сайты открываются в браузере, а не ищутся среди программ.
    if verb == Verb::Open {
        if let Some(site) = crate::web::site_in(&joined) {
            return Some(Thing::Site(site.to_string()));
        }
    }
    // Остальное — программа по названию; поиск, файлы и сайты — модели.
    let words = ["в", "на", "с", "про", "что", "как", "где", "почему", "зачем"];
    let stems = ["найди", "найт", "файл", "папк", "сайт", "страниц", "письм"];
    if rest.iter().any(|w| words.contains(w) || stems.iter().any(|stem| w.starts_with(stem))) {
        return None;
    }
    Some(Thing::Program(joined))
}

/// «Чувак, открой задачи», «чел, закрой браузер» — к Ноа обращаются не только
/// по имени. Такое обращение принимаем, только если сразу за ним команда:
/// иначе любой «чел» в разговоре будил бы помощника. Отдаёт саму команду.
pub fn addressed(text: &str) -> Option<String> {
    const CALLS: &[&str] = &["чел", "чувак", "вак", "слушай", "слухай", "эй", "хэй", "братан", "бро", "нова"];
    let all = words(text);
    let first = all.first()?;
    if !CALLS.contains(&first.as_str()) {
        return None;
    }
    let command = !understand(text, &[]).is_empty()
        || all.iter().skip(1).filter(|w| !FILLER.contains(&w.as_str())).take(3).any(|w| verb_of(w).is_some());
    command.then(|| {
        text.trim_start_matches(|c: char| !c.is_alphanumeric())
            .split_once(|c: char| !c.is_alphanumeric())
            .map_or(String::new(), |(_, rest)| rest.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string())
    })
}

/* ── Исполнение ────────────────────────────────────────────────────────── */

/// Выполнить быструю команду и отдать, что сказать. `None` — фраза не
/// команда или не уверены: пусть разбирает модель.
pub async fn perform(app: &tauri::AppHandle, said: &str) -> Option<String> {
    let modules: Vec<(String, String)> = crate::plugins::installed(app)
        .into_iter()
        .filter(|manifest| !manifest.window.file.trim().is_empty())
        .map(|manifest| (manifest.id, manifest.title))
        .collect();
    let acts = understand(said, &modules);
    if acts.is_empty() {
        return None;
    }
    log::info!("быстрая команда: «{said}» → {acts:?}");
    let mut replies = Vec::new();
    for act in acts {
        match run(app, act).await {
            Some(reply) => replies.push(reply),
            // Одну из частей не поняли уверенно — всё целиком модели.
            None if replies.is_empty() => return None,
            None => {}
        }
    }
    Some(replies.join(" ").trim().to_string())
}

async fn run(app: &tauri::AppHandle, act: Act) -> Option<String> {
    use tauri::Manager;
    let blocking = |work: Box<dyn FnOnce() -> String + Send>| async move {
        tauri::async_runtime::spawn_blocking(work).await.unwrap_or_else(|_| "Не вышло.".into())
    };
    Some(match act {
        Act::Search { site, query } => {
            let place = match site {
                "youtube" => "на YouTube",
                "google" => "в Google",
                _ => "в Яндексе",
            };
            match crate::commands::open_externally(&search_url(site, &query)) {
                Ok(()) => format!("Ищу {place}: «{query}»."),
                Err(err) => format!("Браузер не открылся: {err}"),
            }
        }
        Act::CloseSelf => {
            crate::voice::stop();
            for label in [crate::overlay::POPUP_LABEL, crate::overlay::HUD_LABEL] {
                if let Some(window) = app.get_webview_window(label) {
                    let _ = window.hide();
                }
            }
            crate::planner::hand_off();
            "Хорошо.".into()
        }
        Act::Hush => {
            crate::voice::stop();
            String::new()
        }
        Act::MinimizeAll => {
            // Win+M — свернуть, а не Win+D: тот на втором нажатии возвращает всё обратно.
            crate::pc::tap(&[0x5B, 0x4D], 1);
            "Свернула всё.".into()
        }
        Act::RestoreAll => {
            crate::pc::tap(&[0x5B, 0x10, 0x4D], 1);
            "Вернула окна.".into()
        }
        Act::Volume(volume) => {
            const UP: u16 = 0xAF;
            const DOWN: u16 = 0xAE;
            const MUTE: u16 = 0xAD;
            match volume {
                Volume::Up => crate::pc::tap(&[UP], 5),
                Volume::Down => crate::pc::tap(&[DOWN], 5),
                // Клавиша громкости — шаг в 2%: сперва в ноль, потом до нужного.
                Volume::Set(level) => {
                    crate::pc::tap(&[DOWN], 50);
                    crate::pc::tap(&[UP], usize::from(level / 2));
                }
                Volume::Mute => crate::pc::tap(&[MUTE], 1),
                Volume::Unmute => {
                    crate::pc::tap(&[UP], 1);
                    crate::pc::tap(&[DOWN], 1);
                }
            }
            match volume {
                Volume::Set(level) => format!("Громкость {level}%."),
                Volume::Mute => "Выключила звук.".into(),
                // Громче-тише — без слов: голос сам покажет, что изменилось.
                _ => String::new(),
            }
        }
        Act::Media(media) => {
            let key = match media {
                Media::PlayPause => 0xB3,
                Media::Next => 0xB0,
                Media::Previous => 0xB1,
            };
            crate::pc::tap(&[key], 1);
            String::new()
        }
        Act::Timer(seconds) => {
            let at = chrono::Local::now() + chrono::Duration::seconds(seconds as i64);
            crate::timers::start(app, at, "Таймер".into());
            format!("Таймер на {}.", duration(seconds))
        }
        Act::Open(thing) => {
            remember(&thing);
            match thing {
                Thing::Own(id) => open_own(app, id),
                Thing::Module(id) => match crate::modules::open(app, &id) {
                    Ok(()) => "Открываю.".into(),
                    Err(err) => format!("Окно модуля не открылось: {err}."),
                },
                Thing::Kind(kind) => {
                    let name = kind_names(kind).iter().find(|name| crate::pc::knows(name)).map(|name| name.to_string());
                    match name {
                        Some(name) => blocking(Box::new(move || crate::pc::launch_checked(&name))).await,
                        None => format!("Не нашла, чем открыть {}.", kind_title(kind)),
                    }
                }
                Thing::Program(name) => {
                    let known = {
                        let name = name.clone();
                        tauri::async_runtime::spawn_blocking(move || crate::pc::knows(&name)).await.unwrap_or(false)
                    };
                    if known {
                        blocking(Box::new(move || crate::pc::launch_checked(&name))).await
                    } else if let Some((site, _)) = crate::web::named_site(&name).await {
                        crate::web::open_site(&site, "").await
                    } else {
                        return None;
                    }
                }
                Thing::Site(site) => crate::web::open_site(&site, "").await,
            }
        }
        Act::Close(thing) | Act::Kill(thing) if matches!(thing, Thing::Own(_) | Thing::Module(_)) => {
            match thing {
                Thing::Own("everything") => blocking(Box::new(crate::pc::close_everything)).await,
                Thing::Own("front") => close_front(app).await,
                Thing::Own(id) => close_own(app, id),
                Thing::Module(id) => {
                    let label = format!("module-{}", id.replace(|c: char| !c.is_alphanumeric(), "-"));
                    match app.get_webview_window(&label) {
                        Some(window) if window.is_visible().unwrap_or(false) => {
                            let _ = window.hide();
                            "Закрыла.".into()
                        }
                        _ => "Он и так закрыт.".into(),
                    }
                }
                _ => return None,
            }
        }
        Act::Close(thing) => close_thing(thing, false).await,
        Act::Kill(thing) => close_thing(thing, true).await,
    })
}

async fn close_thing(thing: Thing, force: bool) -> String {
    remember(&thing);
    let work: Box<dyn FnOnce() -> String + Send> = match thing {
        Thing::Kind(kind) => Box::new(move || crate::pc::close_kind(kind_exes(kind), kind_title(kind), force)),
        Thing::Program(name) => Box::new(move || crate::pc::close(&name, force)),
        Thing::Site(site) => Box::new(move || crate::pc::close(&site, force)),
        _ => return String::new(),
    };
    tauri::async_runtime::spawn_blocking(work).await.unwrap_or_else(|_| "Не вышло.".into())
}

/// Своё окно по id: label окна и как его назвать.
fn own_window(id: &str) -> Option<(String, &'static str)> {
    Some(match id {
        "tasks" => (crate::overlay::TASKS_LABEL.into(), "задачи"),
        "watchlist" => (crate::overlay::WATCH_LABEL.into(), "активы"),
        "learning" | "guides" => (crate::overlay::LEARN_LABEL.into(), "обучение"),
        "practice" => (crate::practice::LABEL.into(), "практику"),
        "order" => (crate::overlay::ORDER_LABEL.into(), "заказ"),
        "usage" => (crate::overlay::USAGE_LABEL.into(), "расход"),
        "settings" => (crate::overlay::ONBOARDING_LABEL.into(), "настройки"),
        "answer" => (crate::overlay::POPUP_LABEL.into(), "окно ответа"),
        _ => return None,
    })
}

fn open_own(app: &tauri::AppHandle, id: &str) -> String {
    use tauri::Emitter;
    let opened = match id {
        "tasks" => crate::overlay::show_tasks(app),
        "watchlist" => crate::overlay::show_watchlist(app),
        "learning" => crate::overlay::show_learning(app),
        "guides" => {
            crate::guides::want(None);
            let shown = crate::overlay::show_learning(app);
            let _ = app.emit("guides:open", "");
            shown
        }
        "practice" => crate::overlay::show_practice(app),
        "order" => crate::overlay::show_order(app),
        "usage" => crate::overlay::show_usage_widget(app),
        "settings" => crate::overlay::show_onboarding(app),
        // «Открой окно диалоговое» — показывать ответы окном.
        "answer" => return crate::set_show_window(app, true),
        _ => return "Такого окна нет.".into(),
    };
    let name = own_window(id).map_or("окно", |(_, name)| name);
    match opened {
        Ok(()) => format!("Открываю {name}."),
        Err(err) => format!("Не открылось: {err}."),
    }
}

fn close_own(app: &tauri::AppHandle, id: &str) -> String {
    use tauri::Manager;
    let Some((label, name)) = own_window(id) else {
        return String::new();
    };
    match app.get_webview_window(&label) {
        Some(window) if window.is_visible().unwrap_or(false) => {
            let _ = window.hide();
            format!("Закрыла {name}.")
        }
        _ => format!("{} и так закрыто.", capitalized(name)),
    }
}

/// «Закрой это окно»: своё — если впереди оно, иначе то, где человек работал.
async fn close_front(app: &tauri::AppHandle) -> String {
    use tauri::Manager;
    for label in [crate::overlay::POPUP_LABEL, crate::overlay::LEARN_LABEL, crate::overlay::TASKS_LABEL, crate::practice::LABEL, crate::overlay::ORDER_LABEL, crate::overlay::WATCH_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            if window.is_focused().unwrap_or(false) && window.is_visible().unwrap_or(false) {
                let _ = window.hide();
                return "Закрыла.".into();
            }
        }
    }
    tauri::async_runtime::spawn_blocking(crate::pc::close_front).await.unwrap_or_else(|_| "Не вышло.".into())
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

fn duration(seconds: u64) -> String {
    let plural = |n: u64, one: &str, few: &str, many: &str| {
        let (tens, last) = (n % 100, n % 10);
        let word = if (11..=14).contains(&tens) { many } else if last == 1 { one } else if (2..=4).contains(&last) { few } else { many };
        format!("{n} {word}")
    };
    match seconds {
        s if s % 3600 == 0 => plural(s / 3600, "час", "часа", "часов"),
        s if s % 60 == 0 => plural(s / 60, "минуту", "минуты", "минут"),
        s if s > 60 => format!("{} и {}", plural(s / 60, "минуту", "минуты", "минут"), plural(s % 60, "секунду", "секунды", "секунд")),
        s => plural(s, "секунду", "секунды", "секунд"),
    }
}

/// Латиница — как её прочтут по-русски: «Pogodach» — «погодач».
fn cyrillic(word: &str) -> String {
    if !word.chars().any(|c| c.is_ascii_alphabetic()) {
        return word.to_string();
    }
    let pairs = [("sch", "щ"), ("ch", "ч"), ("sh", "ш"), ("zh", "ж"), ("kh", "х"), ("ts", "ц"), ("ya", "я"), ("yu", "ю"), ("yo", "е")];
    let mut text = word.to_lowercase();
    for (latin, russian) in pairs {
        text = text.replace(latin, russian);
    }
    let mut out = String::new();
    for c in text.chars() {
        let russian = match c {
            'a' => "а", 'b' => "б", 'c' => "к", 'd' => "д", 'e' => "е", 'f' => "ф", 'g' => "г", 'h' => "х", 'i' => "и",
            'j' => "й", 'k' => "к", 'l' => "л", 'm' => "м", 'n' => "н", 'o' => "о", 'p' => "п", 'q' => "к", 'r' => "р",
            's' => "с", 't' => "т", 'u' => "у", 'v' => "в", 'w' => "в", 'x' => "кс", 'y' => "ы", 'z' => "з",
            other => {
                out.push(other);
                continue;
            }
        };
        out.push_str(russian);
    }
    out
}

/// О чём шла речь — для «его», «он всё ещё открыт».
static LAST: Mutex<Option<(Thing, Instant)>> = Mutex::new(None);

pub fn remember(thing: &Thing) {
    *LAST.lock().unwrap_or_else(|err| err.into_inner()) = Some((thing.clone(), Instant::now()));
}

fn last() -> Option<Thing> {
    let guard = LAST.lock().unwrap_or_else(|err| err.into_inner());
    guard.as_ref().filter(|(_, at)| at.elapsed() < Duration::from_secs(180)).map(|(thing, _)| thing.clone())
}

/// Исполнимые файлы вида программы — для закрытия.
pub fn kind_exes(kind: &str) -> &'static [&'static str] {
    match kind {
        "browser" => &["chrome", "msedge", "firefox", "brave", "opera", "browser", "vivaldi", "arc", "yandex", "chromium", "zen"],
        "messenger" => &["telegram", "whatsapp", "discord", "viber", "signal", "slack", "skype"],
        "mail" => &["outlook", "olk", "hxoutlook", "thunderbird", "mailspring"],
        "music" => &["spotify", "yandexmusic", "яндекс музыка", "aimp", "foobar2000", "vlc", "music.ui"],
        "terminal" => &["windowsterminal", "openconsole", "powershell", "pwsh", "cmd", "wezterm-gui", "alacritty", "mintty"],
        _ => &[],
    }
}

/// Названия вида программы — для открытия: первая установленная.
pub fn kind_names(kind: &str) -> &'static [&'static str] {
    match kind {
        "browser" => &["google chrome", "brave", "microsoft edge", "яндекс браузер", "firefox", "opera"],
        "messenger" => &["telegram", "whatsapp", "discord"],
        "mail" => &["outlook", "почта", "thunderbird"],
        "music" => &["яндекс музыка", "spotify", "aimp"],
        "terminal" => &["терминал", "windows terminal", "powershell"],
        _ => &[],
    }
}

pub fn kind_title(kind: &str) -> &'static str {
    match kind {
        "browser" => "браузер",
        "messenger" => "мессенджер",
        "mail" => "почту",
        "music" => "музыку",
        "terminal" => "терминал",
        _ => "программу",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> Vec<(String, String)> {
        vec![("pogodych".into(), "Погодыч".into())]
    }

    #[test]
    fn web_search_goes_to_the_browser() {
        let search = |site: &'static str, query: &str| vec![Act::Search { site, query: query.into() }];
        assert_eq!(understand("Найди мне на YouTube ролики про секреты в кубере.", &mods()), search("youtube", "секреты в кубере"));
        assert_eq!(understand("Найди на youtube ролике про секреты кубернетис.", &mods()), search("youtube", "секреты кубернетис"));
        assert_eq!(understand("включи на ютубе лофай", &mods()), search("youtube", "лофай"));
        assert_eq!(understand("загугли что такое etcd", &mods()), search("yandex", "что такое etcd"));
        assert_eq!(understand("поищи в интернете курс доллара", &mods()), search("yandex", "курс доллара"));
        // Открыть сам сайт — не поиск.
        assert!(!matches!(understand("открой ютуб", &mods()).first(), Some(Act::Search { .. })));
        assert!(understand("я вчера смотрел ютуб весь вечер", &mods()).iter().all(|a| !matches!(a, Act::Search { .. })));
        assert_eq!(search_url("youtube", "секреты в k8s"), "https://www.youtube.com/results?search_query=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D1%8B+%D0%B2+k8s");
    }

    fn u(said: &str) -> Vec<Act> {
        understand(said, &mods())
    }

    #[test]
    fn open_survives_misheard_verbs_and_names() {
        for said in ["Ноа, открой практику.", "Но открою практику.", "Чел, откроем модуль практика", "Ну а открой модуль практика."] {
            assert_eq!(u(said), vec![Act::Open(Thing::Own("practice"))], "{said}");
        }
        assert_eq!(u("Ноа, открою обучение."), vec![Act::Open(Thing::Own("learning"))]);
        assert_eq!(u("Чувак, открой задачи"), vec![Act::Open(Thing::Own("tasks"))]);
        assert_eq!(u("Открой, блять, обучение."), vec![Act::Open(Thing::Own("learning"))]);
    }

    #[test]
    fn self_is_not_another_window() {
        for said in ["Закройся, пожалуйста.", "Все, закройся.", "Я говорю, блядь, закройся.", "Закройся. Пока.", "закрой себя"] {
            assert_eq!(u(said), vec![Act::CloseSelf], "{said}");
        }
        assert_eq!(u("Замолчи"), vec![Act::Hush]);
        assert_eq!(u("Закрой это окошко, в котором мы разговариваем."), vec![Act::Close(Thing::Own("answer"))]);
    }

    #[test]
    fn kinds_and_kills() {
        assert_eq!(u("Закрой браузер."), vec![Act::Close(Thing::Kind("browser"))]);
        assert_eq!(u("Но, мать твою, закрой браузер."), vec![Act::Close(Thing::Kind("browser"))]);
        assert_eq!(u("снять задачу Google Chrome."), vec![Act::Kill(Thing::Program("google chrome".into()))]);
        assert_eq!(u("Ноа, сними задачу Клод."), vec![Act::Kill(Thing::Program("клод".into()))]);
        assert_eq!(u("Открой терминал, пожалуйста."), vec![Act::Open(Thing::Kind("terminal"))]);
    }

    #[test]
    fn several_things_at_once() {
        assert_eq!(u("Открою обучение и практику."), vec![Act::Open(Thing::Own("learning")), Act::Open(Thing::Own("practice"))]);
    }

    #[test]
    fn state_and_past_are_not_commands() {
        assert!(u("Он все еще открыт.").is_empty());
        assert!(u("Ты ничего не закрыл.").is_empty());
        assert!(u("Чисто на рабочем столе у меня открыт браузер").is_empty());
        assert!(u("не закрывай браузер").is_empty());
    }

    #[test]
    fn retelling_is_not_a_command() {
        assert!(u("Открываешь Клаудиаку.").is_empty());
        assert!(u("Сейчас я уберу закругление углов.").is_empty());
        assert!(u("Грубо говоря, открываешь, кликаешь, дай.").is_empty());
        assert!(u("Смотрите, как и в следующем видео.").is_empty());
        assert!(u("Вторая задержка — это трайс, экспансионная пауза и уже трав.").is_empty());
    }

    #[test]
    fn everything_and_front() {
        assert_eq!(u("Ноа, закрой все окна."), vec![Act::Close(Thing::Own("everything"))]);
        assert_eq!(u("Закрой все модули суфлёра."), vec![Act::CloseSelf]);
        assert_eq!(u("Закрой раздел этот."), vec![Act::Close(Thing::Own("front"))]);
        assert_eq!(u("Да себя закрой, нахер ты закрыл обучение, блядь."), vec![Act::CloseSelf]);
        assert_eq!(u("дебил, блядь, нахер ты обучение закрываешь? Я тебе говорю закрыться, тебе."), vec![Act::CloseSelf]);
    }

    #[test]
    fn names_sites_and_repeats() {
        assert_eq!(u("Ну а открой Brave браузер."), vec![Act::Open(Thing::Program("brave".into()))]);
        assert_eq!(u("просто откроем mail.ru"), vec![Act::Open(Thing::Site("mail.ru".into()))]);
        assert_eq!(u("Откроем Mail.ru в браузере"), vec![Act::Open(Thing::Site("mail.ru".into()))]);
        assert_eq!(u("Открой это окно с погодой."), vec![Act::Open(Thing::Module("pogodych".into()))]);
        assert_eq!(u("Открой модуль Pogodach."), vec![Act::Open(Thing::Module("pogodych".into()))]);
        assert_eq!(u("динара, открой телеграм. динара, открой телеграм."), vec![Act::Open(Thing::Program("телеграм".into()))]);
        assert_eq!(u("Открою установку и удаление программ."), vec![Act::Open(Thing::Program("установку и удаление программ".into()))]);
        assert_eq!(u("Ну-ка, а закрой диспетчер задачи."), vec![Act::Close(Thing::Program("диспетчер задачи".into()))]);
    }

    #[test]
    fn sites_by_name_and_verb_order() {
        assert_eq!(u("Ноа, открой сайт TradingView."), vec![Act::Open(Thing::Site("tradingview".into()))]);
        assert_eq!(u("открой сайт лента."), vec![Act::Open(Thing::Site("лента".into()))]);
        assert_eq!(u("Я говорю про раздел задачи, модуль задачи закрой."), vec![Act::Close(Thing::Own("tasks"))]);
        assert_eq!(u("Не вижу. Открой, разверни окно, погодыч."), vec![Act::Open(Thing::Module("pogodych".into()))]);
    }

    #[test]
    fn quoted_commands_in_a_story_are_not_commands() {
        assert!(u("Он невероятно тупой. Я говорю: «Закройся» — он закрывает мне окно. Я говорю: «Закрой браузер». Он говорит: «Нет открытых браузеров».").is_empty());
        assert!(u("Понял, спасибо, закрой эти задачи.").is_empty());
        assert!(u("Ты можешь открыть меня, Нова?").is_empty());
        assert_eq!(u("Открой окно диалоговое, а то я не понимаю команды, которую ты озвучиваешь."), vec![Act::Open(Thing::Own("answer"))]);
    }

    #[test]
    fn modules_by_name() {
        assert_eq!(u("чел, открой модуль Погодыч."), vec![Act::Open(Thing::Module("pogodych".into()))]);
        assert_eq!(u("Чел, откроем модуль погоды."), vec![Act::Open(Thing::Module("pogodych".into()))]);
    }

    #[test]
    fn timers_volume_media() {
        assert_eq!(u("Ноа, поставь таймер на две минуты."), vec![Act::Timer(120)]);
        assert_eq!(u("засеки полчаса"), vec![Act::Timer(1800)]);
        assert_eq!(u("Ну а сделай погромче компьютер."), vec![Act::Volume(Volume::Up)]);
        assert_eq!(u("Сделай громкость 80%."), vec![Act::Volume(Volume::Set(80))]);
        assert_eq!(u("выключи звук"), vec![Act::Volume(Volume::Mute)]);
        assert_eq!(u("поставь на паузу"), vec![Act::Media(Media::PlayPause)]);
        assert_eq!(u("следующий трек"), vec![Act::Media(Media::Next)]);
    }

    #[test]
    fn windows_together() {
        assert_eq!(u("Сверни все окна на рабочем столе."), vec![Act::MinimizeAll]);
        assert_eq!(u("Сверни вся программа."), vec![Act::MinimizeAll]);
        assert_eq!(u("Верни все окна на рабочем столе."), vec![Act::RestoreAll]);
    }

    #[test]
    fn hard_requests_go_to_the_model() {
        assert!(u("Открой мне на сайте Алиэкспресс, найди мне там трехмембранный микрофон").is_empty());
        assert!(u("хорошо открой почту последнее письмо").is_empty());
        assert!(u("Ноа, закрой задачу купить коту корм.").is_empty());
        assert!(u("Выключи компьютер.").is_empty());
        assert!(u("Как ты думаешь, наш модуль обучения можно улучшить?").is_empty());
    }
}

/// Прогон реальных фраз из журналов: `QUICK_PHRASES=phrases.jsonl
/// QUICK_OUT=out.txt cargo test quick_on_real_phrases -- --ignored`. Файл
/// фраз остаётся на компьютере владельца — в нём его речь.
#[cfg(test)]
#[test]
#[ignore]
fn quick_on_real_phrases() {
    let path = std::env::var("QUICK_PHRASES").expect("QUICK_PHRASES");
    let out = std::env::var("QUICK_OUT").expect("QUICK_OUT");
    let modules = vec![("pogodych".to_string(), "Погодыч".to_string()), ("afisha".to_string(), "Афиша".to_string())];
    let mut lines = Vec::new();
    let mut hits = 0;
    let mut total = 0;
    for line in std::fs::read_to_string(path).expect("файл фраз").lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("строка");
        let said = value["said"].as_str().unwrap_or_default();
        total += 1;
        let acts = understand(said, &modules);
        if !acts.is_empty() {
            hits += 1;
            lines.push(format!("{acts:?} <= {said}"));
        }
    }
    lines.push(format!("сработало: {hits} из {total}"));
    std::fs::write(out, lines.join("\n")).expect("запись");
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    #[test]
    fn casual_address_needs_a_command_right_after() {
        assert_eq!(addressed("Чувак, открой задачи").as_deref(), Some("открой задачи"));
        assert!(addressed("Чел, ну как дела вообще").is_none());
        assert!(addressed("Открой задачи").is_none());
    }

    #[test]
    fn json_replies_are_spoken_as_text() {
        assert_eq!(crate::spoken::plain("{ \"answer\": [\"Робуксы продают в магазинах\"] }"), "Робуксы продают в магазинах");
        assert_eq!(crate::spoken::plain("Обычный ответ"), "Обычный ответ");
    }
}
