//! Заказ продуктов через FoodPilot.
//!
//! Ноа здесь только голос и решение: что человек имел в виду, разбирает
//! `planner`, а что из этого следует заказать — знает FoodPilot. Он же держит
//! профиль: вкусы, нелюбимые продукты, дневной лимит калорий.
//!
//! Про подтверждение. FoodPilot по своей архитектуре требует подтверждать
//! каждый внешний заказ вручную — это записано у него в архитектурном
//! документе и повторено в разборах нескольких стадий. Здесь это подтверждение
//! снято намеренно: голосовой заказ, в котором надо идти к экрану и нажимать
//! кнопку, не голосовой. Взамен стоит потолок суммы (`FoodConfig::max_order`):
//! дешёвая ошибка оформляется молча, дорогая — не оформляется вовсе.
//!
//! Пока это работает только с тренировочным магазином FoodPilot
//! (`mock-store`): настоящих денег там нет. Боевые магазины — отдельное
//! решение, которое принимает человек, а не эта правка.

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::config::FoodConfig;
use crate::state::AppState;

/// Собранная корзина, готовая к оплате.
#[derive(Debug)]
pub struct Cart {
    pub id: String,
    /// Во что обходится, в рублях.
    pub total: u32,
    /// Что в ней лежит — чтобы было что сказать вслух.
    pub items: Vec<String>,
}

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartResponse {
    cart: CartBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartBody {
    id: String,
    #[serde(default)]
    total_rub: u32,
    #[serde(default)]
    items: Vec<CartItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CartItem {
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentIntent {
    id: String,
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

/// Собирает корзину по названиям блюд.
///
/// Блюда, а не продукты: FoodPilot сам раскладывает блюдо на ингредиенты по
/// своим рецептам и своей же порционности. Просить его о продуктах поштучно
/// значило бы дублировать эту логику голосом.
pub async fn build_cart(app: &AppHandle, dishes: &[String]) -> Result<Cart, String> {
    let config = food_config(app)?;
    let body = serde_json::json!({
        "userId": config.user_id,
        "menu": {
            "title": "Заказ голосом",
            "storeCode": config.store_code,
            "dishes": dishes.iter().map(|slug| serde_json::json!({ "slug": slug, "servings": 2 })).collect::<Vec<_>>(),
        }
    });

    let response = client()
        .post(format!("{}/cart-builder/menu/cart", config.endpoint.trim_end_matches('/')))
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("FoodPilot не ответил: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("FoodPilot отказал: {}", response.status()));
    }

    let parsed: CartResponse = response
        .json()
        .await
        .map_err(|err| format!("не разобрать ответ FoodPilot: {err}"))?;

    Ok(Cart {
        id: parsed.cart.id,
        total: parsed.cart.total_rub,
        items: parsed.cart.items.into_iter().map(|item| item.title).collect(),
    })
}

/// Оплачивает корзину без участия человека.
///
/// Два шага подряд, как того требует FoodPilot: сначала намерение об оплате,
/// потом его подтверждение. Разделение не наше, и обходить его нельзя —
/// именно на этих двух шагах FoodPilot сверяет, что оплачивается ровно та
/// корзина, которую собрали.
pub async fn pay(app: &AppHandle, cart: &Cart) -> Result<(), String> {
    let config = food_config(app)?;
    let base = config.endpoint.trim_end_matches('/');

    let intent: PaymentIntent = client()
        .post(format!("{base}/checkout/carts/{}/payment-intents", cart.id))
        .json(&serde_json::json!({ "provider": "MOCK" }))
        .send()
        .await
        .map_err(|err| format!("не создать оплату: {err}"))?
        .json()
        .await
        .map_err(|err| format!("не разобрать оплату: {err}"))?;

    let confirmed = client()
        .post(format!("{base}/checkout/payment-intents/{}/confirm", intent.id))
        .send()
        .await
        .map_err(|err| format!("не подтвердить оплату: {err}"))?;

    if !confirmed.status().is_success() {
        return Err(format!("оплата не прошла: {}", confirmed.status()));
    }

    log::info!("заказ оплачен: корзина {}, {} ₽", cart.id, cart.total);
    Ok(())
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
