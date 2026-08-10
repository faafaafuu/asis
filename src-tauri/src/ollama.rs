//! Разговор с локальной Ollama: что у неё установлено и как скачать модель.
//!
//! Зачем отдельно от ai_client: тот занят объяснениями и знает про модель ровно
//! одно — её имя. Здесь же всё, что нужно окну настройки, чтобы человек выбирал
//! модель из списка, а не вписывал `qwen2.5:7b` по памяти.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Куда стучаться, если в настройках не указано ничего осмысленного.
/// Именно 127.0.0.1, а не localhost: на машинах с включённым IPv6 localhost
/// иногда разрешается в ::1, где Ollama не слушает, и получается «сервис не
/// отвечает» при работающем сервисе.
const DEFAULT_HOST: &str = "http://127.0.0.1:11434";

/// Событие с ходом загрузки. Летит в окно настройки много раз в секунду.
const PULL_EVENT: &str = "model:pull";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub name: String,
    /// Гигабайты с одним знаком: человеку важен порядок, а не байты.
    pub size_gb: f64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Отвечает ли Ollama. Не «установлена» — установленная, но не запущенная
    /// служба для нас неотличима от отсутствующей, и совет будет один и тот же.
    pub running: bool,
    pub installed: Vec<Model>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Progress {
    model: String,
    /// 0..100. Пока Ollama не сообщила общий размер, шлём 0 — полоса стоит,
    /// но подпись уже объясняет, что происходит.
    percent: u8,
    /// Человеческая строка: «скачиваю», «проверяю», «готово».
    status: String,
    done: bool,
    error: Option<String>,
}

/// Ответ `/api/tags`.
#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
    #[serde(default)]
    size: u64,
}

/// Строка потока `/api/pull`.
#[derive(Deserialize)]
struct PullLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    error: Option<String>,
}

/// Базовый адрес Ollama по адресу чата из настроек.
///
/// В настройках лежит полный адрес запроса, а служебные ручки живут рядом
/// (`/api/tags`, `/api/pull`). Отрезаем хвост — но только у родного адреса
/// Ollama: если человек настроил облачный сервис, спрашивать у того про
/// локальные модели бессмысленно, там своя вселенная. В таком случае идём
/// к Ollama по умолчанию — она либо запущена, либо нет, и окно это покажет.
pub fn host_from(endpoint: &str) -> String {
    let endpoint = endpoint.trim().trim_end_matches('/');
    match endpoint.strip_suffix("/api/chat") {
        Some(host) if host.starts_with("http") => host.to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}

/// Что сейчас установлено. Молчаливо: неответ Ollama — не ошибка, а «не
/// запущена», и окно покажет это отдельным сообщением.
pub async fn status(host: &str) -> Status {
    let client = reqwest::Client::new();
    let url = format!("{host}/api/tags");

    let response = match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => {
            return Status {
                running: false,
                installed: Vec::new(),
            }
        }
    };

    let Ok(tags) = response.json::<TagsResponse>().await else {
        return Status {
            running: false,
            installed: Vec::new(),
        };
    };

    let mut installed: Vec<Model> = tags
        .models
        .into_iter()
        .map(|m| Model {
            name: m.name,
            size_gb: (m.size as f64 / 1e9 * 10.0).round() / 10.0,
        })
        .collect();
    installed.sort_by(|a, b| a.name.cmp(&b.name));

    Status {
        running: true,
        installed,
    }
}

/// Скачивает модель, докладывая о ходе событиями.
///
/// Поток, а не один запрос: файл на несколько гигабайт качается минутами, и
/// молчаливое ожидание неотличимо от зависания. Ollama отдаёт построчный JSON,
/// поэтому читаем кусками и разбираем по строкам — целиком такой ответ
/// не разобрать.
pub async fn pull(app: AppHandle, host: String, model: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("{host}/api/pull");

    let mut response = client
        .post(&url)
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(|err| format!("Ollama не ответила: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("Ollama ответила ошибкой {}", response.status()));
    }

    let mut buffer = String::new();
    let mut last_percent = u8::MAX;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| format!("загрузка прервалась: {err}"))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find('\n') {
            let line: String = buffer.drain(..=end).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Ok(parsed) = serde_json::from_str::<PullLine>(line) else {
                continue;
            };

            if let Some(error) = parsed.error {
                emit(&app, &model, 0, &error, true, Some(error.clone()));
                return Err(error);
            }

            let percent = if parsed.total > 0 {
                ((parsed.completed as f64 / parsed.total as f64) * 100.0) as u8
            } else {
                0
            };

            // Событие на каждую строку — это сотни событий в секунду на пустом
            // месте: Ollama шлёт их щедро. Окну хватает шага в один процент.
            if percent != last_percent || parsed.status.contains("success") {
                last_percent = percent;
                emit(&app, &model, percent, &parsed.status, false, None);
            }
        }
    }

    emit(&app, &model, 100, "готово", true, None);
    Ok(())
}

fn emit(app: &AppHandle, model: &str, percent: u8, status: &str, done: bool, error: Option<String>) {
    let _ = app.emit(
        PULL_EVENT,
        Progress {
            model: model.to_string(),
            percent,
            status: human(status),
            done,
            error,
        },
    );
}

/// Ollama рапортует по-английски и подробностями, которые человеку не нужны.
fn human(status: &str) -> String {
    let status = status.trim();
    if status.starts_with("pulling manifest") {
        return "готовлюсь".into();
    }
    if status.starts_with("pulling") {
        return "скачиваю".into();
    }
    if status.starts_with("verifying") {
        return "проверяю".into();
    }
    if status.starts_with("writing") || status.starts_with("extracting") {
        return "распаковываю".into();
    }
    if status.contains("success") {
        return "готово".into();
    }
    status.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_is_cut_before_api() {
        assert_eq!(
            host_from("http://localhost:11434/api/chat"),
            "http://localhost:11434"
        );
        assert_eq!(
            host_from("http://192.168.0.5:11434/api/chat"),
            "http://192.168.0.5:11434"
        );
    }

    #[test]
    fn foreign_endpoint_falls_back_to_default() {
        // Настроен облачный сервис — спрашивать у него про локальные модели
        // бессмысленно, идём к своей Ollama.
        assert_eq!(
            host_from("https://openrouter.ai/api/v1/chat/completions"),
            DEFAULT_HOST
        );
        assert_eq!(host_from(""), DEFAULT_HOST);
        assert_eq!(host_from("совсем не адрес"), DEFAULT_HOST);
    }

    #[test]
    fn english_chatter_becomes_russian() {
        assert_eq!(human("pulling manifest"), "готовлюсь");
        assert_eq!(human("pulling 8934d96d3f08"), "скачиваю");
        assert_eq!(human("verifying sha256 digest"), "проверяю");
        assert_eq!(human("success"), "готово");
    }
}
