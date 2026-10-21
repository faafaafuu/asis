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
    // iPhone — через зеркало: прямой путь режется на длинных запросах.
    if use_mirror() && url == DEFAULT_PLATFORM_URL {
        return (MIRROR_URL.to_string(), token);
    }
    (reachable(url), token)
}

/* ── Зеркало и туннель (iPhone) ──────────────────────────────────────────
   Мобильные операторы в России режут соединение с зарубежным сервером на
   16–32 КБ: короткий вопрос доходил, а сценарий практики (урок в запросе)
   обрывался — «модель не ответила». Российское зеркало (CDN) длинное
   пропускает, но только GET: на POST отвечает 405. Поэтому на iPhone всё
   идёт через зеркало, а запись — GET-туннелем, как у Ноа онлайн в
   браузере (src/js/web/tunnel.js, platform/server/tunnel.mjs): тело кусками
   в адресе, затем «выполнить», долгий ответ — короткими запросами. */

/// Зеркало сайта для России.
pub const MIRROR_URL: &str = "https://m.noahlab.ru";

/// Всё — через зеркало: на iPhone (мобильная сеть).
fn use_mirror() -> bool {
    cfg!(target_os = "ios")
}

/// Байт тела на один кусок: в base64 это ~4 КБ адреса — в пределах CDN.
const TUNNEL_PART: usize = 3000;

/// Метка туннеля: сервер сверяет её в cookie и в адресе (защита от чужих
/// сайтов в браузере; программе — просто постоянная случайная строка).
fn tunnel_mark() -> &'static str {
    static MARK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MARK.get_or_init(random_hex)
}

fn random_hex() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn base64url(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ABC[(n >> 18) as usize & 63] as char);
        out.push(ABC[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ABC[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ABC[n as usize & 63] as char);
        }
    }
    out
}

/// Запрос к своему сайту: `(статус, тело)`. Через зеркало запись идёт
/// туннелем, чтение — обычным GET; иначе — обычный запрос.
pub async fn site_call(url: &str, method: &str, path: &str, token: &str, body: Option<&Value>) -> Result<(u16, Vec<u8>), String> {
    let bytes = body.map(|value| serde_json::to_vec(value).unwrap_or_default()).unwrap_or_default();
    if url == MIRROR_URL && method != "GET" {
        return tunnel(path, method, token, &bytes).await;
    }
    let client = client()?;
    let mut request = if method == "GET" { client.get(format!("{url}{path}")) } else { client.post(format!("{url}{path}")) };
    if !token.is_empty() {
        request = request.bearer_auth(token);
    }
    if body.is_some() {
        request = request.header("Content-Type", "application/json").body(bytes);
    }
    let response = request.send().await.map_err(|err| format!("площадка недоступна: {err}"))?;
    let status = response.status().as_u16();
    let data = response.bytes().await.map_err(|err| format!("ответ не дошёл: {err}"))?;
    Ok((status, data.to_vec()))
}

/// Запись через GET-туннель зеркала. Ответ — тот же, что дал бы POST.
async fn tunnel(path: &str, method: &str, token: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|err| err.to_string())?;
    let mark = tunnel_mark();
    let id = random_hex();
    let cookie = format!("noah_t={mark}");
    let get = |address: String| {
        let mut request = client.get(address).header("Cookie", cookie.clone());
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
        request
    };
    let mut go = vec![
        ("k", mark.to_string()),
        ("id", id.clone()),
        ("m", method.to_string()),
        ("p", path.to_string()),
        ("t", "application/json".to_string()),
    ];
    if body.len() <= TUNNEL_PART {
        go.push(("d", base64url(body)));
    } else {
        let parts: Vec<&[u8]> = body.chunks(TUNNEL_PART).collect();
        // Куски — по четыре сразу: каждый — своё короткое соединение.
        for (batch_at, batch) in parts.chunks(4).enumerate() {
            let handles: Vec<_> = batch
                .iter()
                .enumerate()
                .map(|(offset, part)| {
                    let index = batch_at * 4 + offset;
                    let request = get(format!("{MIRROR_URL}/api/tunnel/part?k={mark}&id={id}&i={index}&d={}", base64url(part)));
                    tauri::async_runtime::spawn(async move { request.send().await })
                })
                .collect();
            for handle in handles {
                let response = handle
                    .await
                    .map_err(|err| err.to_string())?
                    .map_err(|err| format!("кусок запроса не ушёл: {err}"))?;
                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let data = response.bytes().await.unwrap_or_default();
                    return Ok((status, data.to_vec()));
                }
            }
        }
        go.push(("n", parts.len().to_string()));
    }
    let query: String = go
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let mut response = get(format!("{MIRROR_URL}/api/tunnel/go?{query}"))
        .send()
        .await
        .map_err(|err| format!("площадка недоступна: {err}"))?;
    // «Ещё работаю» (202): долгий ответ (модель) — короткими запросами.
    let mut failures = 0;
    loop {
        let status = response.status().as_u16();
        let pending_header = response.headers().get("X-Noah-Pending").is_some_and(|v| v == "1");
        let data = response.bytes().await.map_err(|err| format!("ответ не дошёл: {err}"))?.to_vec();
        let pending = status == 202
            && (pending_header || serde_json::from_slice::<Value>(&data).ok().is_some_and(|v| v["tunnel"] == "pending"));
        if !pending {
            return Ok((status, data));
        }
        loop {
            match get(format!("{MIRROR_URL}/api/tunnel/wait?k={mark}&id={id}")).send().await {
                Ok(next) => {
                    response = next;
                    failures = 0;
                    break;
                }
                // Сбой сети посреди ожидания работу не губит — она идёт на сервере.
                Err(err) => {
                    failures += 1;
                    if failures > 5 {
                        return Err(format!("ответ не дошёл: {err}"));
                    }
                    tokio::time::sleep(Duration::from_secs(failures)).await;
                }
            }
        }
    }
}

/// Строка о сбое — в журнал сервера (`/api/app/diag`), без ожидания. Журнал
/// самого телефона не посмотреть, а голос и ответы иначе чинились вслепую.
pub fn diag(line: impl Into<String>) {
    let line = line.into();
    log::info!("диагностика: {line}");
    tauri::async_runtime::spawn(async move {
        let (url, token) = settings();
        if token.is_empty() {
            return;
        }
        let _ = post(&url, "/api/app/diag", &token, &json!({ "lines": [line] })).await;
    });
}

/// Мост к подпискам через сайт — с адресом и ключом площадки на сейчас.
///
/// В настройках модели они запоминались один раз, при входе: новый вход на
/// телефоне выдаёт новый ключ, а модель ходила со старым («сервис не принял
/// ключ»); домен noahlab.ru мобильные операторы режут — а по IP сайт
/// отвечает. Другие адреса не трогаются.
pub fn live_bridge(endpoint: &str, key: &str) -> (String, String) {
    let Some(at) = endpoint.find("/api/noa/bridge") else {
        return (endpoint.to_string(), key.to_string());
    };
    let (base, token) = settings();
    if token.is_empty() {
        return (endpoint.to_string(), key.to_string());
    }
    (format!("{base}{}", &endpoint[at..]), token)
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
    let (status, data) = site_call(url, "POST", path, token, Some(body)).await?;
    read_bytes(status, &data)
}

/// Ответ сайта из `(статус, тело)` — как `read`.
fn read_bytes(status: u16, data: &[u8]) -> Result<Value, String> {
    let body: Value = serde_json::from_slice(data).unwrap_or_default();
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(body["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("площадка ответила {status}")))
    }
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

/// Связь с площадкой: Ноа забирает черновики модулей и курсов, которые
/// нейросеть пользователя собрала через MCP по ссылке, проверяет их, ставит
/// прошедшие и отправляет отчёт обратно. Без ключа площадки — молчит.
/// Вход в аккаунт через Telegram — без ключа вручную: сайт даёт ссылку на
/// бота, человек жмёт Start, программа забирает ключ. Отдаёт код и ссылку.
pub async fn pair_start() -> Result<(String, String), String> {
    let (url, _) = settings();
    let reply = post(&url, "/api/app/pair/start", "", &json!({})).await?;
    let code = reply["code"].as_str().unwrap_or_default().to_string();
    let link = reply["link"].as_str().unwrap_or_default().to_string();
    if code.is_empty() || link.is_empty() {
        return Err("Сайт не дал ссылку для входа.".into());
    }
    Ok((code, link))
}

/// Ждёт, пока человек нажмёт Start у бота. Отдаёт ключ и имя аккаунта.
pub async fn pair_wait(code: &str) -> Result<(String, String), String> {
    let (url, _) = settings();
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    while std::time::Instant::now() < deadline {
        let path = format!("{url}/api/app/pair/status?code={}", urlencode(code));
        if let Ok(response) = client()?.get(&path).send().await {
            let reply = read(response).await?;
            if reply["done"].as_bool() == Some(true) {
                let token = reply["token"].as_str().unwrap_or_default().to_string();
                let name = reply["name"].as_str().unwrap_or_default().to_string();
                return Ok((token, name));
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err("Время входа вышло — нажмите «Войти через Telegram» ещё раз.".into())
}

/// Собрать новый курс на сервере — через мост, как «Собрать курс» в Ноа
/// онлайн. Курс приходит в программу сам: сайт ставит его в очередь, а
/// `sync` забирает. `quality` — sonnet (точнее) или haiku (бережёт лимит).
pub async fn build_course(goal: &str, quality: &str) -> Result<Value, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Err("Сначала впишите ключ площадки в настройках — по нему курс соберётся на сервере.".into());
    }
    let reply = post(&url, "/api/noa/builds", &token, &json!({ "goal": goal, "quality": quality })).await?;
    Ok(reply["build"].clone())
}

/* ── Ролики к урокам ──────────────────────────────────────────────────── */

/// Заказать ролик: раздел урока (`section`) или вся тема (`None`).
/// `again` — пересобрать, даже если готовый уже есть.
pub async fn video_order(course: &str, topic: &str, section: Option<usize>, material: &str, again: bool) -> Result<Value, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Err("Ролики собираются на сервере — войдите в аккаунт NOAH.".into());
    }
    post(&url, "/api/noa/videos", &token, &json!({ "course": course, "topic": topic, "section": section, "material": material, "again": again })).await
}

/// Ролики темы — по одному на раздел, последний.
pub async fn video_list(course: &str, topic: &str) -> Result<Value, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Ok(json!({ "videos": [] }));
    }
    // Метка в адресе — зеркалу: оно держит в памяти даже /api/.
    let path = format!("/api/noa/videos?course={}&topic={}&_={}", urlencode(course), urlencode(topic), random_hex());
    get_with(&url, &path, &token).await
}

pub async fn video_status(id: &str) -> Result<Value, String> {
    let (url, token) = settings();
    get_with(&url, &format!("/api/noa/videos/{}?_={}", urlencode(id), random_hex()), &token).await
}

/// Свой сайт отвечает через зеркало целиком: напрямую канал рвёт ответ на
/// первых килобайтах, и 4 МБ ролика мелкими кусками шли минуты, а зеркало
/// отдаёт их за пару секунд. Метка `_` — чтобы зеркало не держало копию.
/// `None` — сайт не свой или зеркало не ответило: тогда напрямую.
async fn through_mirror(url: &str, path: &str, token: &str) -> Option<Vec<u8>> {
    let own = [crate::config::DEFAULT_PLATFORM_URL, crate::config::OLD_PLATFORM_URL, MIRROR_URL].contains(&url);
    if !own {
        return None;
    }
    let client = crate::net::client_builder().timeout(Duration::from_secs(90)).build().ok()?;
    let joiner = if path.contains('?') { '&' } else { '?' };
    let whole = format!("{MIRROR_URL}{path}{joiner}_={}", random_hex());
    match client.get(&whole).bearer_auth(token).send().await {
        Ok(response) if response.status().is_success() => match response.bytes().await {
            Ok(data) if !data.is_empty() => Some(data.to_vec()),
            Ok(_) => {
                log::info!("через зеркало пришло пустое: {path}");
                None
            }
            Err(err) => {
                log::info!("через зеркало оборвалось: {path}: {err}");
                None
            }
        },
        Ok(response) => {
            log::info!("через зеркало: {path}: сайт ответил {}", response.status());
            None
        }
        Err(err) => {
            log::info!("через зеркало не пришло: {path}: {err}");
            None
        }
    }
}

/* ── Путеводители ─────────────────────────────────────────────────────── */

/// Путеводители аккаунта — карточками.
pub async fn guide_cards() -> Result<Value, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Err("Путеводители живут в аккаунте — войдите в аккаунт NOAH.".into());
    }
    let reply = get_with(&url, &format!("/api/noa/guides?_={}", random_hex()), &token).await?;
    Ok(reply["guides"].clone())
}

/// Путеводитель целиком — через зеркало, он весит десятки килобайт.
pub async fn guide_get(id: &str) -> Result<Value, String> {
    let (url, token) = settings();
    let path = format!("/api/noa/guides/{}", urlencode(id));
    let reply = match through_mirror(&url, &path, &token).await {
        Some(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|err| format!("путеводитель пришёл битым: {err}"))?,
        None => get_with(&url, &format!("{path}?_={}", random_hex()), &token).await?,
    };
    if let Some(error) = reply["error"].as_str() {
        return Err(error.to_string());
    }
    Ok(reply["guide"].clone())
}

/// Картинка путеводителя.
pub async fn guide_picture(id: &str, file: &str) -> Result<Vec<u8>, String> {
    let (url, token) = settings();
    let path = format!("/api/noa/guides/{}/pictures/{}", urlencode(id), urlencode(file));
    if let Some(bytes) = through_mirror(&url, &path, &token).await {
        return Ok(bytes);
    }
    let response = client()?
        .get(format!("{url}{path}"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|err| format!("картинка не пришла: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("картинка не пришла: сайт ответил {}", response.status()));
    }
    response.bytes().await.map(|b| b.to_vec()).map_err(|err| format!("картинка оборвалась: {err}"))
}

pub async fn guide_start(goal: &str) -> Result<String, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Err("Путеводители собираются на сервере — войдите в аккаунт NOAH.".into());
    }
    let reply = post(&url, "/api/noa/guides", &token, &json!({ "goal": goal })).await?;
    reply["id"].as_str().map(str::to_string).ok_or_else(|| "сайт не прислал путеводитель".into())
}

pub async fn guide_resume(id: &str, wish: Option<&str>) -> Result<(), String> {
    let (url, token) = settings();
    post(&url, &format!("/api/noa/guides/{}/resume", urlencode(id)), &token, &json!({ "wish": wish })).await.map(|_| ())
}

pub async fn guide_stop() -> Result<(), String> {
    let (url, token) = settings();
    post(&url, "/api/noa/guides/stop", &token, &json!({})).await.map(|_| ())
}

pub async fn guide_remove(id: &str) -> Result<(), String> {
    let (url, token) = settings();
    post(&url, &format!("/api/noa/guides/{}/remove", urlencode(id)), &token, &json!({})).await.map(|_| ())
}

/// Путеводитель с устройства — в аккаунт, текстом и кусками: канал рвёт
/// длинные запросы. Картинки сервер берёт заново по тем же статьям.
pub async fn guide_upload(id: &str, text: &str) -> Result<(), String> {
    let (url, token) = settings();
    // Сервер считает длину строки JavaScript — единицами UTF-16.
    let size = text.encode_utf16().count();
    let (mut at, mut part, mut units) = (0usize, String::new(), 0usize);
    let path = format!("/api/noa/guides/{}/part", urlencode(id));
    let mut chars = text.chars().peekable();
    while chars.peek().is_some() {
        while let Some(&ch) = chars.peek() {
            if units + ch.len_utf16() > 5000 {
                break;
            }
            part.push(ch);
            units += ch.len_utf16();
            chars.next();
        }
        post(&url, &path, &token, &json!({ "at": at, "text": part, "size": size })).await?;
        at += units;
        part.clear();
        units = 0;
    }
    Ok(())
}

/// Файл ролика. Канал из России рвёт длинные ответы, поэтому — кусками по
/// диапазону байт, по шесть сразу, каждый своим соединением. Сперва крупно;
/// рвётся — мелко.
pub async fn video_bytes(id: &str, file: &str) -> Result<Vec<u8>, String> {
    let (url, token) = settings();
    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(20))
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|err| err.to_string())?;
    if let Some(data) = through_mirror(&url, &format!("/api/noa/videos/{}/{file}", urlencode(id)), &token).await {
        return Ok(data);
    }
    let path = format!("{url}/api/noa/videos/{}/{file}", urlencode(id));
    let fetch = {
        let (client, path, token) = (client.clone(), path.clone(), token.clone());
        move |start: usize, end: usize| {
            let request = client.get(&path).bearer_auth(&token).header("Range", format!("bytes={start}-{end}"));
            async move {
                let response = request.send().await.map_err(|err| format!("ролик не скачался: {err}"))?;
                let status = response.status().as_u16();
                if status != 206 && status != 200 {
                    return Err(format!("ролик не скачался: сайт ответил {status}"));
                }
                let total = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.rsplit('/').next())
                    .and_then(|v| v.parse::<usize>().ok());
                let data = response.bytes().await.map_err(|err| format!("кусок ролика оборвался: {err}"))?;
                Ok::<_, String>((total, data.to_vec()))
            }
        }
    };
    let (total, first) = fetch(0, 0).await?;
    let size = total.unwrap_or(first.len());
    let mut out = vec![0u8; size];
    for chunk in [512 * 1024, 12 * 1024] {
        let offsets: Vec<usize> = (0..size).step_by(chunk).collect();
        let mut ok = true;
        'batches: for batch in offsets.chunks(6) {
            let handles: Vec<_> = batch
                .iter()
                .map(|&at| tauri::async_runtime::spawn(fetch(at, (at + chunk).min(size) - 1)))
                .collect();
            for (at, handle) in batch.iter().zip(handles) {
                let want = (at + chunk).min(size) - at;
                match handle.await {
                    Ok(Ok((_, data))) if data.len() == want => out[*at..*at + want].copy_from_slice(&data),
                    _ => {
                        ok = false;
                        break 'batches;
                    }
                }
            }
        }
        if ok {
            return Ok(out);
        }
        log::info!("ролик: крупные куски не проходят — качаю мелкими");
    }
    Err("Ролик не скачался — канал рвёт соединение. Попробуйте ещё раз.".into())
}

/// Сборки курсов: что собирается и чем кончилось.
pub async fn course_builds() -> Result<Value, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Ok(json!([]));
    }
    let reply = get_with(&url, "/api/noa/builds", &token).await?;
    Ok(reply["builds"].clone())
}

/// Остановить сборку — готовые темы остаются.
pub async fn stop_build(id: &str) -> Result<(), String> {
    let (url, token) = settings();
    post(&url, &format!("/api/noa/builds/{id}/stop"), &token, &json!({})).await.map(|_| ())
}

/// Продолжить остановленную или упавшую сборку — с последней готовой темы.
pub async fn resume_build(id: &str) -> Result<Value, String> {
    let (url, token) = settings();
    let reply = post(&url, &format!("/api/noa/builds/{id}/resume"), &token, &json!({})).await?;
    Ok(reply["build"].clone())
}

/// Убрать сборку из списка; собранные темы остаются в курсе.
pub async fn remove_build(id: &str) -> Result<(), String> {
    let (url, token) = settings();
    post(&url, &format!("/api/noa/builds/{id}/remove"), &token, &json!({})).await.map(|_| ())
}

/// Сверка с курсами аккаунта сейчас — «потянуть вниз» на экране курсов.
/// Отдаёт, появилось ли что-то новое.
pub fn sync_courses_now() -> Result<bool, String> {
    let (url, token) = settings();
    if token.is_empty() {
        return Ok(false);
    }
    account_courses(&url, &token)
}

pub fn sync(app: &tauri::AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("sufler-platform".into()).spawn(move || {
        let mut last_hello = std::time::Instant::now() - Duration::from_secs(3600);
        let mut last_progress = std::time::Instant::now() - Duration::from_secs(3600);
        let mut last_courses = std::time::Instant::now() - Duration::from_secs(3600);
        loop {
            let (url, token) = settings();
            if !token.is_empty() {
                // Сверка с курсами аккаунта — сразу после входа и раз в десять минут.
                if last_courses.elapsed() > Duration::from_secs(600) {
                    match account_courses(&url, &token) {
                        Ok(changed) => {
                            last_courses = std::time::Instant::now();
                            if changed {
                                use tauri::Emitter;
                                let _ = app.emit("learn:courses", ());
                            }
                        }
                        Err(err) => log::debug!("площадка: курсы аккаунта не сверены ({err})"),
                    }
                }
                if last_hello.elapsed() > Duration::from_secs(240) {
                    let env = crate::mcp::environment();
                    let courses = crate::mcp::list_courses();
                    let ids: Vec<String> = crate::learning::overview().into_iter().map(|card| card.id).collect();
                    let hello = tauri::async_runtime::block_on(post(
                        &url,
                        "/api/app/hello",
                        &token,
                        &json!({ "env": env, "courses": courses, "courseIds": ids }),
                    ));
                    match hello {
                        Ok(_) => last_hello = std::time::Instant::now(),
                        Err(err) => log::debug!("площадка: не поздоровались ({err})"),
                    }
                }
                if let Err(err) = take_drafts(&url, &token) {
                    log::debug!("площадка: черновики не взяты ({err})");
                }
                if let Err(err) = take_courses(&url, &token) {
                    log::debug!("площадка: курсы не взяты ({err})");
                }
                // Прогресс обучения — сразу после ответов и раз в две минуты,
                // чтобы подтянуть сделанное в браузере и на телефоне.
                let unsent = crate::learning::take_unsent();
                if unsent || last_progress.elapsed() > Duration::from_secs(120) {
                    match sync_progress(&url, &token) {
                        Ok(changed) => {
                            last_progress = std::time::Instant::now();
                            if changed {
                                use tauri::Emitter;
                                let _ = app.emit_to(crate::overlay::LEARN_LABEL, "learn:changed", ());
                            }
                        }
                        Err(err) => {
                            if unsent {
                                crate::learning::mark_unsent();
                            }
                            log::debug!("площадка: прогресс не сверен ({err})");
                        }
                    }
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

/// Отправляет прогресс обучения и берёт общий, слитый с браузером и
/// телефоном. Отдаёт, пришло ли что-то новое.
fn sync_progress(url: &str, token: &str) -> Result<bool, String> {
    let body = json!({ "data": crate::learning::snapshot() });
    let reply = tauri::async_runtime::block_on(post(url, "/api/app/progress", token, &body))?;
    Ok(reply.get("data").is_some_and(crate::learning::adopt))
}

async fn get_with(url: &str, path: &str, token: &str) -> Result<Value, String> {
    let response = client()?
        .get(format!("{url}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    read(response).await
}

/// Курсы, которые нейросеть отправила через ссылку MCP.
///
/// Курс забирается кусками по несколько килобайт, каждый своим соединением:
/// на части каналов длинный ответ зарубежного сервера обрывается после
/// первых десятков килобайт, а курс на двадцать тем весит сотни.
fn take_courses(url: &str, token: &str) -> Result<(), String> {
    let body = tauri::async_runtime::block_on(get_with(url, "/api/app/course-jobs", token))?;
    for job in body["jobs"].as_array().cloned().unwrap_or_default() {
        let id = job["id"].as_str().unwrap_or_default().to_string();
        let course = job["course"].as_str().unwrap_or_default().to_string();
        let kind = job["kind"].as_str().unwrap_or_default().to_string();
        log::info!("площадка: {} курса «{course}» — забираю", if kind == "topic" { "тема" } else { "курс" });
        let outcome = fetch_payload(url, token, &id, job["size"].as_u64().unwrap_or(0) as usize).and_then(|payload| {
            let value: Value = serde_json::from_str(&payload).map_err(|err| format!("курс пришёл битым: {err}"))?;
            if kind == "topic" {
                let topic = serde_json::from_value(value["topic"].clone()).map_err(|err| format!("Тема не разобралась: {err}"))?;
                crate::learning::add_topic(&course, topic)
            } else {
                let course = serde_json::from_value(value).map_err(|err| format!("Курс не разобрался: {err}"))?;
                crate::learning::save_course(course)
            }
        });
        let (ok, report) = match outcome {
            Ok(text) => (true, text),
            Err(text) => (false, text),
        };
        log::info!("площадка: курс «{course}» {}", if ok { "принят" } else { "не принят" });
        let sent = tauri::async_runtime::block_on(post(
            url,
            &format!("/api/app/course-jobs/{id}/report"),
            token,
            &json!({ "ok": ok, "report": report }),
        ));
        if let Err(err) = sent {
            log::warn!("площадка: отчёт по курсу «{course}» не ушёл: {err}");
        }
    }
    Ok(())
}

fn fetch_payload(url: &str, token: &str, id: &str, size: usize) -> Result<String, String> {
    fetch_parts(url, token, &format!("/api/app/course-jobs/{id}/part"), size)
}

/// Курсы аккаунта на этом устройстве. Очередь `course_jobs` отдаёт курс один
/// раз на аккаунт: его забирал компьютер — и телефон, вошедший позже, не
/// получал ничего. Здесь устройство само сверяется со списком аккаунта:
/// недостающее и обновлённое на сайте скачивает кусками, свои курсы (на
/// компьютере) выгружает в аккаунт. Отдаёт, появилось ли что-то новое.
fn account_courses(url: &str, token: &str) -> Result<bool, String> {
    // Фоновая сверка и «потянуть вниз» — по одной: обе пишут курсы и отметки.
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let list = tauri::async_runtime::block_on(get_with(url, "/api/app/courses", token))?;
    let account = list["courses"].as_array().cloned().unwrap_or_default();
    let stamps_path = kit::data_dir().ok_or("папка данных не найдена")?.join("account-courses.json");
    // Какую версию курса аккаунта устройство уже взяло: курс обновили на
    // сайте (дописалась тема) — версия сменилась, курс скачивается заново.
    let mut stamps: BTreeMap<String, String> = std::fs::read_to_string(&stamps_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    // Удалённые на каком-нибудь устройстве — убрать и здесь.
    let removed: Vec<String> = list["removed"]
        .as_array()
        .map(|items| items.iter().filter_map(|id| id.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut changed = false;
    for id in &removed {
        if kit::valid_id(id) && crate::learning::shelve_course(id).unwrap_or(false) {
            stamps.remove(id);
            changed = true;
        }
    }
    let local: Vec<String> = crate::learning::courses().into_iter().map(|course| course.id).collect();
    for item in &account {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let updated = item["updated"].as_str().unwrap_or_default().to_string();
        if !kit::valid_id(&id) {
            continue;
        }
        let have = local.contains(&id);
        let stale = stamps.get(&id).is_some_and(|seen| *seen != updated);
        if have && !stale {
            stamps.insert(id, updated);
            continue;
        }
        let size = item["size"].as_u64().unwrap_or(0) as usize;
        let part = list["part"].as_u64().unwrap_or(0) as usize;
        let outcome = fetch_parts_fast(url, token, &format!("/api/app/courses/{id}/part"), size, part).and_then(|text| {
            let course = serde_json::from_str(&text).map_err(|err| format!("Курс не разобрался: {err}"))?;
            crate::learning::save_course(course)
        });
        match outcome {
            Ok(_) => {
                log::info!("площадка: курс аккаунта «{id}» взят");
                stamps.insert(id, updated);
                changed = true;
            }
            Err(err) => log::warn!("площадка: курс аккаунта «{id}» не взят: {err}"),
        }
    }
    // Курсы, собранные на компьютере, — в аккаунт: иначе телефон и Ноа
    // онлайн их не видят.
    #[cfg(desktop)]
    for course in crate::learning::own_courses() {
        if account.iter().any(|item| item["id"].as_str() == Some(course.id.as_str())) || removed.contains(&course.id) {
            continue;
        }
        match upload_course(url, token, &course) {
            Ok(()) => log::info!("площадка: курс «{}» выгружен в аккаунт", course.id),
            Err(err) => log::warn!("площадка: курс «{}» не выгружен: {err}", course.id),
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&stamps) {
        let _ = std::fs::write(&stamps_path, text);
    }
    Ok(changed)
}

/// Удалить курс: из аккаунта (с отметкой — другие устройства уберут его
/// у себя) и с этого устройства (в `courses/.removed`).
pub async fn remove_course(course_id: &str) -> Result<(), String> {
    if !kit::valid_id(course_id) {
        return Err("Такого курса нет.".into());
    }
    let (url, token) = settings();
    if !token.is_empty() {
        post(&url, &format!("/api/app/courses/{course_id}/remove"), &token, &json!({}))
            .await
            .map_err(|err| format!("Курс не удалился из аккаунта: {err}"))?;
    }
    crate::learning::shelve_course(course_id)?;
    if let Some(path) = kit::data_dir().map(|dir| dir.join("account-courses.json")) {
        let mut stamps: BTreeMap<String, String> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if stamps.remove(course_id).is_some() {
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&stamps).unwrap_or_default());
        }
    }
    Ok(())
}

/// Курс в аккаунт кусками: канал до хостинга рвёт длинные запросы.
#[cfg(desktop)]
fn upload_course(url: &str, token: &str, course: &crate::learning::Course) -> Result<(), String> {
    let text = serde_json::to_string(course).map_err(|err| err.to_string())?;
    // Сервер считает длину строки JavaScript — единицами UTF-16.
    let size = text.encode_utf16().count();
    let mut at = 0usize;
    let mut chars = text.chars().peekable();
    while chars.peek().is_some() {
        let mut part = String::new();
        let mut part_units = 0usize;
        while let Some(&ch) = chars.peek() {
            if part_units + ch.len_utf16() > 5000 {
                break;
            }
            part.push(ch);
            part_units += ch.len_utf16();
            chars.next();
        }
        let path = format!("/api/app/courses/{}/part", course.id);
        tauri::async_runtime::block_on(post(url, &path, token, &json!({ "at": at, "text": part, "size": size })))?;
        at += part_units;
    }
    Ok(())
}

/// То же, что `fetch_parts`, но куски — по шесть сразу: курс на сотни
/// килобайт по одному куску шёл по мобильной сети полминуты. Каждый кусок —
/// своё короткое соединение, как и прежде (длинные ответы канал рвёт).
fn fetch_parts_fast(url: &str, token: &str, path: &str, size: usize, part: usize) -> Result<String, String> {
    if part == 0 || size == 0 {
        return fetch_parts(url, token, path, size);
    }
    let offsets: Vec<usize> = (0..size).step_by(part).collect();
    let glued = tauri::async_runtime::block_on(async {
        let mut texts = Vec::with_capacity(offsets.len());
        for batch in offsets.chunks(6) {
            let handles: Vec<_> = batch
                .iter()
                .map(|&at| {
                    let (url, token, path) = (url.to_string(), token.to_string(), format!("{path}?at={at}"));
                    tauri::async_runtime::spawn(async move { get_with(&url, &path, &token).await })
                })
                .collect();
            for handle in handles {
                let part = handle.await.map_err(|err| err.to_string())??;
                texts.push(part["text"].as_str().unwrap_or_default().to_string());
            }
        }
        Ok::<String, String>(texts.concat())
    })
    .map_err(|err| format!("кусок курса не пришёл: {err}"))?;
    // Курс поменялся, пока скачивался, — по одному куску, с размером из ответов.
    if glued.encode_utf16().count() != size {
        return fetch_parts(url, token, path, size);
    }
    Ok(glued)
}

/// Текст кусками с адреса `path?at=…`: `{ text, size }` за запрос.
fn fetch_parts(url: &str, token: &str, path: &str, size: usize) -> Result<String, String> {
    let mut text = String::with_capacity(size);
    // Сервер режет строку JavaScript — по единицам UTF-16, не по байтам.
    let mut at = 0usize;
    let mut size = size.max(1);
    while at < size {
        let part = tauri::async_runtime::block_on(get_with(url, &format!("{path}?at={at}"), token))
            .map_err(|err| format!("кусок курса не пришёл: {err}"))?;
        size = part["size"].as_u64().map_or(size, |n| n as usize);
        let chunk = part["text"].as_str().unwrap_or_default();
        if chunk.is_empty() {
            break;
        }
        at += chunk.encode_utf16().count();
        text.push_str(chunk);
    }
    Ok(text)
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

#[cfg(test)]
mod live {
    /// Удаление курса против живой площадки: NOAH_TEST_URL, NOAH_TEST_TOKEN,
    /// NOAH_TEST_DIR, NOAH_TEST_COURSE.
    #[test]
    #[ignore]
    fn course_remove() {
        let var = |name| std::env::var(name).expect(name);
        let dir = std::path::PathBuf::from(var("NOAH_TEST_DIR"));
        crate::module_kit::set_data_dir(dir.clone());
        crate::learning::load(dir);
        let url = var("NOAH_TEST_URL");
        let token = var("NOAH_TEST_TOKEN");
        let course = var("NOAH_TEST_COURSE");
        tauri::async_runtime::block_on(async {
            super::post(&url, &format!("/api/app/courses/{course}/remove"), &token, &serde_json::json!({})).await.expect("remove");
        });
        println!("removed on server");
    }

    /// Туннель через зеркало против настоящего сайта: маленький запрос и
    /// большое тело (напрямую мобильная сеть его обрывала).
    #[test]
    #[ignore]
    fn tunnel_through_mirror() {
        tauri::async_runtime::block_on(async {
            let small = super::site_call(super::MIRROR_URL, "POST", "/api/app/diag", "noah_tunneltest000000000000", Some(&serde_json::json!({ "lines": [] })))
                .await
                .expect("small");
            println!("small: {} {}", small.0, String::from_utf8_lossy(&small.1));
            let big = serde_json::json!({ "email": "nobody@example.invalid", "password": "x".repeat(80_000) });
            let started = std::time::Instant::now();
            let reply = super::site_call(super::MIRROR_URL, "POST", "/api/auth/login", "", Some(&big)).await.expect("big");
            println!("big: {} {} за {} мс", reply.0, String::from_utf8_lossy(&reply.1), started.elapsed().as_millis());
        });
    }

    /// Сверка курсов с аккаунтом против живой площадки: NOAH_TEST_URL,
    /// NOAH_TEST_TOKEN, NOAH_TEST_DIR — папка данных (копия, не настоящая).
    #[test]
    #[ignore]
    fn account_courses_sync() {
        let var = |name| std::env::var(name).expect(name);
        let dir = std::path::PathBuf::from(var("NOAH_TEST_DIR"));
        crate::module_kit::set_data_dir(dir.clone());
        crate::learning::load(dir);
        let changed = super::account_courses(&var("NOAH_TEST_URL"), &var("NOAH_TEST_TOKEN")).expect("sync");
        let ids: Vec<String> = crate::learning::own_courses().into_iter().map(|c| c.id).collect();
        println!("changed={changed} own={ids:?}");
    }

    /// Сборка курса из кусков против живой площадки:
    /// NOAH_TEST_URL, NOAH_TEST_TOKEN, NOAH_TEST_JOB, NOAH_TEST_SIZE.
    #[test]
    #[ignore]
    fn payload_is_glued_from_parts() {
        let var = |name| std::env::var(name).expect(name);
        let size = var("NOAH_TEST_SIZE").parse().expect("size");
        let text = super::fetch_payload(&var("NOAH_TEST_URL"), &var("NOAH_TEST_TOKEN"), &var("NOAH_TEST_JOB"), size).expect("payload");
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        let lesson = value["topics"][0]["lesson"].as_str().expect("lesson");
        assert!(lesson.ends_with("ёж 🦔"), "{}", &lesson[lesson.len() - 20..]);
        assert_eq!(lesson.chars().filter(|c| *c == '—').count(), 3000);
    }
}
