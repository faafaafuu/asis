//! Свои модули: папка с `module.json`, инструменты — через MCP.
//!
//! ```json
//! {
//!   "id": "memory",
//!   "title": "Память",
//!   "icon": "◎",
//!   "about": "Запоминает факты и связи между ними",
//!   "voice": "«Ноа, запомни, что встреча с заказчиком — по четвергам»",
//!   "mcp": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"] }
//! }
//! ```
//!
//! Модули лежат в `%APPDATA%\app.sufler.popup\modules\<id>\`. Ноа запускает
//! MCP-сервер каждого модуля, спрашивает у него инструменты и отдаёт их
//! разбору реплик: подходящая фраза становится вызовом инструмента, а его
//! ответ Ноа пересказывает голосом. Библиотека модулей — `modules/index.json`
//! в репозитории программы.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

const LIBRARY_URL: &str = "https://raw.githubusercontent.com/faafaafuu/asis/main/modules/index.json";
const PROTOCOL: &str = "2024-11-05";
/// Первый запуск через npx скачивает пакет — это не секунды.
const START_TIMEOUT: Duration = Duration::from_secs(120);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct McpSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub about: String,
    pub voice: String,
    pub mcp: McpSpec,
}

/// Инструмент модуля — для разбора реплик.
#[derive(Debug, Clone)]
pub struct Tool {
    pub module: String,
    pub name: String,
    pub description: String,
    pub schema: Value,
}

impl Tool {
    /// Полное имя: `модуль.инструмент`.
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

struct Server {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<Value>,
    next_id: u64,
}

static SERVERS: Mutex<Option<HashMap<String, Arc<Mutex<Server>>>>> = Mutex::new(None);
static TOOLS: Mutex<Vec<Tool>> = Mutex::new(Vec::new());
static STATUS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Описания запущенных модулей — для правил разбора.
static MANIFESTS: Mutex<Option<BTreeMap<String, Manifest>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

fn set_status(id: &str, text: String) {
    lock(&STATUS).get_or_insert_with(HashMap::new).insert(id.to_string(), text);
}

pub fn status(id: &str) -> String {
    lock(&STATUS)
        .as_ref()
        .and_then(|all| all.get(id).cloned())
        .unwrap_or_else(|| "запускается…".into())
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("modules"))
        .map_err(|err| err.to_string())
}

/// Имя модуля — латиница, цифры и дефис: из него складывается путь к папке.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Установленные модули.
pub fn installed(app: &AppHandle) -> Vec<Manifest> {
    let Ok(root) = root(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut list: Vec<Manifest> = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("module.json")).ok())
        .filter_map(|text| serde_json::from_str::<Manifest>(&text).ok())
        .filter(|manifest| valid_id(&manifest.id))
        .collect();
    list.sort_by(|a, b| a.title.cmp(&b.title));
    list
}

/// `%USERPROFILE%` и прочие переменные в аргументах — как в командной строке.
fn expand(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => out.push_str(&value),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

impl Server {
    fn send(&mut self, message: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{message}").map_err(|err| format!("сервер модуля закрыт: {err}"))?;
        self.stdin.flush().map_err(|err| err.to_string())
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let reply = self
                .replies
                .recv_timeout(left)
                .map_err(|_| format!("модуль не ответил на {method}"))?;
            // Уведомления и чужие ответы пропускаем.
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                return Err(error["message"].as_str().unwrap_or("ошибка модуля").to_string());
            }
            return Ok(reply["result"].clone());
        }
    }
}

fn spawn(app: &AppHandle, manifest: &Manifest) -> Result<Server, String> {
    let spec = &manifest.mcp;
    if spec.command.trim().is_empty() {
        return Err("не указана команда MCP-сервера".into());
    }
    let dir = root(app)?.join(&manifest.id);
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let log = std::fs::File::create(dir.join("server.log")).map_err(|err| err.to_string())?;

    // npx, uvx и прочие на Windows — это .cmd: без cmd /C их не запустить.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.arg("/C").arg(expand(&spec.command));
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = std::process::Command::new(expand(&spec.command));

    command
        .args(spec.args.iter().map(|arg| expand(arg)))
        .envs(spec.env.iter().map(|(key, value)| (key, expand(value))))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log);
    let mut child = command
        .spawn()
        .map_err(|err| format!("не запустился «{}»: {err}", spec.command))?;
    crate::jobs::adopt(&child);

    let stdin = child.stdin.take().ok_or("нет ввода сервера")?;
    let stdout = child.stdout.take().ok_or("нет вывода сервера")?;
    let (tx, replies) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    Ok(Server { child, stdin, replies, next_id: 0 })
}

/// Запускает модуль и берёт у него инструменты.
fn start(app: &AppHandle, manifest: &Manifest) -> Result<usize, String> {
    stop(&manifest.id);
    let mut server = spawn(app, manifest)?;
    server.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": { "name": "noa", "version": env!("CARGO_PKG_VERSION") },
        }),
        START_TIMEOUT,
    )?;
    server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
    let listed = server.request("tools/list", json!({}), CALL_TIMEOUT)?;
    let tools: Vec<Tool> = listed["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| {
                    Some(Tool {
                        module: manifest.id.clone(),
                        name: tool["name"].as_str()?.to_string(),
                        description: tool["description"].as_str().unwrap_or_default().to_string(),
                        schema: tool["inputSchema"].clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let count = tools.len();
    {
        let mut all = lock(&TOOLS);
        all.retain(|tool| tool.module != manifest.id);
        all.extend(tools);
    }
    lock(&SERVERS)
        .get_or_insert_with(HashMap::new)
        .insert(manifest.id.clone(), Arc::new(Mutex::new(server)));
    lock(&MANIFESTS)
        .get_or_insert_with(BTreeMap::new)
        .insert(manifest.id.clone(), manifest.clone());
    Ok(count)
}

fn stop(id: &str) {
    let server = lock(&SERVERS).as_mut().and_then(|all| all.remove(id));
    if let Some(server) = server {
        let _ = lock(&server).child.kill();
    }
    lock(&TOOLS).retain(|tool| tool.module != id);
    if let Some(all) = lock(&MANIFESTS).as_mut() {
        all.remove(id);
    }
}

fn start_in_background(app: &AppHandle, manifest: Manifest) {
    set_status(&manifest.id, "запускается…".into());
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name(format!("sufler-module-{}", manifest.id))
        .spawn(move || match start(&app, &manifest) {
            Ok(count) => {
                log::info!("модуль «{}»: инструментов {count}", manifest.title);
                set_status(&manifest.id, format!("инструментов: {count}"));
            }
            Err(err) => {
                log::warn!("модуль «{}» не запустился: {err}", manifest.title);
                set_status(&manifest.id, format!("не запустился: {err}"));
            }
        });
}

/// Запускает все установленные модули. Зовётся при старте программы.
pub fn start_all(app: &AppHandle) {
    for manifest in installed(app) {
        start_in_background(app, manifest);
    }
}

/// Инструменты всех запущенных модулей.
pub fn tools() -> Vec<Tool> {
    lock(&TOOLS).clone()
}

/// Вызывает инструмент `модуль.инструмент`. Возвращает текст ответа.
pub fn call(full_name: &str, args: &Value) -> Result<String, String> {
    let (module, name) = full_name.split_once('.').ok_or("неполное имя инструмента")?;
    let server = lock(&SERVERS)
        .as_ref()
        .and_then(|all| all.get(module).cloned())
        .ok_or_else(|| format!("модуль «{module}» не запущен"))?;
    let result = lock(&server).request(
        "tools/call",
        json!({ "name": name, "arguments": if args.is_object() { args.clone() } else { json!({}) } }),
        CALL_TIMEOUT,
    )?;
    let text: Vec<String> = result["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let text = text.join("\n");
    if result["isError"].as_bool().unwrap_or(false) {
        return Err(if text.is_empty() { "инструмент вернул ошибку".into() } else { text });
    }
    Ok(text)
}

/// Разбирает командную строку на части с учётом кавычек.
pub fn split_command(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        parts.push(current);
    }
    parts
}

/// Ставит модуль: пишет описание и запускает.
pub fn install(app: &AppHandle, manifest: Manifest) -> Result<(), String> {
    if !valid_id(&manifest.id) {
        return Err("имя модуля — латиница, цифры и дефис".into());
    }
    if manifest.title.trim().is_empty() || manifest.mcp.command.trim().is_empty() {
        return Err("нужны название и команда".into());
    }
    let dir = root(app)?.join(&manifest.id);
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let text = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
    std::fs::write(dir.join("module.json"), text).map_err(|err| err.to_string())?;
    log::info!("модуль «{}» установлен", manifest.title);
    start_in_background(app, manifest);
    Ok(())
}

/// Удаляет модуль вместе с папкой.
pub fn uninstall(app: &AppHandle, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("нет такого модуля".into());
    }
    stop(id);
    let dir = root(app)?.join(id);
    std::fs::remove_dir_all(&dir).map_err(|err| format!("папка модуля не удалилась: {err}"))?;
    lock(&STATUS).as_mut().map(|all| all.remove(id));
    Ok(())
}

/// Библиотека, вшитая в программу: на случай, когда репозиторий недоступен.
const BUILTIN_LIBRARY: &str = include_str!("../../modules/index.json");

async fn fetch_library() -> Option<Value> {
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(15))
        .build()
        .ok()?;
    let response = client.get(LIBRARY_URL).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

/// Библиотека модулей: свежая из репозитория, иначе — вшитая.
pub async fn library() -> Result<Vec<Manifest>, String> {
    let body = match fetch_library().await {
        Some(body) => body,
        None => serde_json::from_str(BUILTIN_LIBRARY).map_err(|err| err.to_string())?,
    };
    Ok(body["modules"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| serde_json::from_value::<Manifest>(item.clone()).ok())
                .filter(|manifest| valid_id(&manifest.id))
                .collect()
        })
        .unwrap_or_default())
}

/// Схема аргументов одной строкой: `{name*: string, tags: [string]}`,
/// звёздочка — обязательное поле. Вложенные объекты раскрываются: без них
/// модель не знала, какие поля нужны внутри массива, и сервер отказывал.
fn compact_schema(schema: &Value, depth: usize) -> String {
    match schema["type"].as_str() {
        Some("object") => {
            let Some(props) = schema["properties"].as_object() else {
                return "object".into();
            };
            if depth > 3 {
                return "object".into();
            }
            let required: Vec<&str> = schema["required"]
                .as_array()
                .map(|list| list.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let fields: Vec<String> = props
                .iter()
                .map(|(key, value)| {
                    let mark = if required.contains(&key.as_str()) { "*" } else { "" };
                    format!("{key}{mark}: {}", compact_schema(value, depth + 1))
                })
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
        Some("array") => format!("[{}]", compact_schema(&schema["items"], depth + 1)),
        Some(kind) => kind.to_string(),
        None => match schema["enum"].as_array() {
            Some(values) => values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("|"),
            None => "any".into(),
        },
    }
}

/// Инструменты для правил разбора — по модулям: что модуль умеет, пример
/// фразы, инструменты с описанием и схемой аргументов.
pub fn rules_section() -> String {
    let tools = tools();
    if tools.is_empty() {
        return String::new();
    }
    let manifests = lock(&MANIFESTS).clone().unwrap_or_default();
    let mut blocks = Vec::new();
    for (id, manifest) in &manifests {
        let lines: Vec<String> = tools
            .iter()
            .filter(|tool| &tool.module == id)
            .map(|tool| {
                let about: String = tool.description.chars().take(160).collect();
                format!(
                    "  - {} — {about} Аргументы: {}",
                    tool.full_name(),
                    compact_schema(&tool.schema, 0)
                )
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        blocks.push(format!(
            "Модуль «{}» — {}. Пример: {}\n{}",
            manifest.title,
            manifest.about,
            manifest.voice,
            lines.join("\n")
        ));
    }
    format!(
        "Свои модули. Если человек просит то, что делает инструмент модуля, или \
         спрашивает о том, что модуль хранит, intent — tool, поле tool — полное \
         имя инструмента, args — объект аргументов по схеме (* — обязательное \
         поле, заполни все обязательные):\n{}",
        blocks.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_is_split_with_quotes() {
        assert_eq!(
            split_command(r#"npx -y @modelcontextprotocol/server-filesystem "C:\My Docs""#),
            vec!["npx", "-y", "@modelcontextprotocol/server-filesystem", r"C:\My Docs"]
        );
        assert_eq!(split_command("  uvx   mcp-server-time "), vec!["uvx", "mcp-server-time"]);
    }

    #[test]
    fn module_ids_are_checked() {
        assert!(valid_id("memory"));
        assert!(valid_id("my-module-2"));
        assert!(!valid_id("../evil"));
        assert!(!valid_id("Память"));
        assert!(!valid_id(""));
    }

    #[test]
    fn environment_variables_are_expanded() {
        std::env::set_var("SUFLER_TEST_DIR", r"C:\Users\test");
        assert_eq!(expand(r"%SUFLER_TEST_DIR%\Documents"), r"C:\Users\test\Documents");
        assert_eq!(expand("100% готово"), "100% готово");
    }

    #[test]
    fn nested_schema_is_compacted_with_required_marks() {
        let schema = json!({
            "type": "object",
            "required": ["entities"],
            "properties": {
                "entities": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name", "entityType"],
                        "properties": {
                            "name": { "type": "string" },
                            "entityType": { "type": "string" },
                            "observations": { "type": "array", "items": { "type": "string" } }
                        }
                    }
                }
            }
        });
        let compact = compact_schema(&schema, 0);
        assert!(compact.starts_with("{entities*: [{"), "{compact}");
        for part in ["name*: string", "entityType*: string", "observations: [string]"] {
            assert!(compact.contains(part), "{compact}");
        }
    }

    #[test]
    fn builtin_library_is_valid() {
        let body: Value = serde_json::from_str(BUILTIN_LIBRARY).unwrap();
        let modules = body["modules"].as_array().unwrap();
        assert!(!modules.is_empty());
        for item in modules {
            let manifest: Manifest = serde_json::from_value(item.clone()).unwrap();
            assert!(valid_id(&manifest.id), "{}", manifest.id);
            assert!(!manifest.mcp.command.is_empty());
        }
    }

    #[test]
    fn manifest_reads_with_defaults() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"id":"memory","title":"Память","mcp":{"command":"npx","args":["-y","pkg"]}}"#,
        )
        .unwrap();
        assert_eq!(manifest.mcp.args, vec!["-y", "pkg"]);
        assert!(manifest.voice.is_empty());
    }
}
