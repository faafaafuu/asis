//! Ноа как MCP-сервер: `sufler.exe --mcp`.
//!
//! MCP (Model Context Protocol) — открытый способ дать модели инструменты.
//! Claude Desktop, Claude Code и другие клиенты запускают эту программу с
//! ключом `--mcp` и говорят с ней по JSON-RPC через стандартные ввод и вывод.
//! Окон и голоса в этом режиме нет — только инструменты:
//!
//! - курсы обучения: формат, создать курс, добавить тему, удалить, список с
//!   прогрессом. «Составь мне курс по китайскому» — Claude пишет материал,
//!   Ноа кладёт его к себе и показывает в окне «Обучение» как встроенный;
//! - передать фразу работающей Ноа — как сказанную вслух: так клиенту
//!   доступны дела, активы, таймеры и всё остальное, что Ноа умеет.
//!
//! В стандартный вывод здесь не пишется ничего, кроме ответов протокола:
//! любая лишняя строка сломала бы клиенту разбор.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

/// Версия протокола, если клиент не назвал свою.
const PROTOCOL: &str = "2024-11-05";

/// Где лежат данные Ноа: `%APPDATA%\app.sufler.popup`.
fn data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("app.sufler.popup"))
}

/// Разбирает запросы, пока клиент не закроет ввод.
pub fn serve() {
    if let Some(dir) = data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        crate::learning::load(dir);
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle(&request),
            Err(err) => Some(error(Value::Null, -32700, &format!("не JSON: {err}"))),
        };
        if let Some(reply) = reply {
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

/// Ответ на один запрос. Уведомлениям ответ не положен — `None`.
fn handle(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request["method"].as_str().unwrap_or_default();
    let params = &request["params"];
    let result: Result<Value, String> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "noa", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => Ok(call(
            params["name"].as_str().unwrap_or_default(),
            &params["arguments"],
        )),
        // Уведомления: initialized, cancelled и прочие.
        _ if id.is_none() => return None,
        _ => return Some(error(id.unwrap_or(Value::Null), -32601, "нет такого метода")),
    };
    let id = id?;
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => error(id, -32603, &message),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

const INSTRUCTIONS: &str = "Ноа — голосовой ассистент на компьютере пользователя. \
Через эти инструменты можно создавать курсы обучения, которые Ноа показывает в окне \
«Обучение» (урок, задачи, мини-экзамен в каждой теме, финальный экзамен), и передавать \
Ноа распоряжения как сказанные голосом. Перед созданием курса вызови course_format. \
Большой курс создавай по частям: create_course с первой темой, потом add_topic.";

/// Описание инструментов для клиента.
fn tools() -> Value {
    let course_schema = json!({
        "type": "object",
        "description": "Курс в формате из course_format",
    });
    json!([
        {
            "name": "course_format",
            "description": "Формат курса обучения Ноа с примером. Вызови перед create_course.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_course",
            "description": "Создать или заменить курс обучения. Ноа проверит его и покажет в окне «Обучение». Возвращает ошибки проверки, если они есть.",
            "inputSchema": {
                "type": "object",
                "properties": { "course": course_schema },
                "required": ["course"]
            }
        },
        {
            "name": "add_topic",
            "description": "Добавить тему в свой курс или заменить тему с тем же id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "course": { "type": "string", "description": "id курса" },
                    "topic": { "type": "object", "description": "Тема в формате из course_format" }
                },
                "required": ["course", "topic"]
            }
        },
        {
            "name": "delete_course",
            "description": "Удалить свой курс (встроенные курсы удалить нельзя). Прогресс по нему остаётся.",
            "inputSchema": {
                "type": "object",
                "properties": { "course": { "type": "string" } },
                "required": ["course"]
            }
        },
        {
            "name": "list_courses",
            "description": "Курсы Ноа: встроенные и свои, с темами и прогрессом пользователя.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "ask_noa",
            "description": "Передать работающей Ноа фразу, как будто её сказали вслух: «напомни завтра в 10 позвонить в банк», «покажи мои активы», «поставь таймер на 20 минут». Ноа ответит вслух на компьютере; сюда ответ не возвращается.",
            "inputSchema": {
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }
        }
    ])
}

/// Вызов инструмента. Ошибка — тоже ответ, с отметкой isError.
fn call(name: &str, args: &Value) -> Value {
    let outcome: Result<String, String> = match name {
        "course_format" => Ok(crate::learning::FORMAT.to_string()),
        "create_course" => serde_json::from_value(args["course"].clone())
            .map_err(|err| format!("Курс не разобрался: {err}"))
            .and_then(crate::learning::save_course),
        "add_topic" => serde_json::from_value(args["topic"].clone())
            .map_err(|err| format!("Тема не разобралась: {err}"))
            .and_then(|topic| crate::learning::add_topic(args["course"].as_str().unwrap_or_default(), topic)),
        "delete_course" => crate::learning::delete_course(args["course"].as_str().unwrap_or_default()),
        "list_courses" => Ok(list_courses()),
        "ask_noa" => ask_noa(args["text"].as_str().unwrap_or_default()),
        other => Err(format!("Нет инструмента «{other}».")),
    };
    match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
    }
}

fn list_courses() -> String {
    let cards = crate::learning::overview();
    if cards.is_empty() {
        return "Курсов нет.".into();
    }
    cards
        .iter()
        .map(|card| {
            let topics = card
                .topics
                .iter()
                .map(|topic| {
                    let exam = topic
                        .exam_best
                        .map(|best| format!(", экзамен {best}%"))
                        .unwrap_or_default();
                    format!(
                        "  - {} ({}): {}, задачи {}/{}{exam}, ошибок {}",
                        topic.title, topic.id, topic.status, topic.tasks_done, topic.tasks_total, topic.mistakes
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{} (id: {}) — пройдено {}%\n{}",
                card.title, card.id, card.percent, topics
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Передаёт фразу работающей Ноа — тем же путём, что `sufler.exe --ask`.
fn ask_noa(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Пустая фраза.".into());
    }
    if !crate::instance::running() {
        return Err("Ноа не запущена — фразу передать некому.".into());
    }
    crate::instance::request_ask(text);
    Ok(format!("Передал Ноа: «{text}». Она ответит вслух на компьютере."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_is_answered() {
        let reply = handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t" } }
        }))
        .expect("ответ");
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply["result"]["serverInfo"]["name"], "noa");
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).expect("список");
        assert!(tools["result"]["tools"].as_array().is_some_and(|list| list.len() >= 5));
        let unknown = handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" })).expect("ошибка");
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn the_format_is_given() {
        let reply = call("course_format", &json!({}));
        let text = reply["content"][0]["text"].as_str().unwrap_or_default();
        assert!(text.contains("\"lesson\""));
    }
}
