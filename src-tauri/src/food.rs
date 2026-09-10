//! Подбор продуктов через FoodPilot.
//!
//! Ноа здесь только голос и решение: что человек имел в виду, разбирает
//! `planner`, а что из этого следует искать — знает FoodPilot. Он ходит на
//! страницы магазинов и отдаёт настоящие товары с настоящими ценами.
//!
//! Магазинов несколько, и спрашиваются они разом. По одной полке не видно,
//! дорого это или дёшево: «молоко за сто пятьдесят» — это много или мало,
//! понятно только рядом с другой ценой. Вторая причина проще: магазины падают
//! поодиночке, и пока один недостижим, заказ можно собрать в другом.
//!
//! Собирается заказ всё равно в одном магазине. Корзина у каждого своя, и
//! набор, разложенный по трём магазинам, — это три доставки и три оформления,
//! то есть не экономия, а трата. Поэтому полки сравниваются целиком, и
//! выбирается магазин, а не отдельный товар: см. `best_store`.
//!
//! Корзину Ноа собирает пока только во ВкусВилле. Там это возможно без входа:
//! официальный MCP-сервер магазина по списку товаров отдаёт ссылку на уже
//! набранную корзину, и человеку остаются доставка и оплата на сайте. Если же
//! настроена сессия браузера, где человек вошёл, корзина наполняется прямо в
//! ней. В остальных магазинах Ноа доводит дело до подбора с ценами и на этом
//! честно останавливается: показать цены и промолчать про то, что заказ не
//! оформлен, было бы хуже, чем не искать там вовсе.

mod pick;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::config::FoodConfig;
use crate::state::AppState;

/// Найденный в магазине товар с настоящей ценой.
#[derive(Debug, Clone)]
pub struct Product {
    pub name: String,
    /// Рубли. `None` — цену со страницы вытащить не удалось.
    pub price: Option<u32>,
    /// Адрес карточки. По нему товар кладётся в корзину: класть можно только
    /// то, на что есть ссылка, а не то, что удалось назвать.
    pub url: String,
    /// Номер товара в магазине. У ВкусВилла это `xml_id` — по нему собирается
    /// ссылка на корзину. Пусто, если магазин номера не отдал.
    pub external_id: String,
}

/// Как магазин называется вслух.
///
/// Коды magnit и metro человеку ничего не говорят, а слышать он должен то же
/// слово, которым сам магазин называется на вывеске.
pub fn store_name(code: &str) -> &str {
    match code {
        "vkusvill" => "ВкусВилл",
        "magnit" => "Магнит",
        "metro" => "Метро",
        other => other,
    }
}

/// Название магазина в форме «в …».
///
/// Ноа отвечает голосом, и «набрал в Магнит» — это слышимая ошибка, из тех, по
/// которым сразу понятно, что говорит программа. Падеж дешевле хранить рядом с
/// названием, чем склонять: магазинов единицы, и добавляются они по одному.
pub fn store_in(code: &str) -> &str {
    match code {
        "vkusvill" => "во ВкусВилле",
        "magnit" => "в Магните",
        "metro" => "в Метро",
        other => other,
    }
}

/// Умеет ли Ноа складывать корзину в этом магазине.
///
/// Поиск по магазину — это чтение страницы, и он одинаков для всех. Корзина —
/// это нажатие настоящих кнопок в браузере, где человек вошёл, и каждая кнопка
/// своя. Поэтому магазинов для поиска больше, чем для заказа, и разницу надо
/// проговаривать, а не прятать.
pub fn cart_supported(code: &str) -> bool {
    code == "vkusvill"
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultiSearchResponse {
    #[serde(default)]
    stores: Vec<ShelfResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShelfResponse {
    provider: String,
    /// Дошёл ли запрос до магазина. Пустая полка и молчащий магазин выглядят
    /// одинаково, но означают разное: попросить другое или попробовать позже.
    #[serde(default)]
    reachable: bool,
    #[serde(default)]
    products: Vec<SearchProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchProduct {
    name: String,
    #[serde(default)]
    product_url: String,
    #[serde(default)]
    external_id: String,
    /// Копейки: так отдаёт FoodPilot, чтобы не терять на дробях.
    #[serde(default)]
    price_cents: Option<u32>,
    #[serde(default)]
    available: bool,
}

/// Что один магазин ответил про один товар.
pub struct Shelf {
    pub store: String,
    pub reachable: bool,
    /// Лучшее с этой полки. `None` — товара нет или полка пуста.
    pub best: Option<Product>,
}

/// Ищет товар во всех магазинах разом.
///
/// Из дюжины вариантов на каждой полке выбирается один — тот, что дешевле за
/// литр или килограмм, а не просто дешевле; почему именно так, объяснено в
/// `pick`.
pub async fn search(app: &AppHandle, query: &str) -> Result<Vec<Shelf>, String> {
    let config = food_config(app)?;
    // Запрос кодируется вручную: reqwest здесь собран без фичи, дающей
    // `query()`, а тянуть её ради одного параметра незачем — тем же способом
    // собираются адреса в `calendar`.
    let url = format!(
        "{}/store-adapters/page/search?query={}",
        config.endpoint.trim_end_matches('/'),
        urlencode(query)
    );

    let response = client()
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("FoodPilot не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("поиск отказал: {}", response.status()));
    }

    let parsed: MultiSearchResponse = response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ поиска: {err}"))?;

    Ok(parsed
        .stores
        .into_iter()
        .map(|shelf| {
            // Номер товара нужен корзине, а подбору — нет, поэтому в `pick`
            // он не идёт: его находим по адресу выбранной карточки.
            let numbers: Vec<(String, String)> = shelf
                .products
                .iter()
                .map(|item| (item.product_url.clone(), item.external_id.clone()))
                .collect();
            let candidates: Vec<pick::Candidate> = shelf
                .products
                .into_iter()
                .map(|item| pick::Candidate {
                    name: item.name,
                    // Копейки в рубли: вслух «триста восемьдесят рублей», а не
                    // «тридцать восемь тысяч копеек».
                    price: item.price_cents.map(|cents| cents / 100),
                    available: item.available,
                    url: item.product_url,
                })
                .collect();

            Shelf {
                store: shelf.provider,
                reachable: shelf.reachable,
                best: pick::best(&candidates).map(|chosen| Product {
                    name: chosen.name.clone(),
                    price: chosen.price,
                    url: chosen.url.clone(),
                    external_id: numbers
                        .iter()
                        .find(|(url, _)| *url == chosen.url)
                        .map(|(_, number)| number.clone())
                        .unwrap_or_default(),
                }),
            }
        })
        .collect())
}

/// Что удалось набрать по запросу человека в одном магазине.
#[derive(Debug, Default, Clone)]
pub struct StoreQuote {
    /// Код магазина: `vkusvill`, `magnit`, `metro`.
    pub store: String,
    /// Найденное: название из магазина и цена.
    pub found: Vec<Product>,
    /// Чего в магазине не нашлось — об этом надо сказать, а не умолчать.
    pub missing: Vec<String>,
    /// Сумма найденного в рублях.
    pub total: u32,
    /// Сколько запросов сорвалось из-за самого магазина, а не из-за товара.
    ///
    /// «Нет в продаже» и «магазин не отвечает» для человека совсем не одно и
    /// то же: в первом случае надо просить другое, во втором — просить позже.
    /// Раньше и то и другое сливалось в «ничего не нашёл», и человек искал
    /// причину в своих словах, тогда как магазин просто лежал.
    pub unreachable: usize,
}

impl StoreQuote {
    /// Сколько не хватает до бесплатной доставки. `None` — уже бесплатно.
    pub fn until_free_delivery(&self, threshold: u32) -> Option<u32> {
        if threshold == 0 || self.total >= threshold {
            return None;
        }
        Some(threshold - self.total)
    }

    /// Как магазин называется вслух.
    pub fn store_name(&self) -> &str {
        store_name(&self.store)
    }

    /// Название магазина в форме «в …».
    pub fn store_in(&self) -> &str {
        store_in(&self.store)
    }
}

/// В каком магазине брать.
///
/// Сначала полнота, потом цена. Магазин, где нашлось четыре позиции из пяти,
/// лучше магазина с двумя, даже если те две дешевле: недостающее придётся
/// докупать отдельно, и вторая доставка съест разницу. При равной полноте
/// выигрывает сумма.
///
/// Магазины, где не нашлось ничего, не участвуют: в них нечего заказывать.
pub fn best_store(quotes: &[StoreQuote]) -> Option<&StoreQuote> {
    quotes
        .iter()
        .filter(|quote| !quote.found.is_empty())
        .min_by_key(|quote| (std::cmp::Reverse(quote.found.len()), quote.total))
}

/// Ищет всё, что просили, рассказывая о найденном по ходу дела.
///
/// Поиск идёт по одному товару, и каждый — это поход на страницы магазинов, то
/// есть секунда-другая. На пяти товарах человек ждёт молча почти десять секунд
/// и всё это время не знает, работает ли программа. Поэтому найденное отдаётся
/// сразу, а не в конце: в окне товары появляются по одному, с ценами.
///
/// Показывается по ходу дела лучший на текущий момент магазин. Показывать все
/// три полки разом значило бы показывать втрое больше строк, из которых две
/// трети человеку не пригодятся.
pub async fn quote_reporting(
    app: &AppHandle,
    items: &[String],
    mut progress: impl FnMut(&StoreQuote),
) -> Result<Vec<StoreQuote>, String> {
    let mut quotes: Vec<StoreQuote> = Vec::new();

    for item in items {
        let shelves = match search(app, item).await {
            Ok(shelves) => shelves,
            // Отказал не магазин, а сам FoodPilot: спрашивать про остальные
            // товары нечего, отвечать будет некому.
            Err(err) => return Err(err),
        };

        for shelf in shelves {
            let quote = match quotes.iter_mut().find(|quote| quote.store == shelf.store) {
                Some(quote) => quote,
                None => {
                    quotes.push(StoreQuote {
                        store: shelf.store.clone(),
                        ..StoreQuote::default()
                    });
                    quotes.last_mut().expect("только что добавили")
                }
            };

            if !shelf.reachable {
                log::warn!("магазин {} не ответил про «{item}»", shelf.store);
                quote.missing.push(item.clone());
                quote.unreachable += 1;
                continue;
            }

            match shelf.best {
                Some(product) => {
                    quote.total += product.price.unwrap_or(0);
                    quote.found.push(product);
                }
                // Товара нет в продаже или парсер его не увидел — для человека
                // это одно и то же: заказать не выйдет.
                None => quote.missing.push(item.clone()),
            }
        }

        if let Some(leading) = best_store(&quotes) {
            progress(leading);
        }
    }

    Ok(quotes)
}

/// Что получилось положить в корзину.
#[derive(Debug, Default)]
pub struct CartResult {
    /// Названия того, что легло.
    pub added: Vec<String>,
    /// Что не легло и почему — об этом надо сказать, а не умолчать.
    pub failed: Vec<String>,
    /// Итог корзины по данным самого магазина. `None` — прочитать не удалось.
    pub total: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartResponse {
    #[serde(default)]
    items: Vec<CartItemResult>,
    #[serde(default)]
    cart: CartSnapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartItemResult {
    #[serde(default)]
    product_url: String,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    problem: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CartSnapshot {
    #[serde(default)]
    total_rub: Option<u32>,
}

/// Кладёт подобранное в настоящую корзину магазина.
///
/// Требует сессии браузера, в которой человек уже вошёл: без входа магазин
/// уводит нажатие на страницу логина, и товар не добавляется. Поэтому
/// отсутствие сессии — это не ошибка, а отказ с внятной причиной.
pub async fn add_to_cart(app: &AppHandle, products: &[Product]) -> Result<CartResult, String> {
    let config = food_config(app)?;
    if config.session_id.trim().is_empty() {
        return Err("не в какую корзину класть: сессия браузера не настроена".into());
    }

    let items: Vec<serde_json::Value> = products
        .iter()
        .filter(|product| !product.url.is_empty())
        .map(|product| serde_json::json!({ "productUrl": product.url, "quantity": 1 }))
        .collect();

    if items.is_empty() {
        return Err("у подобранного нет адресов карточек".into());
    }

    let response = client()
        .post(format!(
            "{}/store-adapters/browser-session/vkusvill/cart",
            config.endpoint.trim_end_matches('/')
        ))
        .json(&serde_json::json!({ "sessionId": config.session_id, "items": items }))
        .send()
        .await
        .map_err(|err| format!("FoodPilot не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("корзина отказала: {}", response.status()));
    }

    let parsed: CartResponse = response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ корзины: {err}"))?;

    // Названия берём свои: магазин возвращает адреса, а вслух нужно то, как
    // товар называется.
    let name_of = |url: &str| {
        products
            .iter()
            .find(|product| product.url == url)
            .map(|product| product.name.clone())
            .unwrap_or_else(|| url.to_string())
    };

    let mut result = CartResult {
        total: parsed.cart.total_rub,
        ..CartResult::default()
    };
    for item in parsed.items {
        if item.ok {
            result.added.push(name_of(&item.product_url));
        } else {
            let why = item.problem.unwrap_or_else(|| "без объяснения".into());
            result
                .failed
                .push(format!("{} ({why})", name_of(&item.product_url)));
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
struct CartLinkResponse {
    link: String,
}

/// Собирает корзину ВкусВилла ссылкой — через официальный MCP магазина.
///
/// Входа не нужно: корзина набирается на стороне магазина и открывается по
/// ссылке уже полной. Не нужно и того, чтобы витрина открывалась с этой
/// машины: MCP живёт на своём адресе и отвечает там, где сайт недоступен.
/// Доставку и оплату человек выбирает на сайте сам — оформить за него так
/// нельзя, но довести до одной кнопки можно.
pub async fn cart_link(app: &AppHandle, products: &[Product]) -> Result<String, String> {
    let config = food_config(app)?;
    let items: Vec<serde_json::Value> = products
        .iter()
        .filter_map(|product| product.external_id.parse::<u64>().ok())
        .map(|xml_id| serde_json::json!({ "xmlId": xml_id, "quantity": 1 }))
        .collect();

    if items.is_empty() {
        return Err("у подобранного нет номеров товаров".into());
    }

    let response = client()
        .post(format!(
            "{}/store-adapters/page/vkusvill/cart-link",
            config.endpoint.trim_end_matches('/')
        ))
        .json(&serde_json::json!({ "items": items }))
        .send()
        .await
        .map_err(|err| format!("FoodPilot не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("ссылку на корзину собрать не вышло: {}", response.status()));
    }

    let parsed: CartLinkResponse = response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ: {err}"))?;
    Ok(parsed.link)
}

/// Магазин по тому, как его назвал человек. `None` — не назван или незнаком.
///
/// Разбор реплики отдаёт название, как его произнесли: «вкусвилл», «во
/// вкусвилле», «магнит». Трёх магазинов хватает на прямое сравнение по корню
/// слова, и оно надёжнее сравнения на слух: «во» и «в» перед названием
/// сбивали бы подсчёт совпавших слов.
pub fn store_code(said: &str) -> Option<&'static str> {
    let said = said.to_lowercase();
    if said.contains("вкус") || said.contains("vkus") {
        Some("vkusvill")
    } else if said.contains("магнит") || said.contains("magnit") {
        Some("magnit")
    } else if said.contains("метро") || said.contains("metro") {
        Some("metro")
    } else {
        None
    }
}

/// В каком магазине собирать заказ.
///
/// Назван магазин — только он, даже если в другом дешевле. Человек, который
/// просит семечки во ВкусВилле, выбрал магазин сам, и подсунуть ему Магнит
/// значит сделать не то, о чём просили. Чего в названном магазине нет, то
/// называется ненайденным, а не докупается в соседнем: заказ собирается в
/// одном магазине.
///
/// Не назван — среди магазинов, где Ноа умеет собрать корзину. Сравнение с
/// остальными полезно, но заказать там всё равно нельзя, а просьба «закажи»
/// ждёт корзины, а не таблицы цен. Только если заказать негде вовсе, выбор
/// идёт среди всех — чтобы хотя бы показать, почём это.
pub fn choose_store<'a>(quotes: &'a [StoreQuote], named: Option<&str>) -> Option<&'a StoreQuote> {
    if let Some(code) = named {
        return quotes
            .iter()
            .find(|quote| quote.store == code && !quote.found.is_empty());
    }
    quotes
        .iter()
        .filter(|quote| cart_supported(&quote.store) && !quote.found.is_empty())
        .min_by_key(|quote| (std::cmp::Reverse(quote.found.len()), quote.total))
        .or_else(|| best_store(quotes))
}

/// Чем кончилось оформление.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    /// Заказ оформлен и оплачен.
    #[serde(default)]
    pub placed: bool,
    /// Сумма, на которую оформлено.
    #[serde(default)]
    pub total_rub: Option<u32>,
    /// Что сказал магазин — причина, если не оформилось.
    #[serde(default)]
    pub message: String,
}

/// Оформляет и оплачивает заказ способом, сохранённым в самом магазине.
///
/// Только в сессии браузера, где человек вошёл: карта или СБП лежат в его
/// аккаунте ВкусВилла, и платёжных данных здесь нет и не бывает.
///
/// `expected_total` — сумма корзины, которую только что показал сам магазин.
/// FoodPilot нажимает «оформить», только если на последнем шаге сумма та же:
/// другая сумма значит, что оформляется не то, что проверено потолком.
pub async fn checkout(app: &AppHandle, expected_total: u32) -> Result<Checkout, String> {
    let config = food_config(app)?;
    if config.session_id.trim().is_empty() {
        return Err("сессия браузера не настроена".into());
    }

    let response = client()
        .post(format!(
            "{}/store-adapters/browser-session/vkusvill/checkout/confirm",
            config.endpoint.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "sessionId": config.session_id,
            "expectedTotalRub": expected_total,
        }))
        .send()
        .await
        .map_err(|err| format!("FoodPilot не ответил: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let reason = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value["message"].as_str().map(str::to_string))
            .unwrap_or(body);
        return Err(format!("магазин отказал ({status}): {reason}"));
    }

    response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ оформления: {err}"))
}

/// Настроена ли сессия браузера, в которой человек вошёл во ВкусВилл.
///
/// От неё зависит, как собирается корзина: в сессии — прямо в корзине
/// магазина, где потом можно и оплатить; без неё — ссылкой.
pub fn has_browser_session(app: &AppHandle) -> bool {
    food_config(app)
        .map(|config| !config.session_id.trim().is_empty())
        .unwrap_or(false)
}

/// Настройки заказа, если он вообще включён.
fn food_config(app: &AppHandle) -> Result<FoodConfig, String> {
    let config = app.state::<AppState>().config().food.clone();
    if !config.ready() {
        return Err("заказ продуктов не настроен".into());
    }
    Ok(config)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Проценты вместо небезопасных байтов. Названия товаров по-русски, и без
/// кодирования запрос до магазина не доходит.
fn urlencode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn product(name: &str, price: u32) -> Product {
        Product {
            name: name.into(),
            price: Some(price),
            url: format!("https://example.test/{name}"),
            external_id: String::new(),
        }
    }

    fn quote(store: &str, found: &[(&str, u32)], missing: usize) -> StoreQuote {
        StoreQuote {
            store: store.into(),
            found: found.iter().map(|(name, price)| product(name, *price)).collect(),
            missing: (0..missing).map(|i| format!("нет-{i}")).collect(),
            total: found.iter().map(|(_, price)| price).sum(),
            unreachable: 0,
        }
    }

    #[test]
    fn a_fuller_basket_beats_a_cheaper_one() {
        // Магазин с четырьмя позициями выигрывает у магазина с двумя, даже
        // когда те две дешевле: за недостающим придётся ехать отдельно.
        let quotes = vec![
            quote("magnit", &[("молоко", 80), ("хлеб", 50)], 2),
            quote(
                "metro",
                &[("молоко", 90), ("хлеб", 60), ("яйца", 120), ("сыр", 300)],
                0,
            ),
        ];

        assert_eq!(best_store(&quotes).map(|best| best.store.as_str()), Some("metro"));
    }

    #[test]
    fn at_equal_fullness_the_cheaper_store_wins() {
        let quotes = vec![
            quote("magnit", &[("молоко", 80), ("хлеб", 50)], 0),
            quote("metro", &[("молоко", 90), ("хлеб", 60)], 0),
        ];

        assert_eq!(best_store(&quotes).map(|best| best.store.as_str()), Some("magnit"));
    }

    #[test]
    fn a_store_with_nothing_is_not_a_choice() {
        // Пустой магазин дешевле любого непустого, и без отбора он побеждал бы
        // всегда, оставляя человека с заказом из ничего.
        let quotes = vec![
            quote("vkusvill", &[], 3),
            quote("magnit", &[("молоко", 80)], 2),
        ];

        assert_eq!(
            best_store(&quotes).map(|best| best.store.as_str()),
            Some("magnit")
        );
    }

    #[test]
    fn nowhere_to_order_is_an_answer_too() {
        let quotes = vec![quote("vkusvill", &[], 2), quote("magnit", &[], 2)];

        assert!(best_store(&quotes).is_none());
    }

    #[test]
    fn stores_are_named_the_way_they_are_spoken() {
        assert_eq!(store_name("magnit"), "Магнит");
        assert_eq!(store_name("metro"), "Метро");
        assert_eq!(store_name("vkusvill"), "ВкусВилл");
        // Незнакомый код лучше показать как есть, чем потерять.
        assert_eq!(store_name("lenta"), "lenta");
    }

    #[test]
    fn stores_are_declined_for_the_voice() {
        assert_eq!(store_in("magnit"), "в Магните");
        assert_eq!(store_in("vkusvill"), "во ВкусВилле");
        assert_eq!(store_in("metro"), "в Метро");
    }

    #[test]
    fn a_named_store_is_kept_even_when_another_is_cheaper() {
        let quotes = vec![
            quote("magnit", &[("семечки", 60)], 0),
            quote("vkusvill", &[("семечки", 120)], 0),
        ];
        assert_eq!(
            choose_store(&quotes, Some("vkusvill")).map(|q| q.store.as_str()),
            Some("vkusvill")
        );
    }

    #[test]
    fn a_named_store_without_the_goods_is_not_swapped_for_another() {
        let quotes = vec![
            quote("vkusvill", &[], 1),
            quote("magnit", &[("семечки", 60)], 0),
        ];
        assert!(choose_store(&quotes, Some("vkusvill")).is_none());
    }

    #[test]
    fn without_a_name_the_basket_goes_where_it_can_be_ordered() {
        // Магнит полнее и дешевле, но собрать корзину можно только во ВкусВилле.
        let quotes = vec![
            quote("magnit", &[("молоко", 80), ("хлеб", 50)], 0),
            quote("vkusvill", &[("молоко", 100)], 1),
        ];
        assert_eq!(
            choose_store(&quotes, None).map(|q| q.store.as_str()),
            Some("vkusvill")
        );
    }

    #[test]
    fn with_nowhere_to_order_the_prices_are_still_shown() {
        let quotes = vec![quote("vkusvill", &[], 2), quote("magnit", &[("молоко", 80)], 1)];
        assert_eq!(
            choose_store(&quotes, None).map(|q| q.store.as_str()),
            Some("magnit")
        );
    }

    #[test]
    fn stores_are_recognised_as_said() {
        assert_eq!(store_code("вкусвилл"), Some("vkusvill"));
        assert_eq!(store_code("во Вкусвилле"), Some("vkusvill"));
        assert_eq!(store_code("магнит"), Some("magnit"));
        assert_eq!(store_code("в метро"), Some("metro"));
        assert_eq!(store_code(""), None);
        assert_eq!(store_code("пятёрочка"), None);
    }

    #[test]
    fn the_cart_is_only_promised_where_it_works() {
        assert!(cart_supported("vkusvill"));
        assert!(!cart_supported("magnit"));
        assert!(!cart_supported("metro"));
    }
}
