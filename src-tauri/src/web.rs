//! Интернет голосом: открыть сайт, поискать на нём и узнать ответ из поиска.
//!
//! Сайты открываются в браузере человека, а не внутри программы. Там у него
//! вход, корзины, закладки, и «открой вайлдберриз» значит его Wildberries, а не
//! пустую страницу без аккаунта. Поиск по сайту — это адрес страницы поиска
//! самого сайта: «давай посмотрим чехлы для айфона» после открытого Wildberries
//! открывает его выдачу, как если бы человек набрал запрос сам.
//!
//! Вопросы к интернету — другое дело: ответ нужен голосом, а не вкладкой. Они
//! уходят в ленты новостей и Википедию, найденные фрагменты — в модель, и она
//! отвечает по ним. Модель на этой машине не знает сегодняшних новостей: без
//! поиска она бы их выдумала. Часы работы и адреса — это карты: обычная выдача
//! поисковиков программам их не отдаёт, и место открывается в Яндекс Картах.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;

/* ── Сайты ──────────────────────────────────────────────────────────────── */

struct Site {
    /// Как сайт называют вслух.
    names: &'static [&'static str],
    /// Как он называется в ответе.
    title: &'static str,
    home: &'static str,
    /// Страница поиска; `{q}` заменяется запросом.
    search: &'static str,
}

const SITES: &[Site] = &[
    Site {
        names: &["вайлдберриз", "валберис", "wildberries", "вб"],
        title: "Wildberries",
        home: "https://www.wildberries.ru",
        search: "https://www.wildberries.ru/catalog/0/search.aspx?search={q}",
    },
    Site {
        names: &["озон", "ozon"],
        title: "Ozon",
        home: "https://www.ozon.ru",
        search: "https://www.ozon.ru/search/?text={q}",
    },
    Site {
        names: &["яндекс маркет", "маркет"],
        title: "Яндекс Маркет",
        home: "https://market.yandex.ru",
        search: "https://market.yandex.ru/search?text={q}",
    },
    Site {
        names: &["авито", "avito"],
        title: "Авито",
        home: "https://www.avito.ru",
        search: "https://www.avito.ru/rossiya?q={q}",
    },
    Site {
        names: &["ютуб", "youtube"],
        title: "YouTube",
        home: "https://www.youtube.com",
        search: "https://www.youtube.com/results?search_query={q}",
    },
    Site {
        names: &["яндекс", "yandex"],
        title: "Яндекс",
        home: "https://ya.ru",
        search: "https://ya.ru/search/?text={q}",
    },
    Site {
        names: &["гугл", "google"],
        title: "Google",
        home: "https://www.google.com",
        search: "https://www.google.com/search?q={q}",
    },
    Site {
        names: &["википедия", "wikipedia"],
        title: "Википедия",
        home: "https://ru.wikipedia.org",
        search: "https://ru.wikipedia.org/w/index.php?search={q}",
    },
    Site {
        names: &["яндекс карты", "карты"],
        title: "Яндекс Карты",
        home: "https://yandex.ru/maps",
        search: "https://yandex.ru/maps/?text={q}",
    },
    Site {
        names: &["2гис", "два гис", "2gis"],
        title: "2ГИС",
        home: "https://2gis.ru",
        search: "https://2gis.ru/search/{q}",
    },
    Site {
        names: &["кинопоиск", "kinopoisk"],
        title: "Кинопоиск",
        home: "https://www.kinopoisk.ru",
        search: "https://www.kinopoisk.ru/index.php?kp_query={q}",
    },
    Site {
        names: &["хедхантер", "hh", "эйчэйч"],
        title: "hh.ru",
        home: "https://hh.ru",
        search: "https://hh.ru/search/vacancy?text={q}",
    },
    Site {
        names: &["вконтакте", "вк", "vk"],
        title: "ВКонтакте",
        home: "https://vk.com",
        search: "https://vk.com/search?c%5Bq%5D={q}",
    },
    Site {
        names: &["гитхаб", "github"],
        title: "GitHub",
        home: "https://github.com",
        search: "https://github.com/search?q={q}",
    },
    Site {
        names: &["вкусвилл", "vkusvill"],
        title: "ВкусВилл",
        home: "https://vkusvill.ru",
        search: "https://vkusvill.ru/search/?q={q}",
    },
];

/// Сайт, открытый последним.
///
/// «Открой вайлдберриз», а следом «давай посмотрим чехлы» — второй просьбе
/// сайт не нужен: он тот же. Держим его здесь и подсказываем разбору реплики.
static LAST: Mutex<Option<usize>> = Mutex::new(None);

/// Строка для разбора реплики: какой сайт открыт. Пусто — никакой.
pub fn site_line() -> String {
    match *LAST.lock().unwrap_or_else(|err| err.into_inner()) {
        Some(index) => format!("Последний открытый сайт: {}.", SITES[index].title),
        None => String::new(),
    }
}

fn find_site(said: &str) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, site) in SITES.iter().enumerate() {
        for name in site.names {
            let value = crate::pc::score(said, name);
            if value >= 0.6 && best.map_or(true, |(_, top)| value > top) {
                best = Some((index, value));
            }
        }
    }
    best.map(|(index, _)| index)
}

/// Открывает сайт или его поиск в браузере человека и отдаёт ответ вслух.
///
/// Сайт не назван — ищем на последнем открытом. Сайт незнакомый — открываем
/// первый результат поиска по его названию: «открой сайт театра на Таганке»
/// должно вести на сайт театра, а не на страницу поисковика.
pub fn open_site(site: &str, query: &str) -> String {
    let (site, query) = (site.trim(), query.trim());
    let known = if site.is_empty() {
        *LAST.lock().unwrap_or_else(|err| err.into_inner())
    } else {
        find_site(site)
    };

    let (url, spoken) = match known {
        Some(index) => {
            let chosen = &SITES[index];
            *LAST.lock().unwrap_or_else(|err| err.into_inner()) = Some(index);
            if query.is_empty() {
                (chosen.home.to_string(), format!("Открываю {}.", chosen.title))
            } else {
                (
                    chosen.search.replace("{q}", &encode(query)),
                    format!("Ищу «{query}» на {}.", chosen.title),
                )
            }
        }
        None if site.contains('.') && !site.contains(' ') => {
            let bare = site
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            (format!("https://{bare}"), format!("Открываю {bare}."))
        }
        None if !site.is_empty() => {
            let wanted = if query.is_empty() {
                site.to_string()
            } else {
                format!("{site} {query}")
            };
            // Обратная косая черта в начале запроса — «мне повезёт» у
            // DuckDuckGo: сразу первый результат, без страницы выдачи.
            (
                format!("https://duckduckgo.com/?q=%5C{}", encode(&wanted)),
                format!("Открываю {site}."),
            )
        }
        None if !query.is_empty() => (
            format!("https://ya.ru/search/?text={}", encode(query)),
            format!("Ищу «{query}»."),
        ),
        None => return "Не понял, какой сайт открыть.".into(),
    };

    match crate::pc::open(&url) {
        Ok(()) => spoken,
        Err(err) => format!("Браузер не открылся: {err}."),
    }
}

/// Открывает поиск на известном сайте, если это он. `None` — сайт незнакомый.
///
/// Нужна заказу: «закажи чехол на вайлдберриз» разбор иногда принимает за
/// заказ продуктов в магазине «wildberries». Заказать там Ноа не может, но
/// открыть поиск на самом сайте — может, и это ровно то, чего человек ждёт.
pub fn open_known(site: &str, query: &str) -> Option<String> {
    find_site(site)?;
    Some(open_site(site, query))
}

/// Проценты вместо небезопасных байтов; пробел — `%20`.
fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/* ── Ответ из поиска ────────────────────────────────────────────────────── */

/// Один результат поиска.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    title: String,
    /// Сайт, откуда результат, — его называют в ответе.
    site: String,
    snippet: String,
}

/// Обычный поиск — запасной путь, когда ни новости, ни Википедия ничего не
/// нашли.
///
/// Только DuckDuckGo. Bing отсюда убран: программам он отдаёт посторонние
/// страницы — на запрос про Олимпиаду в Париже вернул Roblox и Microsoft
/// Teams, — и ответ по ним хуже, чем честное «не нашёл».
const ENGINES: &[(&str, &str)] = &[("DuckDuckGo", "https://lite.duckduckgo.com/lite/?q={q}")];

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Отвечает на вопрос по свежему поиску и отдаёт ответ вслух.
pub async fn lookup(app: &AppHandle, question: &str) -> String {
    let question = question.trim();
    if question.is_empty() {
        return "Не понял, что посмотреть.".into();
    }

    // Часы работы, адрес, как доехать — это карты: там они точные и в городе
    // человека, а выдача поисковиков программам их не отдаёт.
    if about_place(question) {
        let url = format!("https://yandex.ru/maps/?text={}", encode(question));
        return match crate::pc::open(&url) {
            Ok(()) => "Открыл в Яндекс Картах — часы работы и адрес там.".into(),
            Err(err) => format!("Карты не открылись: {err}."),
        };
    }

    // Новости и Википедия; не нашлось — обычный поиск; и там пусто — поиск
    // открывается в браузере: честнее, чем ответ по посторонним страницам.
    let hits = match evidence(question).await {
        Ok(hits) => hits,
        Err(_) => match search(question).await {
            Ok(hits) => hits,
            Err(err) => {
                log::warn!("поиск «{question}» не удался: {err}");
                let _ = crate::pc::open(&format!("https://ya.ru/search/?text={}", encode(question)));
                return "Ответа не нашёл — открыл поиск в браузере.".into();
            }
        },
    };
    log::info!("поиск «{question}»: результатов {}", hits.len());

    let found = hits
        .iter()
        .take(6)
        .enumerate()
        .map(|(at, hit)| format!("[{}] {} ({})\n{}", at + 1, hit.title, hit.site, hit.snippet))
        .collect::<Vec<_>>()
        .join("\n\n");

    let rules = format!(
        "Ты — Ноа, голосовой помощник. Ответь на вопрос человека по фрагментам из \
         поиска ниже: одним-двумя короткими предложениями, по-русски, обычным \
         текстом, без списков и ссылок. Опирайся только на фрагменты; если ответа \
         в них нет, так и скажи — не выдумывай. {} Источник, если он важен, назови \
         словом, без адреса сайта.\n\nФрагменты:\n{found}",
        crate::planner::now_line()
    );

    let provider = app.state::<AppState>().provider();
    match native_answer(provider.as_ref(), &rules, question, 3).await {
        Some(answer) => answer,
        // Модель не ответила — лучше прочитать лучший фрагмент как есть, чем
        // промолчать: поиск-то сработал.
        None => format!("Нашёл: {}. {}", hits[0].title, hits[0].snippet),
    }
}

/// Проверяет, правда ли написанное, — по свежему поиску.
///
/// Запрос — само утверждение, которое модель выписывает из текста (см.
/// `main_claim`). Ответ — «Правда», «Фейк» или «Не подтверждается» и один факт в
/// доказательство: он идёт вслух, и ссылки, адреса и перечни сайтов в нём не
/// нужны. Модели прямо сказано не судить по своей памяти: сегодняшних новостей
/// она не знает.
pub async fn fact_check(app: &AppHandle, text: &str, question: &str) -> String {
    let provider = app.state::<AppState>().provider();
    check_with(provider.as_ref(), text, question).await
}

/// Проверка — отдельно от программы: модель передаётся снаружи.
async fn check_with<P>(provider: &P, text: &str, question: &str) -> String
where
    P: crate::ai_client::AiProvider + ?Sized,
{
    let claim = match main_claim(provider, text).await {
        Claim::Found(claim) => claim,
        Claim::Nothing => {
            return "Здесь нечего проверять: в тексте нет утверждения о фактах.".into();
        }
        // Модель не ответила — берётся самая содержательная строка.
        Claim::Unknown => claim_query(text),
    };
    if claim.is_empty() {
        return "Не нашёл, что проверять.".into();
    }
    let hits = match evidence(&claim).await {
        Ok(hits) => hits,
        Err(err) => {
            log::warn!("проверка «{claim}»: поиск не удался: {err}");
            return "Проверить не вышло: поиск сейчас не отвечает.".into();
        }
    };
    log::info!("проверка «{claim}»: результатов {}", hits.len());

    let rules = verdict_rules(&claim, &fragments(&hits));
    verdict(provider, &claim, &rules, question)
        .await
        .unwrap_or_else(|| "Проверить не вышло: модель не ответила.".into())
}

/// Фрагменты поиска для модели. Сайты не подписываются: модель тянет их в
/// ответ, а адреса вслух не нужны.
fn fragments(hits: &[Hit]) -> String {
    hits.iter()
        .take(8)
        .enumerate()
        .map(|(at, hit)| format!("[{}] {}\n{}", at + 1, hit.title, hit.snippet))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Правила вывода проверки.
///
/// Сначала — что говорят найденные страницы, потом вывод. С выводом первым
/// словом маленькая модель ставила его наугад: вживую Олимпиаду 2024 в Париже,
/// подтверждённую первыми же фрагментами, назвала фейком.
fn verdict_rules(claim: &str, found: &str) -> String {
    format!(
        "Сравни утверждение «{claim}» с фрагментами свежего поиска ниже. Ответь только \
         JSON: {{\"fact\": \"...\", \"verdict\": \"...\"}}. fact — сам факт из \
         фрагментов, который решает дело: что именно произошло — с датой, местом или \
         числом, если они есть, — одной короткой фразой по-русски, без ссылок и \
         названий сайтов. Не пиши «утверждение подтверждается» — пиши, что именно \
         сказано во фрагментах. verdict — «правда», если фрагменты подтверждают \
         утверждение по сути; «фейк», если прямо опровергают; «не подтверждается», если \
         о нём в них нет. Суди только по фрагментам, не по памяти; то, что событие было \
         в прошлом, не делает утверждение ложным. {}\n\nФрагменты поиска:\n{found}",
        crate::planner::now_line()
    )
}

/// Вывод проверки для голоса: «Правда. Игры прошли в Париже.»
///
/// «Правда» и «Фейк» говорятся, только если найденный факт выдерживает второй,
/// узкий вопрос — подтверждает (опровергает) ли он утверждение целиком. Без
/// него новость «Маск купил энергетическую компанию» засчитывалась как
/// подтверждение того, что он купил Луну. Нет конкретного факта — вывод не
/// проверить, и «правду» наугад Ноа не говорит.
async fn verdict<P>(provider: &P, claim: &str, rules: &str, question: &str) -> Option<String>
where
    P: crate::ai_client::AiProvider + ?Sized,
{
    const UNCONFIRMED: &str = "Не подтверждается.";
    let answer = provider.interpret(rules, question).await.ok()?;
    let parsed = answer
        .find('{')
        .zip(answer.rfind('}'))
        .filter(|(from, to)| from < to)
        .and_then(|(from, to)| serde_json::from_str::<serde_json::Value>(&answer[from..=to]).ok())?;
    let word = match parsed["verdict"].as_str().unwrap_or_default().trim().to_lowercase().as_str() {
        "правда" => "Правда",
        "фейк" => "Фейк",
        _ => "Не подтверждается",
    };
    let fact = spoken(parsed["fact"].as_str().unwrap_or_default(), 1);
    // «Утверждение подтверждается» — не факт, а пересказ вывода.
    let retold = ["утвержден", "подтвержда", "фрагмент", "является правд", "является ложн"]
        .iter()
        .any(|word| fact.to_lowercase().contains(word));
    if fact.is_empty() || retold || crate::ai_client::has_foreign_script(&fact) {
        return Some(UNCONFIRMED.into());
    }
    let fact = {
        let mut letters = fact.chars();
        match letters.next() {
            Some(first) => first.to_uppercase().chain(letters).collect::<String>(),
            None => String::new(),
        }
    };
    let fact = if fact.ends_with(['.', '!', '?']) { fact } else { format!("{fact}.") };

    if word != "Не подтверждается" {
        let relation = if word == "Правда" { "Подтверждает" } else { "Опровергает" };
        let check = format!(
            "Утверждение: «{claim}». Факт из новостей: «{}». {relation} ли этот факт \
             утверждение целиком — все его части, а не только упомянутых людей и места?",
            fact.trim_end_matches('.')
        );
        let agrees = provider
            .interpret("Отвечай одним словом: да или нет.", &check)
            .await
            .map(|answer| answer.trim().to_lowercase().starts_with("да"))
            .unwrap_or(false);
        if !agrees {
            log::info!("вывод «{word}» не выдержал перепроверки: «{fact}»");
            return Some(UNCONFIRMED.into());
        }
    }
    Some(format!("{word}. {fact}"))
}

/// Что нашлось в тексте для проверки.
enum Claim {
    Found(String),
    /// Проверять нечего: переписка, реклама, меню.
    Nothing,
    /// Модель не ответила.
    Unknown,
}

/// Главное утверждение текста — одной фразой для поиска.
///
/// Скриншот начинается с меню, дат и кнопок, и первая длинная строка бывала
/// обрывком интерфейса: вживую в поиск ушло «но какие из них можно было бы
/// взять на», и проверялось совсем не то. Утверждение выписывает модель.
async fn main_claim<P>(provider: &P, text: &str) -> Claim
where
    P: crate::ai_client::AiProvider + ?Sized,
{
    // Ответ — JSON. На свободный ответ со словом «нет» для случая «нечего
    // проверять» модель отвечала «нет» и на голое утверждение: «Олимпиада 2024
    // прошла в Париже» проверять оказывалось нечего.
    const RULES: &str = "Тебе дают текст, который человек скопировал или \
        сфотографировал с экрана: новость, пост или переписку — вместе с меню, \
        датами и кнопками. Найди главное утверждение о фактах, которое можно \
        проверить поиском в интернете. Ответь только JSON: {\"claim\": \
        \"утверждение одной фразой до двенадцати слов, по-русски\", \"checkable\": \
        true}. Если проверять нечего — это переписка о личном, реклама или меню, — \
        ответь {\"claim\": \"\", \"checkable\": false}.";
    let text: String = text.chars().take(3000).collect();
    let Ok(answer) = provider.interpret(RULES, &text).await else {
        return Claim::Unknown;
    };
    let Some(parsed) = answer
        .find('{')
        .zip(answer.rfind('}'))
        .filter(|(from, to)| from < to)
        .and_then(|(from, to)| serde_json::from_str::<serde_json::Value>(&answer[from..=to]).ok())
    else {
        return Claim::Unknown;
    };
    if parsed["checkable"].as_bool() == Some(false) {
        return Claim::Nothing;
    }
    let claim = parsed["claim"].as_str().unwrap_or_default().trim().to_string();
    if claim.is_empty() || crate::ai_client::has_foreign_script(&claim) {
        return Claim::Unknown;
    }
    Claim::Found(claim.split_whitespace().take(16).collect::<Vec<_>>().join(" "))
}

/// Ответ модели для голоса: по-русски, без ссылок и хвостов, не длиннее
/// `sentences` предложений.
///
/// Мелкая модель на длинном тексте срывается на другой язык — вживую ответ о
/// новости ушёл в китайский, — поэтому такой ответ переспрашивается и всё
/// равно вычищается.
pub(crate) async fn native_answer<P>(
    provider: &P,
    rules: &str,
    question: &str,
    sentences: usize,
) -> Option<String>
where
    P: crate::ai_client::AiProvider + ?Sized,
{
    let mut answer = provider.interpret(rules, question).await.ok()?;
    if crate::ai_client::has_foreign_script(&answer) {
        log::warn!("ответ сорвался на другой язык — переспрашиваю");
        if let Ok(second) = provider.interpret(rules, question).await {
            answer = second;
        }
    }
    let answer = crate::ai_client::purge(&answer).ok()?;
    let said = spoken(&answer, sentences);
    (!said.is_empty()).then_some(said)
}

/// Текст для голоса: без ссылок, адресов, разметки и перечня источников, не
/// длиннее `sentences` предложений.
///
/// Вживую модель дописывала «Источники: wikipedia.org, benchchem.com…» и
/// ссылки в разметке — и всё это читалось вслух.
pub(crate) fn spoken(answer: &str, sentences: usize) -> String {
    const TAILS: &[&str] = &[
        "Источники:", "источники:", "Источник:", "источник:", "Сайты", "сайты для",
        "Вывод основ", "вывод основ", "Ссылки:", "ссылки:",
    ];
    let cut = TAILS
        .iter()
        .filter_map(|tail| answer.find(tail))
        .min()
        .unwrap_or(answer.len());
    let body = drop_linked_parts(&strip_markdown_links(&answer[..cut])).replace(['*', '#'], "");
    let words: Vec<&str> = body
        .split_whitespace()
        .filter(|word| !word.contains("http") && !word.contains("www.") && !looks_like_domain(word))
        // Номера списка — «1.», «2)» — вслух читались бы числами.
        .filter(|word| {
            let bare = word.trim_end_matches(['.', ')']);
            !(bare.len() < word.len() && !bare.is_empty() && bare.chars().all(|ch| ch.is_ascii_digit()))
        })
        .map(|word| word.trim_start_matches(['-', '•']))
        .filter(|word| !word.is_empty())
        .collect();
    let flat = words.join(" ").replace(" .", ".").replace(" ,", ",");
    first_sentences(&flat, sentences)
}

/// Ссылки разметки — «[CARLA](https://…)» — становятся просто текстом.
fn strip_markdown_links(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](") else {
            break;
        };
        let Some(end) = rest[open + close..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..open + close]);
        rest = &rest[open + close + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Скобки с адресами — «(ru.wikipedia.org, olymps.ru)» — выбрасываются целиком.
fn drop_linked_parts(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let Some(length) = rest[open..].find(')') else {
            break;
        };
        let inner = &rest[open + 1..open + length];
        out.push_str(&rest[..open]);
        let linked = inner.contains("http")
            || inner
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .any(looks_like_domain);
        if !linked {
            out.push_str(&rest[open..=open + length]);
        }
        rest = &rest[open + length + 1..];
    }
    out.push_str(rest);
    out
}

/// Похоже ли слово на адрес сайта: «wikipedia.org», «ru.wikipedia.org».
fn looks_like_domain(word: &str) -> bool {
    let bare = word.trim_matches(|ch: char| !ch.is_alphanumeric());
    match bare.rsplit_once('.') {
        Some((name, zone)) => {
            name.chars().any(|ch| ch.is_ascii_alphanumeric())
                && (2..=6).contains(&zone.len())
                && zone.chars().all(|ch| ch.is_ascii_lowercase())
        }
        None => false,
    }
}

/// Первые `count` предложений. Точка внутри числа — «25.39» — концом не считается.
fn first_sentences(text: &str, count: usize) -> String {
    let mut seen = 0;
    for (at, ch) in text.char_indices() {
        if !matches!(ch, '.' | '!' | '?') {
            continue;
        }
        let after = at + ch.len_utf8();
        if text[after..].chars().next().map_or(true, char::is_whitespace) {
            seen += 1;
            if seen == count {
                return text[..after].trim().to_string();
            }
        }
    }
    text.trim().to_string()
}

/// Запрос для проверки: самая содержательная из первых строк текста.
///
/// Скриншот новости начинается с обвязки сайта — меню, даты, «Реклама», — а
/// заголовок обычно самая длинная из первых строк. Берётся она, до
/// четырнадцати слов: длиннее поисковик режет сам и находит хуже.
fn claim_query(text: &str) -> String {
    let best = text
        .lines()
        .map(str::trim)
        .filter(|line| line.split_whitespace().count() >= 4)
        .take(6)
        .max_by_key(|line| line.split_whitespace().count())
        .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or_default();
    best.split_whitespace().take(14).collect::<Vec<_>>().join(" ")
}

/// Свидетельства для проверки утверждения: свежие новости и справка.
///
/// Ленты новостей Google и Bing и поиск Википедии — адреса для программ (RSS
/// и API), и отвечают они по делу. Обычная выдача поисковиков программам для
/// этого больше не годится: DuckDuckGo отдаёт заглушку для роботов, а Bing на
/// запрос про Олимпиаду в Париже вернул страницы про Roblox и Microsoft Teams.
/// Все три опрашиваются сразу: у каждого своя сторона дела.
async fn evidence(query: &str) -> Result<Vec<Hit>, String> {
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))?;
    let q = encode(query);
    let fetch = |url: String| {
        let client = client.clone();
        async move {
            let response = client
                .get(&url)
                .header("User-Agent", USER_AGENT)
                .header("Accept-Language", "ru-RU,ru;q=0.9")
                .send()
                .await
                .ok()?;
            if !response.status().is_success() {
                return None;
            }
            response.text().await.ok()
        }
    };
    let (google, bing, wikipedia) = tokio::join!(
        fetch(format!("https://news.google.com/rss/search?q={q}&hl=ru&gl=RU&ceid=RU:ru")),
        fetch(format!("https://www.bing.com/news/search?q={q}&format=rss&setlang=ru")),
        fetch(format!(
            "https://ru.wikipedia.org/w/api.php?action=query&list=search&format=json&srlimit=4&srsearch={q}"
        )),
    );

    let mut hits = Vec::new();
    hits.extend(google.as_deref().map(parse_rss).unwrap_or_default().into_iter().take(5));
    hits.extend(bing.as_deref().map(parse_rss).unwrap_or_default().into_iter().take(4));
    hits.extend(wikipedia.as_deref().map(parse_wikipedia).unwrap_or_default().into_iter().take(3));
    // Одна новость в двух лентах — один раз.
    let mut seen = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(hit.title.to_lowercase()));
    if hits.is_empty() {
        return Err("ни ленты новостей, ни Википедия ничего не нашли".into());
    }
    Ok(hits)
}

/// Новости из ленты RSS: заголовок, источник, дата и описание.
fn parse_rss(xml: &str) -> Vec<Hit> {
    xml.split("<item>")
        .skip(1)
        .filter_map(|item| {
            let field = |tag: &str| {
                between(item, &format!("<{tag}>"), &format!("</{tag}>")).map(|raw| {
                    clean(&unescape(
                        raw.trim().trim_start_matches("<![CDATA[").trim_end_matches("]]>"),
                    ))
                })
            };
            let full = field("title").filter(|title| !title.is_empty())?;
            // У Google источник — хвост заголовка после « - »: он отдельно, а в
            // заголовке остаётся только сама новость.
            let (title, site) = match full.rsplit_once(" - ") {
                Some((headline, site)) => (headline.to_string(), site.to_string()),
                None => (full.clone(), String::new()),
            };
            let date = field("pubDate").unwrap_or_default();
            // Описание у Google — тот же заголовок ссылкой; такое не нужно.
            let description = field("description")
                .filter(|text| !text.contains(title.as_str()))
                .unwrap_or_default();
            let snippet = [date, description]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" — ");
            Some(Hit { title, site, snippet })
        })
        .collect()
}

/// Статьи из поиска Википедии: название и кусок текста с найденными словами.
fn parse_wikipedia(json: &str) -> Vec<Hit> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    parsed["query"]["search"]
        .as_array()
        .map(|pages| {
            pages
                .iter()
                .filter_map(|page| {
                    let title = page["title"].as_str()?;
                    Some(Hit {
                        title: format!("Википедия: {title}"),
                        site: "ru.wikipedia.org".into(),
                        snippet: clean(page["snippet"].as_str().unwrap_or_default()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// HTML-сущности, в том числе числовые — «&#0183;», «&#171;».
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            _ => entity
                .strip_prefix('#')
                .and_then(|code| match code.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => code.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Вопрос про место: часы работы, адрес, дорога.
fn about_place(question: &str) -> bool {
    let lower = question.to_lowercase();
    [
        "до скольки", "часы работы", "режим работы", "во сколько открыва",
        "во сколько закрыва", "работает ли", "адрес", "где находится", "как добраться",
        "как доехать", "как пройти",
    ]
    .iter()
    .any(|words| lower.contains(words))
}

async fn search(query: &str) -> Result<Vec<Hit>, String> {
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))?;

    let mut problems = Vec::new();
    for (name, template) in ENGINES {
        let url = template.replace("{q}", &encode(query));
        let page = client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Accept-Language", "ru-RU,ru;q=0.9")
            .send()
            .await;
        let html = match page {
            Ok(response) if response.status().is_success() => {
                response.text().await.unwrap_or_default()
            }
            Ok(response) => {
                problems.push(format!("{name}: {}", response.status()));
                continue;
            }
            Err(err) => {
                problems.push(format!("{name}: {err}"));
                continue;
            }
        };

        let hits = match *name {
            "DuckDuckGo" => parse_duckduckgo(&html),
            _ => parse_bing(&html),
        };
        if !hits.is_empty() {
            return Ok(hits);
        }
        problems.push(format!("{name}: пусто"));
    }
    Err(problems.join("; "))
}

/// Выдача облегчённого DuckDuckGo: ссылка `result-link`, под ней
/// `result-snippet` и адрес в `link-text`.
fn parse_duckduckgo(html: &str) -> Vec<Hit> {
    html.split("class='result-link'")
        .skip(1)
        .take(8)
        .filter_map(|chunk| {
            let title = clean(between(chunk, ">", "</a>")?);
            let snippet = between(chunk, "class='result-snippet'>", "</td>")
                .map(clean)
                .unwrap_or_default();
            let site = between(chunk, "class='link-text'>", "</span>")
                .map(clean)
                .map(|address| address.split('/').next().unwrap_or_default().to_string())
                .unwrap_or_default();
            (!title.is_empty()).then_some(Hit { title, site, snippet })
        })
        .collect()
}

/// Выдача Bing: блоки `b_algo`, заголовок в `h2`, сайт в `tptt`, текст в
/// абзаце `b_lineclamp`.
fn parse_bing(html: &str) -> Vec<Hit> {
    html.split("<li class=\"b_algo\"")
        .skip(1)
        .take(8)
        .filter_map(|chunk| {
            let heading = between(chunk, "<h2", "</h2>")?;
            let title = clean(&heading[heading.find('>')? + 1..]);
            let site = between(chunk, "<div class=\"tptt\">", "</div>")
                .map(clean)
                .unwrap_or_default();
            let snippet = between(chunk, "<p class=\"b_lineclamp", "</p>")
                .and_then(|paragraph| paragraph.find('>').map(|at| &paragraph[at + 1..]))
                .map(clean)
                .unwrap_or_default();
            (!title.is_empty()).then_some(Hit { title, site, snippet })
        })
        .collect()
}

fn between<'a>(text: &'a str, from: &str, to: &str) -> Option<&'a str> {
    let start = text.find(from)? + from.len();
    let end = text[start..].find(to)? + start;
    Some(&text[start..end])
}

/// Текст без разметки и HTML-сущностей, пробелы схлопнуты.
fn clean(fragment: &str) -> String {
    let mut text = String::with_capacity(fragment.len());
    let mut in_tag = false;
    for ch in fragment.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    let text = unescape(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_are_found_by_how_they_are_said() {
        let title = |said: &str| find_site(said).map(|index| SITES[index].title);
        assert_eq!(title("вайлдберриз"), Some("Wildberries"));
        assert_eq!(title("Wildberries"), Some("Wildberries"));
        assert_eq!(title("озон"), Some("Ozon"));
        assert_eq!(title("яндекс маркет"), Some("Яндекс Маркет"));
        assert_eq!(title("ютуб"), Some("YouTube"));
        assert_eq!(title("сайт кинотеатра октябрь"), None);
    }

    #[test]
    fn queries_are_encoded_for_the_address() {
        assert_eq!(encode("iphone 14"), "iphone%2014");
        assert_eq!(encode("чехол"), "%D1%87%D0%B5%D1%85%D0%BE%D0%BB");
    }

    #[test]
    fn duckduckgo_results_are_read() {
        let html = r#"<tr><td><a rel="nofollow" href="//duckduckgo.com/l/?uddg=x" class='result-link'>Уют, кафе, пер. Сивцев Вражек, 43 — Яндекс Карты</a></td></tr>
            <tr><td class='result-snippet'> <b>Кафе</b> «Уют»: часы работы 09:00 - 18:00. </td></tr>
            <tr><td><span class='link-text'>yandex.ru/maps/org/uyut/147590784274/</span></td></tr>"#;
        let hits = parse_duckduckgo(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Уют, кафе, пер. Сивцев Вражек, 43 — Яндекс Карты");
        assert_eq!(hits[0].site, "yandex.ru");
        assert_eq!(hits[0].snippet, "Кафе «Уют»: часы работы 09:00 - 18:00.");
    }

    #[test]
    fn bing_results_are_read() {
        let html = r#"<ol><li class="b_algo" data-id><div class="tptt">restoclub.ru</div>
            <h2 class=""><a href="https://www.bing.com/ck/a?u=1"><strong>Уют</strong> на Сивцевом Вражке</a></h2>
            <div class="b_caption"><p class="b_lineclamp2">Ежедневно с 9:00 до 18:00.</p></div></li></ol>"#;
        let hits = parse_bing(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Уют на Сивцевом Вражке");
        assert_eq!(hits[0].site, "restoclub.ru");
        assert_eq!(hits[0].snippet, "Ежедневно с 9:00 до 18:00.");
    }

    #[test]
    fn markup_and_entities_leave_only_text() {
        assert_eq!(clean(" <b>Кафе</b>&nbsp;&laquo;Уют&raquo; &amp; бар "), "Кафе «Уют» & бар");
    }
}

#[cfg(test)]
mod checking {
    use super::{about_place, claim_query, parse_rss, parse_wikipedia, spoken, unescape};

    #[test]
    fn places_go_to_the_map() {
        assert!(about_place("до скольки работает кафе Уют на Сивцевом Вражке"));
        assert!(about_place("где находится ближайшая аптека"));
        assert!(!about_place("кто выиграл вчерашний матч"));
    }

    #[test]
    fn news_feeds_are_read() {
        let google = "<rss><channel><title>Лента</title><item><title>В Париже завершилась \
            Олимпиада-2024 - Ведомости</title><pubDate>Mon, 12 Aug 2024 07:00:00 GMT</pubDate>\
            <description>&lt;a href=\"x\"&gt;В Париже завершилась Олимпиада-2024 - \
            Ведомости&lt;/a&gt;</description></item></channel></rss>";
        let hits = parse_rss(google);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "В Париже завершилась Олимпиада-2024");
        assert_eq!(hits[0].site, "Ведомости");
        assert_eq!(hits[0].snippet, "Mon, 12 Aug 2024 07:00:00 GMT");

        let bing = "<item><title><![CDATA[Париж примет игры]]></title><description>Игры \
            пройдут &#171;летом&#187; 2024 года</description></item>";
        let hits = parse_rss(bing);
        assert_eq!(hits[0].title, "Париж примет игры");
        assert_eq!(hits[0].snippet, "Игры пройдут «летом» 2024 года");
    }

    #[test]
    fn wikipedia_search_is_read() {
        let json = r#"{"query":{"search":[{"title":"Летние Олимпийские игры 2024",
            "snippet":"прошли в <span class=\"searchmatch\">Париже</span>"}]}}"#;
        let hits = parse_wikipedia(json);
        assert_eq!(hits[0].title, "Википедия: Летние Олимпийские игры 2024");
        assert_eq!(hits[0].snippet, "прошли в Париже");
    }

    #[test]
    fn numeric_entities_are_decoded() {
        assert_eq!(unescape("16 апр.&#0183;&#32;текст &amp; &#x41;"), "16 апр.· текст & A");
        assert_eq!(unescape("A & B"), "A & B");
    }

    #[test]
    fn the_spoken_answer_has_no_links_and_no_tail() {
        let answer = "Правда. Игры прошли в Париже с 26 июля по 11 августа 2024 года \
                      (ru.wikipedia.org, olymps.ru). Это подтверждают несколько источников.\n\n\
                      Источники: wikipedia.org";
        assert_eq!(
            spoken(answer, 2),
            "Правда. Игры прошли в Париже с 26 июля по 11 августа 2024 года."
        );
        let answer = "Не подтверждается. Проект есть на [CARLA Simulator](https://carla.readthedocs.io/).\n\
                      1. Другие ресурсы";
        assert_eq!(spoken(answer, 2), "Не подтверждается. Проект есть на CARLA Simulator.");
        assert_eq!(
            spoken("**Фейк**. Курс 25.39 никто не обещал.", 2),
            "Фейк. Курс 25.39 никто не обещал."
        );
    }

    #[test]
    fn the_claim_is_the_headline_not_the_menu() {
        let text = "Новости\nГлавное\n12:40\nВ Москве с понедельника отменят все электрички на два \
                    года, сообщили в мэрии\nРеклама";
        assert_eq!(
            claim_query(text),
            "В Москве с понедельника отменят все электрички на два года, сообщили в мэрии"
        );
    }
}

#[cfg(test)]
mod live {
    /// Что видит проверка: утверждение, фрагменты поиска и сырой ответ модели.
    ///
    /// `cargo test --lib web::live::what_the_check_sees -- --ignored --nocapture`
    #[test]
    #[ignore = "ходит в поиск и в локальную модель"]
    fn what_the_check_sees() {
        use crate::ai_client::AiProvider;

        let config = crate::config::AiConfig {
            endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
            model: "qwen2.5:7b".into(),
            ..Default::default()
        };
        let provider = crate::ai_client::HttpProvider::new(&config, "ru").expect("провайдер");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for text in ["Летние Олимпийские игры 2024 года прошли в Париже"] {
            let claim = match runtime.block_on(super::main_claim(&provider, text)) {
                super::Claim::Found(claim) => claim,
                _ => text.to_string(),
            };
            let hits = runtime.block_on(super::evidence(&claim)).unwrap_or_default();
            for hit in &hits {
                println!("  · {} ({})\n    {}", hit.title, hit.site, hit.snippet);
            }
            let found = super::fragments(&hits);
            let rules = super::verdict_rules(&claim, &found);
            let raw = runtime
                .block_on(provider.interpret(&rules, "это правда?"))
                .unwrap_or_default();
            println!("утверждение: {claim}\nответ модели: {raw}\n");
        }
    }

    /// `cargo test --lib web::live -- --ignored --nocapture`
    #[test]
    #[ignore = "ходит в поиск и в локальную модель"]
    fn a_claim_is_checked_against_the_news() {
        let config = crate::config::AiConfig {
            endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
            model: "qwen2.5:7b".into(),
            ..Default::default()
        };
        let provider = crate::ai_client::HttpProvider::new(&config, "ru").expect("провайдер");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for text in [
            "Летние Олимпийские игры 2024 года прошли в Париже",
            "Илон Маск купил Луну и переименовал её в Теслу, сообщили в NASA",
        ] {
            let started = std::time::Instant::now();
            let answer = runtime.block_on(super::check_with(&provider, text, "это правда?"));
            println!("{} мс — «{text}»:\n  {answer}\n", started.elapsed().as_millis());
            assert!(!answer.is_empty());
        }
    }
}
