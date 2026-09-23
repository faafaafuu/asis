//! Подписки на этом компьютере: Qwen Code, Gemini CLI, Codex, Claude Code.
//!
//! У каждой такой программы свой вход в аккаунт (qwen.ai, Google, ChatGPT,
//! Claude) — человек входит в неё один раз сам, а Ноа зовёт её разовым
//! вопросом, как мост на сервере, только без сервера. Источник в настройках —
//! адрес вида `cli:qwen`; ответ перекладывается в тот же вид, что у
//! OpenAI-совместимых сервисов, и дальше работает обычный разбор.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Сколько ждать ответа: у CLI холодный старт в несколько секунд.
const TIMEOUT: Duration = Duration::from_secs(150);
/// Сколько разговора отдавать: у CLI нет своей сессии, история идёт текстом.
const HISTORY: usize = 12;

/// Программа и где её искать.
struct Engine {
    id: &'static str,
    title: &'static str,
    /// Пакет npm: программа — node-скрипт из его `bin`.
    package: Option<&'static str>,
    /// Или исполняемый файл в PATH.
    exe: Option<&'static str>,
}

const ENGINES: &[Engine] = &[
    Engine { id: "qwen", title: "Qwen Code", package: Some("@qwen-code/qwen-code"), exe: None },
    Engine { id: "gemini", title: "Gemini CLI", package: Some("@google/gemini-cli"), exe: None },
    Engine { id: "codex", title: "Codex", package: Some("@openai/codex"), exe: None },
    Engine { id: "claude", title: "Claude Code", package: None, exe: Some("claude") },
];

pub fn is_cli(endpoint: &str) -> bool {
    endpoint.trim().starts_with("cli:")
}

fn engine(endpoint: &str) -> Option<&'static Engine> {
    let id = endpoint.trim().strip_prefix("cli:")?;
    ENGINES.iter().find(|engine| engine.id == id)
}

#[cfg(windows)]
fn hide(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}
#[cfg(not(windows))]
fn hide(_: &mut Command) {}

/// Папка глобальных пакетов npm — у nvm своя, поэтому спрашиваем сам npm.
fn npm_root() -> Option<PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let mut command = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "npm", "root", "-g"]);
            c
        } else {
            let mut c = Command::new("npm");
            c.args(["root", "-g"]);
            c
        };
        hide(&mut command);
        let out = command.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
        let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        path.is_dir().then_some(path)
    })
    .clone()
}

/// Как запустить программу: node со скриптом пакета или сам файл.
///
/// Не через `qwen.cmd`: командная строка Windows разбирает кавычки по-своему,
/// и вопрос с кавычками внутри доходил бы до программы искажённым.
fn launcher(engine: &Engine) -> Option<(PathBuf, Vec<String>)> {
    if let Some(package) = engine.package {
        let dir = npm_root()?.join(package);
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
        let bin = match &manifest["bin"] {
            serde_json::Value::String(path) => path.clone(),
            serde_json::Value::Object(map) => map.values().next()?.as_str()?.to_string(),
            _ => return None,
        };
        let script = dir.join(bin);
        let node = which("node")?;
        return script.is_file().then(|| (node, vec![script.to_string_lossy().into_owned()]));
    }
    Some((which(engine.exe?)?, Vec::new()))
}

fn which(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            exts.iter().map(|ext| dir.join(format!("{name}{ext}"))).find(|path| path.is_file())
        })
    })
}

/// Подписки, которые стоят на этом компьютере: `(адрес, название)`.
pub fn installed() -> Vec<(String, &'static str)> {
    ENGINES
        .iter()
        .filter(|engine| launcher(engine).is_some())
        .map(|engine| (format!("cli:{}", engine.id), engine.title))
        .collect()
}

/// Разговор одним текстом: у CLI нет своей сессии.
fn prompt_of(body: &serde_json::Value) -> String {
    let mut system = Vec::new();
    let mut lines = Vec::new();
    for message in body["messages"].as_array().into_iter().flatten() {
        let content = message["content"].as_str().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        match message["role"].as_str() {
            Some("system") => system.push(content.to_string()),
            Some("assistant") => lines.push(format!("Ассистент: {content}")),
            _ => lines.push(format!("Пользователь: {content}")),
        }
    }
    let tail: Vec<String> = lines.iter().rev().take(HISTORY).rev().cloned().collect();
    let dialog = if tail.len() == 1 {
        tail[0].split_once(": ").map(|(_, text)| text.to_string()).unwrap_or_default()
    } else {
        tail.join("\n\n")
    };
    let json = if body.get("response_format").is_some() || body.get("format").is_some() {
        "\n\nОтветь только JSON, без пояснений и разметки."
    } else {
        ""
    };
    format!("{}\n\n{dialog}{json}", system.join("\n\n")).trim().to_string()
}

/// Спрашивает программу и отдаёт ответ в виде OpenAI-совместимого сервиса.
pub async fn ask(endpoint: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let engine = engine(endpoint).ok_or_else(|| format!("неизвестная программа «{endpoint}»"))?;
    let (program, mut args) = launcher(engine)
        .ok_or_else(|| format!("{} не установлен на этом компьютере", engine.title))?;
    let prompt = prompt_of(body);
    let id = engine.id;
    let started = std::time::Instant::now();
    log::info!("запрос к модели: {} на этом компьютере", engine.title);

    let text = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let dir = std::env::temp_dir().join("sufler-cli");
        let _ = std::fs::create_dir_all(&dir);
        let answer_file = dir.join(format!("answer-{}.txt", std::process::id()));
        match id {
            "codex" => args.extend([
                "exec".into(), "--skip-git-repo-check".into(), "--sandbox".into(), "read-only".into(),
                "--ephemeral".into(), "--color".into(), "never".into(), "-o".into(),
                answer_file.to_string_lossy().into_owned(), "-".into(),
            ]),
            "claude" => args.extend(["-p".into(), "--output-format".into(), "text".into()]),
            // Qwen Code и Gemini CLI читают вопрос из stdin, если `-p` пустой:
            // так вопрос любой длины не упирается в предел командной строки.
            _ => args.extend(["-p".into(), " ".into()]),
        }
        let mut command = Command::new(&program);
        command.args(&args).current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        hide(&mut command);
        let mut child = command.spawn().map_err(|err| format!("не запустился: {err}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(prompt.as_bytes());
        }
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            if child.try_wait().map_err(|err| err.to_string())?.is_some() {
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                return Err("не ответил вовремя".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let out = child.wait_with_output().map_err(|err| err.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let text = if id == "codex" {
            let text = std::fs::read_to_string(&answer_file).unwrap_or_default();
            let _ = std::fs::remove_file(&answer_file);
            text.trim().to_string()
        } else {
            stdout
        };
        if out.status.success() && !text.is_empty() {
            return Ok(text);
        }
        let said = if stderr.is_empty() { text } else { stderr };
        let lower = said.to_lowercase();
        if ["login", "auth", "sign in", "credential", "unauthorized"].iter().any(|word| lower.contains(word)) {
            return Err("нужен вход — запустите программу в терминале и войдите в аккаунт".into());
        }
        Err(said.chars().take(300).collect::<String>())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| format!("{}: {err}", engine.title))?;

    log::info!("{} ответил за {} мс", engine.title, started.elapsed().as_millis());
    Ok(serde_json::json!({
        "model": format!("{id}-cli"),
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": text }, "finish_reason": "stop" }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_becomes_one_prompt() {
        let body = serde_json::json!({ "messages": [
            { "role": "system", "content": "Ты Ноа." },
            { "role": "user", "content": "Привет" },
        ]});
        assert_eq!(prompt_of(&body), "Ты Ноа.\n\nПривет");
        let body = serde_json::json!({ "messages": [
            { "role": "user", "content": "Сколько будет 2+2?" },
            { "role": "assistant", "content": "Четыре." },
            { "role": "user", "content": "А 3+3?" },
        ], "response_format": { "type": "json_object" } });
        let prompt = prompt_of(&body);
        assert!(prompt.contains("Ассистент: Четыре.") && prompt.ends_with("Ответь только JSON, без пояснений и разметки."));
    }

    #[test]
    fn only_cli_addresses_are_cli() {
        assert!(is_cli("cli:qwen"));
        assert!(!is_cli("http://127.0.0.1:11434/api/chat"));
        assert!(engine("cli:nope").is_none());
    }
}
