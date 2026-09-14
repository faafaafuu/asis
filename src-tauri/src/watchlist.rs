//! Список отслеживаемых активов: криптовалюты, акции, валюты.
//!
//! Цены берутся там, где их отдают программам без ключей: криптовалюты — у
//! CoinGecko, акции, фонды, индексы и валютные пары — у Yahoo, российские
//! акции — у Московской биржи. Окно показывает изменение за день, неделю,
//! месяц, год и всё время; щелчок по тикеру открывает график TradingView.
//!
//! Активы раскладываются по вкладкам, которые человек называет сам:
//! «Избранное», «Крипто», «РФ фонды». Актив может лежать в нескольких
//! вкладках сразу; во вкладке «Все» видны все.
//!
//! Список лежит в `watchlist.json` рядом с настройками.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Сколько держать цены, прежде чем спросить снова. CoinGecko без ключа даёт
/// десяток запросов в минуту, а окно, пока открыто, обновляется само.
const FRESH: Duration = Duration::from_secs(60);

/// Откуда цена.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Криптовалюта — CoinGecko.
    Crypto,
    /// Акция, фонд, индекс, валютная пара — Yahoo.
    Stock,
    /// Российская акция — Московская биржа.
    Moex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub kind: Kind,
    /// Как актив знает источник: `bitcoin` у CoinGecko, `AAPL` у Yahoo, `SBER`
    /// у биржи.
    pub id: String,
    pub symbol: String,
    pub name: String,
    /// Символ графика TradingView: `BTCUSDT`, `AAPL`, `MOEX:SBER`.
    pub chart: String,
    /// Первая известная цена — для изменения «за всё время». Она не меняется,
    /// поэтому спрашивается один раз.
    #[serde(default)]
    pub first: Option<f64>,
    /// Первую цену уже искали: не нашлась — значит, её и не будет.
    #[serde(default)]
    pub first_checked: bool,
    /// Вкладки, в которых актив лежит.
    #[serde(default)]
    pub tabs: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    assets: Vec<Asset>,
    /// Вкладки в том порядке, в каком их завели.
    #[serde(default)]
    tabs: Vec<String>,
}

/// Строка окна.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub kind: Kind,
    pub symbol: String,
    pub name: String,
    pub price: Option<f64>,
    pub currency: String,
    pub day: Option<f64>,
    pub week: Option<f64>,
    pub month: Option<f64>,
    pub year: Option<f64>,
    pub all: Option<f64>,
    /// Точки маленького графика: у монет — неделя по часам, у акций — месяц
    /// по дням.
    pub spark: Vec<f64>,
    pub tabs: Vec<String>,
}

static STORE: Mutex<Option<(PathBuf, Store)>> = Mutex::new(None);
static CACHE: Mutex<Option<(Instant, Vec<Row>)>> = Mutex::new(None);
/// Вкладка, которую голосом попросили открыть, пока окна ещё не было: окно,
/// загрузившись, забирает её само — событие до него бы не дошло.
static OPEN_TAB: Mutex<Option<String>> = Mutex::new(None);

/// Читает список. Зовётся при запуске.
pub fn load(dir: PathBuf) {
    let path = dir.join("watchlist.json");
    let store = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Store>(&text).ok())
        .unwrap_or_default();
    log::info!("активов в списке: {}", store.assets.len());
    *STORE.lock().unwrap_or_else(|err| err.into_inner()) = Some((path, store));
}

/// Меняет список под замком; `true` во втором значении — записать на диск.
fn with<T>(change: impl FnOnce(&mut Store) -> (T, bool)) -> Option<T> {
    let mut guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
    let (path, store) = guard.as_mut()?;
    let (value, dirty) = change(store);
    if dirty {
        match serde_json::to_string_pretty(store) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&*path, text) {
                    log::warn!("список активов не сохранился: {err}");
                }
            }
            Err(err) => log::warn!("список активов не сложился в JSON: {err}"),
        }
        // Список изменился — прежние строки окна уже не про него.
        *CACHE.lock().unwrap_or_else(|err| err.into_inner()) = None;
    }
    Some(value)
}

pub fn assets() -> Vec<Asset> {
    with(|store| (store.assets.clone(), false)).unwrap_or_default()
}

/// Добавляет актив по тикеру или названию и отдаёт его.
///
/// С вкладкой — кладёт и во вкладку; вкладки с таким именем нет — заводит.
/// Актив, который уже в списке, во вкладку просто перекладывается: так
/// биткоин из «Всех» попадает ещё и в «Избранное».
pub async fn add(query: &str, tab: Option<&str>) -> Result<Asset, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Не понял, какой актив добавить.".into());
    }
    let client = client()?;
    let found = resolve(&client, query).await?;
    let tab = tab.map(str::trim).filter(|tab| !tab.is_empty());
    let (asset, fresh) = with(|store| {
        let tab = tab.map(|tab| tab_in(store, tab));
        let known = store
            .assets
            .iter_mut()
            .find(|known| known.kind == found.kind && known.id == found.id);
        if let Some(known) = known {
            let tagged = match &tab {
                Some(tab) if !known.tabs.contains(tab) => {
                    known.tabs.push(tab.clone());
                    true
                }
                _ => false,
            };
            return ((known.clone(), tagged), tagged);
        }
        let mut asset = found.clone();
        asset.tabs.extend(tab);
        store.assets.push(asset.clone());
        ((asset, true), true)
    })
    .ok_or("список активов не загружен")?;
    if !fresh {
        return Err(format!("{} уже в списке.", asset.name));
    }
    log::info!("добавлен актив {} ({}), вкладки {:?}", asset.name, asset.id, asset.tabs);
    Ok(asset)
}

/// Убирает актив: по внутреннему имени, тикеру или названию.
///
/// С вкладкой — только из неё: в «Всех» и других вкладках актив остаётся.
pub fn remove(key: &str, tab: Option<&str>) -> Option<Asset> {
    let key = key.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    let tab = tab.map(str::trim).filter(|tab| !tab.is_empty());
    with(|store| {
        let at = store.assets.iter().position(|asset| {
            let name = asset.name.to_lowercase();
            asset.id.to_lowercase() == key
                || asset.symbol.to_lowercase() == key
                || name == key
                || name.contains(&key)
        });
        let Some(at) = at else {
            return (None, false);
        };
        match tab {
            Some(tab) => {
                let asset = &mut store.assets[at];
                let before = asset.tabs.len();
                asset.tabs.retain(|known| !same_tab(known, tab));
                let changed = asset.tabs.len() != before;
                (changed.then(|| asset.clone()), changed)
            }
            None => (Some(store.assets.remove(at)), true),
        }
    })
    .flatten()
}

/// Вкладки по порядку.
pub fn tabs() -> Vec<String> {
    with(|store| (store.tabs.clone(), false)).unwrap_or_default()
}

/// Заводит вкладку; такая уже есть — отдаёт её имя.
pub fn add_tab(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("У вкладки нет названия.".into());
    }
    with(|store| {
        let before = store.tabs.len();
        let name = tab_in(store, name);
        let fresh = store.tabs.len() != before;
        (name, fresh)
    })
    .ok_or_else(|| "список активов не загружен".to_string())
}

/// Убирает вкладку. Активы остаются — во «Всех» и в других вкладках.
pub fn remove_tab(name: &str) -> bool {
    with(|store| {
        let before = store.tabs.len();
        store.tabs.retain(|known| !same_tab(known, name));
        for asset in &mut store.assets {
            asset.tabs.retain(|known| !same_tab(known, name));
        }
        let changed = store.tabs.len() != before;
        (changed, changed)
    })
    .unwrap_or(false)
}

/// Вкладка, как её назвали голосом: «крипту» — это «Крипто».
pub fn find_tab(name: &str) -> Option<String> {
    tabs().into_iter().find(|known| same_tab(known, name))
}

/// Просит окно открыть вкладку. `None` — оставить ту, что открыта.
pub fn open_tab(tab: Option<String>) {
    *OPEN_TAB.lock().unwrap_or_else(|err| err.into_inner()) = tab;
}

/// Вкладка, которую просили открыть, — один раз.
pub fn take_open_tab() -> Option<String> {
    OPEN_TAB.lock().unwrap_or_else(|err| err.into_inner()).take()
}

/// Имя вкладки из списка; нет такой — заводит.
fn tab_in(store: &mut Store, name: &str) -> String {
    if let Some(known) = store.tabs.iter().find(|known| same_tab(known, name)) {
        return known.clone();
    }
    let name = capitalized(name.trim());
    store.tabs.push(name.clone());
    name
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Одна ли это вкладка. Голос называет её в любом падеже — «в крипту», «из
/// фондов», — поэтому окончания не в счёт: совпадает основа.
fn same_tab(a: &str, b: &str) -> bool {
    let norm = |text: &str| -> Vec<char> {
        text.to_lowercase()
            .replace('ё', "е")
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if short.is_empty() {
        return false;
    }
    let stem = short.len().saturating_sub(2).max(3).min(short.len());
    long.starts_with(&short[..stem]) && long.len() <= short.len() + 3
}

/// Адрес графика TradingView для актива из списка.
pub fn chart_url(id: &str) -> Option<String> {
    let asset = assets().into_iter().find(|asset| asset.id == id)?;
    Some(format!(
        "https://ru.tradingview.com/chart/?symbol={}",
        crate::web::encode(&asset.chart)
    ))
}

/// Строки окна: цены и изменения. Минуту держатся в памяти.
pub async fn rows() -> Vec<Row> {
    if let Some((at, rows)) = CACHE.lock().unwrap_or_else(|err| err.into_inner()).as_ref() {
        if at.elapsed() < FRESH {
            return rows.clone();
        }
    }
    let assets = assets();
    let Ok(client) = client() else {
        return Vec::new();
    };

    // Все монеты — одним запросом: так бережётся предел CoinGecko.
    let coins: Vec<&Asset> = assets.iter().filter(|asset| asset.kind == Kind::Crypto).collect();
    let markets = if coins.is_empty() {
        serde_json::Value::Null
    } else {
        crypto_markets(&client, &coins).await.unwrap_or_default()
    };

    let mut rows = Vec::new();
    let mut firsts: Vec<(String, Option<f64>)> = Vec::new();
    for asset in &assets {
        let mut row = match asset.kind {
            Kind::Crypto => crypto_row(asset, &markets),
            Kind::Stock => stock_row(&client, asset).await,
            Kind::Moex => moex_row(&client, asset).await,
        };
        // Первая цена — один раз за жизнь актива в списке, и только когда
        // источник вообще ответил: сбой сети — не повод записать «истории нет».
        let first = if asset.first_checked || row.price.is_none() {
            asset.first
        } else {
            let found = first_price(&client, asset, row.price).await;
            firsts.push((asset.id.clone(), found));
            found
        };
        if let Some(price) = row.price {
            row.all = change(price, first);
        }
        rows.push(row);
    }

    if !firsts.is_empty() {
        with(|store| {
            for (id, first) in &firsts {
                if let Some(asset) = store.assets.iter_mut().find(|asset| &asset.id == id) {
                    asset.first = *first;
                    asset.first_checked = true;
                }
            }
            ((), true)
        });
    }
    *CACHE.lock().unwrap_or_else(|err| err.into_inner()) = Some((Instant::now(), rows.clone()));
    rows
}

/// Сводка вслух: как за сутки сходили первые активы списка.
pub fn summary(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "Список активов пуст. Скажите «добавь биткоин в активы» или впишите тикер в окне."
            .into();
    }
    let parts: Vec<String> = rows
        .iter()
        .take(4)
        .filter_map(|row| {
            let day = row.day?;
            let value = (day.abs() * 10.0).round() / 10.0;
            let sign = if day >= 0.0 { "плюс" } else { "минус" };
            Some(format!(
                "{} {sign} {} {}",
                row.name,
                crate::prices::amount(value),
                crate::prices::plural(value, "процент", "процента", "процентов")
            ))
        })
        .collect();
    if parts.is_empty() {
        return "Открыл активы — цены пока не пришли.".into();
    }
    format!("За сутки: {}.", parts.join(", "))
}

/* ── Какой это актив ─────────────────────────────────────────────────── */

/// Находит актив по тому, что ввели: «btc», «pump», «AAPL», «SBER», «EURUSD».
///
/// Порядок важен. Монета с точным тикером и заметным местом — первой: «BTC» у
/// Yahoo — это фонд, а просят почти всегда биткоин. Потом Московская биржа —
/// «SBER» в Yahoo нет. Потом Yahoo: акции, фонды, пары. Последней — монета по
/// названию: «bitcoin», «pump.fun».
async fn resolve(client: &reqwest::Client, query: &str) -> Result<Asset, String> {
    let upper = query.to_uppercase();
    let plain = upper.chars().all(|ch| ch.is_ascii_alphanumeric());
    let ticker = upper
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '=' | '^'));

    if let Some(asset) = coin(client, query, true).await {
        return Ok(asset);
    }
    if plain && (2..=6).contains(&upper.len()) {
        if let Some(asset) = moex(client, &upper).await {
            return Ok(asset);
        }
    }
    if ticker {
        if let Some(asset) = yahoo(client, &upper).await {
            return Ok(asset);
        }
    }
    if let Some(asset) = coin(client, query, false).await {
        return Ok(asset);
    }
    Err(format!("Не нашёл «{query}» ни среди монет, ни среди акций."))
}

/// Монета CoinGecko. `exact` — только с таким тикером и не ниже пятисотого
/// места; иначе — первая, у которой место вообще есть: безымянные жетоны с
/// любыми названиями в список не попадают.
async fn coin(client: &reqwest::Client, query: &str, exact: bool) -> Option<Asset> {
    let body = get_json(
        client,
        &format!(
            "https://api.coingecko.com/api/v3/search?query={}",
            crate::web::encode(query.trim())
        ),
    )
    .await
    .ok()?;
    let found = body["coins"].as_array()?.iter().find(|coin| {
        let Some(rank) = coin["market_cap_rank"].as_u64() else {
            return false;
        };
        !exact
            || (rank <= 500
                && coin["symbol"]
                    .as_str()
                    .is_some_and(|symbol| symbol.eq_ignore_ascii_case(query.trim())))
    })?;
    let symbol = found["symbol"].as_str()?.to_uppercase();
    Some(Asset {
        kind: Kind::Crypto,
        id: found["id"].as_str()?.to_string(),
        name: found["name"].as_str().unwrap_or(&symbol).to_string(),
        chart: format!("{symbol}USDT"),
        symbol,
        first: None,
        first_checked: false,
        tabs: Vec::new(),
    })
}

/// Российская акция в основном режиме торгов Московской биржи.
async fn moex(client: &reqwest::Client, secid: &str) -> Option<Asset> {
    let body = get_json(
        client,
        &format!(
            "{}.json?iss.meta=off&iss.only=securities&securities.columns=SECID,SHORTNAME",
            moex_base(secid)
        ),
    )
    .await
    .ok()?;
    let row = body["securities"]["data"].as_array()?.first()?;
    let id = row[0].as_str()?.to_string();
    let name = row[1].as_str().unwrap_or(&id).to_string();
    Some(Asset {
        kind: Kind::Moex,
        symbol: id.clone(),
        chart: format!("MOEX:{id}"),
        name,
        id,
        first: None,
        first_checked: false,
        tabs: Vec::new(),
    })
}

/// Акция, фонд, индекс или валютная пара у Yahoo. Шесть букв без суффикса —
/// ещё и пара: «EURUSD» у Yahoo называется «EURUSD=X».
async fn yahoo(client: &reqwest::Client, ticker: &str) -> Option<Asset> {
    let mut candidates = vec![ticker.to_string()];
    if ticker.len() == 6 && ticker.chars().all(|ch| ch.is_ascii_alphabetic()) {
        candidates.push(format!("{ticker}=X"));
    }
    for symbol in candidates {
        let Ok(body) = get_json(client, &yahoo_url(&symbol, "5d", "1d")).await else {
            continue;
        };
        let meta = &body["chart"]["result"][0]["meta"];
        if meta["regularMarketPrice"].as_f64().is_none() {
            continue;
        }
        let name = meta["longName"]
            .as_str()
            .or(meta["shortName"].as_str())
            .unwrap_or(&symbol)
            .to_string();
        return Some(Asset {
            kind: Kind::Stock,
            symbol: symbol.trim_end_matches("=X").to_string(),
            chart: symbol.trim_end_matches("=X").trim_start_matches('^').to_string(),
            name,
            id: symbol,
            first: None,
            first_checked: false,
            tabs: Vec::new(),
        });
    }
    None
}

/* ── Цены ────────────────────────────────────────────────────────────── */

async fn crypto_markets(
    client: &reqwest::Client,
    coins: &[&Asset],
) -> Result<serde_json::Value, String> {
    let ids = coins
        .iter()
        .map(|asset| crate::web::encode(&asset.id))
        .collect::<Vec<_>>()
        .join(",");
    get_json(
        client,
        &format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&ids={ids}\
             &price_change_percentage=24h,7d,30d,1y&sparkline=true"
        ),
    )
    .await
}

fn crypto_row(asset: &Asset, markets: &serde_json::Value) -> Row {
    let mut row = empty_row(asset, "USD");
    let entry = markets.as_array().and_then(|list| {
        list.iter()
            .find(|coin| coin["id"].as_str() == Some(asset.id.as_str()))
    });
    if let Some(coin) = entry {
        row.price = coin["current_price"].as_f64();
        row.day = coin["price_change_percentage_24h_in_currency"].as_f64();
        row.week = coin["price_change_percentage_7d_in_currency"].as_f64();
        row.month = coin["price_change_percentage_30d_in_currency"].as_f64();
        row.year = coin["price_change_percentage_1y_in_currency"].as_f64();
        let points = coin["sparkline_in_7d"]["price"]
            .as_array()
            .map(|points| points.iter().filter_map(|point| point.as_f64()).collect())
            .unwrap_or_default();
        row.spark = thin(points, 48);
    }
    row
}

async fn stock_row(client: &reqwest::Client, asset: &Asset) -> Row {
    let mut row = empty_row(asset, "USD");
    let Ok(body) = get_json(client, &yahoo_url(&asset.id, "1y", "1d")).await else {
        return row;
    };
    let result = &body["chart"]["result"][0];
    row.currency = result["meta"]["currency"].as_str().unwrap_or("USD").to_string();
    let closes = closes(&result["indicators"]["quote"][0]["close"]);
    let price = result["meta"]["regularMarketPrice"]
        .as_f64()
        .or(closes.last().copied());
    fill(&mut row, price, &closes);
    row
}

async fn moex_row(client: &reqwest::Client, asset: &Asset) -> Row {
    let mut row = empty_row(asset, "RUB");
    let base = moex_base(&asset.id);
    let now = get_json(
        client,
        &format!(
            "{base}.json?iss.meta=off&iss.only=marketdata&marketdata.columns=LAST,LASTTOPREVPRICE"
        ),
    )
    .await
    .ok();
    let last = now
        .as_ref()
        .and_then(|body| body["marketdata"]["data"][0][0].as_f64());
    row.day = now
        .as_ref()
        .and_then(|body| body["marketdata"]["data"][0][1].as_f64());
    let from = (chrono::Local::now() - chrono::Duration::days(400)).format("%Y-%m-%d");
    let history = get_json(
        client,
        &format!("{base}/candles.json?iss.meta=off&interval=24&from={from}&candles.columns=close"),
    )
    .await
    .ok();
    let closes: Vec<f64> = history
        .as_ref()
        .and_then(|body| body["candles"]["data"].as_array())
        .map(|rows| rows.iter().filter_map(|row| row[0].as_f64()).collect())
        .unwrap_or_default();
    // Торги закрыты — последней сделки нет: цена — последнее закрытие.
    fill(&mut row, last.or(closes.last().copied()), &closes);
    row
}

/// Первая известная цена — для «за всё время».
async fn first_price(client: &reqwest::Client, asset: &Asset, price: Option<f64>) -> Option<f64> {
    match asset.kind {
        Kind::Stock => yahoo_first(client, &asset.id).await,
        // У CoinGecko история за всё время без ключа закрыта. У крупных монет
        // она есть у Yahoo — берётся, если цена там сходится с сегодняшней:
        // иначе под тем же тикером у Yahoo другая монета.
        Kind::Crypto => {
            let symbol = format!("{}-USD", asset.symbol.to_uppercase());
            let body = get_json(client, &yahoo_url(&symbol, "5d", "1d")).await.ok()?;
            let there = body["chart"]["result"][0]["meta"]["regularMarketPrice"].as_f64()?;
            let here = price?;
            if (there / here - 1.0).abs() > 0.2 {
                return None;
            }
            yahoo_first(client, &symbol).await
        }
        Kind::Moex => {
            let body = get_json(
                client,
                &format!(
                    "{}/candles.json?iss.meta=off&interval=31&from=1997-01-01&candles.columns=close",
                    moex_base(&asset.id)
                ),
            )
            .await
            .ok()?;
            body["candles"]["data"][0][0].as_f64()
        }
    }
}

async fn yahoo_first(client: &reqwest::Client, symbol: &str) -> Option<f64> {
    let body = get_json(client, &yahoo_url(symbol, "max", "3mo")).await.ok()?;
    closes(&body["chart"]["result"][0]["indicators"]["quote"][0]["close"])
        .first()
        .copied()
}

/* ── Вспомогательное ─────────────────────────────────────────────────── */

fn client() -> Result<reqwest::Client, String> {
    crate::net::client_builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    response.json().await.map_err(|err| err.to_string())
}

fn yahoo_url(symbol: &str, range: &str, interval: &str) -> String {
    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?range={range}&interval={interval}",
        crate::web::encode(symbol)
    )
}

fn moex_base(secid: &str) -> String {
    format!(
        "https://iss.moex.com/iss/engines/stock/markets/shares/boards/TQBR/securities/{}",
        crate::web::encode(secid)
    )
}

fn empty_row(asset: &Asset, currency: &str) -> Row {
    Row {
        id: asset.id.clone(),
        kind: asset.kind,
        symbol: asset.symbol.clone(),
        name: asset.name.clone(),
        price: None,
        currency: currency.to_string(),
        day: None,
        week: None,
        month: None,
        year: None,
        all: None,
        spark: Vec::new(),
        tabs: asset.tabs.clone(),
    }
}

/// Цены закрытия без пропусков: в выходные и праздники у Yahoo там `null`.
fn closes(value: &serde_json::Value) -> Vec<f64> {
    value
        .as_array()
        .map(|points| points.iter().filter_map(|point| point.as_f64()).collect())
        .unwrap_or_default()
}

/// Изменение в процентах.
fn change(now: f64, then: Option<f64>) -> Option<f64> {
    let then = then?;
    (then > 0.0).then(|| (now / then - 1.0) * 100.0)
}

/// Изменения по дневным закрытиям: день — к прошлому закрытию, неделя — пять
/// торговых дней, месяц — двадцать один, год — первое закрытие за год.
fn fill(row: &mut Row, price: Option<f64>, closes: &[f64]) {
    row.price = price;
    let Some(price) = price else {
        return;
    };
    let back = |days: usize| closes.len().checked_sub(days + 1).map(|at| closes[at]);
    row.day = row.day.or(change(price, back(1)));
    row.week = change(price, back(5));
    row.month = change(price, back(21));
    row.year = change(price, closes.first().copied());
    row.spark = closes.iter().rev().take(22).rev().copied().collect();
}

/// Прореживает линию графика: в маленькой картинке 168 точек не различить.
fn thin(points: Vec<f64>, most: usize) -> Vec<f64> {
    if points.len() <= most || most < 2 {
        return points;
    }
    let step = (points.len() - 1) as f64 / (most - 1) as f64;
    (0..most)
        .map(|at| points[((at as f64 * step).round() as usize).min(points.len() - 1)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            kind: Kind::Crypto,
            id: name.to_lowercase(),
            symbol: name.to_uppercase(),
            name: name.to_string(),
            chart: String::new(),
            first: None,
            first_checked: false,
            tabs: Vec::new(),
        }
    }

    #[test]
    fn changes_are_counted_from_closes() {
        let closes: Vec<f64> = (0..252).map(|day| 100.0 + day as f64).collect();
        let mut row = empty_row(&asset("Test"), "USD");
        fill(&mut row, Some(351.0), &closes);
        let round = |value: Option<f64>| value.map(|value| (value * 100.0).round() / 100.0);
        assert_eq!(round(row.day), Some(0.29));
        assert_eq!(round(row.week), Some(1.45));
        assert_eq!(round(row.year), Some(251.0));
        assert_eq!(row.spark.len(), 22);
        assert_eq!(change(10.0, Some(0.0)), None);
    }

    #[test]
    fn a_tab_is_named_in_any_case() {
        assert!(same_tab("Крипто", "крипту"));
        assert!(same_tab("Фонды", "фондов"));
        assert!(same_tab("Избранное", "избранном"));
        assert!(!same_tab("Фонды", "РФ фонды"));
        assert!(!same_tab("Крипто", "акции"));
        assert_eq!(capitalized("ру фонды"), "Ру фонды");
    }

    #[test]
    fn a_long_line_is_thinned() {
        let points: Vec<f64> = (0..168).map(f64::from).collect();
        let thin = thin(points, 48);
        assert_eq!(thin.len(), 48);
        assert_eq!(thin.first(), Some(&0.0));
        assert_eq!(thin.last(), Some(&167.0));
    }

    #[test]
    fn the_summary_is_spoken() {
        let mut bitcoin = empty_row(&asset("Bitcoin"), "USD");
        bitcoin.day = Some(-1.04);
        let mut pump = empty_row(&asset("Pump.fun"), "USD");
        pump.day = Some(8.6);
        assert_eq!(
            summary(&[bitcoin, pump]),
            "За сутки: Bitcoin минус 1 процент, Pump.fun плюс 8,6 процента."
        );
        assert!(summary(&[]).starts_with("Список активов пуст"));
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib watchlist::live -- --ignored --nocapture`
    #[test]
    #[ignore = "ходит в CoinGecko, Yahoo и Мосбиржу"]
    fn assets_are_found_and_priced() {
        let dir = std::env::temp_dir().join("sufler-watchlist-test");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::remove_file(dir.join("watchlist.json"));
        super::load(dir);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for query in ["btc", "pump", "AAPL", "SBER", "EURUSD", "нечто несуществующее"] {
            match runtime.block_on(super::add(query, None)) {
                Ok(asset) => println!(
                    "{query} → {:?} {} ({}), график {}",
                    asset.kind, asset.name, asset.id, asset.chart
                ),
                Err(err) => println!("{query} → {err}"),
            }
        }
        let rows = runtime.block_on(super::rows());
        for row in &rows {
            println!(
                "{:>7} {:?} {} день {:?} неделя {:?} месяц {:?} год {:?} всё {:?}, точек {}",
                row.symbol, row.price, row.currency, row.day, row.week, row.month, row.year, row.all,
                row.spark.len()
            );
        }
        println!("{}", super::summary(&rows));
    }
}
