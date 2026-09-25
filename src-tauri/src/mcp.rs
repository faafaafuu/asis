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
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::module_kit::{self as kit, Manifest};

/// Версия протокола, если клиент не назвал свою.
const PROTOCOL: &str = "2024-11-05";

/// Разбирает запросы, пока клиент не закроет ввод.
pub fn serve() {
    if let Some(dir) = kit::data_dir() {
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

const INSTRUCTIONS: &str = "Ноа — мультимодальная оболочка для ИИ на компьютере пользователя. Главное, что здесь делается, — модуль под задачу пользователя: ты пишешь небольшой MCP-сервер, Ноа проверяет его по своему регламенту и ставит, после чего пользователь пользуется им голосом. Порядок строгий: 1) module_format — прочитай регламент целиком; 2) environment — узнай, на чём можно писать; 3) уточни у пользователя задачу, фразы и нужные ключи; 4) create_module; 5) если отчёт с ошибками — исправь и отправь снова, пока проверка не пройдёт; не говори пользователю, что модуль готов, до успешного отчёта. Ещё можно создавать курсы обучения (course_format, create_course, add_topic) и передавать Ноа распоряжения как сказанные голосом (ask_noa).";

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
            "description": "Отправить модуль Ноа: описание и файлы MCP-сервера. Ноа проверяет модуль по регламенту (правила, запуск, тесты, устойчивость) и ставит только прошедший; иначе возвращает отчёт с ошибками — исправь и отправь снова. Прошедший модуль Ноа сразу запускает.",
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
            "name": "environment",
            "description": "Что установлено у пользователя для модулей (Node.js, Python и др.) и запущена ли Ноа. Вызови перед тем, как писать сервер модуля.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "check_module",
            "description": "Проверить установленный модуль заново (например, после того как пользователь ввёл ключи). Возвращает отчёт проверки; прошедший модуль Ноа запускает.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }
        },
        {
            "name": "search_modules",
            "description": "Найти готовые модули на площадке NOAH по словам. Перед тем как писать свой модуль, проверь, нет ли готового.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Что ищем, например «погода»" } }
            }
        },
        {
            "name": "install_module",
            "description": "Поставить модуль с площадки NOAH по id. Ноа проверит его так же, как созданный тобой, и запустит.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }
        },
        {
            "name": "publish_module",
            "description": "Опубликовать проверенный модуль пользователя на площадке NOAH от его имени (нужен ключ площадки в настройках Ноа). Спроси пользователя перед публикацией.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "description": { "type": "string", "description": "Подробное описание для страницы модуля, 1–3 абзаца" },
                    "category": { "type": "string", "enum": ["work", "home", "finance", "dev", "health", "media", "other"] }
                },
                "required": ["id"]
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
        "environment" => Ok(environment()),
        "search_modules" => search_modules(args["query"].as_str().unwrap_or_default()),
        "install_module" => install_module(args["id"].as_str().unwrap_or_default()),
        "publish_module" => tauri::async_runtime::block_on(crate::platform::publish(
            args["id"].as_str().unwrap_or_default(),
            args["description"].as_str().unwrap_or_default(),
            args["category"].as_str().unwrap_or("other"),
        )),
        "check_module" => check_module(args["id"].as_str().unwrap_or_default()),
        "delete_module" => delete_module(args["id"].as_str().unwrap_or_default()),
        "ask_noa" => ask_noa(args["text"].as_str().unwrap_or_default()),
        other => Err(format!("Нет инструмента «{other}».")),
    };
    match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
    }
}

pub(crate) fn list_courses() -> String {
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
    kit::modules_root().ok_or_else(|| "не нашёл папку данных Ноа".to_string())
}

/// Что есть у пользователя для запуска модулей.
pub(crate) fn environment() -> String {
    let mut lines = vec![format!("Ноа {}, регламент модулей v{}.", env!("CARGO_PKG_VERSION"), kit::FORMAT)];
    for name in ["node", "npx", "python", "py", "uvx", "deno", "bun"] {
        match kit::runtime_version(name) {
            Some(version) => lines.push(format!("{name}: {version}")),
            None => lines.push(format!("{name}: нет")),
        }
    }
    lines.push(if noa_running() {
        "Ноа запущена: модуль заработает сразу после проверки.".into()
    } else {
        "Ноа сейчас не запущена: модуль заработает при её запуске.".into()
    });
    lines.push(
        "Пиши сервер под то, что установлено. Если нет ни Node.js, ни Python — попроси пользователя \
         поставить Node.js LTS с nodejs.org и вызови environment снова."
            .into(),
    );
    lines.join("\n")
}

/// Ждёт, пока работающая Ноа запустит модуль, и говорит, чем кончилось.
fn wait_started(dir: &Path) -> String {
    if !noa_running() {
        return "Ноа сейчас не запущена — модуль заработает при её запуске.".into();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        if let Some(status) = kit::read_status(dir) {
            match status.state.as_str() {
                "running" => return format!("Ноа запустила модуль: {}.", status.detail.trim_end_matches('.')),
                "failed" | "crashed" | "needs_secret" => {
                    return format!(
                        "Ноа не запустила модуль: {}.\nЖурнал:\n{}",
                        status.detail,
                        kit::log_tail(dir, 15)
                    )
                }
                _ => last = status.detail,
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    format!("Ноа ещё не запустила модуль ({last}). Проверь позже через list_modules.")
}

pub(crate) fn create_module(module: &Value, files: &Value) -> Result<String, String> {
    let manifest: Manifest =
        serde_json::from_value(module.clone()).map_err(|err| format!("module.json не разобрался: {err}"))?;
    let files = kit::parse_files(files)?;
    let problems = kit::lint(&manifest, &files);
    if !problems.is_empty() {
        let list: Vec<String> = problems.iter().map(|p| format!("  ✗ {p}")).collect();
        return Err(format!(
            "Модуль не принят — нарушен регламент (module_format):\n{}",
            list.join("\n")
        ));
    }

    let root = modules_dir()?;
    let installed = root.join(&manifest.id);
    let secrets = kit::load_secrets(&installed);
    let missing = manifest.missing_secrets(&secrets);

    // Нет ключей — проверить модуль нельзя. Ставим его ожидающим: ключи
    // вводит человек в окне Ноа, и после этого Ноа проверит модуль сама.
    if !missing.is_empty() {
        let updating = kit::Updating::start(&installed)?;
        kit::replace_files(&installed, &files)?;
        let _ = std::fs::remove_file(installed.join(kit::CHECKED_FILE));
        kit::write_manifest(&installed, &manifest)?;
        drop(updating);
        let list: Vec<String> = missing.iter().map(|s| format!("  • {} — {}", s.title, s.hint)).collect();
        return Ok(format!(
            "Модуль «{}» сохранён, но ещё не проверен: нужны ключи.\n{}\n\
             Попроси пользователя открыть Ноа → Модули → «{}» → «Ключи» и ввести их. \
             Сами ключи у пользователя не спрашивай и в чат не записывай. \
             Когда он скажет, что ввёл, вызови check_module(\"{}\").",
            manifest.title,
            list.join("\n"),
            manifest.title,
            manifest.id
        ));
    }

    // Проверка — на черновике: работающая версия модуля не трогается, пока
    // новая не прошла.
    let draft = root.join(".drafts").join(&manifest.id);
    let _ = std::fs::remove_dir_all(&draft);
    kit::replace_files(&draft, &files)?;
    kit::write_manifest(&draft, &manifest)?;
    let report = kit::check(&draft, &manifest, &secrets);
    let text = report.render(&manifest.title);
    if !report.ok {
        return Err(text);
    }

    let updating = kit::Updating::start(&installed)?;
    kit::replace_files(&installed, &files)?;
    kit::write_manifest(&installed, &manifest)?;
    kit::write_checked(&installed, &report.fingerprint, &report.tools)?;
    // Прежнее состояние относится к старой версии — ждём новое.
    kit::write_status(&installed, "starting", "запускается…");
    drop(updating);
    let _ = std::fs::remove_dir_all(&draft);
    let started = wait_started(&installed);
    Ok(format!(
        "{text}\n{started}\nСкажи пользователю, как позвать модуль: {}",
        manifest.voice
    ))
}

fn search_modules(query: &str) -> Result<String, String> {
    let found = tauri::async_runtime::block_on(crate::platform::list(query))?;
    if found.is_empty() {
        return Ok("На площадке ничего не нашлось — можно собрать свой модуль.".into());
    }
    Ok(found
        .iter()
        .take(20)
        .map(|m| {
            format!(
                "{} (id: {}) — {}; автор {}, установок {}",
                m["title"].as_str().unwrap_or_default(),
                m["id"].as_str().unwrap_or_default(),
                m["about"].as_str().unwrap_or_default(),
                m["author"].as_str().unwrap_or_default(),
                m["installs"].as_u64().unwrap_or(0)
            )
        })
        .collect::<Vec<_>>()
        .join("
"))
}

fn install_module(id: &str) -> Result<String, String> {
    let (manifest, files) = tauri::async_runtime::block_on(crate::platform::package(id))?;
    let files: serde_json::Map<String, Value> = files
        .into_iter()
        .map(|(name, bytes)| (name, Value::String(String::from_utf8_lossy(&bytes).into_owned())))
        .collect();
    let manifest = serde_json::to_value(&manifest).map_err(|err| err.to_string())?;
    create_module(&manifest, &Value::Object(files))
}

fn check_module(id: &str) -> Result<String, String> {
    if !kit::valid_id(id) {
        return Err("Нет такого модуля.".into());
    }
    let dir = modules_dir()?.join(id);
    let manifest = kit::read_manifest(&dir).ok_or_else(|| format!("Модуля «{id}» нет."))?;
    let report = kit::check(&dir, &manifest, &kit::load_secrets(&dir));
    let text = report.render(&manifest.title);
    if !report.ok {
        return Err(text);
    }
    // Новая отметка о проверке — Ноа запустит модуль, даже если раньше
    // от него отказалась.
    kit::write_checked(&dir, &report.fingerprint, &report.tools)?;
    kit::write_status(&dir, "starting", "запускается…");
    Ok(format!("{text}\n{}", wait_started(&dir)))
}

fn list_modules() -> String {
    let Ok(entries) = modules_dir().and_then(|root| std::fs::read_dir(root).map_err(|err| err.to_string())) else {
        return "Модулей нет.".into();
    };
    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(manifest) = kit::read_manifest(&dir) else { continue };
        if !kit::valid_id(&manifest.id) {
            continue;
        }
        let state = kit::read_status(&dir)
            .map(|s| format!("{} — {}", s.state, s.detail))
            .unwrap_or_else(|| "ещё не запускался".into());
        let verified = if kit::is_verified(&dir) { "проверен" } else { "не проверен" };
        let tail = kit::log_tail(&dir, 6);
        lines.push(format!(
            "{} (id: {}, {verified}) — {}\n  состояние: {state}\n  запуск: {} {}{}",
            manifest.title,
            manifest.id,
            manifest.about,
            manifest.mcp.command,
            manifest.mcp.args.join(" "),
            if tail.is_empty() { String::new() } else { format!("\n  журнал:\n    {}", tail.replace('\n', "\n    ")) },
        ));
    }
    if lines.is_empty() {
        return "Модулей нет.".into();
    }
    lines.join("\n\n")
}

fn delete_module(id: &str) -> Result<String, String> {
    if !kit::valid_id(id) {
        return Err("Нет такого модуля.".into());
    }
    let dir = modules_dir()?.join(id);
    if !dir.join(kit::MANIFEST_FILE).exists() {
        return Err(format!("Модуля «{id}» нет."));
    }
    // Сначала описание: Ноа остановит сервер, и его файлы освободятся.
    std::fs::remove_file(dir.join(kit::MANIFEST_FILE)).map_err(|err| err.to_string())?;
    for _ in 0..20 {
        if std::fs::remove_dir_all(&dir).is_ok() {
            return Ok(format!("Модуль «{id}» удалён."));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(format!("Модуль «{id}» отключён; папка {} ещё занята и удалится позже.", dir.display()))
}

/// Передаёт фразу работающей Ноа — тем же путём, что `sufler.exe --ask`.
fn ask_noa(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Пустая фраза.".into());
    }
    if !noa_running() {
        return Err("Ноа не запущена — фразу передать некому.".into());
    }
    #[cfg(target_os = "windows")]
    {
        crate::instance::request_ask(text);
        Ok(format!("Передал Ноа: «{text}». Она ответит вслух на компьютере."))
    }
    #[cfg(not(target_os = "windows"))]
    Err("Передать фразу работающей Ноа пока можно только на Windows.".into())
}

/// Запущена ли Ноа: на Windows это видно по её именованному мьютексу.
fn noa_running() -> bool {
    #[cfg(target_os = "windows")]
    return crate::instance::running();
    #[cfg(not(target_os = "windows"))]
    false
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
