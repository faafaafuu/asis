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

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::config::FoodConfig;
use crate::state::AppState;

/// Найденный в магазине товар с настоящей ценой.
#[derive(Debug)]
pub struct Product {
    pub name: String,
    /// Рубли. `None` — цену со страницы вытащить не удалось.
    pub price: Option<u32>,
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
    /// Копейки: так отдаёт FoodPilot, чтобы не терять на дробях.
    #[serde(default)]
    price_cents: Option<u32>,
    #[serde(default)]
    available: bool,
}

/// Ищет товар в настоящем магазине.
///
/// ВкусВилл, а не `mock-store`: цены живые, со страницы поиска. Берётся первый
/// доступный товар — выбирать лучший из двенадцати вариантов на слух человеку
/// негде, а показать их все голосом нельзя.
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

    Ok(parsed
        .products
        .into_iter()
        .find(|item| item.available)
        .map(|item| Product {
            name: item.name,
            // Копейки в рубли: вслух «триста восемьдесят рублей», а не
            // «тридцать восемь тысяч копеек».
            price: item.price_cents.map(|cents| cents / 100),
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
