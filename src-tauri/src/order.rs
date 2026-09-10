//! Что сейчас с заказом: из чего он собран, почём и на каком шаге.
//!
//! Голос хорош, чтобы попросить, и плох, чтобы проверить. «Набрал молоко за
//! девяносто три, хлеб за пятьдесят пять, творог за сто десять, итого двести
//! пятьдесят восемь» — на слух это не проверяется: к третьей позиции первая
//! забыта. Поэтому то же самое показывается окном, где видно построчно.
//!
//! Здесь только состояние и его изменения. Кто его меняет — `planner`, когда
//! подбирает и складывает; кто показывает — окно заказа.

use serde::Serialize;

/// На каком шаге заказ.
///
/// Шаги кончаются на «оформлен» намеренно. Дальше идут сборка и доставка, но
/// узнать их можно только со страницы заказа в магазине, а её чтения пока
/// нет. Придумывать шаги, которых программа не видит, нельзя: окно, которое
/// показывает «собирается», ничего об этом не зная, хуже окна, которое честно
/// говорит «оформление за вами».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Ищем товары и цены в магазине.
    Picking,
    /// Нашли и посчитали, но в корзину ещё не клали.
    Picked,
    /// Лежит в корзине магазина.
    InCart,
    /// Оформлен и оплачен: Ноа нажал «оформить» сам, по разрешению в настройках.
    Placed,
    /// Корзина собрана, а оплата ждёт подтверждения человеком: сумма выше
    /// пределов оплаты без подтверждения или такая оплата выключена.
    AwaitingPayment,
    /// Не вышло: магазин не ответил, вход не выполнен, товары не нашлись.
    Failed,
}

/// Строка заказа.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// Название из магазина, а не то, как это назвал человек: просил «фарш»,
    /// кладётся «Фарш из индейки, 400 г», и знать надо второе.
    pub name: String,
    /// Рубли за штуку. `None` — цену со страницы вытащить не удалось.
    pub price: Option<u32>,
    /// Сколько штук или упаковок.
    pub quantity: u32,
    /// Легло ли в корзину. `false` — нашли, но положить не вышло.
    pub in_cart: bool,
}

/// Всё, что известно о текущем заказе.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub stage: Stage,
    /// В каком магазине набрано, человеческим именем: «Магнит», «ВкусВилл».
    ///
    /// Магазин выбирается сравнением полок и от заказа к заказу меняется.
    /// Не назвать его — значит показать цены, по которым непонятно, где это
    /// лежит и куда идти оформлять.
    pub store: String,
    /// Что просил человек — его словами и с количеством.
    ///
    /// Нужно разбору следующей реплики: «пять штук» и «нет, полосатые» — это
    /// уточнение этого заказа, а понять это можно, только зная, что в нём.
    pub asked: Vec<crate::food::Wanted>,
    pub lines: Vec<Line>,
    /// Чего в магазине не нашлось.
    pub missing: Vec<String>,
    pub total: u32,
    /// Сколько не хватает до бесплатной доставки. `None` — уже бесплатно.
    pub until_free_delivery: Option<u32>,
    /// Потолок суммы, если он задан.
    pub max_order: u32,
    /// Что сказать человеку про положение дел: причина отказа или что дальше.
    pub note: String,
    /// Ссылка на собранную корзину в магазине, если корзина собрана ссылкой.
    ///
    /// Окно показывает её кнопкой: браузер мог открыться за другими окнами или
    /// его закрыли по ошибке, и без кнопки к корзине было бы не вернуться.
    pub link: Option<String>,
    /// Когда обновлялось, чтобы окно могло показать свежесть.
    pub updated_at: String,
}

impl Default for Order {
    fn default() -> Self {
        Self {
            stage: Stage::Picked,
            store: String::new(),
            asked: Vec::new(),
            lines: Vec::new(),
            missing: Vec::new(),
            total: 0,
            until_free_delivery: None,
            max_order: 0,
            note: String::new(),
            link: None,
            updated_at: chrono::Local::now().to_rfc3339(),
        }
    }
}

static CURRENT: std::sync::Mutex<Option<Order>> = std::sync::Mutex::new(None);

/// Что сейчас с заказом. `None` — заказа ещё не было.
pub fn current() -> Option<Order> {
    CURRENT
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone()
}

/// Записывает новое положение дел и сообщает об этом окну.
pub fn set(app: &tauri::AppHandle, mut order: Order) {
    use tauri::Emitter;

    order.updated_at = chrono::Local::now().to_rfc3339();
    let mut current = CURRENT.lock().unwrap_or_else(|err| err.into_inner());
    // Шаги заказа пишутся с чистого листа и о просьбе не знают — она переходит
    // от предыдущего состояния, иначе терялась бы на первом же шаге.
    if order.asked.is_empty() {
        if let Some(previous) = current.as_ref() {
            order.asked = previous.asked.clone();
        }
    }
    *current = Some(order);
    drop(current);

    let _ = app.emit("order:changed", ());
}

/// Отмечает, что подбор начался. Окно показывает это сразу, не дожидаясь цен:
/// поиск по магазину идёт секундами, и всё это время человеку надо видеть, что
/// его услышали.
pub fn start(app: &tauri::AppHandle, asked: &[crate::food::Wanted]) {
    let listed = asked
        .iter()
        .map(|item| match item.quantity {
            1 => item.name.clone(),
            many => format!("{} × {many}", item.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    set(
        app,
        Order {
            stage: Stage::Picking,
            asked: asked.to_vec(),
            note: format!("Ищу в магазине: {listed}"),
            ..Order::default()
        },
    )
}
