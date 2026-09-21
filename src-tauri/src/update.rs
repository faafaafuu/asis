//! Обновление поверх установленной программы.
//!
//! Раньше новая версия значила для человека одно и то же: найти релиз, скачать
//! установщик, закрыть программу, поставить заново. Половина людей так и
//! остаётся на той версии, с которой начала, — не потому что новая им не нужна,
//! а потому что это пять шагов ради того, чего они ещё не видели.
//!
//! Теперь программа сама раз в несколько часов смотрит, не вышло ли новое, и
//! говорит об этом один раз — не переспрашивая на каждом запуске. Всё остальное
//! делается кнопкой в настройках: загрузить, поставить поверх, перезапуститься.
//!
//! Чему верить, решает подпись. Обновление приходит по сети, и без проверки
//! подписи любой, кто сумел вмешаться в соединение, подменил бы программу целиком.
//! Открытый ключ лежит в `tauri.conf.json`, закрытый — только у того, кто
//! выпускает версии, и в секретах сборки. Файл, подписанный не им, не поставится.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Что нашлось. Пустое поле заметок — не ошибка: описание к версии
/// необязательно.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub version: String,
    pub notes: String,
}

/// Найденное обновление: держим, чтобы окно настроек показало его сразу, не
/// дожидаясь собственной проверки.
static FOUND: std::sync::Mutex<Option<Found>> = std::sync::Mutex::new(None);

/// О какой версии человеку уже сказали. Сказать один раз — помощь, повторять
/// при каждой проверке — навязчивость.
static TOLD: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Через сколько после запуска смотреть в первый раз.
///
/// Не сразу: первые секунды заняты тем, ради чего программу и запускали, —
/// модели, микрофоном, окнами. Обновление подождёт.
const FIRST_LOOK: std::time::Duration = std::time::Duration::from_secs(120);

/// Как часто смотреть дальше. Версии выходят не чаще раза в неделю, и чаще
/// шести часов проверять незачем.
const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Какая версия установлена сейчас.
pub fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Спрашивает, вышло ли новое. Отдаёт `None`, если стоит последнее.
pub async fn look(app: &AppHandle) -> Result<Option<Found>, String> {
    let updater = app.updater().map_err(|err| format!("обновление недоступно: {err}"))?;
    let answer = updater.check().await.map_err(|err| format!("не удалось проверить: {err}"))?;

    let found = answer.map(|update| Found {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
    });
    *FOUND.lock().unwrap_or_else(|err| err.into_inner()) = found.clone();
    Ok(found)
}

/// Что нашли в прошлый раз.
pub fn found() -> Option<Found> {
    FOUND.lock().unwrap_or_else(|err| err.into_inner()).clone()
}

/// Загружает и ставит новую версию поверх текущей, затем перезапускает программу.
///
/// Установщик работает молча и сохраняет папку, в которую программа была
/// поставлена: для человека это одна кнопка, а не мастер установки заново.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|err| format!("обновление недоступно: {err}"))?;
    let update = updater
        .check()
        .await
        .map_err(|err| format!("не удалось проверить: {err}"))?
        .ok_or("Уже стоит последняя версия.")?;

    let version = update.version.clone();
    log::info!("обновление {version}: загружаю");
    update
        .download_and_install(|_, _| {}, || log::info!("обновление {version}: загружено, ставлю"))
        .await
        .map_err(|err| format!("не удалось поставить: {err}"))?;

    log::info!("обновление поставлено — перезапускаюсь");
    app.restart()
}

/// Фоновая проверка: первый раз вскоре после запуска, дальше раз в несколько
/// часов. О находке говорит один раз на версию.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            match look(&app).await {
                Ok(Some(found)) => tell(&app, &found),
                Ok(None) => log::info!("обновлений нет, стоит {}", current(&app)),
                Err(err) => log::warn!("проверка обновления: {err}"),
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Говорит о находке — и только о новой. Окно с ответами для этого уже есть:
/// заводить ради одной строки системное уведомление, которое Windows покажет
/// поверх всего и запишет в свой центр, значит вести себя навязчивее, чем
/// того стоит новость.
fn tell(app: &AppHandle, found: &Found) {
    if !first_word_about(&found.version) {
        return;
    }
    log::info!("вышла версия {}", found.version);
    let text = format!(
        "Вышла версия {}. Обновиться — в настройках, раздел «Обновление»: программа поставит её поверх и перезапустится.",
        found.version
    );
    if let Err(err) = crate::overlay::show_for_reminder(app, text) {
        log::warn!("не удалось показать весть об обновлении: {err}");
    }
}

/// Первая ли это весть об этой версии.
///
/// Проверка идёт каждые несколько часов, а новость одна: сказать о ней стоит
/// один раз. О следующей версии — снова один раз.
fn first_word_about(version: &str) -> bool {
    let mut told = TOLD.lock().unwrap_or_else(|err| err.into_inner());
    if *told == version {
        return false;
    }
    *told = version.to_string();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_one_version_we_speak_once() {
        *TOLD.lock().unwrap() = String::new();

        assert!(first_word_about("1.7.0"), "о новой версии говорим");
        assert!(!first_word_about("1.7.0"), "о той же — молчим");
        assert!(first_word_about("1.8.0"), "о следующей — снова говорим");
    }
}
