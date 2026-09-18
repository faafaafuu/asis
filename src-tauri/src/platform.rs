//! Площадка модулей NOAH: поиск, установка и публикация.
//!
//! Работает и из окна программы, и из `sufler.exe --mcp`: настройки читаются
//! прямо из `config.json`, запросы идут своим рантаймом. Площадка код модулей
//! не запускает — установленный модуль проверяет Ноа, опубликованный проверен
//! Ноа у автора.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::DEFAULT_PLATFORM_URL;
use crate::module_kit::{self as kit, Manifest, Server};

/// Адрес площадки и ключ автора из настроек.
pub fn settings() -> (String, String) {
    let config = kit::data_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("config.json")).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_default();
    let url = config["platform"]["url"].as_str().unwrap_or_default().trim().trim_end_matches('/').to_string();
    let token = crate::secret::reveal(config["platform"]["token"].as_str().unwrap_or_default());
    let url = if url.is_empty() || url == crate::config::OLD_PLATFORM_URL { DEFAULT_PLATFORM_URL.to_string() } else { url };
    (reachable(url), token)
}

/// Отвечает ли площадка по домену — и когда это проверяли.
static DOMAIN_OK: std::sync::Mutex<Option<(bool, std::time::Instant)>> = std::sync::Mutex::new(None);

/// Адрес, по которому площадка сейчас доступна.
///
/// Часть провайдеров режет соединения с именем noahlab.ru — сервер тот же, а
/// по голому IP он отвечает. Поэтому домен проверяется раз в десять минут, и
/// пока он не отвечает (или ещё не проверен), Ноа ходит по IP.
fn reachable(url: String) -> String {
    if url != DEFAULT_PLATFORM_URL {
        return url;
    }
    let known = *DOMAIN_OK.lock().unwrap_or_else(|err| err.into_inner());
    let fresh = known.filter(|(_, at)| at.elapsed() < Duration::from_secs(600));
    if fresh.is_none() {
        // Проверка — в своём потоке: ответ «по какому адресу» нужен сразу.
        *DOMAIN_OK.lock().unwrap_or_else(|err| err.into_inner()) =
            Some((known.is_some_and(|(ok, _)| ok), std::time::Instant::now()));
        let _ = std::thread::Builder::new().name("sufler-platform-probe".into()).spawn(|| {
            let ok = tauri::async_runtime::block_on(async {
                let Ok(client) = crate::net::client_builder().timeout(Duration::from_secs(6)).build() else {
                    return false;
                };
                client
                    .get(format!("{DEFAULT_PLATFORM_URL}/api/health"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
            });
            if !ok {
                log::info!("площадка по домену не отвечает — хожу по IP");
            }
            *DOMAIN_OK.lock().unwrap_or_else(|err| err.into_inner()) = Some((ok, std::time::Instant::now()));
        });
    }
    match fresh.or(known) {
        Some((true, _)) => url,
        _ => crate::config::OLD_PLATFORM_URL.to_string(),
    }
}

fn client() -> Result<reqwest::Client, String> {
    crate::net::client_builder()
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(|err| err.to_string())
}

async fn read(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        Err(body["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("площадка ответила {status}")))
    }
}

pub async fn get(url: &str, path: &str) -> Result<Value, String> {
    let response = client()?
        .get(format!("{url}{path}"))
        .send()
        .await
        .map_err(|err| format!("площадка недоступна: {err}"))?;
    read(response).await
}

async fn post(url: &str, path: &str, token: &str, body: &Value) -> Result<Value, String> {
    let response = client()?
        .post(format!("{url}{path}"))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|err| format!("площадка недоступна: {err}"))?;
    read(response).await
}

/// Кто владеет ключом — для кнопки «Проверить» в настройках.
pub async fn whoami(url: &str, token: &str) -> Result<String, String> {
    let response = client()?
        .get(format!("{}/api/whoami", url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| format!("площадка недоступна: {err}"))?;
    Ok(read(response).await?["name"].as_str().unwrap_or_default().to_string())
}

/// Модули площадки.
pub async fn list(query: &str) -> Result<Vec<Value>, String> {
    let (url, _) = settings();
    let path = format!("/api/modules?q={}", urlencode(query));
    Ok(get(&url, &path).await?["modules"].as_array().cloned().unwrap_or_default())
}

/// Пакет модуля: описание и файлы.
pub async fn package(id: &str) -> Result<(Manifest, BTreeMap<String, Vec<u8>>), String> {
    if !kit::valid_id(id) {
        return Err("Нет такого модуля.".into());
    }
    let (url, _) = settings();
    let body = get(&url, &format!("/api/modules/{id}/package")).await?;
    let manifest: Manifest =
        serde_json::from_value(body["manifest"].clone()).map_err(|err| format!("описание модуля не разобралось: {err}"))?;
    if manifest.id != id {
        return Err("площадка прислала другой модуль".into());
    }
    let files = kit::parse_files(&body["files"])?;
    let problems = kit::lint(&manifest, &files);
    if let Some(problem) = problems.first() {
        return Err(format!("модуль не соответствует стандарту: {problem}"));
    }
    Ok((manifest, files))
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Публикует проверенный модуль от имени владельца ключа.
pub async fn publish(id: &str, description: &str, category: &str) -> Result<String, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Err(format!(
            "Нет ключа площадки. Пусть пользователь войдёт на {url}, создаст ключ в кабинете и вставит его в Ноа → Настройки → Площадка."
        ));
    }
    let root = kit::modules_root().ok_or("нет папки модулей")?;
    let dir = root.join(id);
    let manifest = kit::read_manifest(&dir).ok_or_else(|| format!("Модуля «{id}» нет."))?;
    if !kit::is_verified(&dir) {
        return Err("Модуль не прошёл проверку в этой версии — сначала check_module.".into());
    }
    if !manifest.secrets.is_empty() && kit::read_files(&dir).values().any(|bytes| {
        let text = String::from_utf8_lossy(bytes);
        kit::load_secrets(&dir).values().any(|secret| secret.len() >= 6 && text.contains(secret.as_str()))
    }) {
        return Err("В файлах модуля найден ключ пользователя — уберите его, ключи приходят через окружение.".into());
    }

    // Описания инструментов берём у самого сервера: так на площадке ровно то,
    // что модуль умеет.
    let tools = {
        let dir = dir.clone();
        let manifest = manifest.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Value>, String> {
            let mut server = Server::spawn(&dir, &manifest, &kit::load_secrets(&dir))?;
            let listed = server.handshake(manifest.start_timeout())?;
            server.kill();
            Ok(listed
                .iter()
                .map(|tool| json!({ "name": tool["name"], "about": tool["description"] }))
                .collect())
        })
        .await
        .map_err(|err| err.to_string())??
    };

    let files: BTreeMap<String, String> = kit::read_files(&dir)
        .into_iter()
        .map(|(name, bytes)| String::from_utf8(bytes).map(|text| (name.clone(), text)).map_err(|_| format!("файл «{name}» не текстовый")))
        .collect::<Result<_, _>>()?;
    let body = json!({
        "manifest": manifest,
        "files": files,
        "tools": tools,
        "description": description,
        "category": category,
    });
    let answer = post(&url, "/api/publish", &token, &body).await?;
    Ok(format!(
        "Модуль «{}» опубликован: {url}{}",
        manifest.title,
        answer["url"].as_str().unwrap_or("/")
    ))
}

/// Связь с площадкой: Ноа забирает черновики модулей, которые нейросеть
/// пользователя собрала через MCP по ссылке, проверяет их по регламенту,
/// ставит прошедшие и отправляет отчёт обратно. Без ключа площадки — молчит.
pub fn sync(app: &tauri::AppHandle) {
    let _ = app;
    let _ = std::thread::Builder::new().name("sufler-platform".into()).spawn(|| {
        let mut last_hello = std::time::Instant::now() - Duration::from_secs(3600);
        loop {
            let (url, token) = settings();
            if !token.is_empty() {
                if last_hello.elapsed() > Duration::from_secs(240) {
                    let env = crate::mcp::environment();
                    let hello = tauri::async_runtime::block_on(post(&url, "/api/app/hello", &token, &json!({ "env": env })));
                    match hello {
                        Ok(_) => last_hello = std::time::Instant::now(),
                        Err(err) => log::debug!("площадка: не поздоровались ({err})"),
                    }
                }
                if let Err(err) = take_drafts(&url, &token) {
                    log::debug!("площадка: черновики не взяты ({err})");
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

fn take_drafts(url: &str, token: &str) -> Result<(), String> {
    let body = tauri::async_runtime::block_on(async {
        let response = client()?
            .get(format!("{url}/api/app/drafts"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        read(response).await
    })?;
    for draft in body["drafts"].as_array().cloned().unwrap_or_default() {
        let id = draft["id"].as_str().unwrap_or_default().to_string();
        let module_id = draft["manifest"]["id"].as_str().unwrap_or_default().to_string();
        log::info!("площадка: черновик модуля «{module_id}» — проверяю");
        let outcome = crate::mcp::create_module(&draft["manifest"], &draft["files"]);
        let (ok, report) = match outcome {
            Ok(text) => (true, text),
            Err(text) => (false, text),
        };
        let tools: Vec<Value> = crate::plugins::tools()
            .into_iter()
            .filter(|tool| tool.module == module_id)
            .map(|tool| json!({ "name": tool.name, "about": tool.description }))
            .collect();
        log::info!("площадка: «{module_id}» {}", if ok { "прошёл проверку" } else { "не прошёл проверку" });
        let sent = tauri::async_runtime::block_on(post(
            url,
            &format!("/api/app/drafts/{id}/report"),
            token,
            &json!({ "ok": ok, "report": report, "tools": tools }),
        ));
        if let Err(err) = sent {
            log::warn!("площадка: отчёт по «{module_id}» не ушёл: {err}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_encoded() {
        assert_eq!(urlencode("погода & ветер"), "%D0%BF%D0%BE%D0%B3%D0%BE%D0%B4%D0%B0%20%26%20%D0%B2%D0%B5%D1%82%D0%B5%D1%80");
        assert_eq!(urlencode("weather-1"), "weather-1");
    }
}
