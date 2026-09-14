//! Цены: криптовалюты — по CoinGecko, валюты — по курсу ЦБ.
//!
//! «Сколько стоит pump.fun» поисковик не отвечает: выдача поисковиков
//! программам закрыта, а цена в новостях — вчерашняя. У CoinGecko и ЦБ
//! открытые адреса для программ, без ключей, и цена там сегодняшняя.

use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Валюты ЦБ: как их называют, код и как называть в ответе.
const CURRENCIES: &[(&[&str], &str, &str)] = &[
    (&["доллар", "usd", "бакс"], "USD", "Доллар"),
    (&["евро", "eur"], "EUR", "Евро"),
    (&["юан", "cny"], "CNY", "Юань"),
    (&["фунт", "gbp"], "GBP", "Фунт"),
    (&["тенге", "kzt"], "KZT", "Тенге"),
    (&["лир", "try"], "TRY", "Турецкая лира"),
    (&["дирхам", "aed"], "AED", "Дирхам"),
    (&["иен", "йен", "jpy"], "JPY", "Иена"),
    (&["франк", "chf"], "CHF", "Швейцарский франк"),
    (&["белорусск", "byn"], "BYN", "Белорусский рубль"),
];

/// Сколько стоит актив — фраза для голоса. `None` — не нашлось: пусть ищет
/// обычный поиск.
pub async fn price(asset: &str) -> Option<String> {
    let lower = asset.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    if let Some((_, code, name)) = CURRENCIES
        .iter()
        .find(|(words, _, _)| words.iter().any(|word| lower.contains(word)))
    {
        return currency(&client, code, name).await;
    }
    crypto(&client, &lower).await
}

/// Курс валюты по ЦБ на сегодня.
async fn currency(client: &reqwest::Client, code: &str, name: &str) -> Option<String> {
    let body: serde_json::Value = client
        .get("https://www.cbr-xml-daily.ru/daily_json.js")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let valute = &body["Valute"][code];
    let value = valute["Value"].as_f64()?;
    // Иена и тенге у ЦБ — за сто и за сто тенге: считаем за одну.
    let nominal = valute["Nominal"].as_f64().unwrap_or(1.0).max(1.0);
    let rate = value / nominal;
    Some(format!(
        "{name} по курсу ЦБ — {} {}.",
        amount(rate),
        plural(rate, "рубль", "рубля", "рублей")
    ))
}

/// Цена монеты по CoinGecko: сперва поиск монеты, потом её цена.
///
/// Монета без места в рейтинге не берётся: у CoinGecko тысячи безымянных
/// жетонов с любыми названиями, и «сколько стоит айфон» нашло бы мем-монету
/// «iPhone» вместо честного «не нашёл».
async fn crypto(client: &reqwest::Client, asset: &str) -> Option<String> {
    let query = asset
        .trim_end_matches(" токен")
        .trim_end_matches(" token")
        .trim_end_matches(" coin")
        .trim_end_matches(" монета")
        .trim();
    let found: serde_json::Value = client
        .get(format!(
            "https://api.coingecko.com/api/v3/search?query={}",
            crate::web::encode(query)
        ))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let coin = found["coins"]
        .as_array()?
        .iter()
        .find(|coin| coin["market_cap_rank"].as_u64().is_some())?;
    let id = coin["id"].as_str()?;
    let name = coin["name"].as_str().unwrap_or(id);
    let symbol = coin["symbol"].as_str().unwrap_or_default();

    let prices: serde_json::Value = client
        .get(format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={id}&vs_currencies=usd,rub&include_24hr_change=true"
        ))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let entry = &prices[id];
    Some(describe_coin(
        name,
        symbol,
        entry["usd"].as_f64()?,
        entry["rub"].as_f64(),
        entry["usd_24h_change"].as_f64(),
    ))
}

/// «Pump.fun (PUMP) стоит 0,0037 доллара, это 0,31 рубля. За сутки минус 8,4 процента.»
fn describe_coin(name: &str, symbol: &str, usd: f64, rub: Option<f64>, change: Option<f64>) -> String {
    let named = match symbol.trim() {
        "" => name.to_string(),
        symbol => format!("{name} ({})", symbol.to_uppercase()),
    };
    let mut said = format!(
        "{named} стоит {} {}",
        amount(usd),
        plural(usd, "доллар", "доллара", "долларов")
    );
    if let Some(rub) = rub {
        said.push_str(&format!(", это {} {}", amount(rub), plural(rub, "рубль", "рубля", "рублей")));
    }
    said.push('.');
    if let Some(change) = change {
        let percent = (change.abs() * 10.0).round() / 10.0;
        let sign = if change >= 0.0 { "плюс" } else { "минус" };
        said.push_str(&format!(
            " За сутки {sign} {} {}.",
            amount(percent),
            plural(percent, "процент", "процента", "процентов")
        ));
    }
    said
}

/// Число для голоса: запятая вместо точки, пробелы между тысячами; у мелочи —
/// две значащие цифры после нулей, «0,0037».
pub(crate) fn amount(value: f64) -> String {
    let value = value.abs();
    if value >= 1000.0 {
        return thousands(value.round() as u64);
    }
    if value >= 1.0 || value == 0.0 {
        let text = format!("{value:.2}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        return text.replace('.', ",");
    }
    let digits = (-value.log10()).floor() as usize + 2;
    format!("{value:.digits$}").replace('.', ",")
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(' ');
        }
        out.push(digit);
    }
    out
}

/// Форма слова после числа. У дробных — «доллара», как «0,31 рубля».
pub(crate) fn plural<'a>(value: f64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let shown = amount(value);
    if shown.contains(',') {
        return few;
    }
    let whole = value.abs().round() as u64;
    match (whole % 100, whole % 10) {
        (11..=14, _) => many,
        (_, 1) => one,
        (_, 2..=4) => few,
        _ => many,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_are_spoken_in_russian() {
        assert_eq!(amount(0.00365443), "0,0037");
        assert_eq!(amount(84.3508), "84,35");
        assert_eq!(amount(60123.4), "60 123");
        assert_eq!(amount(2.0), "2");
        assert_eq!(plural(0.31, "рубль", "рубля", "рублей"), "рубля");
        assert_eq!(plural(60123.0, "доллар", "доллара", "долларов"), "доллара");
        assert_eq!(plural(11.0, "доллар", "доллара", "долларов"), "долларов");
        assert_eq!(plural(21.0, "доллар", "доллара", "долларов"), "доллар");
    }

    #[test]
    fn a_coin_is_described_briefly() {
        assert_eq!(
            describe_coin("Pump.fun", "pump", 0.00365443, Some(0.308249), Some(-8.3706)),
            "Pump.fun (PUMP) стоит 0,0037 доллара, это 0,31 рубля. За сутки минус 8,4 процента."
        );
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib prices::live -- --ignored --nocapture`
    #[test]
    #[ignore = "ходит в CoinGecko и ЦБ"]
    fn prices_are_found() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for asset in ["pump.fun", "bitcoin", "доллар", "евро", "iphone"] {
            let said = runtime.block_on(super::price(asset));
            println!("{asset}: {said:?}");
        }
    }
}
