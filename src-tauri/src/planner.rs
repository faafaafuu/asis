//! Задачи в разговоре.
//!
//! Что человек имел в виду, решает модель, а не список слов. Прежде здесь
//! стояло угадывание по основам («напомни», «задача», «сделал»), и оно
//! проваливалось на обычной живой речи: «добавь на сегодня доделать резюме
//! задача» и «какие у меня задачи на сегодня» мимо него проходили. Список слов
//! в принципе не может покрыть язык — можно лишь бесконечно его дополнять,
//! каждый раз узнавая о новой формулировке от человека, у которого не сработало.
//!
//! Поэтому каждая реплика разговора разбирается одним обращением к модели,
//! которое отвечает строгим JSON: что это было и о каком деле речь. Цена —
//! примерно секунда на реплику; она окупается тем, что распоряжение понимается
//! так, как оно сказано, а не так, как заранее угадали.
//!
//! Открытые дела перечисляются в самом запросе, поэтому «отметь резюме» и
//! «перенеси банк на завтра» указывают на конкретную задачу по номеру, а не
//! через сравнение слов.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone};
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::tasks::{self, Task};

/// Что человек имел в виду.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Обычный вопрос или разговор — задачи ни при чём.
    Chat,
    /// Завести дело.
    Add {
        title: String,
        due: Option<DateTime<Local>>,
        /// Просил именно в календарь, а не просто напомнить.
        calendar: bool,
    },
    /// Перечислить, что предстоит.
    List,
    /// Отметить сделанным.
    Done { task: Option<String> },
    /// Перенести на другой срок.
    Postpone {
        task: Option<String>,
        due: Option<DateTime<Local>>,
    },
    /// Помочь: разложить дело на шаги и подсказать, с чего начать.
    Breakdown { task: Option<String> },
    /// Заказать еду. Названия блюд, как их знает FoodPilot, и магазин, если
    /// человек его назвал.
    Order { items: Vec<crate::food::Wanted>, store: String },
    /// Спрашивает, что с заказом: что набрано и на каком оно шаге.
    OrderStatus,
    /// Открыть программу, окно или игру. `sandbox` — в песочнице Sandboxie.
    Launch {
        program: String,
        sandbox: bool,
        sandbox_box: String,
    },
    /// Закрыть программу.
    Close { program: String },
    /// Что-то сделать с самим компьютером: сон, выключение, блокировка.
    Power { action: crate::pc::Power },
    /// Открыть сайт или поискать на нём.
    Web { site: String, query: String },
    /// Посмотреть в интернете и ответить голосом.
    Lookup { query: String },
    /// Включить или выключить VPN.
    Vpn { on: bool },
    /// Полистать страницу, вернуться назад, закрыть вкладку.
    Nav { action: crate::pc::Nav },
}

/// Название дела, которому не хватает срока.
///
/// Между «напомни доделать резюме» и «завтра к обеду» проходит целый круг
/// разговора, и название надо где-то держать.
static AWAITING_TIME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Ждём ли, что человек назовёт срок.
pub fn awaiting_time() -> bool {
    AWAITING_TIME
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .is_some()
}

/// Забывает недоспрошенное дело. Зовётся, когда разговор кончается.
pub fn forget_pending() {
    *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = None;
}

/// Выполняет распоряжение и отдаёт то, что сказать вслух.
///
/// `None` означает «это был обычный вопрос» — фразу надо обработать как всегда.
pub async fn handle(app: &AppHandle, said: &str) -> Option<String> {
    // Дело ждёт срока — значит, сказанное сейчас и есть срок.
    if awaiting_time() {
        return Some(finish_pending(app, said).await);
    }

    let open = open_tasks();
    match read_intent(app, said, &open).await {
        Intent::Chat => None,
        Intent::Add {
            title,
            due,
            calendar,
        } => Some(add(app, title, due, calendar)),
        Intent::List => {
            // Заодно показываем окно: список из пяти дел на слух запоминается
            // плохо, а глазами читается сразу весь.
            if let Err(err) = crate::overlay::show_tasks(app) {
                log::warn!("окно задач не открылось: {err}");
            }
            Some(list())
        }
        Intent::Done { task } => Some(done(app, task.as_deref(), &open)),
        Intent::Postpone { task, due } => Some(postpone(app, task.as_deref(), due, &open)),
        Intent::Breakdown { task } => Some(breakdown(app, task.as_deref(), &open).await),
        Intent::Order { items, store } => Some(order(app, &items, &store).await),
        Intent::OrderStatus => Some(order_status(app)),
        // Поиск программы читает меню «Пуск» и диски — это блокирующая
        // работа, и занимать ею асинхронную задачу нельзя.
        Intent::Launch {
            program,
            sandbox,
            sandbox_box,
        } => Some(match own_window(&program) {
            Some(window) => open_own(app, window),
            None => blocking(move || crate::pc::launch(&program, sandbox, &sandbox_box)).await,
        }),
        Intent::Close { program } => Some(blocking(move || crate::pc::close(&program)).await),
        Intent::Power { action } => Some(crate::pc::power(action)),
        Intent::Web { site, query } => {
            Some(blocking(move || crate::web::open_site(&site, &query)).await)
        }
        Intent::Lookup { query } => Some(crate::web::lookup(app, &query).await),
        Intent::Vpn { on } => Some(blocking(move || crate::pc::vpn(on)).await),
        Intent::Nav { action } => Some(blocking(move || crate::pc::navigate(action)).await),
    }
}

/// Собственное окно Суфлёра, которое просят открыть.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnWindow {
    /// Настройки целиком.
    Settings,
    /// Настройки на разделе «Заказы».
    FoodSettings,
    /// Окно текущего заказа.
    Order,
}

/// Просят ли открыть окно самого Суфлёра, а не чужую программу.
///
/// «Настройки заказов» и «окно заказа» — не программы на диске: искать их в
/// меню «Пуск» бесполезно, а похожее по звучанию там найдётся всегда.
fn own_window(program: &str) -> Option<OwnWindow> {
    let lower = program.to_lowercase();
    let about_orders = lower.contains("заказ") || lower.contains("продукт");
    let settings = lower.contains("настро") || lower.contains("парамет");
    match (about_orders, settings) {
        (true, true) => Some(OwnWindow::FoodSettings),
        (true, false) => Some(OwnWindow::Order),
        _ if lower.contains("суфл") || lower.contains("ноа") => Some(OwnWindow::Settings),
        _ => None,
    }
}

fn open_own(app: &AppHandle, window: OwnWindow) -> String {
    let (opened, spoken) = match window {
        OwnWindow::Settings => (crate::overlay::show_onboarding(app), "Открываю настройки."),
        OwnWindow::FoodSettings => (
            crate::overlay::show_settings_section(app, "food"),
            "Открываю настройки заказов.",
        ),
        OwnWindow::Order => (crate::overlay::show_order(app), "Открываю заказ."),
    };
    match opened {
        Ok(()) => spoken.into(),
        Err(err) => format!("Окно не открылось: {err}."),
    }
}

/// Оплата, подтверждённая человеком кнопкой в окне заказа.
///
/// Сумма берётся из заказа — та, что написана на кнопке, — и FoodPilot
/// оформит, только если на странице оплаты она та же. В учёт оплат без
/// подтверждения она не идёт: человек её видел и одобрил сам.
pub async fn pay_confirmed(app: &AppHandle) -> Result<(), String> {
    let Some(mut shown) = crate::order::current() else {
        return Err("заказа нет".into());
    };
    if shown.stage != crate::order::Stage::AwaitingPayment {
        return Err("этот заказ оплачивать не нужно".into());
    }
    let total = shown.total;
    shown.note = "Оплачиваю…".into();
    crate::order::set(app, shown.clone());

    match crate::food::checkout(app, total).await {
        Ok(done) if done.placed => {
            shown.stage = crate::order::Stage::Placed;
            shown.total = done.total_rub.unwrap_or(total);
            shown.note = "Заказ оформлен и оплачен.".into();
            crate::order::set(app, shown);
            Ok(())
        }
        Ok(done) => {
            shown.note = format!("Не оформлено: {}", done.message);
            crate::order::set(app, shown);
            Err(done.message)
        }
        Err(err) => {
            shown.note = format!("Не оплачено: {err}");
            crate::order::set(app, shown);
            Err(err)
        }
    }
}

/// Собирает корзину ссылкой через MCP магазина и открывает её в браузере.
///
/// `fallback` — корзину пытались собрать в аккаунте, и не вышло: витрина
/// недоступна, вход слетел. Тогда ссылка — запасной путь, которому витрина не
/// нужна, и об этом говорится: оплатить такую корзину придётся самому.
async fn basket_by_link(
    app: &AppHandle,
    quote: &crate::food::StoreQuote,
    mut shown: crate::order::Order,
    missing: &str,
    fallback: bool,
) -> String {
    match crate::food::cart_link(app, &quote.found).await {
        Ok(link) => {
            if let Err(err) = crate::pc::open(&link) {
                log::warn!("ссылка на корзину не открылась: {err}");
            }
            for line in &mut shown.lines {
                line.in_cart = quote.found.iter().any(|product| {
                    product.name == line.name && product.external_id.parse::<u64>().is_ok()
                });
            }
            shown.stage = crate::order::Stage::InCart;
            shown.link = Some(link);
            shown.note = "Корзина открыта в браузере. Доставка и оплата — там.".into();
            crate::order::set(app, shown);
            match fallback {
                false => format!("Готово, корзина открыта в браузере.{missing}"),
                true => format!(
                    "В аккаунте собрать не вышло — открыл корзину ссылкой в браузере, оплата там.{missing}"
                ),
            }
        }
        Err(err) => {
            log::warn!("ссылка на корзину не собралась: {err}");
            shown.stage = crate::order::Stage::Failed;
            shown.note = format!("Корзину собрать не вышло: {err}");
            crate::order::set(app, shown);
            format!("Корзину собрать не вышло: {err}.")
        }
    }
}

/// Выполняет блокирующую работу в отдельном потоке и ждёт её ответа.
async fn blocking(work: impl FnOnce() -> String + Send + 'static) -> String {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .unwrap_or_else(|_| "Не вышло: работа оборвалась.".into())
}

/// Рассказывает, что с заказом, и показывает его окном.
///
/// Вслух — коротко: сколько набрано и на сколько. Построчно человек смотрит в
/// окне, потому что список из пяти позиций с ценами на слух не удерживается.
fn order_status(app: &AppHandle) -> String {
    let Some(order) = crate::order::current() else {
        return "Заказа сейчас нет.".into();
    };

    if let Err(err) = crate::overlay::show_order(app) {
        log::warn!("окно заказа не открылось: {err}");
    }

    let in_cart = order.lines.iter().filter(|line| line.in_cart).count();
    let head = match order.stage {
        crate::order::Stage::Picking => format!("Ищу. Пока нашёл {}.", order.lines.len()),
        crate::order::Stage::Picked => {
            format!("Подобрано {} на {} рублей.", order.lines.len(), order.total)
        }
        crate::order::Stage::InCart => {
            format!("В корзине {in_cart} на {} рублей.", order.total)
        }
        crate::order::Stage::AwaitingPayment => format!(
            "Корзина на {} рублей ждёт подтверждения оплаты.",
            order.total
        ),
        crate::order::Stage::Placed => format!("Заказ оформлен на {} рублей.", order.total),
        crate::order::Stage::Failed => "С заказом не вышло.".into(),
    };

    match order.note.is_empty() {
        true => head,
        false => format!("{head} {}", order.note),
    }
}

/// Заказывает еду и отдаёт то, что сказать вслух.
///
/// Единственная преграда между оговоркой и деньгами — потолок суммы: у
/// FoodPilot подтверждение человеком здесь снято намеренно, ради голосового
/// заказа (см. `crate::food`).
async fn order(app: &AppHandle, items: &[crate::food::Wanted], wanted_store: &str) -> String {
    if items.is_empty() {
        return "Не понял, что заказать.".into();
    }

    // Названный магазин, которого Ноа не знает, — не повод молча собрать в
    // другом: человек просил именно его. Говорим прямо и называем, где можем.
    // Пятёрочка читается только через parse.bot, по ключу человека. Без
    // ключа говорим, что нужно, — а не молча ищем в других магазинах.
    if crate::food::store_code(wanted_store) == Some("pyaterochka")
        && app.state::<AppState>().config().food.parse_key.trim().is_empty()
    {
        return "Пятёрочку я читаю через сервис parse.bot, а его ключа нет. \
                Заведите бесплатный ключ на parse.bot и вставьте его в настройках заказов."
            .into();
    }
    if !wanted_store.trim().is_empty() && crate::food::store_code(wanted_store).is_none() {
        // Маркетплейс, а не продуктовый магазин: «закажи чехол на вайлдберриз».
        // Заказать там Ноа не может, но открыть поиск на самом сайте — может,
        // и это то, чего человек ждёт, в отличие от отказа.
        let wanted_goods = items
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(spoken) = crate::web::open_known(wanted_store, &wanted_goods) {
            return spoken;
        }
        return format!(
            "Магазин «{}» я пока не подключил. Продукты заказываю во ВкусВилле, \
             ищу и сравниваю в Магните и Метро.",
            wanted_store.trim()
        );
    }

    // Окно открывается до похода в магазин: поиск идёт секундами, и всё это
    // время человеку надо видеть, что его услышали.
    crate::order::start(app, items);
    if let Err(err) = crate::overlay::show_order(app) {
        log::warn!("окно заказа не открылось: {err}");
    }

    // Сначала настоящие цены: что в магазине есть и почём. Без этого нечего
    // ни называть вслух, ни сверять с потолком.
    // Найденное показывается по мере поиска, а не в конце: пять товаров — это
    // пять походов на страницу магазина, и молчать всё это время нельзя.
    let asked = items.len();
    let quotes = match crate::food::quote_reporting(app, items, |so_far| {
        crate::order::set(
            app,
            crate::order::Order {
                stage: crate::order::Stage::Picking,
                store: so_far.store_name().into(),
                lines: so_far
                    .found
                    .iter()
                    .map(|item| crate::order::Line {
                        name: item.name.clone(),
                        price: item.price,
                        quantity: item.quantity,
                        in_cart: false,
                    })
                    .collect(),
                missing: so_far.missing.clone(),
                total: so_far.total,
                note: format!(
                    "Ищу: {} из {asked}",
                    so_far.found.len() + so_far.missing.len()
                ),
                ..crate::order::Order::default()
            },
        );
    })
    .await
    {
        Ok(quotes) => quotes,
        Err(err) => {
            log::warn!("не удалось узнать цены: {err}");
            crate::order::set(
                app,
                crate::order::Order {
                    stage: crate::order::Stage::Failed,
                    note: format!("Поиск не ответил: {err}"),
                    ..crate::order::Order::default()
                },
            );
            return "Поиск по магазинам не отвечает.".into();
        }
    };

    let wanted = crate::food::store_code(wanted_store);
    let Some(quote) = crate::food::choose_store(&quotes, wanted) else {
        // Назвали магазин, а в нём ничего не нашлось, — говорим именно про
        // него: человек просил ВкусВилл, и молча собрать в Магните значило бы
        // сделать не то, о чём просили.
        //
        // Магазины не ответили ни разу — это не «нет такого товара», а
        // «магазины лежат». Разница для человека решающая: в первом случае он
        // назовёт другое, во втором — попробует позже.
        let note = match wanted {
            Some(code) => {
                let silent = quotes
                    .iter()
                    .any(|quote| quote.store == code && quote.unreachable >= items.len());
                if silent {
                    format!("{} не отвечает — попробуйте позже.", crate::food::store_name(code))
                } else {
                    format!("{} ничего из этого не нашёл.", capitalized(crate::food::store_in(code)))
                }
            }
            None => {
                let all_silent = !quotes.is_empty()
                    && quotes
                        .iter()
                        .all(|quote| quote.unreachable >= items.len());
                match all_silent {
                    true => "Магазины не отвечают — попробуйте позже.".into(),
                    false => "Ничего из этого не нашёл ни в одном магазине.".into(),
                }
            }
        };
        crate::order::set(
            app,
            crate::order::Order {
                stage: crate::order::Stage::Failed,
                missing: items.iter().map(|item| item.name.clone()).collect(),
                note: note.clone(),
                ..crate::order::Order::default()
            },
        );
        return note;
    };

    let food = app.state::<AppState>().config().food.clone();

    // То, что нашлось, — в окно. Дальше состояние только уточняется.
    let mut shown = crate::order::Order {
        stage: crate::order::Stage::Picked,
        store: quote.store_name().into(),
        lines: quote
            .found
            .iter()
            .map(|item| crate::order::Line {
                name: item.name.clone(),
                price: item.price,
                quantity: item.quantity,
                in_cart: false,
            })
            .collect(),
        missing: quote.missing.clone(),
        total: quote.total,
        until_free_delivery: quote.until_free_delivery(food.free_delivery_from),
        max_order: food.max_order,
        note: "Подобрано. Собираю корзину.".into(),
        ..crate::order::Order::default()
    };
    crate::order::set(app, shown.clone());

    // Вслух — коротко: итог и то, что пошло не так.
    //
    // Перечень набранного с ценами на слух не удерживается, а человеку и не
    // нужен: построчно он видит его в окне заказа. Промолчать можно обо всём,
    // кроме ненайденного — о нём человек должен узнать, иначе будет ждать то,
    // что не приедет.
    let missing = match quote.missing.is_empty() {
        true => String::new(),
        false => format!(" Не нашёл: {}.", quote.missing.join(", ")),
    };

    // Дороже предела оплаты без подтверждения — корзина всё равно
    // собирается: предел ограничивает то, что Ноа платит сам, а не то, что
    // он кладёт в корзину. Такая корзина ждёт подтверждения — см. ниже.

    // TODO(human): решить, что говорить про набор дороже потолка. Поведение
    // определено выше — такой набор в корзину не идёт, — а вопрос остался про
    // слова: называть ли его целиком, как сейчас, или честнее сразу сказать
    // про потолок, не перечисляя того, что всё равно не купится.

    // Корзину Ноа собирает не везде. Сюда доходит магазин, названный
    // человеком, — промолчать о том, что заказ не собран, нельзя: услышав
    // «готово», человек станет ждать курьера.
    if !crate::food::cart_supported(&quote.store) {
        shown.note = "Подобрано. Корзину в этом магазине Ноа пока не собирает.".into();
        crate::order::set(app, shown);
        // Магазин назвал сам человек — просто говорим, что оформить придётся
        // самому. Выбрали мы, потому что там выгоднее, — называем цену и
        // предлагаем, где корзину собрать можно: «во ВкусВилле» следующей
        // фразой её соберёт, заказ помнит, о чём речь.
        if wanted.is_some() {
            return format!(
                "Подобрал {} на {} ₽, но корзину там собрать не могу — оформите сами.{missing}",
                quote.store_in(),
                quote.total
            );
        }
        let alternative = match crate::food::orderable_alternative(&quotes, quote) {
            Some(other) if other.found.len() == quote.found.len() => format!(
                " {} — {} ₽; скажите «{}», и соберу там.",
                capitalized(other.store_in()),
                other.total,
                other.store_in()
            ),
            Some(other) => format!(
                " {} нашлось {} из {} на {} ₽; скажите «{}», и соберу там.",
                capitalized(other.store_in()),
                other.found.len(),
                items.len(),
                other.total,
                other.store_in()
            ),
            None => " Оформить там можно только самому.".into(),
        };
        return format!(
            "Дешевле всего {}: {} ₽, но корзину там собрать не могу.{alternative}{missing}",
            quote.store_in(),
            quote.total
        );
    }

    // Входа в браузере нет — корзина собирается ссылкой через MCP магазина.
    //
    // Ему не нужны ни браузер под управлением, ни вход, ни даже то, чтобы
    // витрина открывалась с этой машины, — MCP живёт на своём адресе. Корзина
    // открывается в браузере уже набранной, и человеку остаются доставка и
    // оплата. Оплатить сам Ноа может только в сессии браузера — см. ниже.
    if !crate::food::has_browser_session(app) {
        return basket_by_link(app, quote, shown, &missing, false).await;
    }

    let cart = match crate::food::add_to_cart(app, &quote.found).await {
        Ok(cart) => cart,
        // Сессия не справилась — витрина недоступна, вход слетел. Корзина
        // всё равно собирается: ссылкой через MCP, которому витрина не нужна.
        Err(err) => {
            log::warn!("корзина в сессии не наполнилась: {err} — собираю ссылкой");
            return basket_by_link(app, quote, shown, &missing, true).await;
        }
    };

    // В окне отмечаем построчно, что именно легло: «положил три из пяти» без
    // имён не даёт понять, чего не хватает.
    for line in &mut shown.lines {
        line.in_cart = cart.added.contains(&line.name);
    }
    shown.stage = crate::order::Stage::InCart;
    if let Some(total) = cart.total {
        shown.total = total;
    }
    let failed = match cart.failed.is_empty() {
        true => String::new(),
        false => format!(" Не легло: {}.", cart.failed.join(", ")),
    };

    // Оплата без участия человека — в пределах, которые человек задал сам:
    // суммой одного заказа и суммой за сутки (см. `crate::spend`), и только на
    // сумму, которую показала сама корзина магазина, а не на подсчитанную нами
    // по ценам поиска. Всё сверх пределов ждёт подтверждения кнопкой в окне
    // заказа: собрать корзину можно, а платить без человека — нет.
    let Some(total) = cart.total else {
        shown.note = "Сумму корзины прочитать не удалось — платить вслепую не стал.".into();
        crate::order::set(app, shown);
        return format!("Корзина собрана, но сумму прочитать не вышло — оплату не запускаю.{missing}");
    };
    let verdict = match food.auto_pay {
        true => crate::spend::verdict(
            &crate::spend::history(),
            Local::now(),
            total,
            food.max_order,
            food.daily_limit,
        ),
        false => crate::spend::Verdict::OverOrder,
    };
    let waiting = match verdict {
        crate::spend::Verdict::Allowed => None,
        _ if !food.auto_pay => Some(format!(
            "Готово, корзина на {total} ₽ собрана. Оплатить — кнопкой в окне заказа."
        )),
        crate::spend::Verdict::OverOrder => Some(format!(
            "Корзина на {total} ₽ — больше {} ₽, без вас не плачу. Подтвердите оплату в окне заказа.",
            food.max_order
        )),
        crate::spend::Verdict::OverDay { spent } => Some(format!(
            "Корзина на {total} ₽, а за сутки уже оплачено {spent} из {} ₽. Подтвердите оплату в окне заказа.",
            food.daily_limit
        )),
    };
    if let Some(spoken) = waiting {
        shown.stage = crate::order::Stage::AwaitingPayment;
        shown.total = total;
        shown.note = "Ждёт вашего подтверждения — кнопка ниже.".into();
        crate::order::set(app, shown);
        return format!("{spoken}{missing}{failed}");
    }

    match crate::food::checkout(app, total).await {
        Ok(done) if done.placed => {
            let paid = done.total_rub.unwrap_or(total);
            crate::spend::record(paid);
            shown.stage = crate::order::Stage::Placed;
            shown.total = paid;
            shown.note = "Заказ оформлен и оплачен.".into();
            crate::order::set(app, shown);
            // Сумму называем всегда: это деньги, списанные без вопроса.
            format!("Готово, заказ оформлен и оплачен на {paid} рублей.{missing}{failed}")
        }
        Ok(done) => {
            shown.note = format!("Не оформлено: {}", done.message);
            crate::order::set(app, shown);
            format!("Корзина собрана, а оформить не вышло: {}.", done.message)
        }
        Err(err) => {
            log::warn!("оформление не удалось: {err}");
            shown.note = format!("Не оплачено: {err}");
            crate::order::set(app, shown);
            format!("Корзина собрана, а оплатить не вышло: {err}.")
        }
    }
}

/// Первая буква — заглавная: «во ВкусВилле» в начале фразы.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Незакрытые дела — те, о которых может идти речь.
fn open_tasks() -> Vec<Task> {
    let mut open: Vec<Task> = tasks::all()
        .into_iter()
        .filter(|task| task.done_at.is_none())
        .collect();
    // Ближайшие сверху: о них и говорят чаще всего.
    open.sort_by_key(|task| task.due);
    open.truncate(20);
    open
}

/* ── Разбор реплики ──────────────────────────────────────────────────────── */

async fn read_intent(app: &AppHandle, said: &str, open: &[Task]) -> Intent {
    let Some(parsed) = interpret(app, &intent_rules(open), said).await else {
        // Не разобрали — считаем обычным вопросом. Промолчать в ответ на
        // вопрос хуже, чем не завести задачу: задачу человек повторит.
        return Intent::Chat;
    };

    let text = |key: &str| {
        parsed[key]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let due = parse_due(&text("due"));
    let task = pick_task(&parsed, open);

    let intent = text("intent");
    if !asked_for(&intent, said) {
        log::info!("«{said}» разобрано как {intent}, но просьбы в нём нет — считаю разговором");
        return Intent::Chat;
    }

    match intent.as_str() {
        "add" => {
            let title = text("title");
            if title.is_empty() {
                Intent::Chat
            } else {
                Intent::Add {
                    title,
                    due,
                    calendar: parsed["calendar"].as_bool().unwrap_or(false),
                }
            }
        }
        "list" => Intent::List,
        "done" => Intent::Done { task },
        "postpone" => Intent::Postpone { task, due },
        "breakdown" => Intent::Breakdown { task },
        "order" => {
            let items = wanted_items(&parsed);
            // Без товаров заказывать нечего, а молчаливый пустой заказ выглядел
            // бы как поломка. Пусть лучше ответит как на обычную фразу.
            if items.is_empty() {
                Intent::Chat
            } else {
                Intent::Order {
                    items,
                    store: text("store"),
                }
            }
        }
        "orderStatus" => Intent::OrderStatus,
        "launch" | "close" => {
            let program = text("app");
            match (program.is_empty(), text("intent") == "launch") {
                (true, _) => Intent::Chat,
                (false, true) => Intent::Launch {
                    program,
                    sandbox: parsed["sandbox"].as_bool().unwrap_or(false),
                    sandbox_box: text("box"),
                },
                (false, false) => Intent::Close { program },
            }
        }
        "power" => match crate::pc::Power::parse(&text("action")) {
            Some(action) => Intent::Power { action },
            None => Intent::Chat,
        },
        "web" => {
            let (site, query) = (text("site"), text("query"));
            if site.is_empty() && query.is_empty() {
                Intent::Chat
            } else {
                Intent::Web { site, query }
            }
        }
        // Без запроса ищем сказанное целиком: вопрос и есть запрос.
        "lookup" => Intent::Lookup {
            query: match text("query") {
                query if query.is_empty() => said.to_string(),
                query => query,
            },
        },
        "vpn" => match text("action").as_str() {
            "on" => Intent::Vpn { on: true },
            "off" => Intent::Vpn { on: false },
            _ => Intent::Chat,
        },
        "nav" => match crate::pc::Nav::parse(&text("action")) {
            Some(action) => Intent::Nav { action },
            None => Intent::Chat,
        },
        _ => Intent::Chat,
    }
}

/// Товары из разбора: `items` с количеством, а у старого вида — `dishes`.
///
/// Количество приходит как угодно — числом, строкой, дробью, — и держится в
/// разумных пределах: ноль значит «одну», а сорок — потолок, выше которого
/// корзина магазина всё равно не примет.
fn wanted_items(parsed: &serde_json::Value) -> Vec<crate::food::Wanted> {
    let quantity = |value: &serde_json::Value| -> u32 {
        let raw = value
            .as_u64()
            .or_else(|| value.as_f64().map(|number| number.round() as u64))
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
            .unwrap_or(1);
        raw.clamp(1, 40) as u32
    };

    let from = |list: &serde_json::Value| -> Vec<crate::food::Wanted> {
        list.as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let (name, count) = match item {
                            serde_json::Value::String(name) => (name.as_str(), 1),
                            object => (object["name"].as_str()?, quantity(&object["quantity"])),
                        };
                        let name = name.trim();
                        (!name.is_empty()).then(|| crate::food::Wanted {
                            name: name.to_string(),
                            quantity: count,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    match from(&parsed["items"]) {
        items if !items.is_empty() => items,
        _ => from(&parsed["dishes"]),
    }
}

/// Строка для разбора реплики: что сейчас в заказе.
///
/// Без неё «пять штук» и «нет, полосатые» разбирались как новый заказ — и Ноа
/// искал в магазине «пять штук». Заказ старше получаса в расчёт не идёт: к
/// тому времени человек говорит уже о другом.
fn order_line() -> String {
    let Some(order) = crate::order::current() else {
        return String::new();
    };
    let fresh = DateTime::parse_from_rfc3339(&order.updated_at)
        .map(|at| Local::now().signed_duration_since(at) < Duration::minutes(30))
        .unwrap_or(false);
    if !fresh || order.asked.is_empty() {
        return String::new();
    }
    let items = order
        .asked
        .iter()
        .map(|item| format!("{} ×{}", item.name, item.quantity))
        .collect::<Vec<_>>()
        .join(", ");
    match order.store.is_empty() {
        true => format!("Текущий заказ: {items}."),
        false => format!("Текущий заказ: {items}, магазин {}.", order.store),
    }
}

/// Есть ли в сказанном сама просьба.
///
/// Разбор реплики делает небольшая модель, и она охотно достраивает команду
/// из любой фразы: «звучит музыка» — открыть музыку, «программа сейчас
/// запущена» — открыть папку ProgramData, «всё по-прежнему открыто» — показать
/// список дел. Микрофон в разговоре слышит и комнату, так что таких фраз много.
/// Поэтому действие выполняется, только если в сказанном есть глагол этого
/// действия; иначе это разговор. Переспросить дешевле, чем открыть не то.
fn asked_for(intent: &str, said: &str) -> bool {
    let said = said.to_lowercase();
    let stems: &[&str] = match intent {
        "launch" => &[
            "открой", "откро", "открыть", "открывай", "запусти", "запуст", "включи", "включить",
            "вруби", "покажи", "зайди", "перейди", "стартани", "open", "launch", "run", "start",
        ],
        "close" => &[
            "закрой", "закро", "закрыть", "закрывай", "выключи", "выключить", "выруби",
            "заверши", "завершить", "убей", "убить", "сними", "снять", "останови",
            "остановить", "выйди", "close", "kill", "quit", "exit",
        ],
        "list" => &[
            "дел", "задач", "план", "напомин", "распис", "на сегодня", "на завтра", "на неделю",
        ],
        "web" => &[
            "открой", "откро", "зайди", "перейди", "найди", "поищи", "ищи", "посмотр", "покажи",
            "сайт",
        ],
        "nav" => &[
            "лист", "прокрут", "мотни", "мотай", "назад", "вперёд", "вперед", "вниз", "вверх",
            "вкладк", "обнови", "обновить", "сверни", "рабочий стол", "в начало", "в конец",
        ],
        "vpn" => &["впн", "vpn", "вэпээн", "випиэн", "ви пи эн"],
        "power" => &[
            "выключ", "перезагр", "перезапуст", "усыпи", "спящ", "сон", "заблокир", "блокир",
            "отмени", "отмена", "гибернац",
        ],
        _ => return true,
    };
    stems.iter().any(|stem| said.contains(stem))
}

/// Какое дело имелось в виду: модель называет его номером в переданном списке.
fn pick_task(parsed: &serde_json::Value, open: &[Task]) -> Option<String> {
    let number = parsed["task"].as_i64()?;
    if number < 1 {
        return None;
    }
    open.get((number - 1) as usize).map(|task| task.id.clone())
}

fn intent_rules(open: &[Task]) -> String {
    let list = if open.is_empty() {
        "Открытых дел нет.".to_string()
    } else {
        let lines: Vec<String> = open
            .iter()
            .enumerate()
            .map(|(at, task)| format!("[{}] {}", at + 1, task.title))
            .collect();
        format!("Открытые дела:\n{}", lines.join("\n"))
    };

    format!(
        "Ты — Ноа, голосовой помощник. Определи, чего хочет человек, и ответь \
         одним объектом JSON без пояснений.\n\
         \n\
         Поле intent — одно из:\n\
         chat — обычный вопрос, разговор, просьба что-то объяснить;\n\
         add — просит завести дело, напомнить о чём-то, записать, запланировать \
         встречу или добавить в календарь;\n\
         list — спрашивает, что у него запланировано, какие дела, что на сегодня;\n\
         done — сообщает, что уже что-то сделал;\n\
         postpone — просит перенести дело на другое время;\n\
         breakdown — просит помощи с делом: как за него взяться, с чего начать, \
         разбить на шаги;\n\
         order — просит заказать еду или продукты, купить их, оформить доставку; \
         только продукты: покупки на маркетплейсах — вайлдберриз, озон, маркет, \
         авито — это web с query;\n\
         orderStatus — спрашивает, что с заказом: что набрано, на сколько, \
         на каком он шаге;\n\
         launch — просит открыть или запустить программу, игру, окно или \
         системную настройку;\n\
         close — просит закрыть или выключить программу или окно (не компьютер);\n\
         power — про сам компьютер: усыпить, выключить, перезагрузить, \
         заблокировать или отменить выключение;\n\
         web — просит открыть сайт или поискать что-то на сайте, в том числе \
         на уже открытом: «давай посмотрим чехлы»;\n\
         lookup — спрашивает то, что надо посмотреть в интернете прямо сейчас: \
         часы работы, адрес, телефон, цены, расписание, новости, погоду, курс;\n\
         vpn — просит включить или выключить VPN;\n\
         nav — просит полистать страницу, вернуться назад или вперёд, закрыть \
         вкладку, обновить страницу или свернуть все окна.\n\
         \n\
         Остальные поля:\n\
         title — название дела для add: коротко, без слов «напомни» и «запиши»;\n\
         due — срок в виде ГГГГ-ММ-ДДTЧЧ:ММ или пустая строка, если не назван;\n\
         task — номер дела из списка ниже для done, postpone и breakdown, иначе 0;\n\
         calendar — true, если человек прямо просил в календарь;\n\
         items — для order список товаров: name — что именно, со всеми \
         уточнениями («полосатые семечки», а не «семечки»), quantity — сколько \
         штук или упаковок, по умолчанию 1;\n\
         app — для launch и close название программы или окна, как его назвал \
         человек, без слов «открой», «запусти», «закрой»;\n\
         sandbox — true, если просил запустить в песочнице;\n\
         box — имя песочницы, если названо, иначе пустая строка;\n\
         action — для power одно из: sleep, shutdown, restart, lock, cancel; \
         для vpn — on или off; для nav — down, up, top, bottom, back, forward, \
         close_tab, reload или desktop;\n\
         store — для order магазин, если человек его назвал (вкусвилл, магнит, \
         метро), иначе пустая строка;\n\
         site — для web сайт, как его назвали, иначе пустая строка;\n\
         query — для web что искать на сайте, для lookup короткий запрос для \
         поисковика, иначе пустая строка.\n\
         \n\
         Про программы и процессы — «сними задачу», «заверши процесс», «убей \
         программу», «закрой окно» — это close, а не дела: done, postpone и \
         breakdown только про дела из списка ниже.\n\
         Команды — только когда человек прямо просит что-то сделать. Рассказ о \
         том, что происходит, вопрос или обрывок фразы — это chat.\n\
         Если человек уточняет текущий заказ — сколько штук, какой именно \
         товар, в каком магазине, — верни order со всем заказом целиком, уже с \
         уточнением; товары, которых он не касался, оставь как были.\n\
         \n\
         Если назван день без времени — ставь 18:00. Полночь никому не нужна: \
         напоминание в это время человек не услышит.\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}",
        [now_line(), crate::web::site_line(), order_line()].join(" "),
        list,
        EXAMPLES
    )
}

/// Разобранные примеры.
///
/// Небольшие модели держат формат по образцу заметно лучше, чем по описанию.
/// Здесь же видно главное: обычный вопрос — это chat, и заводить по нему дело
/// не надо.
const EXAMPLES: &str = "Примеры при «Сейчас 2026-09-03 11:00, четверг, 3 сентября».\n\
     «что такое альбедо» → {\"intent\":\"chat\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false}\n\
     «добавь на сегодня доделать резюме задача» → \
     {\"intent\":\"add\",\"title\":\"доделать резюме\",\"due\":\"2026-09-03T18:00\",\"task\":0,\"calendar\":false}\n\
     «напомни завтра в три позвонить в банк» → \
     {\"intent\":\"add\",\"title\":\"позвонить в банк\",\"due\":\"2026-09-04T15:00\",\"task\":0,\"calendar\":false}\n\
     «поставь в календарь встречу с юристом в понедельник в десять» → \
     {\"intent\":\"add\",\"title\":\"встреча с юристом\",\"due\":\"2026-09-07T10:00\",\"task\":0,\"calendar\":true}\n\
     «какие у меня задачи на сегодня» → {\"intent\":\"list\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false}\n\
     «я доделал резюме» → {\"intent\":\"done\",\"title\":\"\",\"due\":\"\",\"task\":1,\"calendar\":false}\n\
     «перенеси банк на завтра» → \
     {\"intent\":\"postpone\",\"title\":\"\",\"due\":\"2026-09-04T15:00\",\"task\":2,\"calendar\":false}\n\
     «помоги мне с резюме, с чего начать» → \
     {\"intent\":\"breakdown\",\"title\":\"\",\"due\":\"\",\"task\":1,\"calendar\":false}\n\
     «закажи ленивые голубцы и свекольник» → \
     {\"intent\":\"order\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false,\
     \"items\":[{\"name\":\"ленивые голубцы\",\"quantity\":1},{\"name\":\"свекольник\",\"quantity\":1}]}\n\
     «что там с заказом» → \
     {\"intent\":\"orderStatus\",\"title\":\"\",\"due\":\"\",\"task\":0,\"calendar\":false}\n\
     «открой диспетчер устройств» → \
     {\"intent\":\"launch\",\"app\":\"диспетчер устройств\",\"sandbox\":false,\"box\":\"\"}\n\
     «запусти мортал шелл в песочнице» → \
     {\"intent\":\"launch\",\"app\":\"мортал шелл\",\"sandbox\":true,\"box\":\"\"}\n\
     «закрой телеграм» → {\"intent\":\"close\",\"app\":\"телеграм\"}\n\
     «выключи телеграм» → {\"intent\":\"close\",\"app\":\"телеграм\"}\n\
     «переведи компьютер в спящий режим» → {\"intent\":\"power\",\"action\":\"sleep\"}\n\
     «выключи компьютер» → {\"intent\":\"power\",\"action\":\"shutdown\"}\n\
     «отмени выключение» → {\"intent\":\"power\",\"action\":\"cancel\"}\n\
     «сними задачу телеграм» → {\"intent\":\"close\",\"app\":\"телеграм\"}\n\
     «заверши программу квинчат» → {\"intent\":\"close\",\"app\":\"квинчат\"}\n\
     «открой параметры bluetooth» → \
     {\"intent\":\"launch\",\"app\":\"bluetooth\",\"sandbox\":false,\"box\":\"\"}\n\
     «закажи семечки во вкусвилле» → \
     {\"intent\":\"order\",\"items\":[{\"name\":\"семечки\",\"quantity\":1}],\"store\":\"вкусвилл\"}\n\
     «закажи пять пачек полосатых семечек» → \
     {\"intent\":\"order\",\"items\":[{\"name\":\"полосатые семечки\",\"quantity\":5}],\"store\":\"\"}\n\
     при «Текущий заказ: семечки ×1» — «нет, полосатые, и пять штук» → \
     {\"intent\":\"order\",\"items\":[{\"name\":\"полосатые семечки\",\"quantity\":5}],\"store\":\"\"}\n\
     «открой вайлдберриз» → {\"intent\":\"web\",\"site\":\"вайлдберриз\",\"query\":\"\"}\n\
     «закажи чехол для айфона на вайлдберриз» → \
     {\"intent\":\"web\",\"site\":\"вайлдберриз\",\"query\":\"чехол для айфона\"}\n\
     «давай посмотрим чехлы для айфона 14» → \
     {\"intent\":\"web\",\"site\":\"\",\"query\":\"чехлы для айфона 14\"}\n\
     «до скольки работает кафе уют на сивцевом вражке» → \
     {\"intent\":\"lookup\",\"query\":\"кафе Уют Сивцев Вражек часы работы\"}\n\
     «выключи впн» → {\"intent\":\"vpn\",\"action\":\"off\"}\n\
     «полистай вниз» → {\"intent\":\"nav\",\"action\":\"down\"}\n\
     «вернись назад» → {\"intent\":\"nav\",\"action\":\"back\"}\n\
     «сверни все окна» → {\"intent\":\"nav\",\"action\":\"desktop\"}\n\
     «звучит музыка» → {\"intent\":\"chat\"}\n\
     «а что сейчас открыто» → {\"intent\":\"chat\"}\n\
     «программа сейчас запущена» → {\"intent\":\"chat\"}";

/* ── Завести ─────────────────────────────────────────────────────────────── */

fn add(app: &AppHandle, title: String, due: Option<DateTime<Local>>, calendar: bool) -> String {
    if due.is_none() {
        // Срок не назван — спрашиваем, а не назначаем сами. Дело без срока не
        // напомнит о себе, и человек узнает об этом слишком поздно.
        *AWAITING_TIME.lock().unwrap_or_else(|err| err.into_inner()) = Some(title.clone());
        return format!("Записал: {title}. На когда?");
    }

    let task = tasks::add(title, due, None);
    log::info!("заведено дело «{}» на {:?}", task.title, task.due);
    crate::calendar::sync_task(app, &task, calendar);
    changed(app);
    format!("Записал: {}, {}.", task.title, spoken_due(task.due))
}

/// Достаёт срок из ответа на вопрос «на когда?».
async fn finish_pending(app: &AppHandle, said: &str) -> String {
    let title = AWAITING_TIME
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
        .unwrap_or_default();

    if refuses_time(said) {
        let task = tasks::add(title, None, None);
        changed(app);
        return format!("Оставил без срока: {}.", task.title);
    }

    let due = match interpret(app, &time_rules(), said).await {
        Some(parsed) => parse_due(parsed["due"].as_str().unwrap_or_default()),
        None => None,
    };

    let task = tasks::add(title, due, None);
    crate::calendar::sync_task(app, &task, false);
    changed(app);
    match task.due {
        Some(_) => format!("Записал: {}, {}.", task.title, spoken_due(task.due)),
        None => format!("Срок не понял, оставил без него: {}.", task.title),
    }
}

/// Отказ называть срок.
fn refuses_time(said: &str) -> bool {
    let lower = said.to_lowercase();
    [
        "не надо",
        "без срока",
        "потом",
        "когда-нибудь",
        "неважно",
        "не знаю",
        "не важно",
    ]
    .iter()
    .any(|mark| lower.contains(mark))
}

/* ── Перечислить ─────────────────────────────────────────────────────────── */

fn list() -> String {
    let now = Local::now();
    let due = tasks::today(now);

    if due.is_empty() {
        return "На сегодня ничего не запланировано.".into();
    }

    let overdue = due.iter().filter(|task| task.overdue(now)).count();
    let names: Vec<String> = due
        .iter()
        .take(5)
        .map(|task| match task.due {
            Some(_) => format!("{} — {}", task.title, spoken_due(task.due)),
            None => task.title.clone(),
        })
        .collect();

    let mut answer = format!("{}: {}", headline(due.len(), overdue), names.join("; "));
    if due.len() > names.len() {
        answer.push_str(&format!(" и ещё {}", due.len() - names.len()));
    }
    answer.push('.');
    answer
}

fn headline(total: usize, overdue: usize) -> String {
    if overdue > 0 {
        format!("Всего {total}, из них просрочено {overdue}")
    } else {
        format!("На сегодня {total}")
    }
}

/* ── Отметить сделанным ──────────────────────────────────────────────────── */

fn done(app: &AppHandle, id: Option<&str>, open: &[Task]) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, какое дело закрыть.".into();
    };

    let Some(closed) = tasks::set_done(&task.id, true) else {
        return "Такого дела в списке нет.".into();
    };
    crate::calendar::forget_task(app, &closed);
    changed(app);

    let left = tasks::today(Local::now()).len();
    match left {
        0 => format!("Отметил: {}. На сегодня всё.", closed.title),
        _ => format!("Отметил: {}. Осталось {left}.", closed.title),
    }
}

/* ── Перенести ───────────────────────────────────────────────────────────── */

fn postpone(
    app: &AppHandle,
    id: Option<&str>,
    due: Option<DateTime<Local>>,
    open: &[Task],
) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, какое дело перенести.".into();
    };

    // Срок не назвали — переносим на завтра в то же время. Это самое частое
    // намерение, и переспрашивать ради него — лишний круг разговора.
    let to = due.unwrap_or_else(|| {
        task.due
            .map(|due| due + Duration::days(1))
            .unwrap_or_else(|| Local::now() + Duration::days(1))
    });

    let Some(moved) = tasks::postpone(&task.id, to) else {
        return "Такого дела в списке нет.".into();
    };
    crate::calendar::sync_task(app, &moved, false);
    changed(app);

    if moved.postponed >= 3 {
        return format!(
            "Перенёс: {}, {}. Это уже {}-й перенос — может, разбить его на шаги?",
            moved.title,
            spoken_due(moved.due),
            moved.postponed
        );
    }
    format!("Перенёс: {}, {}.", moved.title, spoken_due(moved.due))
}

/* ── Помочь с делом ──────────────────────────────────────────────────────── */

async fn breakdown(app: &AppHandle, id: Option<&str>, open: &[Task]) -> String {
    let Some(task) = only_one_or(id, open) else {
        return "Не понял, с каким делом помочь.".into();
    };

    let Some(parsed) = interpret(app, PLAN_RULES, &task.title).await else {
        return "Не смог придумать план. Повторите, пожалуйста.".into();
    };

    let steps: Vec<String> = parsed["steps"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|step| step.as_str())
                .map(|step| step.trim().to_string())
                .filter(|step| !step.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return "Не смог разбить это на шаги.".into();
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty())
        .map(str::to_string);

    tasks::set_plan(&task.id, steps.clone(), advice.clone());
    if let Err(err) = crate::overlay::show_tasks(app) {
        log::warn!("окно задач не открылось: {err}");
    }
    changed(app);

    let spoken = steps
        .iter()
        .enumerate()
        .map(|(at, step)| format!("{}. {step}", at + 1))
        .collect::<Vec<_>>()
        .join(" ");

    match advice {
        Some(advice) => format!("Разбил на шаги. {spoken} {advice}"),
        None => format!("Разбил на шаги. {spoken}"),
    }
}

/// Разбивает названную задачу на шаги. Зовётся из окна кнопкой.
pub async fn plan_task(app: &AppHandle, id: &str) -> Result<Option<Task>, String> {
    let Some(task) = tasks::all().into_iter().find(|task| task.id == id) else {
        return Ok(None);
    };

    let Some(parsed) = interpret(app, PLAN_RULES, &task.title).await else {
        return Err("модель не ответила".into());
    };

    let steps: Vec<String> = parsed["steps"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|step| step.as_str())
                .map(|step| step.trim().to_string())
                .filter(|step| !step.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return Err("не вышло разбить это на шаги".into());
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty())
        .map(str::to_string);

    Ok(tasks::set_plan(&task.id, steps, advice))
}

const PLAN_RULES: &str = "Разбей дело на 3–5 понятных шагов и дай один короткий совет, \
     с чего начать. Ответь одним объектом JSON без пояснений:\n\
     {\"steps\": [\"…\", \"…\"], \"advice\": \"…\"}\n\
     Шаги — в неопределённой форме, каждый на одно действие, не длиннее семи слов.\n\
     Совет — одно предложение.\n\
     Пример для дела «разослать резюме»:\n\
     {\"steps\":[\"обновить опыт за последний год\",\"собрать список из десяти вакансий\",\
     \"написать сопроводительное письмо\",\"отправить и записать даты\"],\
     \"advice\":\"Начните со списка вакансий — он покажет, что править в резюме.\"}";

/// Дело, о котором речь: названное моделью или единственное открытое.
fn only_one_or<'a>(id: Option<&str>, open: &'a [Task]) -> Option<&'a Task> {
    if let Some(id) = id {
        return open.iter().find(|task| task.id == id);
    }
    // Открыто ровно одно — понятно, о чём речь, даже если не назвали.
    match open {
        [single] => Some(single),
        _ => None,
    }
}

/* ── Разговор с моделью ──────────────────────────────────────────────────── */

async fn interpret(app: &AppHandle, rules: &str, said: &str) -> Option<serde_json::Value> {
    let provider = app.state::<AppState>().provider();
    let raw = match provider.interpret(rules, said).await {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("разбор реплики не удался: {err}");
            return None;
        }
    };

    // Модель охотно добавляет пояснения вокруг ответа. Берём то, что между
    // первой и последней фигурной скобкой.
    let Some((from, to)) = raw.find('{').zip(raw.rfind('}')) else {
        log::warn!("в ответе разбора нет JSON: «{raw}»");
        return None;
    };
    match serde_json::from_str(&raw[from..=to]) {
        Ok(parsed) => {
            log::info!("разобрано: {}", &raw[from..=to]);
            Some(parsed)
        }
        Err(err) => {
            log::warn!("ответ разбора не разобрался ({err}): «{raw}»");
            None
        }
    }
}

fn time_rules() -> String {
    format!(
        "Человек называет срок дела. {}\n\
         Ответь одним объектом JSON и ничем больше: ни приветствия, ни пояснений.\n\
         due — ГГГГ-ММ-ДДTЧЧ:ММ по местному времени, либо пустая строка, если срок не назван.\n\
         Если названо только время без дня — возьми ближайший день, когда оно ещё не прошло.\n\
         Пример при «Сейчас 2026-09-03 11:00, четверг».\n\
         Сказано: завтра утром\n\
         Ответ: {{\"due\": \"2026-09-04T09:00\"}}",
        now_line()
    )
}

/// Точка отсчёта для модели: без неё «завтра» не во что превратить.
pub fn now_line() -> String {
    let now = Local::now();
    format!(
        "Сейчас {}, {}, {}.",
        now.format("%Y-%m-%d %H:%M"),
        weekday(now),
        month_day(now)
    )
}

fn weekday(now: DateTime<Local>) -> &'static str {
    match now.weekday().num_days_from_monday() {
        0 => "понедельник",
        1 => "вторник",
        2 => "среда",
        3 => "четверг",
        4 => "пятница",
        5 => "суббота",
        _ => "воскресенье",
    }
}

fn month_day(now: DateTime<Local>) -> String {
    const MONTHS: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    format!("{} {}", now.day(), MONTHS[(now.month0() as usize).min(11)])
}

/* ── Сроки ───────────────────────────────────────────────────────────────── */

/// Превращает то, что вернула модель, в срок.
pub fn parse_due(raw: &str) -> Option<DateTime<Local>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Ok(exact) = DateTime::parse_from_rfc3339(raw) {
        return Some(exact.with_timezone(&Local));
    }

    for shape in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, shape) {
            // Час перевода стрелок бывает несуществующим и бывает двойным.
            // В первом случае берём ближайший существующий, во втором — ранний.
            if let Some(exact) = Local.from_local_datetime(&naive).earliest() {
                return Some(exact);
            }
        }
    }

    log::warn!("срок «{raw}» не разобрался");
    None
}

/// Срок словами — так, как его произносят.
pub fn spoken_due(due: Option<DateTime<Local>>) -> String {
    let Some(due) = due else {
        return "без срока".into();
    };

    let now = Local::now();
    let days = due
        .date_naive()
        .signed_duration_since(now.date_naive())
        .num_days();
    let time = due.format("%H:%M").to_string();

    match days {
        0 => format!("сегодня в {time}"),
        1 => format!("завтра в {time}"),
        2..=6 => format!("в {} в {time}", weekday(due)),
        _ => format!("{} в {time}", month_day(due)),
    }
}

/// Сообщает окнам, что список изменился.
pub fn changed(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("tasks:changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_not_commands() {
        // Всё это модель однажды разобрала как команды.
        assert!(!asked_for("launch", "Звучит музыка."));
        assert!(!asked_for("launch", "Программа сейчас запущена."));
        assert!(!asked_for("launch", "А что сейчас открыто?"));
        assert!(!asked_for("list", "Но всё по-прежнему открыто."));
        assert!(asked_for("launch", "Ноа, открой телеграм"));
        assert!(asked_for("launch", "запусти мортал шелл в песочнице"));
        assert!(asked_for("close", "Сними задачу с Квинчат"));
        assert!(asked_for("list", "какие у меня задачи на сегодня"));
        assert!(asked_for("nav", "полистай вниз"));
        // Остальные намерения фильтр не трогает.
        assert!(asked_for("chat", "что угодно"));
    }

    fn task(id: &str, title: &str) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            due: None,
            remind_at: None,
            done_at: None,
            created_at: Local::now(),
            reminded: false,
            event_id: None,
            steps: Vec::new(),
            advice: None,
            postponed: 0,
        }
    }

    #[test]
    fn a_deadline_is_read_in_local_time() {
        let parsed = parse_due("2026-08-25T15:30").expect("срок разобран");
        assert_eq!(
            parsed.format("%Y-%m-%d %H:%M").to_string(),
            "2026-08-25 15:30"
        );
        // Модель иногда добавляет секунды или пробел вместо T.
        assert!(parse_due("2026-08-25 15:30").is_some());
        assert!(parse_due("2026-08-25T15:30:00").is_some());

        assert!(parse_due("").is_none());
        assert!(parse_due("когда-нибудь").is_none());
    }

    #[test]
    fn a_task_is_chosen_by_its_number() {
        let open = vec![task("a", "позвонить в банк"), task("b", "доделать резюме")];

        let parsed: serde_json::Value = serde_json::from_str(r#"{"task": 2}"#).unwrap();
        assert_eq!(pick_task(&parsed, &open).as_deref(), Some("b"));

        // Ноль означает «дело не названо».
        let none: serde_json::Value = serde_json::from_str(r#"{"task": 0}"#).unwrap();
        assert!(pick_task(&none, &open).is_none());

        // Номер за пределами списка не должен выбирать наугад.
        let far: serde_json::Value = serde_json::from_str(r#"{"task": 9}"#).unwrap();
        assert!(pick_task(&far, &open).is_none());
    }

    #[test]
    fn the_only_open_task_needs_no_naming() {
        let single = vec![task("a", "позвонить в банк")];
        assert_eq!(
            only_one_or(None, &single).map(|task| task.id.as_str()),
            Some("a"),
            "когда дело одно, называть его незачем"
        );

        let many = vec![task("a", "первое"), task("b", "второе")];
        assert!(
            only_one_or(None, &many).is_none(),
            "из двух дел наугад выбирать нельзя"
        );
    }

    #[test]
    fn refusing_a_deadline_is_understood() {
        assert!(refuses_time("да не надо срока"));
        assert!(refuses_time("потом решу"));
        assert!(!refuses_time("завтра в три"));
    }
}
