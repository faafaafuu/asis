//! Подбор продуктов через FoodPilot.
//!
//! Ноа здесь только голос и решение: что человек имел в виду, разбирает
//! `planner`, а что из этого следует искать — знает FoodPilot. Он ходит на
//! страницу магазина и отдаёт настоящие товары с настоящими ценами.
//!
//! Заказа и оплаты здесь нет намеренно. Их не выбросили из осторожности —
//! их нечем сделать: у FoodPilot автоматизации магазинов фактически не
//! существует. Драйвер браузера умеет открыть страницу логина и на этом
//! кончается, план автоматизации возвращается текстовым описанием шагов, а
//! оплата закрыта на стороне сервера: запрос с `allowPayment` отклоняется, и
//! разрешение платить захардкожено выключенным.
//!
//! Здесь был написан код корзины и оплаты под тренировочный магазин
//! `mock-store` — он удалён. Работать он всё равно не мог, а выглядел рабочим
//! заказом с оплатой, и это хуже, чем его отсутствие: следующий читатель
//! построил бы на нём план, которого сервер не поддержит. История правок его
//! помнит, если тренировочный путь понадобится снова.
//!
//! Настоящий заказ — отдельная работа, и объём в ней не в правилах, а в
//! живых селекторах: под каждый магазин нужны свои поиск, сопоставление
//! товаров, корзина и оформление, плюс обход антибота.

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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    #[serde(default)]
    products: Vec<SearchProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchProduct {
    name: String,
    #[serde(default)]
    product_url: String,
    /// Копейки: так отдаёт FoodPilot, чтобы не терять на дробях.
    #[serde(default)]
    price_cents: Option<u32>,
    #[serde(default)]
    available: bool,
}

/// Ищет товар в настоящем магазине.
///
/// ВкусВилл, а не `mock-store`: цены живые, со страницы поиска. Из дюжины
/// вариантов выбирается один — тот, что дешевле за литр или килограмм, а не
/// просто дешевле; почему именно так, объяснено в `pick`.
pub async fn search(app: &AppHandle, query: &str) -> Result<Option<Product>, String> {
    let config = food_config(app)?;
    // Запрос кодируется вручную: reqwest здесь собран без фичи, дающей
    // `query()`, а тянуть её ради одного параметра незачем — тем же способом
    // собираются адреса в `calendar`.
    let url = format!(
        "{}/store-adapters/page/vkusvill/search?query={}",
        config.endpoint.trim_end_matches('/'),
        urlencode(query)
    );

    let response = client()
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("магазин не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("магазин отказал: {}", response.status()));
    }

    let parsed: SearchResponse = response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ магазина: {err}"))?;

    let shelf: Vec<pick::Candidate> = parsed
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

    Ok(pick::best(&shelf).map(|chosen| Product {
        name: chosen.name.clone(),
        price: chosen.price,
        url: chosen.url.clone(),
    }))
}

/// Что удалось набрать по запросу человека.
#[derive(Debug, Default)]
pub struct Quote {
    /// Найденное: название из магазина и цена.
    pub found: Vec<Product>,
    /// Чего в магазине не нашлось — об этом надо сказать, а не умолчать.
    pub missing: Vec<String>,
    /// Сумма найденного в рублях.
    pub total: u32,
}

impl Quote {
    /// Сколько не хватает до бесплатной доставки. `None` — уже бесплатно.
    pub fn until_free_delivery(&self, threshold: u32) -> Option<u32> {
        if threshold == 0 || self.total >= threshold {
            return None;
        }
        Some(threshold - self.total)
    }
}

/// Ищет всё, что просили, и считает сумму по настоящим ценам.
pub async fn quote(app: &AppHandle, items: &[String]) -> Result<Quote, String> {
    let mut result = Quote::default();

    for item in items {
        match search(app, item).await {
            Ok(Some(product)) => {
                result.total += product.price.unwrap_or(0);
                result.found.push(product);
            }
            // Товара нет в продаже или парсер его не увидел — для человека
            // это одно и то же: заказать не выйдет.
            Ok(None) => result.missing.push(item.clone()),
            Err(err) => {
                log::warn!("поиск «{item}» не удался: {err}");
                result.missing.push(item.clone());
            }
        }
    }

    Ok(result)
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
            "{}/store-adapters/browser-sessions/vkusvill/cart",
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
