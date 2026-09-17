//! Свои модули: папка с `module.json`, инструменты — через MCP.
//!
//! Формат, правила и проверка — в `module_kit`. Здесь — жизнь модулей внутри
//! работающей Ноа: запуск только проверенных версий, проверка новых и
//! изменённых, перезапуск упавших серверов, ключи, вызовы инструментов.
//!
//! Модули лежат в `%APPDATA%\app.sufler.popup\modules\<id>\`. Ноа запускает
//! MCP-сервер каждого модуля, спрашивает у него инструменты и отдаёт их
//! разбору реплик: подходящая фраза становится вызовом инструмента, а его
//! ответ Ноа пересказывает голосом. Библиотека модулей — `modules/index.json`
//! в репозитории программы.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

pub use crate::module_kit::{valid_id, Manifest, McpSpec};
use crate::module_kit::{self as kit, Server};

const LIBRARY_URL: &str = "https://raw.githubusercontent.com/faafaafuu/asis/main/modules/index.json";
/// Сколько падений терпим, прежде чем перестать перезапускать.
const CRASH_LIMIT: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(600);
/// Описание, изменённое только что, ещё может дописываться.
const SETTLE: Duration = Duration::from_secs(3);

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

struct Running {
    server: Arc<Mutex<Server>>,
    dir: PathBuf,
    manifest: Manifest,
    fingerprint: String,
}

#[derive(Default)]
struct Health {
    crashes: Vec<Instant>,
    /// Версия, которая не прошла проверку или падает, — её не трогаем, пока
    /// не изменится или не пройдёт проверку заново (`check_module`).
    given_up: Option<(String, Option<std::time::SystemTime>)>,
    checking: bool,
}

static RUNNING: Mutex<Option<HashMap<String, Running>>> = Mutex::new(None);
static TOOLS: Mutex<Vec<Tool>> = Mutex::new(Vec::new());
static STATUS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static HEALTH: Mutex<Option<HashMap<String, Health>>> = Mutex::new(None);
/// Описания запущенных модулей — для правил разбора.
static MANIFESTS: Mutex<Option<BTreeMap<String, Manifest>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

fn health<R>(id: &str, f: impl FnOnce(&mut Health) -> R) -> R {
    f(lock(&HEALTH).get_or_insert_with(HashMap::new).entry(id.to_string()).or_default())
}

/// Состояние модуля: строка для плитки и файл для нейросети.
fn set_status(dir: &Path, id: &str, state: &str, text: &str) {
    lock(&STATUS).get_or_insert_with(HashMap::new).insert(id.to_string(), text.to_string());
    kit::write_status(dir, state, text);
}

pub fn status(id: &str) -> String {
    lock(&STATUS)
        .as_ref()
        .and_then(|all| all.get(id).cloned())
        .unwrap_or_else(|| "проверяется…".into())
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("modules"))
        .map_err(|err| err.to_string())
}

fn module_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("нет такого модуля".into());
    }
    Ok(root(app)?.join(id))
}

/// Установленные модули.
pub fn installed(app: &AppHandle) -> Vec<Manifest> {
    let Ok(root) = root(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut list: Vec<Manifest> = entries
        .flatten()
        .filter_map(|entry| kit::read_manifest(&entry.path()))
        .filter(|manifest| valid_id(&manifest.id))
        .collect();
    list.sort_by(|a, b| a.title.cmp(&b.title));
    list
}

/// Запускает проверенную версию модуля и берёт у него инструменты.
fn start(dir: &Path, manifest: &Manifest, fingerprint: &str) -> Result<usize, String> {
    stop(&manifest.id);
    let secrets = kit::load_secrets(dir);
    let mut server = Server::spawn(dir, manifest, &secrets)?;
    let listed = server.handshake(manifest.start_timeout())?;
    let tools: Vec<Tool> = listed
        .iter()
        .filter_map(|tool| {
            Some(Tool {
                module: manifest.id.clone(),
                name: tool["name"].as_str()?.to_string(),
                description: tool["description"].as_str().unwrap_or_default().to_string(),
                schema: tool["inputSchema"].clone(),
            })
        })
        .collect();
    let count = tools.len();
    {
        let mut all = lock(&TOOLS);
        all.retain(|tool| tool.module != manifest.id);
        all.extend(tools);
    }
    lock(&RUNNING).get_or_insert_with(HashMap::new).insert(
        manifest.id.clone(),
        Running {
            server: Arc::new(Mutex::new(server)),
            dir: dir.to_path_buf(),
            manifest: manifest.clone(),
            fingerprint: fingerprint.to_string(),
        },
    );
    lock(&MANIFESTS)
        .get_or_insert_with(BTreeMap::new)
        .insert(manifest.id.clone(), manifest.clone());
    Ok(count)
}

fn stop(id: &str) {
    let running = lock(&RUNNING).as_mut().and_then(|all| all.remove(id));
    if let Some(running) = running {
        lock(&running.server).kill();
    }
    lock(&TOOLS).retain(|tool| tool.module != id);
    if let Some(all) = lock(&MANIFESTS).as_mut() {
        all.remove(id);
    }
}

fn start_logged(dir: &Path, manifest: &Manifest, fingerprint: &str) {
    set_status(dir, &manifest.id, "starting", "запускается…");
    match start(dir, manifest, fingerprint) {
        Ok(count) => {
            log::info!("модуль «{}»: инструментов {count}", manifest.title);
            set_status(dir, &manifest.id, "running", &format!("работает · инструментов: {count}"));
        }
        Err(err) => {
            log::warn!("модуль «{}» не запустился: {err}", manifest.title);
            health(&manifest.id, |h| h.crashes.push(Instant::now()));
            set_status(dir, &manifest.id, "crashed", &format!("не запустился: {err}"));
        }
    }
}

/// Проверяет модуль и, если он прошёл, запускает.
fn check_and_start(dir: PathBuf, manifest: Manifest) {
    let id = manifest.id.clone();
    health(&id, |h| h.checking = true);
    set_status(&dir, &id, "checking", "проверяется…");
    let _ = std::thread::Builder::new().name(format!("sufler-check-{id}")).spawn(move || {
        let report = kit::check(&dir, &manifest, &kit::load_secrets(&dir));
        health(&id, |h| h.checking = false);
        if report.ok && kit::dir_fingerprint(&dir).as_deref() == Some(report.fingerprint.as_str()) {
            let _ = kit::write_checked(&dir, &report.fingerprint, &report.tools);
            log::info!("модуль «{}» прошёл проверку", manifest.title);
            start_logged(&dir, &manifest, &report.fingerprint);
        } else if !report.ok {
            let first = report.failed.first().cloned().unwrap_or_default();
            log::warn!("модуль «{}» не прошёл проверку: {first}", manifest.title);
            health(&id, |h| h.given_up = Some((report.fingerprint.clone(), kit::checked_at(&dir))));
            set_status(&dir, &id, "failed", &format!("не прошёл проверку: {first}"));
        }
    });
}

/// Один проход наблюдателя по папке модулей.
fn tick(app: &AppHandle) {
    let Ok(root) = root(app) else { return };
    let mut present = Vec::new();
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let dir = entry.path();
        if kit::is_updating(&dir) {
            continue;
        }
        let Some(manifest) = kit::read_manifest(&dir) else { continue };
        let id = manifest.id.clone();
        if !valid_id(&id) || dir.file_name().and_then(|n| n.to_str()) != Some(id.as_str()) {
            continue;
        }
        present.push(id.clone());
        let Some(fingerprint) = kit::dir_fingerprint(&dir) else { continue };

        // Запущен — жив ли и та ли версия.
        let running = lock(&RUNNING)
            .as_ref()
            .and_then(|all| all.get(&id).map(|r| (r.fingerprint.clone(), r.server.clone())));
        if let Some((running_fp, server)) = running {
            if running_fp != fingerprint {
                log::info!("модуль «{}» изменён — останавливаю до проверки", manifest.title);
                stop(&id);
            } else {
                // Сервер занят вызовом — значит жив; проверим в следующий раз.
                let alive = server.try_lock().map(|mut s| s.alive()).unwrap_or(true);
                if alive {
                    // Отметку «запускается» могла оставить повторная проверка.
                    if kit::read_status(&dir).is_none_or(|s| s.state != "running") {
                        let count = lock(&TOOLS).iter().filter(|tool| tool.module == id).count();
                        set_status(&dir, &id, "running", &format!("работает · инструментов: {count}"));
                    }
                    continue;
                }
                stop(&id);
                let crashes = health(&id, |h| {
                    h.crashes.push(Instant::now());
                    h.crashes.retain(|at| at.elapsed() < CRASH_WINDOW);
                    h.crashes.len()
                });
                let tail = kit::log_tail(&dir, 3);
                log::warn!("модуль «{}» упал ({crashes}-й раз): {tail}", manifest.title);
                if crashes >= CRASH_LIMIT {
                    health(&id, |h| h.given_up = Some((fingerprint.clone(), kit::checked_at(&dir))));
                    set_status(&dir, &id, "crashed", &format!("падает: {}", tail.lines().last().unwrap_or("")));
                } else {
                    start_logged(&dir, &manifest, &fingerprint);
                }
                continue;
            }
        }

        let settled = std::fs::metadata(dir.join(kit::MANIFEST_FILE))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| at.elapsed().ok())
            .is_some_and(|age| age >= SETTLE);
        let (checking, given_up) = health(&id, |h| (h.checking, h.given_up.clone()));
        let stuck = given_up.is_some_and(|(fp, at)| fp == fingerprint && at == kit::checked_at(&dir));
        if !settled || checking || stuck {
            continue;
        }
        let missing: Vec<String> = manifest
            .missing_secrets(&kit::load_secrets(&dir))
            .iter()
            .map(|s| s.title.clone())
            .collect();
        if !missing.is_empty() {
            let text = format!("нужен ключ: {}", missing.join(", "));
            if status(&id) != text {
                set_status(&dir, &id, "needs_secret", &text);
            }
            continue;
        }
        if kit::is_verified(&dir) {
            start_logged(&dir, &manifest, &fingerprint);
        } else {
            check_and_start(dir, manifest);
        }
    }

    let gone: Vec<String> = lock(&RUNNING)
        .as_ref()
        .map(|all| all.keys().filter(|id| !present.contains(id)).cloned().collect())
        .unwrap_or_default();
    for id in gone {
        log::info!("модуль «{id}» удалён — останавливаю");
        stop(&id);
        lock(&STATUS).as_mut().map(|all| all.remove(&id));
    }
}

/// Следит за папкой модулей всё время работы программы.
///
/// Модуль может появиться не из окна: его ставит нейросеть пользователя через
/// `sufler.exe --mcp` — отдельный процесс. Поэтому папка проверяется раз в
/// несколько секунд: новый или изменённый модуль проверяется и запускается,
/// удалённый — останавливается, упавший — перезапускается.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("sufler-modules".into()).spawn(move || loop {
        tick(&app);
        std::thread::sleep(Duration::from_secs(3));
    });
}

/// Инструменты всех запущенных модулей.
pub fn tools() -> Vec<Tool> {
    lock(&TOOLS).clone()
}

/// Вызывает инструмент `модуль.инструмент`. Возвращает текст ответа.
///
/// Упавший сервер поднимается заново, и вызов повторяется один раз: человек не
/// должен узнавать о падении модуля, если его можно пережить.
pub fn call(full_name: &str, args: &Value) -> Result<String, String> {
    let (module, name) = full_name.split_once('.').ok_or("неполное имя инструмента")?;
    let running = |module: &str| {
        lock(&RUNNING)
            .as_ref()
            .and_then(|all| all.get(module).map(|r| (r.server.clone(), r.dir.clone(), r.manifest.clone(), r.fingerprint.clone())))
    };
    let (server, dir, manifest, fingerprint) =
        running(module).ok_or_else(|| format!("модуль «{module}» сейчас не работает: {}", status(module)))?;
    let first = {
        let mut server = lock(&server);
        if server.alive() {
            server.call(name, args, kit::CALL_TIMEOUT)
        } else {
            Err("сервер модуля закрылся".into())
        }
    };
    match first {
        Err(err) if err.contains("закрылся") => {
            log::warn!("модуль «{module}» упал во время вызова — перезапускаю");
            health(module, |h| h.crashes.push(Instant::now()));
            start(&dir, &manifest, &fingerprint)?;
            let (server, ..) = running(module).ok_or("модуль не поднялся")?;
            let mut server = lock(&server);
            server.call(name, args, kit::CALL_TIMEOUT)
        }
        other => other,
    }
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

/// Ставит модуль: пишет описание. Проверит и запустит его наблюдатель.
pub fn install(app: &AppHandle, manifest: Manifest) -> Result<(), String> {
    let problems = kit::lint(&manifest, &BTreeMap::new());
    if let Some(problem) = problems.first() {
        return Err(problem.clone());
    }
    let dir = module_dir(app, &manifest.id)?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let text = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
    std::fs::write(dir.join(kit::MANIFEST_FILE), text).map_err(|err| err.to_string())?;
    health(&manifest.id, |h| *h = Health::default());
    set_status(&dir, &manifest.id, "checking", "проверяется…");
    log::info!("модуль «{}» установлен", manifest.title);
    Ok(())
}

/// Ставит модуль с площадки: описание и файлы. Проверит его наблюдатель.
pub fn install_package(app: &AppHandle, manifest: Manifest, files: BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let dir = module_dir(app, &manifest.id)?;
    let updating = kit::Updating::start(&dir)?;
    kit::replace_files(&dir, &files)?;
    let _ = std::fs::remove_file(dir.join(kit::CHECKED_FILE));
    kit::write_manifest(&dir, &manifest)?;
    drop(updating);
    stop(&manifest.id);
    health(&manifest.id, |h| *h = Health::default());
    set_status(&dir, &manifest.id, "checking", "проверяется…");
    log::info!("модуль «{}» поставлен с площадки", manifest.title);
    Ok(())
}

/// Удаляет модуль вместе с папкой.
pub fn uninstall(app: &AppHandle, id: &str) -> Result<(), String> {
    let dir = module_dir(app, id)?;
    stop(id);
    lock(&STATUS).as_mut().map(|all| all.remove(id));
    // Процесс сервера отпускает файлы не мгновенно.
    for _ in 0..10 {
        if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("папка модуля ещё занята — попробуйте через несколько секунд".into())
}

/// Ключ модуля для окна: значение не показывается, только есть ли оно.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretField {
    pub name: String,
    pub title: String,
    pub hint: String,
    pub optional: bool,
    pub set: bool,
}

pub fn secret_fields(app: &AppHandle, id: &str) -> Result<Vec<SecretField>, String> {
    let dir = module_dir(app, id)?;
    let manifest = kit::read_manifest(&dir).ok_or("нет такого модуля")?;
    let have = kit::load_secrets(&dir);
    Ok(manifest
        .secrets
        .into_iter()
        .map(|secret| SecretField {
            set: have.get(&secret.name).is_some_and(|v| !v.is_empty()),
            name: secret.name,
            title: secret.title,
            hint: secret.hint,
            optional: secret.optional,
        })
        .collect())
}

/// Сохраняет ключи и перезапускает модуль с ними.
pub fn save_secret_values(app: &AppHandle, id: &str, values: BTreeMap<String, String>) -> Result<(), String> {
    let dir = module_dir(app, id)?;
    let manifest = kit::read_manifest(&dir).ok_or("нет такого модуля")?;
    let known: Vec<&String> = manifest.secrets.iter().map(|s| &s.name).collect();
    if let Some(unknown) = values.keys().find(|name| !known.contains(name)) {
        return Err(format!("у модуля нет ключа {unknown}"));
    }
    kit::save_secrets(&dir, &values)?;
    stop(id);
    health(id, |h| *h = Health::default());
    set_status(&dir, id, "checking", "проверяется…");
    Ok(())
}

/// Проверить модуль заново — после правки руками или по кнопке.
pub fn recheck(app: &AppHandle, id: &str) -> Result<(), String> {
    let dir = module_dir(app, id)?;
    stop(id);
    let _ = std::fs::remove_file(dir.join(kit::CHECKED_FILE));
    health(id, |h| *h = Health::default());
    set_status(&dir, id, "checking", "проверяется…");
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
    use serde_json::json;

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
            let problems = kit::lint(&manifest, &BTreeMap::new());
            assert!(problems.is_empty(), "{}: {problems:?}", manifest.id);
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
