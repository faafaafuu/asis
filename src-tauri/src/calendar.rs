//! Google-календарь: дела со сроком уезжают туда событиями.
//!
//! Подключается ключом, который человек заводит сам в Google Cloud. Причина в
//! правилах Google: к календарю пускают только зарегистрированные у него
//! программы, а регистрация от нашего имени означала бы для каждого экран
//! «приложение не проверено» и потолок в сто пользователей, пока Google не
//! проверит программу — это месяцы, сайт и политика конфиденциальности. Со
//! своим ключом человек сам себе разработчик: ни предупреждения, ни потолка.
//!
//! Согласие берётся обычным способом для настольных программ: открывается
//! браузер, а программа на это время слушает случайный порт на 127.0.0.1, куда
//! Google возвращает код. Код обменивается на долгоживущий ключ, и дальше
//! браузер больше не нужен.
//!
//! Отправка событий не должна задерживать разговор, поэтому она идёт отдельным
//! потоком и её неудача ничего не ломает: дело остаётся в списке, а в журнал
//! уходит строка. Календарь здесь — дополнение к списку, а не его хранилище.

use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::tasks::Task;

/// Права, которые запрашиваются у Google.
///
/// Только события календаря и ничего больше: ни почты, ни контактов, ни чтения
/// профиля. Чем уже просьба, тем понятнее человеку на экране согласия, что
/// именно он разрешает.
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API_URL: &str = "https://www.googleapis.com/calendar/v3/calendars";

/// Сколько ждать согласия в браузере.
const CONSENT_WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/* ── Отправка дел ────────────────────────────────────────────────────────── */

/// Отправляет дело в календарь: создаёт событие или правит уже созданное.
///
/// `asked` — человек прямо просил в календарь. Тогда событие создаётся, даже
/// если общая отправка выключена: прямая просьба сильнее настройки.
pub fn sync_task(app: &AppHandle, task: &Task, asked: bool) {
    let config = app.state::<AppState>().config().calendar.clone();
    if !config.ready() && !(asked && !config.refresh_token.trim().is_empty()) {
        if asked {
            log::info!("просили в календарь, но он не подключён");
        }
        return;
    }
    // Дело без срока в календаре не событие, а запись ни о чём.
    let Some(due) = task.due else { return };

    let app = app.clone();
    let task = task.clone();
    std::thread::Builder::new()
        .name("sufler-calendar".into())
        .spawn(move || {
            let sent = tauri::async_runtime::block_on(put_event(&config, &task, due));
            match sent {
                Ok(event_id) => {
                    crate::tasks::set_event(&task.id, Some(event_id));
                    crate::planner::changed(&app);
                    log::info!("дело «{}» ушло в календарь", task.title);
                }
                Err(err) => log::warn!("календарь не принял «{}»: {err}", task.title),
            }
        })
        .ok();
}

/// Убирает событие: дело сделано, в календаре ему больше не место.
pub fn forget_task(app: &AppHandle, task: &Task) {
    let config = app.state::<AppState>().config().calendar.clone();
    let Some(event_id) = task.event_id.clone() else {
        return;
    };
    if config.refresh_token.trim().is_empty() {
        return;
    }

    let title = task.title.clone();
    std::thread::Builder::new()
        .name("sufler-calendar-drop".into())
        .spawn(move || {
            if let Err(err) = tauri::async_runtime::block_on(drop_event(&config, &event_id)) {
                log::warn!("событие «{title}» не убралось из календаря: {err}");
            }
        })
        .ok();
}

/// Создаёт или обновляет событие и отдаёт его идентификатор.
async fn put_event(
    config: &crate::config::CalendarConfig,
    task: &Task,
    due: chrono::DateTime<chrono::Local>,
) -> Result<String, String> {
    let token = access_token(config).await?;
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    // Час на дело — разумное умолчание: у списка задач нет длительности, а
    // событие нулевой длины календари показывают засечкой, которую не видно.
    let ends = due + chrono::Duration::hours(1);
    let body = serde_json::json!({
        "summary": task.title,
        "description": task.advice.clone().unwrap_or_default(),
        "start": { "dateTime": due.to_rfc3339() },
        "end": { "dateTime": ends.to_rfc3339() },
    });

    let base = format!("{API_URL}/{}/events", urlencode(&config.calendar_id));
    let request = match &task.event_id {
        // У события уже есть место в календаре — правим его, а не плодим новое.
        Some(id) => client.patch(format!("{base}/{}", urlencode(id))),
        None => client.post(&base),
    };

    let response = request
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("запрос не ушёл: {err}"))?;

    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("ответ не разобрался: {err}"))?;

    if !status.is_success() {
        let reason = value["error"]["message"].as_str().unwrap_or("без объяснения");
        return Err(format!("{status}: {reason}"));
    }

    value["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "в ответе нет идентификатора события".to_string())
}

async fn drop_event(
    config: &crate::config::CalendarConfig,
    event_id: &str,
) -> Result<(), String> {
    let token = access_token(config).await?;
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let url = format!(
        "{API_URL}/{}/events/{}",
        urlencode(&config.calendar_id),
        urlencode(event_id)
    );
    let response = client
        .delete(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| format!("запрос не ушёл: {err}"))?;

    // 410 означает «уже удалено» — для нас это тот же успех.
    if response.status().is_success() || response.status().as_u16() == 410 {
        return Ok(());
    }
    Err(format!("календарь ответил {}", response.status()))
}

/* ── Ключи ───────────────────────────────────────────────────────────────── */

/// Меняет долгоживущий ключ на короткий, которым подписываются запросы.
///
/// Короткий живёт час, поэтому он не хранится: получить его заново стоит одного
/// запроса, а хранение означало бы ещё одно место, где он может протухнуть.
async fn access_token(config: &crate::config::CalendarConfig) -> Result<String, String> {
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", config.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|err| format!("Google не ответил: {err}"))?;

    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("ответ Google не разобрался: {err}"))?;

    if !status.is_success() {
        let reason = value["error_description"]
            .as_str()
            .or_else(|| value["error"].as_str())
            .unwrap_or("без объяснения");
        return Err(format!("вход не подтверждён ({reason})"));
    }

    value["access_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Google не прислал ключ доступа".to_string())
}

/// Проводит человека через согласие Google и отдаёт долгоживущий ключ.
///
/// Слушает случайный свободный порт на 127.0.0.1 — так требует Google для
/// настольных программ, и так браузеру есть куда вернуть код. Порт закрывается
/// сразу после ответа: слушать дольше незачем.
pub async fn connect(app: &AppHandle) -> Result<String, String> {
    let (client_id, client_secret) = {
        let state = app.state::<AppState>();
        let config = state.config();
        (
            config.calendar.client_id.trim().to_string(),
            config.calendar.client_secret.trim().to_string(),
        )
    };
    if client_id.is_empty() {
        return Err("сначала впишите идентификатор клиента из Google Cloud".into());
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("не удалось открыть порт для ответа Google: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| err.to_string())?
        .port();
    let redirect = format!("http://127.0.0.1:{port}");

    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &access_type=offline&prompt=consent",
        urlencode(&client_id),
        urlencode(&redirect),
        urlencode(SCOPE)
    );
    crate::commands::open_externally(&url)?;

    let code = tokio::task::spawn_blocking(move || wait_for_code(listener))
        .await
        .map_err(|err| format!("ожидание согласия сорвалось: {err}"))??;

    exchange_code(&client_id, &client_secret, &redirect, &code).await
}

/// Дожидается, пока браузер придёт с кодом, и отвечает ему страницей.
fn wait_for_code(listener: std::net::TcpListener) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};

    listener
        .set_nonblocking(false)
        .map_err(|err| err.to_string())?;

    let deadline = std::time::Instant::now() + CONSENT_WAIT;
    loop {
        if std::time::Instant::now() > deadline {
            return Err("согласие не подтвердили за пять минут".into());
        }

        let Ok((mut stream, _)) = listener.accept() else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));

        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }

        // Строка вида «GET /?code=… HTTP/1.1». Берём то, что между пробелами.
        let target = line.split_whitespace().nth(1).unwrap_or_default().to_string();
        let found = query_value(&target, "code");
        let refused = query_value(&target, "error");

        let page = if found.is_some() {
            "Готово. Можно закрыть эту вкладку и вернуться в Суфлёр."
        } else {
            "Доступ не выдан. Вернитесь в Суфлёр и попробуйте ещё раз."
        };
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
             Connection: close\r\n\r\n<!doctype html><meta charset=\"utf-8\">\
             <body style=\"font:16px system-ui;padding:3rem\">{page}</body>"
        );
        let _ = stream.flush();

        if let Some(code) = found {
            return Ok(code);
        }
        if let Some(error) = refused {
            return Err(format!("Google отказал: {error}"));
        }
    }
}

/// Меняет одноразовый код на долгоживущий ключ.
async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect: &str,
    code: &str,
) -> Result<String, String> {
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect),
        ])
        .send()
        .await
        .map_err(|err| format!("Google не ответил: {err}"))?;

    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("ответ Google не разобрался: {err}"))?;

    if !status.is_success() {
        let reason = value["error_description"]
            .as_str()
            .or_else(|| value["error"].as_str())
            .unwrap_or("без объяснения");
        return Err(format!("обмен кода не удался ({reason})"));
    }

    value["refresh_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            "Google не прислал долгоживущий ключ — отзовите доступ программе \
             в настройках аккаунта и подключитесь заново"
                .to_string()
        })
}

/* ── Мелочи ──────────────────────────────────────────────────────────────── */

/// Значение параметра из строки запроса.
fn query_value(target: &str, key: &str) -> Option<String> {
    let query = target.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| urldecode(value))
    })
}

/// Процентное кодирование: своё, потому что ради трёх вызовов тянуть отдельную
/// зависимость несоразмерно.
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

fn urldecode(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' if at + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        at += 3;
                    }
                    Err(_) => {
                        out.push(bytes[at]);
                        at += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_taken_out_of_the_browser_request() {
        let target = "/?code=4%2F0Ab_c-d&scope=https%3A%2F%2Fexample";
        assert_eq!(query_value(target, "code").as_deref(), Some("4/0Ab_c-d"));
        assert!(query_value(target, "error").is_none());

        let refused = "/?error=access_denied";
        assert_eq!(
            query_value(refused, "error").as_deref(),
            Some("access_denied")
        );
    }

    #[test]
    fn addresses_survive_encoding() {
        assert_eq!(urlencode("http://127.0.0.1:8080"), "http%3A%2F%2F127.0.0.1%3A8080");
        assert_eq!(urldecode(&urlencode("a b/c?d")), "a b/c?d");
    }
}
