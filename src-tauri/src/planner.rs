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
use std::sync::{Arc, Mutex};

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
    /// Удалить дело из списка — не отметить сделанным.
    Remove { task: Option<String> },
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
    /// `force` — «сними задачу»: завершить все процессы разом.
    Close { program: String, force: bool },
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
    /// Что с компьютером: процессор, память, диски.
    System { topic: crate::sysinfo::Topic },
    /// Что за ошибка на экране и что с ней делать.
    Diagnose,
    /// Найти файл на компьютере.
    Find { query: String, kind: crate::files::Kind },
    /// Напечатать текст в окне, где человек работает.
    Type { text: String },
    /// Написать сообщение в мессенджере: открыть его и заготовить текст.
    Message { app: String, to: String, text: String },
    /// Отправить напечатанное.
    Send,
    /// Вставить заготовленное сообщение в открытый чат.
    Paste,
    /// Вопрос про скриншот в буфере или про то, что на экране. `check` —
    /// проверить, правда ли это.
    Screen { check: bool },
    /// Цена криптовалюты или курс валюты.
    Price { asset: String },
    /// Передать разговор Claude: открыть его и задать ему вопрос.
    Claude { text: String },
    /// Список активов: показать, добавить, убрать.
    Watch { action: String, asset: String, tab: String, price: String },
    /// Обучение: открыть курс, узнать прогресс, опрос голосом.
    Learn { action: String, topic: String },
    /// Инструмент своего модуля (MCP): `модуль.инструмент` и аргументы.
    Tool { tool: String, args: serde_json::Value },
    /// Готовый ответ без действия: переспросить, пояснить.
    Say(String),
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
    // И вопрос «Отправить?»: «да» в следующем разговоре — ответ уже не ему.
    *PENDING_SEND.lock().unwrap_or_else(|err| err.into_inner()) = None;
}

/// Выполняет распоряжение и отдаёт то, что сказать вслух.
///
/// `None` означает «это был обычный вопрос» — фразу надо обработать как всегда.
pub async fn handle(app: &AppHandle, said: &str) -> Option<String> {
    // Звонит будильник — сказанное почти наверняка к нему.
    if let Some(reply) = ringing_reply(said) {
        return Some(reply);
    }
    // Напечатанное ждёт подтверждения: «да» — отправить, «нет» — оставить.
    if let Some(reply) = confirm_send(said) {
        return Some(reply);
    }
    // «Отвечай без окна», «показывай окно» — переключатель, а не вопрос.
    if let Some(show) = window_request(said) {
        return Some(crate::set_show_window(app, show));
    }
    // «Привет», «ты тут?», «проверка связи» — ответ одной фразой, без модели:
    // модель на такое представлялась и предлагала помощь.
    if let Some(reply) = presence_reply(said) {
        return Some(reply.into());
    }
    // «Который час», «какое сегодня число» — по часам компьютера, без модели:
    // точно и сразу.
    if let Some(reply) = clock_answer(said, Local::now()) {
        return Some(reply);
    }
    // Таймер и будильник — тоже по часам компьютера.
    if let Some(reply) = timer_request(app, said, Local::now()) {
        return Some(reply);
    }
    // «Включи музыку» без уточнений — своя станция из настроек.
    if let Some(reply) = music_request(app, said) {
        return Some(reply);
    }
    // «Сделай скриншот», «скинь скрин хрома».
    if let Some(reply) = screenshot_request(app, said) {
        return Some(reply);
    }
    // Дело ждёт срока — значит, сказанное сейчас и есть срок.
    if awaiting_time() {
        return Some(finish_pending(app, said).await);
    }
    // Идёт опрос по курсу — сказанное и есть ответ на вопрос.
    if let Some(reply) = crate::learning::quiz_answer(app, said).await {
        return Some(reply);
    }
    // «Убери всё сделанное», «что в разделе сделано» — по словам, без модели:
    // разбор такие просьбы путал с удалением одного дела и сносил не то.
    if let Some(reply) = done_tasks_request(app, said) {
        return Some(reply);
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
        Intent::Remove { task } => Some(remove(app, said, task.as_deref(), &open)),
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
            None => {
                let reply =
                    blocking(move || crate::pc::launch(&program, sandbox, &sandbox_box)).await;
                // В разговоре без рук «открой то, чего нет» — почти всегда
                // ослышка: музыка или чужая речь, из которой распознавание
                // сложило «открой». Молча, и ход засчитывается пустым.
                if ambient() && reply.starts_with("Не нашёл") {
                    log::info!("{reply} — в разговоре без рук, не произношу");
                    JUNK.store(true, std::sync::atomic::Ordering::SeqCst);
                    String::new()
                } else {
                    reply
                }
            }
        }),
        Intent::Close { program, force } => {
            Some(blocking(move || crate::pc::close(&program, force)).await)
        }
        Intent::Power { action } => Some(crate::pc::power(action)),
        Intent::Web { site, query } => {
            Some(blocking(move || crate::web::open_site(&site, &query)).await)
        }
        Intent::Lookup { query } => Some(crate::web::lookup(app, &query).await),
        Intent::Vpn { on } => Some(blocking(move || crate::pc::vpn(on)).await),
        Intent::Nav { action } => Some(blocking(move || crate::pc::navigate(action)).await),
        Intent::System { topic } => Some(blocking(move || crate::sysinfo::status(topic)).await),
        Intent::Diagnose => Some(crate::sysinfo::diagnose(app, said).await),
        Intent::Find { query, kind } => {
            Some(blocking(move || crate::files::find(&query, kind)).await)
        }
        Intent::Type { text } => Some(blocking(move || type_and_ask(&text)).await),
        Intent::Message {
            app: messenger,
            to,
            text,
        } => Some(blocking(move || draft_message(&messenger, &to, &text)).await),
        // Отправка без напечатанного — нажатие Enter в чужом окне вслепую. Её
        // нет: отправляется только то, что Ноа напечатал и о чём спросил.
        Intent::Send => Some("Отправлять нечего — сначала скажите, что напечатать.".into()),
        Intent::Paste => Some(blocking(paste_pending).await),
        Intent::Screen { check } => Some(crate::screen::answer(app, said, check).await),
        // Точная цена — у CoinGecko и ЦБ; не нашлось там — ищет Google.
        Intent::Price { asset } => Some(match crate::prices::price(&asset).await {
            Some(answer) => answer,
            None => crate::web::lookup(app, said).await,
        }),
        Intent::Watch { action, asset, tab, price } => {
            Some(watch(app, &action, &asset, &tab, &price).await)
        }
        Intent::Learn { action, topic } => Some(crate::learning::voice(app, &action, &topic).await),
        Intent::Tool { tool, args } => Some(use_tool(app, said, tool, args).await),
        Intent::Claude { text } => {
            // Разговор уходит в Claude — Ноа после ответа замолкает и уходит.
            HANDOFF.store(true, std::sync::atomic::Ordering::SeqCst);
            Some(blocking(move || crate::pc::ask_claude(&text)).await)
        }
        Intent::Say(text) => Some(text),
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

/// Напечатанное, которое ждёт «отправь», — когда оно напечатано.
static PENDING_SEND: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Сообщение, которое ждёт, пока человек откроет нужный чат и скажет «вставь».
static PENDING_TEXT: std::sync::Mutex<Option<(String, std::time::Instant)>> =
    std::sync::Mutex::new(None);

/// Печатает и спрашивает, отправлять ли.
///
/// Отправка — отдельный шаг с подтверждением, всегда. Сообщение, ушедшее от
/// имени человека по ошибке распознавания, не вернуть, а «да» стоит секунду.
fn type_and_ask(text: &str) -> String {
    let typed = crate::pc::type_text(text);
    if typed.starts_with("Напечатал") {
        *PENDING_SEND.lock().unwrap_or_else(|err| err.into_inner()) = Some(std::time::Instant::now());
        return "Напечатал. Отправить?".into();
    }
    typed
}

fn send_now() -> String {
    *PENDING_SEND.lock().unwrap_or_else(|err| err.into_inner()) = None;
    match crate::pc::press_enter() {
        true => "Отправил.".into(),
        false => "Не получилось нажать Enter.".into(),
    }
}

fn paste_pending() -> String {
    let pending = PENDING_TEXT.lock().unwrap_or_else(|err| err.into_inner()).take();
    match pending {
        Some((text, at)) if at.elapsed() < std::time::Duration::from_secs(600) => type_and_ask(&text),
        _ => "Вставлять нечего — скажите, что написать.".into(),
    }
}

/// Сообщение в мессенджере: открыть его и заготовить текст.
///
/// Нужный чат человек выбирает сам — щелчком. Искать собеседника по имени в
/// чужом окне значит печатать вслепую: если в окне открыт другой чат, имя
/// собеседника легло бы в его поле ввода. Поэтому: мессенджер открыт, текст
/// заготовлен, чат выбран человеком — и только тогда «вставь», а за ним
/// «отправь».
fn draft_message(messenger: &str, to: &str, text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "Что написать?".into();
    }
    let messenger = match messenger.trim() {
        "" => "telegram",
        named => named,
    };
    let name = match crate::pc::open_named(messenger) {
        Ok(name) => name,
        Err(err) => return format!("Не открыл {messenger}: {err}."),
    };
    *PENDING_TEXT.lock().unwrap_or_else(|err| err.into_inner()) =
        Some((text.to_string(), std::time::Instant::now()));
    let whom = match to.trim() {
        "" => String::new(),
        named => format!(" с {named}"),
    };
    format!(
        "Открыл {name}. Откройте чат{whom} и скажите «вставь» — я впишу: «{text}». \
         Отправлю, только когда скажете."
    )
}

/// Ответ на «Отправить?»: «да» отправляет, «нет» оставляет текст в поле.
/// Любая другая фраза — значит, человек занялся другим, и вопрос снимается.
fn confirm_send(said: &str) -> Option<String> {
    {
        let mut pending = PENDING_SEND.lock().unwrap_or_else(|err| err.into_inner());
        let fresh = pending.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(120));
        if !fresh {
            *pending = None;
            return None;
        }
        *pending = None;
    }
    let lower = said.to_lowercase();
    let words: Vec<&str> = lower
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    const NO: &[&str] = &["нет", "не", "отмена", "стоп", "погоди", "подожди"];
    const YES: &[&str] = &[
        "да", "отправь", "отправляй", "отправить", "давай", "ага", "конечно", "жми", "угу",
    ];
    if words.iter().any(|word| NO.contains(word)) {
        return Some("Хорошо, не отправляю — текст остался в поле.".into());
    }
    if words.iter().any(|word| YES.contains(word)) {
        return Some(send_now());
    }
    None
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

    // Остановили клавишей Esc, пока собиралась корзина, — не платим: человек
    // передумал, а списанные деньги не вернуть.
    if crate::turn_cancelled() {
        shown.stage = crate::order::Stage::AwaitingPayment;
        shown.total = total;
        shown.note = "Остановлено клавишей Esc — оплатить можно кнопкой ниже.".into();
        crate::order::set(app, shown);
        return "Остановил: корзина собрана, но не оплачена.".into();
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
    let Some(parsed) = interpret(app, &intent_rules(app, open), said).await else {
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

    // «Купи яйца, бекон и хлеб» модель порой записывает делом — так в список
    // дел и попал завтрак. Покупка — это заказ: если в сказанном нет ни
    // «напомни», ни «задачи», а есть «купи» или «закажи», то, что модель
    // назвала делом, заказывается.
    if intent == "add" && !asks_to_note(said) && bought(said) {
        let items: Vec<crate::food::Wanted> = text("title")
            .split([',', ';'])
            .flat_map(|part| part.split(" и "))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| crate::food::Wanted {
                name: name.to_string(),
                quantity: 1,
            })
            .collect();
        if !items.is_empty() {
            log::info!("«{said}» разобрано как дело, но это покупка — заказываю");
            return Intent::Order {
                items,
                store: text("store"),
            };
        }
    }

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
                    // «Через час» считает программа — модель в такой арифметике
                    // ошибается.
                    due: relative_due(said, Local::now()).or(due),
                    calendar: parsed["calendar"].as_bool().unwrap_or(false),
                }
            }
        }
        "list" => Intent::List,
        // «Удали эту задачу» модель порой записывает как «сделано» — и
        // отмечает не ту: так вместо лишнего дела закрылось чужое. Удаление —
        // отдельное действие.
        "done" | "remove" if intent == "remove" || erases(said) => Intent::Remove { task },
        "done" => Intent::Done { task },
        "postpone" => Intent::Postpone {
            task,
            due: relative_due(said, Local::now()).or(due),
        },
        "breakdown" => Intent::Breakdown { task },
        "order" => {
            // «Корзина», «заказ», «покупки» — не товары, а слова о самом заказе:
            // «собери во ВкусВилле корзину» модель порой так и записывает —
            // товаром «корзина».
            let mut items: Vec<crate::food::Wanted> = wanted_items(&parsed)
                .into_iter()
                .filter(|item| !about_the_order(&item.name))
                .collect();
            // «Ингредиенты для завтрака» — не товар: искать эти слова в
            // магазине бесполезно, а наберётся что попало. Переспрашиваем.
            if items.iter().any(|item| vague(&item.name)) {
                return Intent::Say(
                    "Скажите, какие именно продукты — например: яйца, хлеб, молоко.".into(),
                );
            }
            // Уточнение без новых товаров — «собери во ВкусВилле корзину».
            // Модель, не найдя товаров в сказанном, берёт их из примеров: так
            // вместо яиц и молока приехали семечки. Если ни один товар в
            // сказанном не звучал, а фраза — продолжение, берём текущий заказ.
            let mentioned = items.iter().any(|item| mentions(said, &item.name));
            if !mentioned && follow_up(said) {
                if let Some(current) = fresh_order_items() {
                    log::info!("«{said}» — уточнение без новых товаров, беру текущий заказ");
                    items = current;
                }
            }
            // Без товаров заказывать нечего, а молчаливый пустой заказ выглядел
            // бы как поломка. Просили купить — переспрашиваем, что именно;
            // иначе пусть ответит как на обычную фразу.
            if items.is_empty() && bought(said) {
                Intent::Say("Что заказать? Назовите продукты — например: яйца, хлеб, молоко.".into())
            } else if items.is_empty() {
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
                (false, false) => Intent::Close {
                    program,
                    force: forced(said),
                },
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
        "system" => Intent::System {
            // «Память видеокарты» модель порой относит к памяти вообще — и
            // отвечала про оперативную. Видеокарта в сказанном — значит, она.
            topic: if about_gpu(said) {
                crate::sysinfo::Topic::Gpu
            } else {
                crate::sysinfo::Topic::parse(&text("topic"))
            },
        },
        "diagnose" => Intent::Diagnose,
        "find" => Intent::Find {
            query: match text("query") {
                query if query.is_empty() => said.to_string(),
                query => query,
            },
            kind: crate::files::Kind::parse(&text("kind")),
        },
        "type" => match text("text") {
            typed if typed.is_empty() => Intent::Say("Что напечатать?".into()),
            typed => Intent::Type { text: typed },
        },
        "message" => Intent::Message {
            app: text("app"),
            to: text("to"),
            text: text("text"),
        },
        "send" => Intent::Send,
        "paste" => Intent::Paste,
        "screen" => Intent::Screen {
            check: parsed["check"].as_bool().unwrap_or(false) || checks_truth(said),
        },
        "price" => Intent::Price {
            asset: match text("asset") {
                asset if asset.is_empty() => said.to_string(),
                asset => asset,
            },
        },
        "claude" => Intent::Claude { text: text("text") },
        "learn" => Intent::Learn {
            action: text("action"),
            topic: text("topic"),
        },
        "watch" => Intent::Watch {
            action: text("action"),
            asset: text("asset"),
            tab: text("tab"),
            // Цену модель пишет и числом, и строкой.
            price: parsed["price"]
                .as_f64()
                .map(|price| price.to_string())
                .or_else(|| parsed["price"].as_str().map(str::to_string))
                .unwrap_or_default(),
        },
        // Инструмент — только из тех, что модули действительно отдали: имя,
        // придуманное моделью, вызвать нечего.
        "tool" => {
            let tool = text("tool");
            if crate::plugins::tools().iter().any(|known| known.full_name() == tool) {
                Intent::Tool {
                    tool,
                    args: parsed["args"].clone(),
                }
            } else {
                log::info!("модель назвала инструмент «{tool}», а такого нет — считаю разговором");
                Intent::Chat
            }
        }
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

/// Просят ли записать дело: «напомни», «запиши», «добавь в задачи».
fn asks_to_note(said: &str) -> bool {
    let said = said.to_lowercase();
    [
        "напомн", "запиш", "задач", "дело", "дела", "делах", "делам", "план", "календар",
        "не забыть", "не забудь", "заплан", "встреч", "дедлайн", "добав",
    ]
    .iter()
    .any(|stem| said.contains(stem))
}

/// Можно ли показать текст русскому читателю: без иероглифов и корейского.
///
/// Локальная модель порой соскальзывает в китайский посреди русской фразы —
/// «написать первое 草稿». Такой шаг или совет не показывается.
fn readable(text: &str) -> bool {
    !text.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
        )
    })
}

/// Последнее записанное дело и когда — для «удали эту».
static LAST_ADDED: std::sync::Mutex<Option<(String, std::time::Instant)>> =
    std::sync::Mutex::new(None);

fn remember_added(id: &str) {
    *LAST_ADDED.lock().unwrap_or_else(|err| err.into_inner()) =
        Some((id.to_string(), std::time::Instant::now()));
}

/// Дело, записанное в последние полчаса.
fn recently_added() -> Option<String> {
    LAST_ADDED
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .filter(|(_, at)| at.elapsed() < std::time::Duration::from_secs(30 * 60))
        .map(|(id, _)| id.clone())
}

/// Удаляет дело.
///
/// Какое — решается по сказанному, а не по номеру от модели: номер она
/// угадывает, и однажды вместо лишнего дела «термин» закрыла «найти битки».
/// Время в просьбе — «на 18:00» — выбирает дело по сроку, слова названия —
/// «про банк» — дело с ними; «эту», «только что», «не просил записывать» — то,
/// что записано последним. Номер от модели — в последнюю очередь. Не нашлось
/// ничего — переспрашивается: удалить наугад хуже, чем спросить.
fn remove(app: &AppHandle, said: &str, task: Option<&str>, open: &[Task]) -> String {
    let by_time = time_in(said).and_then(|(hour, minute)| {
        let wanted = format!("{hour:02}:{minute:02}");
        open.iter()
            .find(|task| task.due.is_some_and(|due| due.format("%H:%M").to_string() == wanted))
            .map(|task| task.id.clone())
    });
    let id = by_time
        .or_else(|| by_title(said, open))
        .or_else(|| recently_added().filter(|_| points_at_last(said)))
        .or_else(|| task.map(str::to_string));
    let Some(id) = id else {
        return "Не понял, какое дело удалить, — назовите его.".into();
    };
    match tasks::remove(&id) {
        Some(task) => {
            log::info!("удалено дело «{}»", task.title);
            changed(app);
            format!("Удалил: {}.", task.title)
        }
        None => "Такого дела уже нет.".into(),
    }
}

/// Дело, чьё название звучит в просьбе: «удали задачу про банк».
fn by_title(said: &str, open: &[Task]) -> Option<String> {
    // Слова, которые есть в любой просьбе о делах, названия не выдают.
    const COMMON: &[&str] = &["задач", "дело", "дела", "удали", "удалить", "запис", "напом"];
    let said = said.to_lowercase().replace('ё', "е");
    let stem = |word: &str| word.chars().take(5).collect::<String>();
    open.iter()
        .map(|task| {
            let title = task.title.to_lowercase().replace('ё', "е");
            let hits = title
                .split(|ch: char| !ch.is_alphabetic())
                .filter(|word| word.chars().count() >= 4)
                .map(stem)
                .filter(|word| !COMMON.iter().any(|common| word.starts_with(common)))
                .filter(|word| said.contains(word.as_str()))
                .count();
            (hits, task)
        })
        .filter(|(hits, _)| *hits > 0)
        .max_by_key(|(hits, _)| *hits)
        .map(|(_, task)| task.id.clone())
}

/// Время, названное в просьбе: «на 18:00», «в 9».
fn time_in(said: &str) -> Option<(u32, u32)> {
    let chars: Vec<char> = said.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at].is_ascii_digit() {
            at += 1;
        }
        let hour: String = chars[start..at].iter().collect();
        let (Ok(hour), true) = (hour.parse::<u32>(), at - start <= 2) else {
            continue;
        };
        let minutes = chars.get(at + 1..at + 3).map(|pair| pair.iter().collect::<String>());
        if let (Some(':' | '.'), Some(Ok(minute))) = (
            chars.get(at).copied(),
            minutes.as_deref().map(str::parse::<u32>),
        ) {
            if hour < 24 && minute < 60 {
                return Some((hour, minute));
            }
        }
    }
    let lower = said.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    words.windows(2).find_map(|pair| {
        let hour = pair[1]
            .trim_matches(|ch: char| !ch.is_ascii_digit())
            .parse::<u32>()
            .ok()?;
        (matches!(pair[0], "в" | "на") && hour < 24).then_some((hour, 0))
    })
}

/// Про только что записанное ли речь: «эту», «последнюю», «не просил».
fn points_at_last(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["эту", "это", "последн", "только что", "не просил", "лишн", "запись", "записал"]
        .iter()
        .any(|word| lower.contains(word))
}

/// Просят удалить, а не отметить сделанным.
fn erases(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["удал", "убери", "убрать", "сотри", "стереть", "стер"]
        .iter()
        .any(|word| lower.contains(word))
}

/// Приветствие или проверка связи — и ничего больше.
///
/// Узнаётся, только если каждое слово из этого набора: «привет, открой
/// телеграм» — уже поручение, и оно уходит дальше.
fn presence_reply(said: &str) -> Option<&'static str> {
    const WORDS: &[&str] = &[
        "привет", "приветик", "здравствуй", "здравствуйте", "здорово", "хай", "салют", "добрый",
        "доброе", "день", "утро", "вечер", "ты", "тут", "здесь", "слышишь", "слышно", "меня",
        "алло", "на", "связи", "проверка", "связь", "а", "ну", "эй", "как", "дела", "ноа",
    ];
    let lower = said.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() || !words.iter().all(|word| WORDS.contains(word)) {
        return None;
    }
    let has = |word: &str| words.contains(&word);
    Some(if has("дела") {
        "Нормально, работаю."
    } else if has("тут") || has("здесь") || has("слышишь") || has("слышно") || has("алло") || has("связи") || has("связь") {
        "Тут, слышу."
    } else {
        "Привет!"
    })
}

/// Что просят сделать со сделанными делами целиком.
#[derive(Debug, PartialEq)]
enum DoneRequest {
    /// Убрать все сделанные.
    Clear,
    /// Перечислить сделанные.
    List,
    /// «Очисти задачи» без уточнения — сделанные или все.
    Which,
}

/// Узнаёт просьбы про сделанные дела целиком.
///
/// Разбор моделью для них не годится: «удали всё, что сделано» он записывал
/// удалением одного дела, и под руку попадало невыполненное. Все дела подряд
/// голосом не удаляются — только сделанные: снести нужное одной фразой
/// слишком легко.
fn done_request(said: &str) -> Option<DoneRequest> {
    let lower = said.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
    let about_done = has(&["сделан", "выполнен", "отмечен", "закрыт"]);
    let clearing = has(&[
        "удал", "убер", "убра", "очист", "отчист", "почист", "сотр", "стер", "снес", "архив",
        "выкин",
    ]);

    if about_done && clearing {
        return Some(DoneRequest::Clear);
    }
    // «Отметь уборку сделанной», «я сделал уборку» — это про одно дело.
    let reports = has(&["отметь", "отметить", "пометь", "сделал", "выполнил", "закрой", "закончил"]);
    if about_done && !reports && has(&["какие", "что", "покажи", "список", "есть", "перечисл"]) {
        return Some(DoneRequest::List);
    }
    if clearing && has(&["задач", "дела", "список"]) && has(&["все", "всё", "очист", "отчист", "почист"]) {
        return Some(DoneRequest::Which);
    }
    None
}

fn done_tasks_request(app: &AppHandle, said: &str) -> Option<String> {
    let request = done_request(said)?;
    if let Err(err) = crate::overlay::show_tasks(app) {
        log::warn!("окно задач не открылось: {err}");
    }
    Some(match request {
        DoneRequest::Clear => {
            let removed = tasks::clear_done();
            log::info!("убраны сделанные дела: {}", removed.len());
            if removed.is_empty() {
                "Сделанных дел нет — убирать нечего.".into()
            } else {
                format!("Убрал сделанные: {}.", titles_of(&removed))
            }
        }
        DoneRequest::List => {
            let done: Vec<Task> = tasks::all()
                .into_iter()
                .filter(|task| task.done_at.is_some())
                .collect();
            if done.is_empty() {
                "В сделанных пусто.".into()
            } else {
                format!("Сделано: {}.", titles_of(&done))
            }
        }
        DoneRequest::Which => "Убрать сделанные или вообще все? Сделанные — скажи «убери сделанные». \
                               Все подряд голосом не удаляю, чтобы не снести нужное: это можно в окне задач."
            .into(),
    })
}

/// Названия дел для ответа вслух: первые пять и сколько ещё.
fn titles_of(list: &[Task]) -> String {
    let mut text = list
        .iter()
        .take(5)
        .map(|task| task.title.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if list.len() > 5 {
        text.push_str(&format!(" и ещё {}", list.len() - 5));
    }
    text
}

/// «Сними задачу», «убей процесс» — завершить программу целиком.
fn forced(said: &str) -> bool {
    let lower = said.to_lowercase();
    [
        "сними задач", "снять задач", "сними процесс", "заверши задач", "завершить задач",
        "заверши процесс", "завершить процесс", "убей", "убить", "прибей", "принудительн",
        "полностью", "совсем", "диспетчер",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

/// Речь о видеокарте.
fn about_gpu(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["видеокарт", "видеопамят", "видюх", "gpu", "джипию", "графическ"]
        .iter()
        .any(|word| lower.contains(word))
}

/// Просят проверить, правда ли это.
fn checks_truth(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["фейк", "правд", "вброс", "достовер", "провер", "врут", "ложь"]
        .iter()
        .any(|word| lower.contains(word))
}

/// Просьба показывать окно с ответами или не показывать.
fn window_request(said: &str) -> Option<bool> {
    let lower = said.to_lowercase();
    // Только просьбы к самому Ноа: «комната без окна» — не просьба.
    const HIDE: &[&str] = &[
        "отвечай без окна", "говори без окна", "работай без окна", "давай без окна",
        "не показывай окно", "окно не показывай", "скрывай окно", "не открывай окно",
        "выключи окно ответ", "отключи окно ответ", "убери окно", "спрячь окно",
        "выключи диалоговое окно", "убери диалоговое окно", "закрой диалоговое окно",
    ];
    // «Покажи окно», «открой диалоговое окно» — так просят вернуть окно ответов:
    // других окон у Ноа нет, а программу по слову «окно» открывать нечего.
    const SHOW: &[&str] = &[
        "показывай окно", "отвечай с окном", "верни окно", "включи окно",
        "открывай окно", "покажи окно", "диалоговое окно", "окно диалоговое",
        "наше окно", "окно ответов",
    ];
    if HIDE.iter().any(|phrase| lower.contains(phrase)) {
        return Some(false);
    }
    SHOW.iter().any(|phrase| lower.contains(phrase)).then_some(true)
}

/// Разговор передан Claude: после ответа Ноа замолкает и закрывает окно.
static HANDOFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Передан ли разговор Claude — и сбросить отметку.
pub fn take_handoff() -> bool {
    HANDOFF.swap(false, std::sync::atomic::Ordering::SeqCst)
}

fn words_of(said: &str) -> Vec<String> {
    said.to_lowercase()
        .replace('ё', "е")
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Таймер и будильник: «поставь таймер на 10 минут», «засеки полчаса»,
/// «разбуди в семь утра», «отмени таймер», «сколько осталось на таймере».
fn timer_request(app: &AppHandle, said: &str, now: DateTime<Local>) -> Option<String> {
    let owned = words_of(said);
    let words: Vec<&str> = owned.iter().map(String::as_str).collect();
    let starts = |stems: &[&str]| words.iter().any(|word| stems.iter().any(|stem| word.starts_with(stem)));
    let timer = starts(&["таймер", "засек"]);
    let alarm = starts(&["будильник", "разбуд"]);
    if !timer && !alarm {
        return None;
    }
    if starts(&["отмен", "выключ", "убер", "сними", "останов", "стоп", "удали"]) {
        if alarm {
            use chrono::Timelike;
            let time = clock_time(&words, now).map(|at| (at.hour(), at.minute()));
            return Some(match crate::alarms::remove(time) {
                0 => "Таких будильников нет.".into(),
                1 => "Убрал будильник.".into(),
                count => format!("Убрал будильников: {count}."),
            });
        }
        return Some(match crate::timers::cancel_all() {
            0 => "Таймеров нет.".into(),
            1 => "Отменил.".into(),
            count => format!("Отменил все: {count}."),
        });
    }
    if timer && starts(&["остал", "сколько"]) {
        return Some(match crate::timers::pending().first() {
            None => "Таймеров нет.".into(),
            Some(next) => format!("Осталось: {}.", spoken_span((next.at - now).num_seconds().max(1))),
        });
    }
    if alarm && starts(&["каки", "покаж", "спис", "назов", "сколько"]) {
        let list = crate::alarms::list();
        return Some(if list.is_empty() {
            "Будильников нет.".into()
        } else {
            let all = list.iter().map(|alarm| alarm.describe()).collect::<Vec<_>>();
            format!("Будильники: {}.", all.join("; "))
        });
    }
    // «Таймеры в JavaScript» — вопрос, а не просьба: нужна просьба завести.
    if !starts(&["постав", "завед", "засек", "запуст", "включ", "сделай", "разбуд", "нужен", "давай"]) {
        return None;
    }
    if alarm {
        use chrono::Timelike;
        // «Разбуди через двадцать минут» — тот же будильник, только время
        // названо не на часах.
        let after = words
            .iter()
            .position(|word| *word == "через")
            .and_then(|at| span_seconds(&words[at + 1..]))
            // Будильник помнит время до минуты, поэтому округляем вверх:
            // «через минуту» в 20:46:54 — это 20:48, а не 20:47, иначе он
            // зазвонил бы через шесть секунд.
            .map(|seconds| {
                let at = now + Duration::seconds(seconds);
                match at.second() {
                    0 => at,
                    second => at + Duration::seconds(60 - i64::from(second)),
                }
            });
        let Some(at) = clock_time(&words, now).or(after) else {
            return Some("Во сколько разбудить? Скажите, например: «разбуди в семь утра».".into());
        };
        let added = crate::alarms::add(at.hour(), at.minute(), crate::alarms::days_of(said), "", now);
        if !added.days.is_empty() {
            return Some(format!("Будильник: {}.", added.describe()));
        }
        let day = if at.date_naive() == now.date_naive() { "" } else { "завтра " };
        return Some(format!("Разбужу {day}в {}.", at.format("%H:%M")));
    }
    let seconds = words
        .iter()
        .enumerate()
        .filter(|(_, word)| matches!(**word, "на" | "через") || word.starts_with("засек"))
        .find_map(|(at, _)| span_seconds(&words[at + 1..]));
    let Some(seconds) = seconds else {
        return Some("На сколько поставить таймер?".into());
    };
    let span = spoken_span(seconds);
    let at = now + Duration::seconds(seconds);
    crate::timers::start(app, at, format!("Время вышло — таймер на {span}."));
    Some(format!("Засёк {span}. Позвоню в {}.", at.format("%H:%M")))
}

/// Звонит будильник: «стоп», «встаю» — выключить, «отложи на десять минут»,
/// «ещё пять минут» — отложить. Пока звенит, любая фраза — к нему.
fn ringing_reply(said: &str) -> Option<String> {
    if !crate::alarms::ringing() {
        return None;
    }
    let owned = words_of(said);
    let words: Vec<&str> = owned.iter().map(String::as_str).collect();
    let later = words
        .iter()
        .any(|word| word.starts_with("отлож") || matches!(*word, "еще" | "через" | "попозже" | "позже"));
    if later {
        let minutes = words
            .iter()
            .position(|word| matches!(*word, "на" | "через" | "еще"))
            .and_then(|at| span_seconds(&words[at + 1..]))
            .map(|seconds| (seconds / 60).max(1))
            .unwrap_or(crate::alarms::SNOOZE_MINUTES);
        return crate::alarms::snooze(minutes).map(|at| format!("Отложил до {at}."));
    }
    crate::alarms::stop();
    Some("Выключил будильник.".into())
}

/// Длительность в секундах в начале слов: «10 минут», «полчаса», «час»,
/// «30 секунд», «полтора часа».
fn span_seconds(rest: &[&str]) -> Option<i64> {
    let first = *rest.first()?;
    if first.starts_with("полчас") {
        return Some(30 * 60);
    }
    let unit = |word: &str| {
        if word.starts_with("сек") {
            Some(1)
        } else {
            unit_of(word).map(|minutes| minutes * 60)
        }
    };
    let (count, seconds) = match unit(first) {
        Some(seconds) => (1.0, seconds),
        None => {
            let (count, used) = count_of(rest)?;
            (count, unit(rest.get(used)?)?)
        }
    };
    let total = (count * seconds as f64).round() as i64;
    (total > 0).then_some(total)
}

/// Время на часах из сказанного: «в семь утра», «на 7 30», «в 19», «в девять
/// вечера». Прошедшее сегодня — значит, завтра.
fn clock_time(words: &[&str], now: DateTime<Local>) -> Option<DateTime<Local>> {
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| matches!(**word, "в" | "на"))
        .find_map(|(at, _)| {
            let rest = &words[at + 1..];
            let (hour, used) = count_of(rest)?;
            let mut hour = hour as u32;
            let mut next = used;
            let mut minute = 0;
            if let Some((value, taken)) = count_of(&rest[next..]) {
                if (0.0..60.0).contains(&value) {
                    minute = value as u32;
                    next += taken;
                }
            }
            match rest.get(next).copied() {
                Some(word) if (word.starts_with("вечер") || word == "дня") && hour < 12 => hour += 12,
                Some(word) if word.starts_with("ноч") && hour == 12 => hour = 0,
                _ => {}
            }
            if hour > 23 {
                return None;
            }
            let day = now.date_naive().and_hms_opt(hour, minute, 0)?;
            let mut when = Local.from_local_datetime(&day).single()?;
            if when <= now {
                when += Duration::days(1);
            }
            Some(when)
        })
}

/// «10 минут», «1 час 30 минут», «45 секунд».
fn spoken_span(seconds: i64) -> String {
    let (hours, minutes, rest) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} {}", crate::prices::plural(hours as f64, "час", "часа", "часов")));
    }
    if minutes > 0 {
        parts.push(format!(
            "{minutes} {}",
            crate::prices::plural(minutes as f64, "минуту", "минуты", "минут")
        ));
    }
    if rest > 0 && hours == 0 {
        parts.push(format!(
            "{rest} {}",
            crate::prices::plural(rest as f64, "секунду", "секунды", "секунд")
        ));
    }
    if parts.is_empty() {
        "0 секунд".into()
    } else {
        parts.join(" ")
    }
}

/// Идёт разговор без рук: фраза могла быть музыкой или чужой речью.
static AMBIENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Ход разговора ушёл впустую: ответ не произнесён.
static JUNK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Отмечает, что сказанное услышано в разговоре без рук, а не по клавише.
pub fn set_ambient(on: bool) {
    use std::sync::atomic::Ordering;
    AMBIENT.store(on, Ordering::SeqCst);
    if on {
        JUNK.store(false, Ordering::SeqCst);
    }
}

fn ambient() -> bool {
    AMBIENT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Ушёл ли последний ход впустую — один раз.
pub fn take_junk() -> bool {
    JUNK.swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// Последний снимок экрана. Попросили из Telegram — он уходит туда
/// фотографией; см. `take_photo`.
static PHOTO: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Снимок, сделанный по последней просьбе, — один раз.
pub fn take_photo() -> Option<Vec<u8>> {
    PHOTO.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Что снимать: `Some(None)` — весь экран, `Some(Some(программа))` — окно
/// программы, `None` — это не просьба о снимке.
fn shot_target(said: &str) -> Option<Option<String>> {
    const VERBS: &[&str] = &["сдела", "пришл", "скин", "сним", "покаж", "отправ", "дай", "кин"];
    const FILLER: &[&str] = &[
        "ноа", "мне", "пожалуйста", "экрана", "экран", "всего", "весь", "целиком", "окна", "окно",
        "программы", "приложения", "мой", "моего", "сейчас", "что", "там", "на", "с", "со", "ка",
        "из", "его", "в", "у", "меня", "рабочего", "стола", "стол", "мониторов", "монитора",
    ];
    let words = words_of(said);
    let shot = words
        .iter()
        .position(|word| word.starts_with("скрин") || word == "снимок")?;
    let verb = |word: &str| VERBS.iter().any(|stem| word.starts_with(stem));
    if !words.iter().any(|word| verb(word)) {
        return None;
    }
    let program: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|(at, word)| *at != shot && !verb(word) && !FILLER.contains(&word.as_str()))
        .map(|(_, word)| word.as_str())
        .collect();
    Some((!program.is_empty()).then(|| program.join(" ")))
}

/// Снимок экрана или окна: в «Изображения\Суфлёр», а для Telegram — в
/// `PHOTO`.
fn screenshot_request(app: &AppHandle, said: &str) -> Option<String> {
    let target = shot_target(said)?;
    let (taken, what) = match target {
        None => (crate::shots::screen(), "экрана".to_string()),
        Some(program) => match crate::pc::window_for(&program) {
            Some((handle, name)) => (crate::shots::window(handle), format!("окна {name}")),
            None => return Some(format!("Не нашёл открытого окна «{program}».")),
        },
    };
    let png = match taken {
        Ok(png) => png,
        Err(err) => return Some(format!("Снимок не получился: {err}")),
    };
    let saved = crate::shots::save(app, &png);
    *PHOTO.lock().unwrap_or_else(|err| err.into_inner()) = Some(png);
    Some(match saved {
        Ok((path, folder)) => {
            log::info!("снимок {what}: {}", path.display());
            format!("Снимок {what} сохранил в {folder}.")
        }
        Err(err) => format!("Снимок {what} сделал, но не сохранил: {err}"),
    })
}

/// «Включи музыку», «поставь музычку фоном» — без названия, исполнителя и
/// жанра. С уточнением это уже поиск, и его разбирает модель.
fn asks_for_music(said: &str) -> bool {
    const VERBS: &[&str] = &["включ", "постав", "вруб", "запуст", "давай", "хочу"];
    /// Чем называют свою станцию.
    const NOUNS: &[&str] = &["музык", "музычк", "музон", "радио", "станци", "трансляц"];
    const FILLER: &[&str] = &[
        "ноа", "мне", "нам", "пожалуйста", "какую", "нибудь", "немного", "фоном", "фоновую",
        "что", "то", "ка", "эй", "можешь", "можно", "плиз", "мою", "свою", "моё", "мое", "эту",
        "клод", "клода", "клауде", "клауд", "клоуд", "claude", "лофи", "lofi", "радио",
        "станцию", "трансляцию",
    ];
    let words = words_of(said);
    let Some(music) = words
        .iter()
        .position(|word| NOUNS.iter().any(|noun| word.starts_with(noun)))
    else {
        return false;
    };
    let verb = |word: &str| VERBS.iter().any(|stem| word.starts_with(stem));
    words.iter().any(|word| verb(word))
        && words
            .iter()
            .enumerate()
            .all(|(at, word)| at == music || verb(word) || FILLER.contains(&word.as_str()))
}

fn music_request(app: &AppHandle, said: &str) -> Option<String> {
    if !asks_for_music(said) {
        return None;
    }
    let url = app.state::<AppState>().config().voice.music_url.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let radio = said.to_lowercase().contains("радио");
    Some(match crate::pc::open(&url) {
        Ok(()) if radio => "Включаю радио.".into(),
        Ok(()) => "Включаю музыку.".into(),
        Err(err) => format!("Не смог открыть музыку: {err}"),
    })
}

/// Вызывает инструмент своего модуля и пересказывает ответ голосом.
///
/// Инструменты отвечают для программ — JSON, списки, длинный текст. Вслух
/// это не читается, поэтому ответ пересказывает модель, зная, что спросили.
async fn use_tool(app: &AppHandle, said: &str, tool: String, args: serde_json::Value) -> String {
    let name = tool.clone();
    let called = tauri::async_runtime::spawn_blocking(move || crate::plugins::call(&name, &args)).await;
    let text = match called {
        Ok(Ok(text)) => text,
        Ok(Err(err)) => {
            log::warn!("инструмент {tool}: {err}");
            return format!("Модуль не справился: {err}");
        }
        Err(_) => return "Модуль не ответил.".into(),
    };
    log::info!("инструмент {tool} ответил, {} знаков", text.chars().count());
    if text.trim().is_empty() {
        return "Готово.".into();
    }
    let context: String = text.chars().take(4000).collect();
    let provider = app.state::<AppState>().provider();
    match provider
        .ask(&format!("ответ инструмента {tool}"), &context, &[], said)
        .await
    {
        Ok(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
        _ => context.chars().take(300).collect(),
    }
}

/// Список активов голосом: показать, добавить, убрать. Окно открывается при
/// любом из трёх — сразу видно, что получилось.
async fn watch(app: &AppHandle, action: &str, asset: &str, tab: &str, price: &str) -> String {
    use tauri::Emitter;

    let tab = tab.trim();
    let named = (!tab.is_empty()).then_some(tab);
    let reply = match action.trim() {
        "alert" => {
            let target = price
                .trim()
                .replace([' ', '\u{a0}'], "")
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|target| *target > 0.0);
            match target {
                None => "Не расслышал цену для оповещения.".to_string(),
                Some(target) => {
                    let found = match crate::watchlist::find(asset) {
                        Some(found) => Ok(found),
                        None => crate::watchlist::add(asset, None).await,
                    };
                    let placed = match found {
                        Ok(found) => crate::watchlist::add_alert(&found.id, target)
                            .await
                            .map(|(alert, money)| (found, alert, money)),
                        Err(err) => Err(err),
                    };
                    match placed {
                        Ok((found, alert, money)) => {
                            let way = if alert.above { "поднимется" } else { "опустится" };
                            let whereto = if crate::telegram::ready(app) {
                                "Напишу в Telegram."
                            } else {
                                "Telegram не подключён — скажу здесь."
                            };
                            format!("Поставил оповещение: {} {way} до {money}. {whereto}", found.name)
                        }
                        Err(err) => err,
                    }
                }
            }
        }
        "add" => match crate::watchlist::add(asset, named).await {
            Ok(added) => match named.and_then(crate::watchlist::find_tab) {
                Some(tab) => format!("Добавил {} во вкладку «{tab}».", added.name),
                None => format!("Добавил {} в активы.", added.name),
            },
            Err(err) => err,
        },
        "remove" => match crate::watchlist::remove(asset, named) {
            Some(removed) => match named {
                Some(tab) => format!("Убрал {} из вкладки «{tab}».", removed.name),
                None => format!("Убрал {} из активов.", removed.name),
            },
            None => format!("«{}» там нет.", asset.trim()),
        },
        _ => {
            let rows = crate::watchlist::rows().await;
            match named {
                Some(tab) => match crate::watchlist::find_tab(tab) {
                    Some(tab) => {
                        let inside: Vec<_> =
                            rows.into_iter().filter(|row| row.tabs.contains(&tab)).collect();
                        format!("«{tab}». {}", crate::watchlist::summary(&inside))
                    }
                    None => format!("Вкладки «{tab}» нет. Её можно завести в окне — плюсом."),
                },
                None => crate::watchlist::summary(&rows),
            }
        }
    };
    // Какую вкладку открыть: названную — если она есть.
    let open = named.and_then(crate::watchlist::find_tab);
    crate::watchlist::open_tab(open.clone());
    if let Err(err) = crate::overlay::show_watchlist(app) {
        log::warn!("окно активов не открылось: {err}");
    }
    let _ = app.emit_to(crate::overlay::WATCH_LABEL, "watchlist:changed", ());
    if let Some(open) = open {
        let _ = app.emit_to(crate::overlay::WATCH_LABEL, "watchlist:tab", open);
    }
    reply
}

/// Ответ по часам компьютера: «который час», «какое сегодня число».
///
/// Модель время знает только из подсказки и в арифметике с ним путается, а
/// часы компьютера точные. «Сколько времени займёт дорога» — не вопрос о
/// часах: такие обороты берутся, только если фраза короткая.
fn clock_answer(said: &str, now: DateTime<Local>) -> Option<String> {
    let lower = said.to_lowercase().replace('ё', "е");
    let words: Vec<&str> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let phrase = words.join(" ");
    let short = words.len() <= 5;
    let any = |phrases: &[&str]| phrases.iter().any(|wanted| phrase.contains(wanted));
    let time = any(&["который час", "сколько на часах"])
        || (short && any(&["сколько времени", "сколько время", "какое время"]));
    let date = any(&["какое сегодня число", "какая сегодня дата", "какой сегодня день", "какой день недели"])
        || (short && any(&["какое число", "какая дата"]));
    match (time, date) {
        (true, true) => Some(format!(
            "Сейчас {}, {}, {}.",
            now.format("%H:%M"),
            weekday(now),
            month_day(now)
        )),
        (true, false) => Some(format!("Сейчас {}.", now.format("%H:%M"))),
        (false, true) => Some(format!("Сегодня {}, {}.", weekday(now), month_day(now))),
        (false, false) => None,
    }
}

/// Срок «через …» от текущего времени: «через час», «через 20 минут»,
/// «через полчаса», «через два дня».
///
/// Считает программа, а не модель: сколько будет «через сорок минут», модель
/// порой считала неверно — и напоминание уезжало на другой час.
fn relative_due(said: &str, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let lower = said.to_lowercase().replace('ё', "е");
    let words: Vec<&str> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let at = words.iter().position(|word| *word == "через")?;
    let rest = &words[at + 1..];
    let first = *rest.first()?;
    if first.starts_with("полчас") {
        return Some(now + Duration::minutes(30));
    }
    let (count, unit) = match unit_of(first) {
        Some(unit) => (1.0, unit),
        None => {
            let (count, used) = count_of(rest)?;
            (count, unit_of(rest.get(used)?)?)
        }
    };
    let minutes = (count * unit as f64).round() as i64;
    (minutes > 0).then(|| now + Duration::minutes(minutes))
}

/// Единица времени в минутах.
fn unit_of(word: &str) -> Option<i64> {
    if word.starts_with("мин") {
        Some(1)
    } else if word.starts_with("час") {
        Some(60)
    } else if word.starts_with("сут") || matches!(word, "день" | "дня" | "дней") {
        Some(24 * 60)
    } else if word.starts_with("недел") {
        Some(7 * 24 * 60)
    } else {
        None
    }
}

/// Число в начале слов: «20», «двадцать пять», «пару», «полтора». Отдаёт число
/// и сколько слов оно заняло.
fn count_of(words: &[&str]) -> Option<(f64, usize)> {
    const UNITS: &[(&str, f64)] = &[
        ("один", 1.0), ("одну", 1.0), ("одна", 1.0), ("два", 2.0), ("две", 2.0), ("три", 3.0),
        ("четыре", 4.0), ("пять", 5.0), ("шесть", 6.0), ("семь", 7.0), ("восемь", 8.0),
        ("девять", 9.0),
    ];
    const TENS: &[(&str, f64)] = &[
        ("десять", 10.0), ("пятнадцать", 15.0), ("двадцать", 20.0), ("тридцать", 30.0),
        ("сорок", 40.0), ("пятьдесят", 50.0),
    ];
    let first = *words.first()?;
    if let Ok(number) = first.parse::<f64>() {
        return Some((number, 1));
    }
    if first.starts_with("пар") {
        return Some((2.0, 1));
    }
    if first.starts_with("полтор") {
        return Some((1.5, 1));
    }
    if let Some((_, tens)) = TENS.iter().find(|(word, _)| *word == first) {
        let units = words
            .get(1)
            .and_then(|next| UNITS.iter().find(|(word, _)| word == next));
        return Some(match units {
            Some((_, units)) => (tens + units, 2),
            None => (*tens, 1),
        });
    }
    UNITS
        .iter()
        .find(|(word, _)| *word == first)
        .map(|(_, value)| (*value, 1))
}

/// Похоже ли на покупку: «купи», «закажи», «привези».
fn bought(said: &str) -> bool {
    let lower = said.to_lowercase();
    ["закаж", "купи", "купить", "заказ", "привез", "достав", "корзин"]
        .iter()
        .any(|stem| lower.contains(stem))
}

/// Не товар, а описание: «ингредиенты для завтрака», «продукты на ужин».
fn vague(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("ингредиент")
        || lower.starts_with("продукт")
        || lower.contains("для завтрак")
        || lower.contains("на завтрак")
        || lower.contains("для ужин")
        || lower.contains("для обед")
        || lower.trim() == "еда"
}

/// Звучал ли товар в сказанном — хотя бы одним словом.
fn mentions(said: &str, name: &str) -> bool {
    let said = said.to_lowercase().replace('ё', "е");
    name.to_lowercase()
        .replace('ё', "е")
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|word| word.chars().count() >= 4)
        .any(|word| said.contains(&word.chars().take(4).collect::<String>()))
}

/// Слово о самом заказе, а не товар: «корзина», «заказ», «покупки».
fn about_the_order(name: &str) -> bool {
    const WORDS: &[&str] = &[
        "корзина", "корзину", "корзинка", "корзинку", "заказ", "покупки", "товары", "продукты",
        "все", "всё", "то же самое",
    ];
    WORDS.contains(&name.trim().to_lowercase().as_str())
}

/// Похоже ли на продолжение заказа, а не на новый: «собери там», «оформи».
fn follow_up(said: &str) -> bool {
    let lower = said.to_lowercase();
    [
        "собери", "корзин", "там", "туда", "оформи", "эту", "эти", "то же", "тоже", "давай",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Товары текущего заказа, если он свежий — не старше получаса.
fn fresh_order_items() -> Option<Vec<crate::food::Wanted>> {
    let order = crate::order::current()?;
    let fresh = DateTime::parse_from_rfc3339(&order.updated_at)
        .map(|at| Local::now().signed_duration_since(at) < Duration::minutes(30))
        .unwrap_or(false);
    (fresh && !order.asked.is_empty()).then_some(order.asked)
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
        "system" => &[
            "груз", "тормоз", "процесс", "памят", "оператив", "диск", "места", "процессор",
            "нагрузк", "видеокарт", "температур", "компьютер", "систем", "видеопамят",
            "видюх", "gpu",
        ],
        "diagnose" => &[
            "ошибк", "не работает", "сломал", "проблем", "случилось", "глючит", "висит", "вылет",
            "зависа", "почему", "что это",
        ],
        "find" => &["найди", "найти", "поищи", "где лежит", "где файл", "где мой", "где мо"],
        "type" => &["напиши", "напечатай", "впиши", "введи", "набери", "напечат"],
        "message" => &["напиши", "отправь", "сообщени", "скажи", "передай"],
        "send" => &["отправ", "жми", "энтер", "enter"],
        "paste" => &["встав"],
        "price" => &["стоит", "цена", "цену", "курс", "стоимост", "почём", "почем", "котиров"],
        "claude" => &["клод", "claude", "клауд", "клоуд"],
        "learn" => &[
            "обуч", "учеб", "курс", "экзамен", "погоня", "прогресс", "урок", "вопрос по",
            "девопс", "devops", "подготов", "собеседован", "изуч",
        ],
        "watch" => &[
            "вотч", "watch", "актив", "портфел", "монет", "акци", "отслежива", "списк", "алерт",
            "оповещ", "уведом",
        ],
        "remove" => &[
            "удал", "убер", "убра", "сотр", "стер", "не просил", "не надо было", "лишн", "очист",
            "отчист", "почист", "архив", "снес", "выкин",
        ],
        "screen" => &[
            "скрин", "экран", "фейк", "правд", "написан", "переведи", "картинк", "снимк",
            "снимок", "провер", "вброс", "достовер", "врут", "что тут", "что здесь",
        ],
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

fn intent_rules(app: &AppHandle, open: &[Task]) -> String {
    rules_with_context(
        open,
        &app.state::<AppState>().wake_name(),
        &[
            now_line(),
            crate::web::site_line(),
            order_line(),
            crate::screen::clipboard_line(),
        ]
        .join(" "),
    )
}

/// Правила разбора с заданной строкой обстановки: время, открытый сайт,
/// текущий заказ. Отдельно — ради сравнения моделей на одной обстановке.
fn rules_with_context(open: &[Task], name: &str, context: &str) -> String {
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
        "Ты — {name}, голосовой помощник. Определи, чего хочет человек, и ответь \
         одним объектом JSON без пояснений.\n\
         \n\
         Поле intent — одно из:\n\
         chat — обычный вопрос, разговор, просьба что-то объяснить;\n\
         add — просит завести дело, напомнить о чём-то, записать, запланировать \
         встречу или добавить в календарь;\n\
         list — спрашивает, что у него запланировано, какие дела, что на сегодня;\n\
         done — сообщает, что уже что-то сделал;\n\
         remove — просит удалить, стереть, убрать дело из списка — не отметить \
         сделанным, а именно удалить: «удали эту задачу», «я не просил это записывать»;\n\
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
         вкладку, обновить страницу или свернуть все окна;\n\
         system — спрашивает о состоянии компьютера: что грузит процессор, память \
         или диск, сколько места, почему тормозит;\n\
         diagnose — спрашивает про ошибку или проблему на компьютере: «что это за \
         ошибка», «почему не работает», «что случилось»;\n\
         find — просит найти файл на компьютере;\n\
         type — просит напечатать или вписать текст в окно;\n\
         message — просит написать кому-то сообщение в мессенджере;\n\
         send — просит отправить напечатанное;\n\
         paste — просит вставить заготовленный текст: «вставь»;\n\
         screen — вопрос про скриншот или скопированный текст в буфере обмена или \
         про то, что сейчас на экране: «это фейк?», «это правда?», «что тут \
         написано», «переведи»;\n\
         price — спрашивает цену или курс криптовалюты, токена, монеты или валюты: \
         «сколько стоит биткоин», «курс доллара», «почём pump.fun»;\n\
         claude — просит спросить Claude (Клода) или перейти к нему: «спроси Клода, \
         как …», «позови Клода», «хочу поговорить с Клодом»;\n\
         watch — про список отслеживаемых активов (вотчлист, «мои активы»): \
         показать его, добавить или убрать монету, акцию, валюту.\n\
         learn — обучение по курсу (DevOps и другие): открыть курс или тему, \
         узнать прогресс, «погоняй меня», «задай вопрос по докеру», подготовка \
         к собеседованию.\n\
         \n\
         Остальные поля:\n\
         title — название дела для add: коротко, без слов «напомни» и «запиши»;\n\
         due — срок в виде ГГГГ-ММ-ДДTЧЧ:ММ или пустая строка, если не назван;\n\
         task — номер дела из списка ниже для done, remove, postpone и breakdown, \
         иначе 0; «эту», «последнюю», «только что записанную» — 0;\n\
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
         поисковика, для find что за файл («паспорт»), иначе пустая строка;\n\
         topic — для system одно из: cpu, memory, gpu, disk, overview; gpu — \
         видеокарта: видеопамять, её загрузка и температура;\n\
         check — для screen true, если просят проверить, правда ли это;\n\
         asset — для price и watch что именно: монету — латиницей, как на бирже \
         (bitcoin, pump.fun), акцию — тикером (AAPL, SBER), валюту — по-русски \
         (доллар, евро); для watch show — пусто;\n\
         action — для watch одно из: show, add, remove, alert (оповещение о цене, алерт);\n\
         price — для watch alert цена-цель числом, без валюты;\n\
         action — для learn одно из: open, progress, quiz;\n\
         topic — для learn тема или курс словами человека (докер, сети, девопс), \
         иначе пусто;\n\
         tab — для watch вкладка списка, если названа («в избранное», «в крипту», \
         «вкладку фонды»), в именительном падеже; не названа — пусто;\n\
         kind — для find тип файла: image, document, video, audio или any;\n\
         text — для type, message и claude сам текст, слово в слово, без «напиши» и \
         «спроси»; для claude без вопроса — пусто;\n\
         to — для message кому писать, как назвали; app для message — мессенджер, \
         по умолчанию telegram.\n\
         \n\
         Про программы и процессы — «сними задачу», «заверши процесс», «убей \
         программу», «закрой окно» — это close, а не дела: done, postpone и \
         breakdown только про дела из списка ниже.\n\
         Команды — только когда человек прямо просит что-то сделать. Рассказ о \
         том, что происходит, вопрос или обрывок фразы — это chat.\n\
         Если человек уточняет текущий заказ — сколько штук, какой именно \
         товар, в каком магазине, — верни order со всем заказом целиком, уже с \
         уточнением; товары, которых он не касался, оставь как были.\n\
         Покупки и продукты — это order, а не add: add только когда просят \
         записать дело или напомнить о нём. «Продукты на яичницу» — это order с \
         конкретными продуктами (яйца, масло), а не «ингредиенты».\n\
         \n\
         Если назван день без времени — ставь 18:00. Полночь никому не нужна: \
         напоминание в это время человек не услышит.\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}\n\
         \n\
         {}",
        // Инструменты своих модулей меняются только при установке модуля.
        crate::plugins::rules_section(),
        // Меняющееся — в конце. Облачные сервисы кешируют совпадающее начало
        // запроса; время в середине сбрасывало кеш на примерах каждую минуту.
        EXAMPLES,
        list,
        context
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
     «программа сейчас запущена» → {\"intent\":\"chat\"}\n\
     «что сейчас грузит компьютер» → {\"intent\":\"system\",\"topic\":\"overview\"}\n\
     «сколько места на диске» → {\"intent\":\"system\",\"topic\":\"disk\"}\n\
     «вылезла какая-то ошибка, что это» → {\"intent\":\"diagnose\"}\n\
     «найди на компьютере фото паспорта» → \
     {\"intent\":\"find\",\"query\":\"паспорт\",\"kind\":\"image\"}\n\
     «напечатай: буду через десять минут» → \
     {\"intent\":\"type\",\"text\":\"буду через десять минут\"}\n\
     «напиши в телеграм маше, что я опоздаю» → \
     {\"intent\":\"message\",\"app\":\"telegram\",\"to\":\"маша\",\"text\":\"я опоздаю\"}\n\
     «вставь» → {\"intent\":\"paste\"}\n\
     «отправляй» → {\"intent\":\"send\"}\n\
     «закажи продукты на яичницу» → \
     {\"intent\":\"order\",\"items\":[{\"name\":\"яйца\",\"quantity\":1},{\"name\":\"сливочное масло\",\"quantity\":1}],\"store\":\"\"}\n\
     «купи яйца, бекон и хлеб» → \
     {\"intent\":\"order\",\"items\":[{\"name\":\"яйца\",\"quantity\":1},{\"name\":\"бекон\",\"quantity\":1},{\"name\":\"хлеб\",\"quantity\":1}],\"store\":\"\"}\n\
     «сколько занято видеопамяти» → {\"intent\":\"system\",\"topic\":\"gpu\"}\n\
     «удали эту задачу» → {\"intent\":\"remove\",\"task\":0}\n\
     «я ничего не просил записывать, удали» → {\"intent\":\"remove\",\"task\":0}\n\
     «это фейк?» → {\"intent\":\"screen\",\"check\":true}\n\
     «что тут написано» → {\"intent\":\"screen\",\"check\":false}\n\
     «это правда?» → {\"intent\":\"screen\",\"check\":true}\n\
     «сколько стоит пампфан токен» → {\"intent\":\"price\",\"asset\":\"pump.fun\"}\n\
     «курс доллара» → {\"intent\":\"price\",\"asset\":\"доллар\"}\n\
     «спроси клода, как написать резюме» → {\"intent\":\"claude\",\"text\":\"как написать резюме\"}\n\
     «позови клода» → {\"intent\":\"claude\",\"text\":\"\"}\n\
     «покажи мои активы» → {\"intent\":\"watch\",\"action\":\"show\",\"asset\":\"\"}\n\
     «добавь биткоин в вотчлист» → {\"intent\":\"watch\",\"action\":\"add\",\"asset\":\"bitcoin\"}\n\
     «убери эфир из активов» → {\"intent\":\"watch\",\"action\":\"remove\",\"asset\":\"ethereum\"}\n\
     «добавь биткоин в избранное» → {\"intent\":\"watch\",\"action\":\"add\",\"asset\":\"bitcoin\",\"tab\":\"избранное\"}\n\
     «покажи вкладку фонды» → {\"intent\":\"watch\",\"action\":\"show\",\"asset\":\"\",\"tab\":\"фонды\"}\n\
     «поставь алерт на биткоин на 80 тысяч» → {\"intent\":\"watch\",\"action\":\"alert\",\"asset\":\"bitcoin\",\"price\":80000}\n\
     «погоняй меня по докеру» → {\"intent\":\"learn\",\"action\":\"quiz\",\"topic\":\"докер\"}\n\
     «как мой прогресс по девопсу» → {\"intent\":\"learn\",\"action\":\"progress\",\"topic\":\"девопс\"}\n\
     «открой обучение» → {\"intent\":\"learn\",\"action\":\"open\",\"topic\":\"\"}";

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
    remember_added(&task.id);
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
        remember_added(&task.id);
        changed(app);
        return format!("Оставил без срока: {}.", task.title);
    }

    let due = match interpret(app, &time_rules(), said).await {
        Some(parsed) => parse_due(parsed["due"].as_str().unwrap_or_default()),
        None => None,
    };

    let task = tasks::add(title, due, None);
    remember_added(&task.id);
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
        // Пустой сегодняшний день — не пустой список: о завтрашнем деле
        // промолчать значило бы сказать, что его нет.
        let mut ahead: Vec<Task> = tasks::all()
            .into_iter()
            .filter(|task| task.done_at.is_none())
            .collect();
        if ahead.is_empty() {
            return "Дел нет.".into();
        }
        ahead.sort_by_key(|task| task.due);
        let next: Vec<String> = ahead
            .iter()
            .take(3)
            .map(|task| match task.due {
                Some(_) => format!("{} — {}", task.title, spoken_due(task.due)),
                None => task.title.clone(),
            })
            .collect();
        return format!("На сегодня ничего. Дальше: {}.", next.join("; "));
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
                .filter(|step| !step.is_empty() && readable(step))
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return "Не смог разбить это на шаги.".into();
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty() && readable(advice))
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
                .filter(|step| !step.is_empty() && readable(step))
                .collect()
        })
        .unwrap_or_default();

    if steps.is_empty() {
        return Err("не вышло разбить это на шаги".into());
    }

    let advice = parsed["advice"]
        .as_str()
        .map(str::trim)
        .filter(|advice| !advice.is_empty() && readable(advice))
        .map(str::to_string);

    Ok(tasks::set_plan(&task.id, steps, advice))
}

const PLAN_RULES: &str = "Разбей дело на 3–5 понятных шагов и дай один короткий совет, \
     с чего начать. Ответь одним объектом JSON без пояснений:\n\
     {\"steps\": [\"…\", \"…\"], \"advice\": \"…\"}\n\
     Шаги — в неопределённой форме, каждый на одно действие, не длиннее семи слов.\n\
     Пиши только по-русски: ни иероглифов, ни английских слов.\n\
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

/// Своя модель для разбора команд, когда ответы идут из облака: время проверки
/// и найденный провайдер (`None` — Ollama не запущена или моделей нет).
type LocalIntent = Option<(std::time::Instant, Option<Arc<dyn crate::ai_client::AiProvider>>)>;
static LOCAL_INTENT: Mutex<LocalIntent> = Mutex::new(None);

/// Модель, которой разбирать реплику.
///
/// Разбор — короткий JSON с намерением, и своя модель справляется с ним не хуже
/// облачной. Когда ответы идут из облака, каждая фраза стоила бы там двух
/// запросов, а у бесплатных тарифов лимит — десятки запросов в сутки. Поэтому
/// при облачном источнике разбор уходит в Ollama, если в ней есть модель.
async fn intent_provider(app: &AppHandle) -> Arc<dyn crate::ai_client::AiProvider> {
    let state = app.state::<AppState>();
    // Только для бесплатных моделей: платная лимитов не знает, и занимать ради
    // экономии запросов видеокарту — а с ней и распознаванию речи — незачем.
    let (free_cloud, language, wake_name) = {
        let config = state.config();
        (
            config.ai.provider == "http"
                && !crate::config::is_local(&config.ai.endpoint)
                && config.ai.model.ends_with(":free"),
            config.ui.language.clone(),
            config.voice.wake_name().to_string(),
        )
    };
    if !free_cloud {
        return state.provider();
    }

    let cached = LOCAL_INTENT.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let local = match cached {
        Some((checked, local)) if checked.elapsed() < std::time::Duration::from_secs(60) => local,
        _ => {
            let local = local_intent_model().await.and_then(|model| {
                let config = crate::config::AiConfig {
                    endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
                    model: model.clone(),
                    ..Default::default()
                };
                let provider = crate::ai_client::HttpProvider::new(&config, &language, &wake_name).ok()?;
                log::info!("команды разбирает своя модель «{model}», ответы — облако");
                Some(Arc::new(provider) as Arc<dyn crate::ai_client::AiProvider>)
            });
            *LOCAL_INTENT.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((std::time::Instant::now(), local.clone()));
            local
        }
    };
    local.unwrap_or_else(|| state.provider())
}

/// Установленная модель Ollama для разбора: та, что программа выбирает для этой
/// машины по видеопамяти (`ollama::pick`); нет её — qwen (правила разбора
/// выверены на ней), затем самая крупная из тех, что помещаются в 6 ГБ.
async fn local_intent_model() -> Option<String> {
    let status = crate::ollama::status(crate::ollama::DEFAULT_HOST).await;
    let preferred = tokio::task::spawn_blocking(|| crate::ollama::pick(&crate::ollama::hardware()))
        .await
        .ok();
    if let Some(preferred) = preferred {
        if status.installed.iter().any(|m| m.name == preferred) {
            return Some(preferred.to_string());
        }
    }
    let mut models: Vec<&crate::ollama::Model> = status
        .installed
        .iter()
        .filter(|m| m.size_gb <= 6.0 && !m.name.contains("embed"))
        .collect();
    // Крупные первыми: из подходящих по памяти берём самую толковую.
    models.sort_by(|a, b| b.size_gb.total_cmp(&a.size_gb));
    models
        .iter()
        .find(|m| m.name.starts_with("qwen"))
        .or(models.first())
        .map(|m| m.name.clone())
}

async fn interpret(app: &AppHandle, rules: &str, said: &str) -> Option<serde_json::Value> {
    let provider = intent_provider(app).await;
    let raw = match provider.interpret(rules, said).await {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("разбор реплики не удался: {err}");
            // Своя модель не ответила — облако разберёт само.
            let main = app.state::<AppState>().provider();
            if Arc::ptr_eq(&provider, &main) {
                return None;
            }
            *LOCAL_INTENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
            match main.interpret(rules, said).await {
                Ok(raw) => raw,
                Err(err) => {
                    log::warn!("разбор в облаке тоже не удался: {err}");
                    return None;
                }
            }
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
    fn timers_are_measured_by_the_clock() {
        let span = |said: &str| {
            let owned = words_of(said);
            let words: Vec<&str> = owned.iter().map(String::as_str).collect();
            let at = words.iter().position(|word| matches!(*word, "на" | "через" | "засеки"))?;
            span_seconds(&words[at + 1..])
        };
        assert_eq!(span("поставь таймер на 10 минут"), Some(600));
        assert_eq!(span("засеки полчаса"), Some(1800));
        assert_eq!(span("таймер на 30 секунд"), Some(30));
        assert_eq!(span("таймер на полтора часа"), Some(5400));
        assert_eq!(span("таймер на минуту"), Some(60));
        assert_eq!(spoken_span(5400), "1 час 30 минут");
        assert_eq!(spoken_span(45), "45 секунд");
    }

    #[test]
    fn an_alarm_is_set_by_the_clock() {
        let now = Local.with_ymd_and_hms(2026, 9, 11, 15, 7, 0).unwrap();
        let at = |said: &str| {
            let owned = words_of(said);
            let words: Vec<&str> = owned.iter().map(String::as_str).collect();
            clock_time(&words, now).map(|at| at.format("%d %H:%M").to_string())
        };
        assert_eq!(at("разбуди в семь утра").as_deref(), Some("12 07:00"));
        assert_eq!(at("поставь будильник на 7:30").as_deref(), Some("12 07:30"));
        assert_eq!(at("разбуди в девять вечера").as_deref(), Some("11 21:00"));
        assert_eq!(at("будильник на завтра на 8").as_deref(), Some("12 08:00"));
    }

    #[test]
    fn a_screenshot_is_asked_for() {
        assert_eq!(shot_target("Ноа, сделай скриншот"), Some(None));
        assert_eq!(shot_target("пришли снимок экрана"), Some(None));
        assert_eq!(shot_target("скинь скрин хрома"), Some(Some("хрома".into())));
        assert_eq!(shot_target("что такое скриншот"), None);
    }

    #[test]
    fn plain_music_is_the_own_station() {
        assert!(asks_for_music("Ноа, включи музыку"));
        assert!(asks_for_music("поставь музычку фоном"));
        assert!(!asks_for_music("включи музыку Моргенштерна"));
        assert!(!asks_for_music("что такое музыка барокко"));
    }

    #[test]
    fn the_answer_window_is_asked_for_in_plain_words() {
        assert_eq!(window_request("Покажи окно."), Some(true));
        assert_eq!(window_request("Открой диалоговое окно наше."), Some(true));
        assert_eq!(window_request("убери диалоговое окно"), Some(false));
        assert_eq!(window_request("открой окно в браузере"), None);
        assert_eq!(window_request("комната без окна"), None);
    }

    #[test]
    fn relative_times_are_counted_by_the_clock() {
        let now = Local.with_ymd_and_hms(2026, 9, 11, 15, 7, 0).unwrap();
        let later = |said: &str| relative_due(said, now).map(|due| due.format("%d %H:%M").to_string());
        assert_eq!(later("напомни через час сходить в магазин").as_deref(), Some("11 16:07"));
        assert_eq!(later("через 20 минут позвонить").as_deref(), Some("11 15:27"));
        assert_eq!(later("через двадцать пять минут").as_deref(), Some("11 15:32"));
        assert_eq!(later("через полчаса").as_deref(), Some("11 15:37"));
        assert_eq!(later("через пару часов").as_deref(), Some("11 17:07"));
        assert_eq!(later("через два дня").as_deref(), Some("13 15:07"));
        assert_eq!(later("напомни завтра в десять"), None);
    }

    #[test]
    fn the_clock_answers_by_itself() {
        let now = Local.with_ymd_and_hms(2026, 9, 11, 15, 7, 0).unwrap();
        assert_eq!(clock_answer("Ноа, который час?", now).as_deref(), Some("Сейчас 15:07."));
        let date = clock_answer("какое сегодня число", now).unwrap_or_default();
        assert!(date.starts_with("Сегодня пятница, 11 сентября"), "{date}");
        assert_eq!(clock_answer("сколько времени займёт дорога до работы на машине", now), None);
    }

    #[test]
    fn a_task_is_found_by_its_title() {
        let open = vec![task("1", "найти битки"), task("2", "позвонить в банк")];
        assert_eq!(by_title("удали задачу про банк", &open), Some("2".to_string()));
        assert_eq!(by_title("удали эту задачу", &open), None);
        assert_eq!(window_request("как проветрить комнату без окна"), None);
    }

    #[test]
    fn removing_is_not_marking_done() {
        assert!(erases("Удали эту задачу на 18:00."));
        assert!(!erases("я сделал резюме"));
        assert_eq!(time_in("Удали эту задачу на 18:00."), Some((18, 0)));
        assert_eq!(time_in("удали задачу в 9"), Some((9, 0)));
        assert_eq!(time_in("удали эту"), None);
        assert!(points_at_last("Я ничего не просил записывать, удалить эту запись."));
    }

    #[test]
    fn ending_a_task_is_asked_in_words() {
        assert!(forced("Но завершить задачу Google Chrome."));
        assert!(forced("сними задачу хром"));
        assert!(!forced("закрой хром"));
    }

    #[test]
    fn the_video_card_is_not_memory() {
        assert!(about_gpu("сколько занято памяти видеокарты"));
        assert!(!about_gpu("сколько занято памяти"));
    }

    #[test]
    fn the_window_is_switched_in_words() {
        assert_eq!(window_request("отвечай без окна"), Some(false));
        assert_eq!(window_request("не показывай окно"), Some(false));
        assert_eq!(window_request("показывай окно с ответами"), Some(true));
        assert_eq!(window_request("закрой окно"), None);
    }

    #[test]
    fn a_purchase_is_not_a_task() {
        assert!(bought("купи яйца, бекон и хлеб"));
        assert!(!asks_to_note("купи яйца, бекон и хлеб"));
        assert!(asks_to_note("напомни завтра купить хлеб"));
        assert!(asks_to_note("добавь созвон на завтра в десять"));
        // Дела без слов о записи фильтр не отсекает: «завтра в десять созвон»
        // остаётся делом, как решила модель.
        assert!(asked_for("add", "завтра в десять созвон"));
        // Enter вслепую не нажимается: «да» внутри слов — не просьба отправить.
        assert!(!asked_for("send", "давай расскажи, когда откроется"));
        assert!(!readable("написать первое 草稿"));
        assert!(readable("написать первый черновик"));
    }

    #[test]
    fn vague_goods_are_asked_about() {
        assert!(vague("ингредиенты для завтрашнего дня"));
        assert!(vague("продукты на завтрак"));
        assert!(!vague("яйца"));
        assert!(!vague("апельсиновый сок"));
    }

    #[test]
    fn goods_from_the_examples_are_recognised_as_made_up() {
        // Семечки в «собери во ВкусВилле корзину» не звучали — их взяла модель.
        assert!(!mentions("Собери во ВкусВилл, пожалуйста, корзину", "полосатые семечки"));
        assert!(follow_up("Собери во ВкусВилл, пожалуйста, корзину"));
        // А здесь сок звучал — заказ настоящий, даже если яйца модель вывела
        // из «яичницы» сама.
        assert!(mentions("хотел яичницу, апельсиновый сок, хлеб", "апельсиновый сок"));
        // «Корзина» — не товар: так модель записывает «собери корзину».
        assert!(about_the_order("корзина"));
        assert!(about_the_order(" Корзину "));
        assert!(!about_the_order("яйца"));
    }

    #[test]
    fn computer_questions_pass_the_filter() {
        assert!(asked_for("system", "что сейчас грузит компьютер"));
        assert!(asked_for("diagnose", "вылезла ошибка, что это"));
        assert!(asked_for("find", "найди фото паспорта"));
        assert!(asked_for("type", "напечатай привет"));
        assert!(!asked_for("find", "звучит музыка"));
    }

    #[test]
    fn music_and_radio_requests_are_recognised() {
        assert!(asks_for_music("включи музыку"));
        assert!(asks_for_music("Ноа, включи мне музыку"));
        assert!(asks_for_music("включи радио"));
        assert!(asks_for_music("включи клауде радио"));
        assert!(asks_for_music("поставь трансляцию claude радио"));
        assert!(asks_for_music("включи мою станцию"));
        assert!(!asks_for_music("выключи радио на кухне и открой почту"));
        assert!(!asks_for_music("радио сегодня говорило про погоду"));
    }

    #[test]
    fn greetings_are_answered_without_the_model() {
        assert_eq!(presence_reply("Привет!"), Some("Привет!"));
        assert_eq!(presence_reply("Ну, а ты тут?"), Some("Тут, слышу."));
        assert_eq!(presence_reply("Проверка связи."), Some("Тут, слышу."));
        assert_eq!(presence_reply("как дела"), Some("Нормально, работаю."));
        // С поручением — не сюда.
        assert_eq!(presence_reply("привет, открой телеграм"), None);
        assert_eq!(presence_reply("ты тут посчитай сколько времени"), None);
    }

    #[test]
    fn done_tasks_are_cleared_as_a_whole() {
        use super::DoneRequest::*;
        assert_eq!(done_request("Все, что в разделе сделано, удали."), Some(Clear));
        assert_eq!(done_request("убери в архив всё, что отмечено как сделано"), Some(Clear));
        assert_eq!(done_request("Хорошо, перенеси сделанные в архив"), Some(Clear));
        assert_eq!(done_request("Какие дела есть в разделе «Сделано»"), Some(List));
        assert_eq!(done_request("что в разделе сделано"), Some(List));
        assert_eq!(done_request("Можешь очистить мои задачи."), Some(Which));
        // Про одно дело — не сюда.
        assert_eq!(done_request("отметь уборку как сделанную"), None);
        assert_eq!(done_request("удали задачу про уборку"), None);
        assert!(asked_for("remove", "можешь очистить мои задачи"));
    }

    /// Какая модель лучше разбирает реплики — на одних и тех же фразах.
    ///
    /// `cargo test planner::tests::compare_models -- --ignored --nocapture`;
    /// модели — через `SUFLER_MODELS=qwen2.5:7b,gemma3:12b`.
    #[test]
    #[ignore = "ходит в локальную модель"]
    fn compare_models() {
        use crate::ai_client::AiProvider;

        let current = "Текущий заказ: яйца ×1, апельсиновый сок ×1, хлеб ×1, молоко ×1, магазин Магнит.";
        let screenshot = "В буфере обмена лежит картинка — скорее всего, скриншот.";
        let cases: &[(&str, &str, &str)] = &[
            ("Я бы на завтрак хотел яичницу, апельсиновый сок, хлеб и молоко", "", "order"),
            ("закажи продукты на яичницу", "", "order"),
            ("купи яйца, бекон и хлеб на завтрак", "", "order"),
            ("закажи пять пачек полосатых семечек", "", "order"),
            ("Собери во ВкусВилле, пожалуйста, корзину", current, "order"),
            ("напомни завтра купить хлеб", "", "add"),
            ("какие у меня дела на сегодня", "", "list"),
            ("что сейчас грузит компьютер", "", "system"),
            ("сколько места осталось на диске", "", "system"),
            ("выскочила какая-то ошибка, что это", "", "diagnose"),
            ("найди на компьютере фото паспорта", "", "find"),
            ("напиши в телеграм Маше, что я опоздаю", "", "message"),
            ("напечатай: буду через десять минут", "", "type"),
            ("звучит музыка", "", "chat"),
            ("что такое альбедо", "", "chat"),
            ("закрой телеграм", "", "close"),
            ("сними задачу хром", "", "close"),
            ("открой настройки заказов", "", "launch"),
            ("открой вайлдберриз", "", "web"),
            ("до скольки работает ашан на ленинском", "", "lookup"),
            ("удали эту задачу", "", "remove"),
            ("я ничего не просил записывать, удали это", "", "remove"),
            ("сколько занято видеопамяти", "", "system"),
            ("какая температура у видеокарты", "", "system"),
            ("это фейк?", screenshot, "screen"),
            ("что тут написано", screenshot, "screen"),
            ("сколько стоит пампфан токен", "", "price"),
            ("какой сейчас курс евро", "", "price"),
            ("спроси клода, как написать резюме", "", "claude"),
            ("покажи мои активы", "", "watch"),
            ("добавь эфир в вотчлист", "", "watch"),
        ];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let models: Vec<String> = std::env::var("SUFLER_MODELS")
            .map(|list| list.split(',').map(str::to_string).collect())
            .unwrap_or_else(|_| vec!["qwen2.5:7b".into(), "gemma3:12b".into()]);

        for model in models {
            let config = crate::config::AiConfig {
                endpoint: crate::ollama::DEFAULT_ENDPOINT.into(),
                model: model.clone(),
                ..Default::default()
            };
            let provider = crate::ai_client::HttpProvider::new(&config, "ru", "Ноа").expect("провайдер");
            // Первый запрос грузит модель в память — его время не в счёт.
            let _ = runtime.block_on(provider.interpret("Ответь: {}", "прогрев"));
            let started = std::time::Instant::now();
            let mut right = 0;
            for (said, context, expected) in cases {
                let rules = rules_with_context(&[], "Ноа", &format!("{} {context}", now_line()));
                let raw = runtime
                    .block_on(provider.interpret(&rules, said))
                    .unwrap_or_default();
                let json = raw
                    .find('{')
                    .zip(raw.rfind('}'))
                    .map(|(from, to)| raw[from..=to].to_string())
                    .unwrap_or_default();
                let got = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|value| value["intent"].as_str().map(str::to_string))
                    .unwrap_or_else(|| "?".into());
                let ok = got == *expected;
                if ok {
                    right += 1;
                }
                println!(
                    "{model:>11} {} {said:<52} → {got:<9} {}",
                    if ok { "✓" } else { "✗" },
                    json.chars().take(170).collect::<String>()
                );
            }
            println!(
                "{model}: верно {right} из {}, в среднем {} мс на реплику\n",
                cases.len(),
                started.elapsed().as_millis() / cases.len() as u128
            );
            // Сравнение не должно оставлять модели в видеопамяти: провайдер
            // просит Ollama держать их долго, как для настоящих вопросов.
            runtime.block_on(crate::ollama::unload(crate::ollama::DEFAULT_HOST, &model));
        }
    }

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
