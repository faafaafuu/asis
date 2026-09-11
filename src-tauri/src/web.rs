//! Интернет голосом: открыть сайт, поискать на нём и узнать ответ из поиска.
//!
//! Сайты открываются в браузере человека, а не внутри программы. Там у него
//! вход, корзины, закладки, и «открой вайлдберриз» значит его Wildberries, а не
//! пустую страницу без аккаунта. Поиск по сайту — это адрес страницы поиска
//! самого сайта: «давай посмотрим чехлы для айфона» после открытого Wildberries
//! открывает его выдачу, как если бы человек набрал запрос сам.
//!
//! Вопросы вроде «до скольки работает кафе» — другое дело: ответ нужен голосом,
//! а не вкладкой. Такие вопросы уходят в поисковик, найденные фрагменты — в
//! модель, и модель отвечает по ним, называя, откуда ответ. Модель на этой
//! машине не знает ни часов работы кафе, ни сегодняшних цен: без поиска она бы
//! их выдумала.

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

/// Поисковики по порядку.
///
/// DuckDuckGo в облегчённой версии отдаёт выдачу простой таблицей без
/// скриптов — её легко читать. Он же временами обрывает соединение, поэтому
/// запасной — Bing. Яндекс и Brave на запрос без браузера отвечают капчей и в
/// список не входят.
const ENGINES: &[(&str, &str)] = &[
    ("DuckDuckGo", "https://lite.duckduckgo.com/lite/?q={q}"),
    ("Bing", "https://www.bing.com/search?q={q}&mkt=ru-RU&setlang=ru"),
];

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Отвечает на вопрос по свежему поиску и отдаёт ответ вслух.
pub async fn lookup(app: &AppHandle, question: &str) -> String {
    let question = question.trim();
    if question.is_empty() {
        return "Не понял, что посмотреть.".into();
    }

    let hits = match search(question).await {
        Ok(hits) => hits,
        Err(err) => {
            log::warn!("поиск «{question}» не удался: {err}");
            return "Поиск в интернете сейчас не отвечает.".into();
        }
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
         в них нет, так и скажи — не выдумывай. {} В конце коротко назови, по \
         данным какого сайта ответ.\n\nФрагменты:\n{found}",
        crate::planner::now_line()
    );

    let provider = app.state::<AppState>().provider();
    match provider.interpret(&rules, question).await {
        Ok(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
        // Модель не ответила — лучше прочитать лучший фрагмент как есть, чем
        // промолчать: поиск-то сработал.
        _ => format!("Нашёл: {}. {}", hits[0].title, hits[0].snippet),
    }
}

/// Проверяет, правда ли написанное, — по свежему поиску.
///
/// Запрос — само утверждение: заголовок или самая содержательная из первых
/// строк, а не весь текст скриншота с датами и кнопками. Ответ — «подтверждается»,
/// «опровергается» или «проверить не удалось», с тем, по каким сайтам вывод.
/// Модели прямо сказано не судить по своей памяти: сегодняшних новостей она
/// не знает.
pub async fn fact_check(app: &AppHandle, text: &str, question: &str) -> String {
    let provider = app.state::<AppState>().provider();
    check_with(provider.as_ref(), text, question).await
}

/// Проверка — отдельно от программы: модель передаётся снаружи.
async fn check_with<P>(provider: &P, text: &str, question: &str) -> String
where
    P: crate::ai_client::AiProvider + ?Sized,
{
    let query = claim_query(text);
    if query.is_empty() {
        return "Не нашёл, что проверять: в тексте нет ни одной связной фразы.".into();
    }
    let hits = match search(&query).await {
        Ok(hits) => hits,
        Err(err) => {
            log::warn!("проверка «{query}»: поиск не удался: {err}");
            return "Проверить не вышло: поиск в интернете сейчас не отвечает.".into();
        }
    };
    log::info!("проверка «{query}»: результатов {}", hits.len());

    let found = hits
        .iter()
        .take(8)
        .enumerate()
        .map(|(at, hit)| format!("[{}] {} ({})\n{}", at + 1, hit.title, hit.site, hit.snippet))
        .collect::<Vec<_>>()
        .join("\n\n");
    let rules = format!(
        "Ты — Ноа, голосовой помощник. Человек увидел текст ниже — обычно новость со \
         скриншота — и спрашивает, правда ли это. Сравни главное утверждение текста с \
         фрагментами свежего поиска. Скажи прямо, одним-тремя предложениями, по-русски: \
         подтверждается, опровергается или проверить не удалось — и почему. Если об \
         этом пишут только сомнительные сайты или не пишет никто, так и скажи. Не суди \
         по своей памяти: сегодняшних новостей ты не знаешь. В конце назови, по данным \
         каких сайтов вывод. {}\n\nТекст:\n{}\n\nФрагменты поиска:\n{found}",
        crate::planner::now_line(),
        text.chars().take(1500).collect::<String>()
    );

    match provider.interpret(&rules, question).await {
        Ok(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
        _ => match hits.first() {
            Some(hit) => format!(
                "Проверить не вышло: модель не ответила. Первое, что нашлось: {} ({}).",
                hit.title, hit.site
            ),
            None => "Проверить не вышло: модель не ответила.".into(),
        },
    }
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
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&laquo;", "«")
        .replace("&raquo;", "»");
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
    use super::claim_query;

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
