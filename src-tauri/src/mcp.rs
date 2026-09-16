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
//! - свои модули: формат, создать (описание и файлы сервера), список, удалить.
//!   Нейросеть пользователя пишет MCP-сервер под его задачу, работающая Ноа
//!   замечает новую папку и запускает модуль;
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

const INSTRUCTIONS: &str = "Ноа — голосовой помощник на компьютере пользователя, \
оболочка для модулей. Главное, что здесь можно сделать, — собрать пользователю свой \
модуль под его задачу: напиши небольшой MCP-сервер, положи его через create_module — \
и Ноа запустит его и станет вызывать его инструменты по голосу. Перед этим вызови \
module_format, после — list_modules, чтобы убедиться, что модуль запустился. \
Ещё можно создавать курсы обучения (сначала course_format; большой курс — по частям: \
create_course с первой темой, потом add_topic) и передавать Ноа распоряжения как \
сказанные голосом (ask_noa).";

const MODULE_FORMAT: &str = include_str!("module_format.md");

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
            "name": "module_format",
            "description": "Как устроен модуль Ноа: описание module.json и требования к MCP-серверу модуля, с готовым примером. Вызови перед create_module.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_module",
            "description": "Создать или заменить модуль Ноа: описание и файлы его MCP-сервера. Работающая Ноа запустит модуль сама через несколько секунд, и его инструменты станут доступны голосом.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "module": { "type": "object", "description": "module.json в формате из module_format" },
                    "files": {
                        "type": "object",
                        "description": "Файлы сервера: имя файла → содержимое. Кладутся в папку модуля (%MODULE_DIR%).",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["module"]
            }
        },
        {
            "name": "list_modules",
            "description": "Установленные модули Ноа: id, название, команда запуска и хвост журнала сервера — чтобы понять, почему модуль не запустился.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "delete_module",
            "description": "Удалить модуль Ноа вместе с его папкой.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }
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
        "module_format" => Ok(MODULE_FORMAT.to_string()),
        "create_module" => create_module(&args["module"], &args["files"]),
        "list_modules" => Ok(list_modules()),
        "delete_module" => delete_module(args["id"].as_str().unwrap_or_default()),
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

fn modules_dir() -> Result<PathBuf, String> {
    data_dir()
        .map(|dir| dir.join("modules"))
        .ok_or_else(|| "не нашёл папку данных Ноа".to_string())
}

/// Имя файла сервера — без путей, чтобы запись не вышла из папки модуля.
fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name != "module.json"
        && name != "server.log"
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn create_module(module: &Value, files: &Value) -> Result<String, String> {
    let manifest: crate::plugins::Manifest = serde_json::from_value(module.clone())
        .map_err(|err| format!("module.json не разобрался: {err}"))?;
    if !crate::plugins::valid_id(&manifest.id) {
        return Err("id — латиница в нижнем регистре, цифры и дефис, до 40 знаков.".into());
    }
    if manifest.title.trim().is_empty() || manifest.mcp.command.trim().is_empty() {
        return Err("Нужны title и mcp.command.".into());
    }
    let files: Vec<(String, String)> = match files {
        Value::Null => Vec::new(),
        Value::Object(map) => map
            .iter()
            .map(|(name, text)| match text.as_str() {
                Some(text) if safe_file_name(name) => Ok((name.clone(), text.to_string())),
                Some(_) => Err(format!(
                    "Имя файла «{name}» не подходит: только буквы, цифры, точка, дефис и подчёркивание."
                )),
                None => Err(format!("Содержимое «{name}» должно быть строкой.")),
            })
            .collect::<Result<_, _>>()?,
        _ => return Err("files — объект «имя файла → содержимое».".into()),
    };

    let dir = modules_dir()?.join(&manifest.id);
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    for (name, text) in &files {
        std::fs::write(dir.join(name), text).map_err(|err| format!("{name}: {err}"))?;
    }
    // Описание — последним: по нему Ноа запускает модуль, и файлы сервера к
    // этому моменту уже должны лежать на месте.
    let text = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
    std::fs::write(dir.join("module.json"), text).map_err(|err| err.to_string())?;

    let next = if crate::instance::running() {
        "Ноа запустит его через несколько секунд — проверь list_modules."
    } else {
        "Ноа сейчас не запущена — модуль заработает при следующем запуске."
    };
    Ok(format!("Модуль «{}» сохранён в {}. {next}", manifest.title, dir.display()))
}

fn list_modules() -> String {
    let Ok(entries) = modules_dir().and_then(|root| std::fs::read_dir(root).map_err(|err| err.to_string()))
    else {
        return "Модулей нет.".into();
    };
    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(text) = std::fs::read_to_string(dir.join("module.json")) else { continue };
        let Ok(manifest) = serde_json::from_str::<crate::plugins::Manifest>(&text) else { continue };
        let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
        let mut tail: Vec<&str> = log.lines().rev().take(8).collect();
        tail.reverse();
        let journal = if tail.is_empty() {
            "пусто".to_string()
        } else {
            format!("\n    {}", tail.join("\n    "))
        };
        lines.push(format!(
            "{} (id: {}) — {}\n  запуск: {} {}\n  журнал: {journal}",
            manifest.title,
            manifest.id,
            manifest.about,
            manifest.mcp.command,
            manifest.mcp.args.join(" "),
        ));
    }
    if lines.is_empty() {
        return "Модулей нет.".into();
    }
    let state = if crate::instance::running() {
        "Число инструментов запущенного модуля видно на его плитке в главном окне Ноа."
    } else {
        "Ноа сейчас не запущена."
    };
    format!("{}\n\n{state}", lines.join("\n\n"))
}

fn delete_module(id: &str) -> Result<String, String> {
    if !crate::plugins::valid_id(id) {
        return Err("Нет такого модуля.".into());
    }
    let dir = modules_dir()?.join(id);
    if !dir.join("module.json").exists() {
        return Err(format!("Модуля «{id}» нет."));
    }
    // Сначала описание: Ноа остановит сервер, и его файлы освободятся.
    std::fs::remove_file(dir.join("module.json")).map_err(|err| err.to_string())?;
    for _ in 0..20 {
        if std::fs::remove_dir_all(&dir).is_ok() {
            return Ok(format!("Модуль «{id}» удалён."));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(format!(
        "Модуль «{id}» отключён; папку {} удалите вручную — файлы ещё заняты.",
        dir.display()
    ))
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
