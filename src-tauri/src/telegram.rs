//! Сообщения человеку в Telegram — от его собственного бота.
//!
//! Бот свой, а не общий: общему пришлось бы доверить, кому и что Ноа пишет.
//! Свой заводится за минуту у @BotFather, и сообщения идут от него человеку
//! напрямую — без сервера посередине. Токен хранится зашифрованным, как ключ
//! модели (см. `crate::secret`); чат Ноа находит сам, когда человек напишет
//! боту первым.
//!
//! Токен — часть адреса запроса. Поэтому ошибки сети отдаются без адреса:
//! иначе токен попал бы в журнал.

use std::time::Duration;

use tauri::{AppHandle, Manager};

const API: &str = "https://api.telegram.org";

fn client() -> Result<reqwest::Client, String> {
    crate::net::client_builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))
}

/// Токен (расшифрованный) и чат из настроек.
pub fn credentials(app: &AppHandle) -> (String, String) {
    let state = app.state::<crate::state::AppState>();
    let config = state.config();
    (
        crate::secret::reveal(&config.telegram.bot_token).trim().to_string(),
        config.telegram.chat_id.trim().to_string(),
    )
}

/// Подключён ли Telegram: есть и токен, и чат.
pub fn ready(app: &AppHandle) -> bool {
    let (token, chat) = credentials(app);
    !token.is_empty() && !chat.is_empty()
}

/// Шлёт сообщение в чат из настроек.
pub async fn notify(app: &AppHandle, text: &str) -> Result<(), String> {
    let (token, chat) = credentials(app);
    if token.is_empty() || chat.is_empty() {
        return Err("Telegram не подключён".into());
    }
    send(&token, &chat, text).await
}

/// Шлёт сообщение в чат.
pub async fn send(token: &str, chat: &str, text: &str) -> Result<(), String> {
    let response = client()?
        .post(format!("{API}/bot{token}/sendMessage"))
        .json(&serde_json::json!({
            "chat_id": chat,
            "text": text,
            "disable_web_page_preview": true,
        }))
        .send()
        .await
        .map_err(|err| format!("Telegram не ответил: {}", err.without_url()))?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("Telegram ответил непонятно: {}", err.without_url()))?;
    if body["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(describe(&body))
    }
}

/// Находит чат, из которого боту писали последним: его номер и имя.
pub async fn find_chat(token: &str) -> Result<(String, String), String> {
    let body: serde_json::Value = client()?
        .get(format!("{API}/bot{token}/getUpdates"))
        .send()
        .await
        .map_err(|err| format!("Telegram не ответил: {}", err.without_url()))?
        .json()
        .await
        .map_err(|err| format!("Telegram ответил непонятно: {}", err.without_url()))?;
    if body["ok"].as_bool() != Some(true) {
        return Err(describe(&body));
    }
    body["result"]
        .as_array()
        .and_then(|updates| {
            updates.iter().rev().find_map(|update| {
                let chat = &update["message"]["chat"];
                let id = chat["id"].as_i64()?;
                let name = chat["first_name"]
                    .as_str()
                    .or(chat["title"].as_str())
                    .unwrap_or("")
                    .to_string();
                Some((id.to_string(), name))
            })
        })
        .ok_or_else(|| {
            "Боту ещё никто не писал. Откройте его в Telegram, отправьте любое сообщение — \
             например, /start — и нажмите «Проверить» ещё раз."
                .into()
        })
}

/* ── Разговор через Telegram ───────────────────────────────────────────── */

/// Сообщение из Telegram: текст или голосовое.
struct Incoming {
    id: i64,
    chat: String,
    text: Option<String>,
    voice: Option<String>,
}

/// Слушает бота, пока программа работает: на текст и голосовые отвечает так
/// же, как вслух, — только текстом.
///
/// Отвечает единственному чату — тому, что в настройках. Бот доступен любому,
/// кто его найдёт, а через Ноа управляют компьютером: чужие сообщения
/// пропускаются молча.
pub fn listen(app: AppHandle) {
    std::thread::Builder::new()
        .name("sufler-telegram".into())
        .spawn(move || {
            let mut offset: Option<i64> = None;
            let mut history: Vec<crate::ai_client::ThreadItem> = Vec::new();
            loop {
                let (token, chat) = credentials(&app);
                if token.is_empty() || chat.is_empty() {
                    std::thread::sleep(Duration::from_secs(30));
                    continue;
                }
                // При запуске — не отвечать на то, что писали, пока Ноа не было:
                // устаревшее «выключи компьютер» не должно выполниться утром.
                let start = match offset {
                    Some(offset) => offset,
                    None => {
                        let skip = tauri::async_runtime::block_on(latest(&token)).unwrap_or(0);
                        offset = Some(skip);
                        skip
                    }
                };
                match tauri::async_runtime::block_on(updates(&token, start)) {
                    Ok(list) => {
                        for incoming in list {
                            offset = Some(incoming.id + 1);
                            if incoming.chat != chat {
                                log::warn!("Telegram: сообщение из чужого чата — не отвечаю");
                                continue;
                            }
                            tauri::async_runtime::block_on(reply_to(
                                &app,
                                &token,
                                &chat,
                                incoming,
                                &mut history,
                            ));
                        }
                    }
                    Err(err) => {
                        log::debug!("Telegram молчит: {err}");
                        std::thread::sleep(Duration::from_secs(15));
                    }
                }
            }
        })
        .ok();
}

/// Номер, с которого начинать: следующий за последним полученным.
async fn latest(token: &str) -> Result<i64, String> {
    let body: serde_json::Value = client()?
        .get(format!("{API}/bot{token}/getUpdates?offset=-1&timeout=0"))
        .send()
        .await
        .map_err(|err| err.without_url().to_string())?
        .json()
        .await
        .map_err(|err| err.without_url().to_string())?;
    Ok(body["result"][0]["update_id"].as_i64().map_or(0, |id| id + 1))
}

/// Новые сообщения. Долгий опрос: Telegram держит запрос до 25 секунд и
/// отвечает, как только что-то пришло.
async fn updates(token: &str, offset: i64) -> Result<Vec<Incoming>, String> {
    let body: serde_json::Value = crate::net::client_builder()
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(|err| err.to_string())?
        .get(format!("{API}/bot{token}/getUpdates?offset={offset}&timeout=25"))
        .send()
        .await
        .map_err(|err| err.without_url().to_string())?
        .json()
        .await
        .map_err(|err| err.without_url().to_string())?;
    if body["ok"].as_bool() != Some(true) {
        return Err(describe(&body));
    }
    Ok(body["result"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|update| {
                    let message = &update["message"];
                    Some(Incoming {
                        id: update["update_id"].as_i64()?,
                        chat: message["chat"]["id"].as_i64()?.to_string(),
                        text: message["text"].as_str().map(str::to_string),
                        voice: message["voice"]["file_id"]
                            .as_str()
                            .or(message["audio"]["file_id"].as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Отвечает на одно сообщение.
async fn reply_to(
    app: &AppHandle,
    token: &str,
    chat: &str,
    incoming: Incoming,
    history: &mut Vec<crate::ai_client::ThreadItem>,
) {
    let said = match (incoming.text, incoming.voice) {
        (Some(text), _) => text,
        (None, Some(file)) => match heard(app, token, &file).await {
            Ok(text) if !text.trim().is_empty() => {
                log::info!("Telegram, голосовое: «{text}»");
                let _ = send(token, chat, &format!("🎙 {text}")).await;
                text
            }
            Ok(_) => {
                let _ = send(token, chat, "Не расслышал голосовое — повторите, пожалуйста.").await;
                return;
            }
            Err(err) => {
                log::warn!("Telegram: голосовое не разобралось: {err}");
                let _ = send(token, chat, &format!("Голосовое не разобрал: {err}")).await;
                return;
            }
        },
        _ => return,
    };
    let said = said.trim().to_string();
    if said == "/start" {
        let _ = send(token, chat, "Ноа на связи. Пишите или присылайте голосовые.").await;
        return;
    }
    log::info!("Telegram: «{said}»");
    let reply = answer(app, &said, history).await;
    log::info!("Telegram, ответ: «{reply}»");
    if let Err(err) = send(token, chat, &reply).await {
        log::warn!("Telegram: ответ не ушёл: {err}");
    }
}

/// Ответ на сказанное в Telegram: распоряжение — через планировщик, как
/// голосом; остальное — вопрос к модели с памятью о трёх последних обменах.
async fn answer(
    app: &AppHandle,
    said: &str,
    history: &mut Vec<crate::ai_client::ThreadItem>,
) -> String {
    if let Some(reply) = crate::planner::handle_remote(app, said).await {
        return reply;
    }
    let (provider, limit) = {
        let state = app.state::<crate::state::AppState>();
        let limit = state.config().ai.call_limit();
        (state.provider(), limit)
    };
    let recent = history[history.len().saturating_sub(3)..].to_vec();
    match tokio::time::timeout(limit, provider.ask("", "", &recent, said)).await {
        Ok(Ok(reply)) if !reply.trim().is_empty() => {
            let reply = reply.trim().to_string();
            history.push(crate::ai_client::ThreadItem {
                q: said.to_string(),
                a: reply.clone(),
            });
            reply
        }
        Ok(Err(err)) => format!("Модель не ответила: {err}"),
        _ => "Модель не успела ответить.".into(),
    }
}

/// Голосовое сообщение текстом: скачать, разобрать OGG в WAV и отдать тому же
/// распознаванию, что слушает микрофон.
async fn heard(app: &AppHandle, token: &str, file: &str) -> Result<String, String> {
    let meta: serde_json::Value = client()?
        .get(format!("{API}/bot{token}/getFile?file_id={}", crate::web::encode(file)))
        .send()
        .await
        .map_err(|err| err.without_url().to_string())?
        .json()
        .await
        .map_err(|err| err.without_url().to_string())?;
    let path = meta["result"]["file_path"]
        .as_str()
        .ok_or("Telegram не отдал файл")?;
    let bytes = client()?
        .get(format!("{API}/file/bot{token}/{path}"))
        .send()
        .await
        .map_err(|err| err.without_url().to_string())?
        .bytes()
        .await
        .map_err(|err| err.without_url().to_string())?;
    let wav = crate::overlay::decode_audio(app, &bytes)?;
    crate::voice::whisper::transcribe(app, wav, "ru", "").await
}

/// Отказ Telegram — словами, по которым понятно, что делать.
fn describe(body: &serde_json::Value) -> String {
    match body["error_code"].as_i64() {
        Some(401) | Some(404) => "Токен не подходит — скопируйте его у @BotFather заново.".into(),
        Some(400) | Some(403) => {
            "Бот не может написать в этот чат — откройте бота и отправьте ему /start.".into()
        }
        _ => format!(
            "Telegram отказал: {}",
            body["description"].as_str().unwrap_or("без объяснений")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_are_explained() {
        let bad_token = serde_json::json!({"ok": false, "error_code": 401});
        assert!(describe(&bad_token).contains("@BotFather"));
        let blocked = serde_json::json!({"ok": false, "error_code": 403});
        assert!(describe(&blocked).contains("/start"));
    }
}
