//! Поиск через настоящий браузер.
//!
//! Поисковики закрыли выдачу для программ: Google без JavaScript отдаёт стену
//! «включите JavaScript», DuckDuckGo и Brave — капчу, Bing — посторонние
//! страницы. Суфлёр сам работает на движке Edge (WebView2) и поэтому ищет так
//! же, как человек: открывает Google в невидимом окне, ждёт загрузки и забирает
//! текст выдачи вместе с карточками — ценой, часами работы, коротким ответом.
//!
//! Странице поиска программа не открыта: окно не входит ни в одно разрешение,
//! а текст возвращается переходом на служебный адрес, который Суфлёр
//! перехватывает и в сеть не пускает.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "search";

/// Служебный адрес, на который страница «переходит», чтобы отдать текст.
/// Домен `.invalid` зарезервирован и не существует: даже если перехват не
/// сработает, в сеть ничего не уйдёт.
const SENTINEL: &str = "sufler.invalid";

/// Сколько ждать выдачу.
const WAIT: Duration = Duration::from_secs(12);

/// Сколько текста выдачи отдавать модели: у неё окно в четыре тысячи токенов,
/// и длинная выдача вытеснила бы из него сам вопрос.
const LIMIT: usize = 3500;

/// Скрипт на каждой странице окна: дождаться загрузки и отдать текст выдачи.
///
/// Сначала — боковая карточка: там цена, часы работы, короткий ответ; потом —
/// сами результаты. Спросит Google согласие на cookie — «Отклонить все»: для
/// поиска они не нужны.
const SCRIPT: &str = r#"(function () {
  if (location.hostname === 'sufler.invalid') return;
  function send() {
    var body = document.body ? document.body.innerText : '';
    var panel = document.querySelector('#rhs');
    var main = document.querySelector('#center_col') || document.querySelector('#rso') ||
      document.querySelector('#search') || document.body;
    var text = (panel ? panel.innerText.slice(0, 1500) + '\n\n' : '') + (main ? main.innerText : '');
    var blocked = location.pathname.indexOf('/sorry') === 0 ||
      /unusual traffic|необычный трафик/i.test(body);
    location.href = 'https://sufler.invalid/result#' +
      encodeURIComponent(JSON.stringify({ blocked: blocked, text: text.slice(0, 8000) }));
  }
  function ready() {
    if (location.hostname.indexOf('consent.') === 0) {
      var buttons = Array.prototype.slice.call(document.querySelectorAll('button'));
      var reject = buttons.filter(function (button) {
        return /Отклонить все|Reject all/i.test(button.innerText);
      })[0];
      if (reject) { reject.click(); return; }
    }
    setTimeout(send, 700);
  }
  if (document.readyState === 'complete') ready(); else window.addEventListener('load', ready);
})();"#;

/// Ищет в Google и отдаёт текст выдачи.
pub async fn google(app: &AppHandle, query: &str) -> Result<String, String> {
    let url: tauri::Url = format!(
        "https://www.google.com/search?q={}&hl=ru&gl=ru",
        crate::web::encode(query)
    )
    .parse()
    .map_err(|err| format!("адрес поиска не собрался: {err}"))?;

    let (found_tx, found_rx) = tokio::sync::oneshot::channel::<String>();
    let found_tx = Arc::new(Mutex::new(Some(found_tx)));
    let (built_tx, built_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

    let handle = app.clone();
    app.run_on_main_thread(move || {
        // Поиск один за раз: прошлое окно, если осталось, — прочь.
        if let Some(old) = handle.get_webview_window(LABEL) {
            let _ = old.destroy();
        }
        let built = WebviewWindowBuilder::new(&handle, LABEL, WebviewUrl::External(url))
            .title("Суфлёр — поиск")
            .inner_size(1200.0, 900.0)
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            .initialization_script(SCRIPT)
            .on_navigation(move |target| {
                if target.host_str() != Some(SENTINEL) {
                    return true;
                }
                if let Some(tx) = found_tx.lock().unwrap_or_else(|err| err.into_inner()).take() {
                    let _ = tx.send(target.fragment().unwrap_or_default().to_string());
                }
                false
            })
            .build()
            .map(|_| ())
            .map_err(|err| err.to_string());
        let _ = built_tx.send(built);
    })
    .map_err(|err| err.to_string())?;

    built_rx
        .await
        .map_err(|_| "окно поиска не создалось".to_string())??;
    let fragment = tokio::time::timeout(WAIT, found_rx).await;
    close(app);
    let fragment = fragment
        .map_err(|_| "Google не ответил вовремя".to_string())?
        .map_err(|_| "окно поиска закрылось раньше времени".to_string())?;

    let parsed: serde_json::Value = serde_json::from_str(&percent_decode(&fragment))
        .map_err(|err| format!("выдача не разобралась: {err}"))?;
    if parsed["blocked"].as_bool() == Some(true) {
        return Err("Google попросил подтвердить, что это не робот".into());
    }
    let text: String = parsed["text"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(LIMIT)
        .collect();
    if text.is_empty() {
        return Err("выдача пустая".into());
    }
    log::info!("Google по «{query}»: {} символов выдачи", text.chars().count());
    Ok(text)
}

fn close(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(LABEL) {
            let _ = window.destroy();
        }
    });
}

/// Раскодирует `%D0%9F…` обратно в текст.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let byte = std::str::from_utf8(&bytes[at + 1..at + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(byte) = byte {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn the_address_text_is_decoded() {
        assert_eq!(percent_decode("%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82%20a"), "Привет a");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%7B%22text%22%3A%22ok%22%7D"), "{\"text\":\"ok\"}");
    }
}
