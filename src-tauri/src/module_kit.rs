//! Регламент модулей: формат, правила и проверка перед запуском.
//!
//! Модуль пишет не разработчик, а нейросеть пользователя по его словам. Поэтому
//! Ноа не верит модулю на слово: прежде чем модуль станет частью Ноа, он
//! проходит проверку — статические правила описания и файлов, затем живой
//! запуск: рукопожатие MCP, список инструментов, тесты из описания, реакция на
//! неверный вызов. Не прошёл — не запускается, а нейросеть получает отчёт с
//! тем, что именно исправить. Прошёл — отпечаток его файлов записывается в
//! `checked.json`, и запускается ровно эта версия: любая правка без новой
//! проверки модуль останавливает.
//!
//! Здесь нет ничего от окна программы: те же проверки выполняет и
//! `sufler.exe --mcp`, и работающая Ноа.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Версия регламента. Растёт, когда меняются правила.
pub const FORMAT: u32 = 1;
pub const PROTOCOL: &str = "2024-11-05";

pub const MANIFEST_FILE: &str = "module.json";
pub const CHECKED_FILE: &str = "checked.json";
pub const SECRETS_FILE: &str = "secrets.json";
pub const STATUS_FILE: &str = "status.json";
pub const LOG_FILE: &str = "server.log";
/// Метка «модуль переписывается»: пока она есть, Ноа папку не трогает.
pub const UPDATING_FILE: &str = ".updating";
/// Служебные файлы: их не пишет нейросеть и они не входят в отпечаток.
const SERVICE_FILES: &[&str] = &[CHECKED_FILE, SECRETS_FILE, STATUS_FILE, LOG_FILE, UPDATING_FILE];

/// Идёт ли запись модуля. Метка старше минуты — след оборванной записи.
pub fn is_updating(dir: &Path) -> bool {
    std::fs::metadata(dir.join(UPDATING_FILE))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(60))
}

/// Когда модуль последний раз прошёл проверку.
pub fn checked_at(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join(CHECKED_FILE)).and_then(|meta| meta.modified()).ok()
}

/// Чем можно запускать сервер модуля. Остальное — только файлом из папки модуля.
const RUNTIMES: &[&str] = &["node", "npx", "python", "python3", "py", "uv", "uvx", "deno", "bun"];
/// Какие файлы может положить нейросеть. Исполняемых среди них нет.
const FILE_TYPES: &[&str] = &[
    "js", "mjs", "cjs", "ts", "py", "json", "txt", "md", "csv", "yaml", "yml", "toml", "html", "css",
];
const MAX_FILES: usize = 40;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 5 * 1024 * 1024;
const MAX_TOOLS: usize = 25;
const MAX_ANSWER_CHARS: usize = 4000;

/// Первый запуск через npx скачивает пакет — это не секунды.
pub const START_TIMEOUT_PACKAGE: Duration = Duration::from_secs(180);
pub const START_TIMEOUT_FILES: Duration = Duration::from_secs(30);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const TEST_TIMEOUT: Duration = Duration::from_secs(25);
const SLOW_TEST: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct McpSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Ключ, который модулю нужен от пользователя. Значение вводит человек в окне
/// Ноа; нейросети оно не показывается, модулю приходит переменной окружения.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SecretSpec {
    pub name: String,
    pub title: String,
    pub hint: String,
    pub optional: bool,
}

/// Проверка инструмента: вызов с аргументами и что должно быть в ответе.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TestCase {
    pub tool: String,
    pub args: Value,
    /// Подстрока, которая должна быть в ответе (без учёта регистра).
    pub expect: String,
    pub about: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub about: String,
    pub voice: String,
    pub version: String,
    pub mcp: McpSpec,
    pub secrets: Vec<SecretSpec>,
    pub tests: Vec<TestCase>,
    /// Инструменты, которые нельзя вызывать в проверке (отправляют, платят,
    /// удаляют), — с причиной.
    pub untested: BTreeMap<String, String>,
}

impl Manifest {
    /// Модуль из пакета (npx, uvx) без своих файлов.
    pub fn is_package(&self) -> bool {
        !self.mcp.args.iter().any(|arg| arg.contains("%MODULE_DIR%"))
            && !self.mcp.command.contains("%MODULE_DIR%")
    }

    pub fn start_timeout(&self) -> Duration {
        if self.is_package() {
            START_TIMEOUT_PACKAGE
        } else {
            START_TIMEOUT_FILES
        }
    }

    pub fn missing_secrets(&self, have: &BTreeMap<String, String>) -> Vec<&SecretSpec> {
        self.secrets
            .iter()
            .filter(|secret| !secret.optional && have.get(&secret.name).is_none_or(|v| v.trim().is_empty()))
            .collect()
    }
}

/// Имя модуля — латиница, цифры и дефис: из него складывается путь к папке.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_secret_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && name.len() <= 48
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Имя файла модуля: без путей и служебных имён, только разрешённых типов.
pub fn valid_file_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    !stem.is_empty()
        && name.len() <= 80
        && !name.starts_with('.')
        && name != MANIFEST_FILE
        && !SERVICE_FILES.contains(&name)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && FILE_TYPES.contains(&ext.to_ascii_lowercase().as_str())
}

/// Знаки, которыми командная строка склеивает команды. В аргументах модуля им
/// делать нечего.
fn has_shell_meta(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '`' | '\n' | '\r' | '"'))
}

fn looks_secret(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASS", "AUTH", "COOKIE"]
        .iter()
        .any(|mark| key.contains(mark))
}

fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(c)) || c == 'ё')
}

/// Статические правила: описание и файлы, без запуска.
pub fn lint(manifest: &Manifest, files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut need = |ok: bool, text: &str| {
        if !ok {
            problems.push(text.to_string());
        }
    };
    let chars = |text: &str| text.trim().chars().count();

    need(valid_id(&manifest.id), "id — латиница в нижнем регистре, цифры и дефис, до 40 знаков, не с дефиса.");
    need((1..=40).contains(&chars(&manifest.title)), "title — название на плитке, 1–40 знаков.");
    need((1..=2).contains(&chars(&manifest.icon)), "icon — один символ (эмодзи или знак).");
    need((5..=140).contains(&chars(&manifest.about)), "about — что умеет модуль, одной фразой, 5–140 знаков.");
    need(
        chars(&manifest.voice) >= 5,
        "voice — пример фразы пользователя, например «Ноа, какая погода в Казани».",
    );

    let spec = &manifest.mcp;
    let command = spec.command.trim();
    let own_file = command.starts_with("%MODULE_DIR%");
    need(!command.is_empty(), "mcp.command — чем запускать сервер.");
    if !command.is_empty() {
        need(
            own_file || RUNTIMES.contains(&command),
            &format!(
                "mcp.command «{command}» не разрешён: только {} или файл модуля через %MODULE_DIR%.",
                RUNTIMES.join(", ")
            ),
        );
    }
    need(
        !has_shell_meta(command) && spec.args.iter().all(|arg| !has_shell_meta(arg)),
        "В mcp.command и mcp.args нельзя использовать & | < > ^ ` и кавычки.",
    );
    need(spec.args.len() <= 20, "mcp.args — не больше 20 аргументов.");

    for (key, value) in &spec.env {
        need(
            valid_secret_name(key),
            &format!("mcp.env: имя «{key}» — заглавная латиница, цифры и подчёркивание."),
        );
        need(
            !looks_secret(key) || value.trim().is_empty(),
            &format!("mcp.env.{key} похоже на ключ: ключи не пишутся в module.json, их объявляют в secrets."),
        );
    }

    let mut names = Vec::new();
    for secret in &manifest.secrets {
        need(
            valid_secret_name(&secret.name),
            &format!("secrets: имя «{}» — заглавная латиница, цифры и подчёркивание.", secret.name),
        );
        need(
            !secret.title.trim().is_empty(),
            &format!("secrets.{}: title — как ключ называется для человека.", secret.name),
        );
        need(
            !secret.hint.trim().is_empty(),
            &format!("secrets.{}: hint — где человеку взять этот ключ.", secret.name),
        );
        need(!names.contains(&&secret.name), &format!("secrets: имя «{}» повторяется.", secret.name));
        need(
            !spec.env.contains_key(&secret.name),
            &format!("secrets.{} совпадает с именем в mcp.env.", secret.name),
        );
        names.push(&secret.name);
    }

    if !manifest.is_package() {
        need(
            !manifest.tests.is_empty(),
            "tests — нужен хотя бы один тест: {\"tool\": …, \"args\": {…}, \"expect\": …}.",
        );
    }
    for (at, test) in manifest.tests.iter().enumerate() {
        need(!test.tool.trim().is_empty(), &format!("tests[{at}].tool — имя инструмента."));
        need(
            test.args.is_null() || test.args.is_object(),
            &format!("tests[{at}].args — объект аргументов."),
        );
    }

    need(files.len() <= MAX_FILES, &format!("Файлов больше {MAX_FILES}."));
    let mut total = 0;
    for (name, bytes) in files {
        total += bytes.len();
        need(
            valid_file_name(name),
            &format!(
                "Файл «{name}»: только латиница, цифры, точка, дефис и подчёркивание, без папок; типы — {}.",
                FILE_TYPES.join(", ")
            ),
        );
        need(bytes.len() <= MAX_FILE_BYTES, &format!("Файл «{name}» больше 1 МБ."));
        need(!bytes.is_empty(), &format!("Файл «{name}» пустой."));
    }
    need(total <= MAX_TOTAL_BYTES, "Файлы модуля вместе больше 5 МБ.");

    if own_file || !manifest.is_package() {
        for arg in spec.args.iter().chain(std::iter::once(&spec.command)) {
            if let Some(rest) = arg.strip_prefix("%MODULE_DIR%") {
                let name = rest.trim_start_matches(['\\', '/']);
                if !name.is_empty() {
                    need(
                        files.contains_key(name),
                        &format!("mcp ссылается на «{name}», а такого файла в модуле нет."),
                    );
                }
            }
        }
    }
    problems
}

/// Файлы модуля с диска — без служебных.
pub fn read_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return files };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == MANIFEST_FILE || SERVICE_FILES.contains(&name.as_str()) || !entry.path().is_file() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(entry.path()) {
            files.insert(name, bytes);
        }
    }
    files
}

pub fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Отпечаток версии модуля: описание и все его файлы.
pub fn fingerprint(manifest: &Manifest, files: &BTreeMap<String, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(manifest).unwrap_or_default());
    for (name, bytes) in files {
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn dir_fingerprint(dir: &Path) -> Option<String> {
    let manifest = read_manifest(dir)?;
    Some(fingerprint(&manifest, &read_files(dir)))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Checked {
    pub fingerprint: String,
    pub format: u32,
    pub at: String,
    pub tools: Vec<String>,
}

/// Прошла ли проверку именно эта версия модуля.
pub fn is_verified(dir: &Path) -> bool {
    let Some(current) = dir_fingerprint(dir) else { return false };
    std::fs::read_to_string(dir.join(CHECKED_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Checked>(&text).ok())
        .is_some_and(|checked| checked.fingerprint == current && checked.format == FORMAT)
}

pub fn write_checked(dir: &Path, fingerprint: &str, tools: &[String]) -> Result<(), String> {
    let checked = Checked {
        fingerprint: fingerprint.to_string(),
        format: FORMAT,
        at: chrono::Local::now().to_rfc3339(),
        tools: tools.to_vec(),
    };
    let text = serde_json::to_string_pretty(&checked).map_err(|err| err.to_string())?;
    std::fs::write(dir.join(CHECKED_FILE), text).map_err(|err| err.to_string())
}

/* ── Запись модуля ─────────────────────────────────────────────────────── */

/// Файлы из аргумента: имя → содержимое.
pub fn parse_files(files: &Value) -> Result<BTreeMap<String, Vec<u8>>, String> {
    match files {
        Value::Null => Ok(BTreeMap::new()),
        Value::Object(map) => map
            .iter()
            .map(|(name, text)| match text.as_str() {
                Some(text) => Ok((name.clone(), text.as_bytes().to_vec())),
                None => Err(format!("Содержимое «{name}» должно быть строкой.")),
            })
            .collect(),
        _ => Err("files — объект «имя файла → содержимое».".into()),
    }
}

/// Заменяет файлы папки новыми: служебные (ключи, журнал, состояние) остаются.
pub fn replace_files(dir: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    for old in read_files(dir).keys() {
        if !files.contains_key(old) {
            let _ = std::fs::remove_file(dir.join(old));
        }
    }
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).map_err(|err| format!("{name}: {err}"))?;
    }
    Ok(())
}

pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?;
    std::fs::write(dir.join(MANIFEST_FILE), text).map_err(|err| err.to_string())
}

/// Метка записи модуля: снимается при выходе, даже если запись оборвалась.
pub struct Updating(PathBuf);

impl Updating {
    pub fn start(dir: &Path) -> Result<Updating, String> {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        std::fs::write(dir.join(UPDATING_FILE), b"").map_err(|err| err.to_string())?;
        Ok(Updating(dir.join(UPDATING_FILE)))
    }
}

impl Drop for Updating {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/* ── Ключи ─────────────────────────────────────────────────────────────── */

pub fn load_secrets(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(dir.join(SECRETS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| (name, crate::secret::reveal(&value)))
        .collect()
}

/// Сохраняет ключи: пустое значение не трогает сохранённый.
pub fn save_secrets(dir: &Path, values: &BTreeMap<String, String>) -> Result<(), String> {
    let path = dir.join(SECRETS_FILE);
    let mut stored: BTreeMap<String, String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    for (name, value) in values {
        if !valid_secret_name(name) {
            return Err(format!("нет такого ключа: {name}"));
        }
        let value = value.trim();
        if !value.is_empty() {
            stored.insert(name.clone(), crate::secret::protect(value));
        }
    }
    let text = serde_json::to_string_pretty(&stored).map_err(|err| err.to_string())?;
    std::fs::write(path, text).map_err(|err| err.to_string())
}

/* ── Состояние, которое видит нейросеть ───────────────────────────────── */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    /// `running`, `starting`, `needs_secret`, `failed`, `crashed`, `checking`.
    pub state: String,
    pub detail: String,
    pub at: String,
}

pub fn write_status(dir: &Path, state: &str, detail: &str) {
    let status = Status {
        state: state.to_string(),
        detail: detail.to_string(),
        at: chrono::Local::now().to_rfc3339(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&status) {
        let _ = std::fs::write(dir.join(STATUS_FILE), text);
    }
}

pub fn read_status(dir: &Path) -> Option<Status> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(STATUS_FILE)).ok()?).ok()
}

pub fn log_tail(dir: &Path, lines: usize) -> String {
    let mut text = String::new();
    if let Ok(mut file) = std::fs::File::open(dir.join(LOG_FILE)) {
        let _ = file.read_to_string(&mut text);
    }
    let mut tail: Vec<&str> = text.lines().rev().filter(|l| !l.trim().is_empty()).take(lines).collect();
    tail.reverse();
    tail.join("\n")
}

/* ── Сервер ────────────────────────────────────────────────────────────── */

enum Line {
    Json(Value),
    /// Не JSON в выводе — нарушение протокола.
    Garbage(String),
}

pub struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Line>,
    next_id: u64,
    garbage: Vec<String>,
}

/// `%MODULE_DIR%`, ключи и переменные окружения в аргументах.
fn expand(text: &str, dir: &Path, secrets: &BTreeMap<String, String>) -> String {
    let text = text.replace("%MODULE_DIR%", &dir.to_string_lossy());
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match secrets.get(name).cloned().or_else(|| std::env::var(name).ok()) {
            Some(value) if !name.is_empty() => out.push_str(&value),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// Находит программу в PATH так же, как командная строка, но без неё:
/// `npx` — это `npx.cmd`, `node` — `node.exe`.
fn resolve_program(name: &str) -> PathBuf {
    let path = Path::new(name);
    if path.components().count() > 1 || path.extension().is_some() {
        return path.to_path_buf();
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for ext in &exts {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    path.to_path_buf()
}

/// Есть ли программа в системе и какой версии.
pub fn runtime_version(name: &str) -> Option<String> {
    let program = resolve_program(name);
    if !program.is_file() {
        return None;
    }
    let mut command = std::process::Command::new(&program);
    command.arg("--version").stdin(Stdio::null()).stderr(Stdio::piped()).stdout(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
    let line = text.lines().find(|l| !l.trim().is_empty())?.trim().to_string();
    // Заглушка из Microsoft Store печатает приглашение, а не версию.
    (output.status.success() && line.chars().any(|c| c.is_ascii_digit())).then_some(line)
}

impl Server {
    pub fn spawn(dir: &Path, manifest: &Manifest, secrets: &BTreeMap<String, String>) -> Result<Server, String> {
        let spec = &manifest.mcp;
        if spec.command.trim().is_empty() {
            return Err("не указана команда MCP-сервера".into());
        }
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        let log = std::fs::File::create(dir.join(LOG_FILE)).map_err(|err| err.to_string())?;

        let program = resolve_program(&expand(spec.command.trim(), dir, secrets));
        let mut command = std::process::Command::new(&program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
            .args(spec.args.iter().map(|arg| expand(arg, dir, secrets)))
            .envs(spec.env.iter().map(|(key, value)| (key, expand(value, dir, secrets))))
            .envs(secrets.iter())
            .env("NOA_MODULE_DIR", dir)
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log);
        let mut child = command.spawn().map_err(|err| {
            format!("не запустился «{}»: {err}. Установлена ли эта программа?", spec.command)
        })?;
        crate::jobs::adopt(&child);

        let stdin = child.stdin.take().ok_or("нет ввода сервера")?;
        let stdout = child.stdout.take().ok_or("нет вывода сервера")?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let item = match serde_json::from_str::<Value>(&line) {
                    Ok(value) if value.is_object() => Line::Json(value),
                    _ => Line::Garbage(line.chars().take(200).collect()),
                };
                if tx.send(item).is_err() {
                    break;
                }
            }
        });
        Ok(Server { child, stdin, lines, next_id: 0, garbage: Vec::new() })
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{message}").map_err(|_| "сервер модуля закрылся".to_string())?;
        self.stdin.flush().map_err(|_| "сервер модуля закрылся".to_string())
    }

    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!("модуль не ответил на {method} за {} с", timeout.as_secs()))
                }
                Err(_) => return Err("сервер модуля закрылся".into()),
            };
            let reply = match line {
                Line::Json(reply) => reply,
                Line::Garbage(text) => {
                    self.garbage.push(text);
                    continue;
                }
            };
            // Уведомления и ответы на прошлые запросы пропускаем.
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                return Err(error["message"].as_str().unwrap_or("ошибка модуля").to_string());
            }
            return Ok(reply["result"].clone());
        }
    }

    /// Рукопожатие MCP и список инструментов.
    pub fn handshake(&mut self, timeout: Duration) -> Result<Vec<Value>, String> {
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "noa", "version": env!("CARGO_PKG_VERSION") },
            }),
            timeout,
        )?;
        if !init.is_object() {
            return Err("initialize вернул не объект".into());
        }
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?;
        let listed = self.request("tools/list", json!({}), CALL_TIMEOUT)?;
        listed["tools"]
            .as_array()
            .cloned()
            .ok_or_else(|| "tools/list вернул ответ без списка tools".to_string())
    }

    /// Вызов инструмента: текст ответа или текст ошибки.
    pub fn call(&mut self, name: &str, args: &Value, timeout: Duration) -> Result<String, String> {
        let args = if args.is_object() { args.clone() } else { json!({}) };
        let result = self.request("tools/call", json!({ "name": name, "arguments": args }), timeout)?;
        let text = answer_text(&result);
        if result["isError"].as_bool().unwrap_or(false) {
            return Err(if text.is_empty() { "инструмент вернул ошибку".into() } else { text });
        }
        Ok(text)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn answer_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/* ── Проверка ──────────────────────────────────────────────────────────── */

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    pub passed: Vec<String>,
    pub failed: Vec<String>,
    pub warnings: Vec<String>,
    pub tools: Vec<String>,
    pub fingerprint: String,
    pub log: String,
}

impl Report {
    fn pass(&mut self, text: impl Into<String>) {
        self.passed.push(text.into());
    }
    fn fail(&mut self, text: impl Into<String>) {
        self.failed.push(text.into());
    }

    /// Отчёт для нейросети.
    pub fn render(&self, title: &str) -> String {
        let mut out = if self.ok {
            format!("Модуль «{title}» прошёл проверку.\n")
        } else {
            format!("Модуль «{title}» НЕ прошёл проверку — исправь и отправь снова.\n")
        };
        if !self.failed.is_empty() {
            out.push_str("\nОшибки:\n");
            for line in &self.failed {
                out.push_str(&format!("  ✗ {line}\n"));
            }
        }
        if !self.warnings.is_empty() {
            out.push_str("\nЗамечания:\n");
            for line in &self.warnings {
                out.push_str(&format!("  ! {line}\n"));
            }
        }
        if !self.passed.is_empty() {
            out.push_str("\nПройдено:\n");
            for line in &self.passed {
                out.push_str(&format!("  ✓ {line}\n"));
            }
        }
        if !self.ok && !self.log.is_empty() {
            out.push_str(&format!("\nЖурнал сервера (stderr):\n{}\n", self.log));
        }
        out
    }
}

fn check_tool_list(report: &mut Report, tools: &[Value]) {
    if tools.is_empty() {
        report.fail("tools/list пуст: модулю нужен хотя бы один инструмент.");
    }
    if tools.len() > MAX_TOOLS {
        report.fail(format!("Инструментов {} — больше {MAX_TOOLS}: разбейте модуль на несколько.", tools.len()));
    }
    let mut seen = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        if !valid_tool_name(&name) {
            report.fail(format!("Имя инструмента «{name}»: латиница, цифры, _ и -, до 48 знаков."));
            continue;
        }
        if seen.contains(&name) {
            report.fail(format!("Инструмент «{name}» объявлен дважды."));
        }
        let description = tool["description"].as_str().unwrap_or_default();
        if description.trim().chars().count() < 15 {
            report.fail(format!(
                "{name}: description короче 15 знаков — по описанию Ноа решает, когда вызывать инструмент."
            ));
        } else if !has_cyrillic(description) {
            report.warnings.push(format!(
                "{name}: описание не по-русски — фразы пользователя русские, модели проще сопоставить русское описание."
            ));
        }
        let schema = &tool["inputSchema"];
        if schema["type"].as_str() != Some("object") {
            report.fail(format!("{name}: inputSchema должен быть {{\"type\": \"object\", …}}."));
        } else {
            let props = schema["properties"].as_object();
            for required in schema["required"].as_array().into_iter().flatten() {
                let field = required.as_str().unwrap_or_default();
                if !props.is_some_and(|p| p.contains_key(field)) {
                    report.fail(format!("{name}: обязательное поле «{field}» не описано в properties."));
                }
            }
            for (field, value) in props.into_iter().flatten() {
                if value["type"].is_null() && value["enum"].is_null() && value["anyOf"].is_null() {
                    report.warnings.push(format!("{name}.{field}: у поля не указан type."));
                }
            }
        }
        seen.push(name);
    }
    report.tools = seen;
}

/// Полная проверка модуля в папке `dir`: правила, запуск, тесты.
pub fn check(dir: &Path, manifest: &Manifest, secrets: &BTreeMap<String, String>) -> Report {
    let files = read_files(dir);
    let mut report = Report { fingerprint: fingerprint(manifest, &files), ..Default::default() };

    let problems = lint(manifest, &files);
    if problems.is_empty() {
        report.pass("Описание и файлы соответствуют регламенту.");
    }
    for problem in problems {
        report.fail(problem);
    }
    let missing = manifest.missing_secrets(secrets);
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(|s| format!("{} ({})", s.title, s.name)).collect();
        report.fail(format!(
            "Нет ключей: {}. Попроси пользователя ввести их в окне Ноа: Модули → «{}» → «Ключи», затем вызови check_module.",
            names.join(", "),
            manifest.title
        ));
    }
    if !report.failed.is_empty() {
        return report;
    }

    let started = Instant::now();
    let mut server = match Server::spawn(dir, manifest, secrets) {
        Ok(server) => server,
        Err(err) => {
            report.fail(err);
            return report;
        }
    };
    let tools = match server.handshake(manifest.start_timeout()) {
        Ok(tools) => {
            report.pass(format!("Сервер запустился и ответил за {} мс.", started.elapsed().as_millis()));
            tools
        }
        Err(err) => {
            report.fail(format!("Рукопожатие MCP не удалось: {err}."));
            server.kill();
            report.log = log_tail(dir, 20);
            return report;
        }
    };
    check_tool_list(&mut report, &tools);

    for (at, test) in manifest.tests.iter().enumerate() {
        let label = if test.about.is_empty() { format!("тест {} ({})", at + 1, test.tool) } else { test.about.clone() };
        if !report.tools.contains(&test.tool) {
            report.fail(format!("{label}: инструмента «{}» нет в tools/list.", test.tool));
            continue;
        }
        let begun = Instant::now();
        match server.call(&test.tool, &test.args, TEST_TIMEOUT) {
            Ok(text) if text.trim().is_empty() => {
                report.fail(format!("{label}: ответ пустой — Ноа нечего сказать пользователю."))
            }
            Ok(text) if text.chars().count() > MAX_ANSWER_CHARS => report.fail(format!(
                "{label}: ответ длиннее {MAX_ANSWER_CHARS} знаков — сократите, его читают вслух."
            )),
            Ok(text)
                if !test.expect.trim().is_empty()
                    && !text.to_lowercase().contains(&test.expect.trim().to_lowercase()) =>
            {
                report.fail(format!(
                    "{label}: в ответе нет «{}». Ответ: «{}».",
                    test.expect,
                    text.chars().take(300).collect::<String>()
                ))
            }
            Ok(text) => {
                let took = begun.elapsed();
                if took > SLOW_TEST {
                    report.warnings.push(format!("{label}: ответ шёл {} с — пользователь ждёт голосом.", took.as_secs()));
                }
                report.pass(format!(
                    "{label}: «{}» за {} мс.",
                    text.chars().take(120).collect::<String>(),
                    took.as_millis()
                ));
            }
            Err(err) => report.fail(format!("{label}: {err}.")),
        }
    }

    let tested: Vec<&String> = manifest.tests.iter().map(|t| &t.tool).collect();
    for tool in &report.tools.clone() {
        if !tested.contains(&tool) && !manifest.untested.contains_key(tool) && !manifest.is_package() {
            report.fail(format!(
                "Инструмент «{tool}» без теста. Добавьте тест в tests или, если его нельзя вызывать \
                 в проверке (отправляет, платит, удаляет), — причину в untested."
            ));
        }
    }

    // Устойчивость: неизвестный инструмент и пустые аргументы не должны ронять сервер.
    let _ = server.call("__noa_unknown_tool__", &json!({}), Duration::from_secs(8)).map_err(|err| {
        if err.contains("не ответил") {
            report.fail("На вызов несуществующего инструмента сервер не ответил — отвечайте ошибкой.".to_string());
        }
    });
    for tool in report.tools.clone() {
        if manifest.untested.contains_key(&tool) || !server.alive() {
            continue;
        }
        if let Err(err) = server.call(&tool, &json!({}), Duration::from_secs(10)) {
            if err.contains("не ответил") {
                report.fail(format!("{tool} с пустыми аргументами завис — проверяйте аргументы и отвечайте ошибкой."));
            } else if err.contains("закрылся") {
                report.fail(format!(
                    "{tool} с пустыми аргументами уронил сервер — ловите исключения и отвечайте isError."
                ));
            }
        }
    }
    if server.alive() {
        report.pass("Сервер пережил неверные вызовы.");
    } else {
        report.fail("Сервер упал после неверного вызова — ловите исключения и отвечайте isError.");
    }
    if !server.garbage.is_empty() {
        report.fail(format!(
            "В stdout есть строки не в формате JSON-RPC (первая: «{}»). Отладку пишите в stderr.",
            server.garbage[0]
        ));
    }
    server.kill();

    report.ok = report.failed.is_empty();
    if !report.ok {
        report.log = log_tail(dir, 20);
    }
    report
}

/// Путь к папке данных Ноа — без окна программы.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join("app.sufler.popup"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/app.sufler.popup"))
    }
}

pub fn modules_root() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("modules"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather() -> Manifest {
        serde_json::from_value(json!({
            "id": "weather", "title": "Погода", "icon": "☀", "about": "Погода в любом городе",
            "voice": "«Ноа, какая погода в Казани»",
            "mcp": { "command": "node", "args": ["%MODULE_DIR%\\server.mjs"] },
            "tests": [{ "tool": "weather", "args": { "city": "Казань" }, "expect": "Казань" }]
        }))
        .unwrap()
    }

    fn files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([("server.mjs".to_string(), b"console.log(1)".to_vec())])
    }

    #[test]
    fn a_good_manifest_passes_lint() {
        assert!(lint(&weather(), &files()).is_empty(), "{:?}", lint(&weather(), &files()));
    }

    #[test]
    fn lint_catches_what_breaks_modules() {
        let mut bad = weather();
        bad.mcp.command = "cmd".into();
        bad.mcp.args.push("& del C:\\".into());
        bad.mcp.env.insert("API_KEY".into(), "sk-123".into());
        bad.tests.clear();
        let problems = lint(&bad, &BTreeMap::new()).join("\n");
        for part in ["не разрешён", "& | <", "похоже на ключ", "хотя бы один тест", "такого файла в модуле нет"] {
            assert!(problems.contains(part), "{part}: {problems}");
        }
    }

    #[test]
    fn file_names_cannot_escape_or_execute() {
        assert!(valid_file_name("server.mjs"));
        assert!(valid_file_name("data_2.json"));
        for bad in ["..\\x.js", "a/b.js", "run.bat", "tool.exe", ".env", "module.json", "secrets.json", "noext"] {
            assert!(!valid_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn package_modules_do_not_need_tests() {
        let memory: Manifest = serde_json::from_value(json!({
            "id": "memory", "title": "Память", "icon": "◎", "about": "Запоминает факты",
            "voice": "«Ноа, запомни это»",
            "mcp": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"] }
        }))
        .unwrap();
        assert!(memory.is_package());
        assert!(lint(&memory, &BTreeMap::new()).is_empty());
    }

    #[test]
    fn fingerprint_follows_files() {
        let one = fingerprint(&weather(), &files());
        let mut changed = files();
        changed.insert("server.mjs".into(), b"console.log(2)".to_vec());
        assert_ne!(one, fingerprint(&weather(), &changed));
        assert_eq!(one, fingerprint(&weather(), &files()));
    }

    #[test]
    fn secrets_are_expanded_before_environment() {
        let secrets = BTreeMap::from([("WEATHER_KEY".to_string(), "abc".to_string())]);
        let dir = Path::new(r"C:\m");
        assert_eq!(expand("%MODULE_DIR%\\s.mjs", dir, &secrets), r"C:\m\s.mjs");
        assert_eq!(expand("--key=%WEATHER_KEY%", dir, &secrets), "--key=abc");
        assert_eq!(expand("100% готово", dir, &secrets), "100% готово");
    }

    #[test]
    fn missing_required_secrets_are_listed() {
        let mut manifest = weather();
        manifest.secrets = vec![
            SecretSpec { name: "A".into(), title: "A".into(), hint: "h".into(), optional: false },
            SecretSpec { name: "B".into(), title: "B".into(), hint: "h".into(), optional: true },
        ];
        assert_eq!(manifest.missing_secrets(&BTreeMap::new()).len(), 1);
        let have = BTreeMap::from([("A".to_string(), "x".to_string())]);
        assert!(manifest.missing_secrets(&have).is_empty());
    }
}
