//! Обновление на телефоне.
//!
//! Встроенного обновлятора у Tauri под Android нет, а магазина у Ноа пока
//! тоже нет. Поэтому версия сверяется с тем же `latest.json` из последнего
//! релиза, что и на компьютере, а новый .apk скачивает и ставит система:
//! ссылка открывается в браузере, установщик Android ставит файл поверх —
//! ключ подписи у всех версий один, настройки остаются.

use serde::Serialize;
use tauri::AppHandle;

const LATEST: &str = "https://github.com/faafaafuu/asis/releases/latest/download/latest.json";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub version: String,
    pub notes: String,
}

pub fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Вышло ли новое. `None` — стоит последнее.
pub async fn look(app: &AppHandle) -> Result<Option<Found>, String> {
    let client = crate::net::client_builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|err| err.to_string())?;
    let latest: serde_json::Value = client
        .get(LATEST)
        .send()
        .await
        .and_then(|answer| answer.error_for_status())
        .map_err(|err| format!("не удалось проверить: {}", err.without_url()))?
        .json()
        .await
        .map_err(|err| format!("не удалось проверить: {}", err.without_url()))?;
    let version = latest["version"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    if !newer(&version, &current(app)) {
        return Ok(None);
    }
    Ok(Some(Found {
        version,
        notes: latest["notes"].as_str().unwrap_or_default().to_string(),
    }))
}

/// Скачать новый .apk: файл берёт браузер, ставит установщик Android.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let found = look(&app).await?.ok_or("Уже стоит последняя версия.")?;
    let url = format!(
        "https://github.com/faafaafuu/asis/releases/download/v{0}/Sufler_{0}_android.apk",
        found.version
    );
    crate::commands::open_externally(&url)
}

/// Новее ли `found`, чем `have`: сравнение по числам, «1.10» новее «1.9».
fn newer(found: &str, have: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(found) > parts(have)
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("1.13.0", "1.12.0"));
        assert!(newer("1.10.0", "1.9.3"));
        assert!(!newer("1.12.0", "1.12.0"));
        assert!(!newer("", "1.12.0"));
    }
}
