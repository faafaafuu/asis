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
    (if url.is_empty() { DEFAULT_PLATFORM_URL.to_string() } else { url }, token)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_encoded() {
        assert_eq!(urlencode("погода & ветер"), "%D0%BF%D0%BE%D0%B3%D0%BE%D0%B4%D0%B0%20%26%20%D0%B2%D0%B5%D1%82%D0%B5%D1%80");
        assert_eq!(urlencode("weather-1"), "weather-1");
    }
}
