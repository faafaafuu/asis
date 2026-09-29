//! Обновление поверх установленной программы — само, без установщика.
//!
//! Раньше новая версия значила для человека одно и то же: найти релиз, скачать
//! установщик, закрыть программу, поставить заново, согласиться с окнами Windows.
//! Половина людей так и остаётся на той версии, с которой начала.
//!
//! Теперь программа раз в несколько часов смотрит, не вышло ли новое, скачивает
//! его в фоне и ставит в тихую минуту — когда её окна закрыты, Ноа не говорит и
//! не слушает, а к компьютеру пару минут не прикасались. Установщик работает без
//! окна (`/S`), программа стоит в папке пользователя, поэтому Windows не
//! спрашивает прав; после установки он сам запускает новую версию (`/R`).
//! Человек ничего не нажимает. Кнопка в «Помощи» — чтобы не ждать тихой минуты.
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

/// Скачанное и проверенное обновление, которое ждёт тихой минуты.
struct Ready {
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
}

static READY: std::sync::Mutex<Option<Ready>> = std::sync::Mutex::new(None);

/// О какой версии уже сказали, что сама она не встанет. Проверка идёт каждые
/// несколько часов, а сказать об этом стоит один раз.
static TOLD: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Через сколько после запуска смотреть в первый раз.
///
/// Не сразу: первые секунды заняты тем, ради чего программу и запускали, —
/// модели, микрофоном, окнами. Обновление подождёт.
const FIRST_LOOK: std::time::Duration = std::time::Duration::from_secs(120);

/// Как часто смотреть дальше. Версии выходят не чаще раза в неделю, и чаще
/// шести часов проверять незачем.
const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Как часто, пока обновление ждёт, смотреть, не настала ли тихая минута.
const QUIET_POLL: std::time::Duration = std::time::Duration::from_secs(30);

/// Сколько к компьютеру не должны прикасаться, чтобы перезапуск прошёл незаметно.
/// Две минуты: за это время человек успевает отойти, а не задуматься над фразой.
const AWAY: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// Сколько ждём ответа на вопрос «что вышло».
///
/// Без срока запрос висит сколько угодно, и кнопка «Проверить» выглядит
/// сломанной. Минута — с запасом: на плохом канале ответ приходил и через две,
/// но тогда честное «не удалось проверить» лучше тишины.
const ASK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Спрашивающий. Срок ставится только на вопрос о версии: загрузка самого
/// файла идёт минутами, и обрывать её по тому же будильнику нельзя.
fn updater(app: &AppHandle, wait: Option<std::time::Duration>) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder();
    if let Some(wait) = wait {
        builder = builder.timeout(wait);
    }
    builder.build().map_err(|err| format!("обновление недоступно: {err}"))
}

/// Какая версия установлена сейчас.
pub fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Спрашивает, вышло ли новое. Отдаёт `None`, если стоит последнее.
pub async fn look(app: &AppHandle) -> Result<Option<Found>, String> {
    log::info!("смотрю, не вышло ли новое");
    let answer = updater(app, Some(ASK_WAIT))?.check().await.map_err(|err| format!("не удалось проверить: {err}"))?;

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

/// Скачивает найденное в фоне, если оно ещё не скачано. Подпись проверяется
/// при загрузке: неподписанный файл сюда не попадёт.
async fn fetch(update: tauri_plugin_updater::Update) -> Result<(), String> {
    if READY
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .as_ref()
        .is_some_and(|ready| ready.update.version == update.version)
    {
        return Ok(());
    }
    log::info!("обновление {}: скачиваю в фоне", update.version);
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|err| format!("не удалось скачать: {err}"))?;
    log::info!("обновление {}: скачано, жду тихой минуты", update.version);
    *READY.lock().unwrap_or_else(|err| err.into_inner()) = Some(Ready { update, bytes });
    Ok(())
}

/// Ставит скачанное и перезапускается. На Windows установщик запускается без
/// окна, программа выходит, а он после установки поднимает новую версию сам.
fn apply(app: &AppHandle, ready: Ready) -> Result<(), String> {
    if let Some(dir) = guarded_folder(app) {
        return Err(format!(
            "программа стоит в папке {}, а её охраняет Windows (контролируемый доступ к папкам): установщик не сможет заменить файлы. Поставьте NOAH заново в папку, которую установщик предлагает сам, — настройки и модули останутся",
            dir.display()
        ));
    }
    log::info!("обновление {}: ставлю и перезапускаюсь", ready.update.version);
    ready.update.install(&ready.bytes).map_err(|err| format!("не удалось поставить: {err}"))?;
    app.restart()
}

/// Ставит новую версию сейчас, не дожидаясь тихой минуты: кнопка в «Помощи».
pub async fn install(app: AppHandle) -> Result<(), String> {
    let update = updater(&app, None)?
        .check()
        .await
        .map_err(|err| format!("не удалось проверить: {err}"))?
        .ok_or("Уже стоит последняя версия.")?;
    fetch(update).await?;
    let ready = READY.lock().unwrap_or_else(|err| err.into_inner()).take().ok_or("Обновление не скачалось.")?;
    apply(&app, ready)
}

/// Фоновая работа: первый раз вскоре после запуска, дальше раз в несколько
/// часов — посмотреть, скачать и поставить в тихую минуту.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            match updater(&app, Some(ASK_WAIT)) {
                Ok(asker) => match asker.check().await {
                    Ok(Some(update)) => {
                        *FOUND.lock().unwrap_or_else(|err| err.into_inner()) = Some(Found {
                            version: update.version.clone(),
                            notes: update.body.clone().unwrap_or_default(),
                        });
                        let version = update.version.clone();
                        if let Some(dir) = guarded_folder(&app) {
                            // Скачивать незачем: установщик не заменит файлы, а
                            // программу перед этим закроет — и она пропадёт из трея.
                            tell_once(&app, &version, &format!("программа стоит в папке {}, а её охраняет Windows", dir.display()));
                        } else if let Err(err) = fetch(update).await {
                            log::warn!("обновление: {err}");
                        }
                    }
                    Ok(None) => log::info!("обновлений нет, стоит {}", current(&app)),
                    Err(err) => log::warn!("проверка обновления: {err}"),
                },
                Err(err) => log::warn!("проверка обновления: {err}"),
            }
            wait_and_apply(&app).await;
        }
    });
}

/// До следующей проверки: если обновление скачано — ставит его в первую тихую
/// минуту; если нет — просто ждёт.
async fn wait_and_apply(app: &AppHandle) {
    let until = std::time::Instant::now() + EVERY;
    while std::time::Instant::now() < until {
        tokio::time::sleep(QUIET_POLL).await;
        let waiting = READY.lock().unwrap_or_else(|err| err.into_inner()).is_some();
        if !waiting || !quiet(app) {
            continue;
        }
        let Some(ready) = READY.lock().unwrap_or_else(|err| err.into_inner()).take() else {
            continue;
        };
        let version = ready.update.version.clone();
        if let Err(err) = apply(app, ready) {
            log::warn!("обновление: {err}");
            tell_once(app, &version, &err);
        }
    }
}

/// Тихая минута: ни одного открытого окна, Ноа не говорит и не слушает, к
/// компьютеру давно не прикасались. Перезапуск в такую минуту никто не заметит.
fn quiet(app: &AppHandle) -> bool {
    use tauri::Manager;
    let window_open = app.webview_windows().values().any(|window| window.is_visible().unwrap_or(false));
    !window_open && !crate::voice::speaking() && !crate::voice::hotkey::recording() && away() >= AWAY
}

/// Папка пользователя, внутри которой стоит программа, если её охраняет
/// Windows. Контролируемый доступ к папкам не даёт незнакомым программам
/// менять «Документы», «Рабочий стол», «Изображения», «Видео» и «Музыку» —
/// установщик обновления туда не запишет.
fn guarded_folder(app: &AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    let exe = std::env::current_exe().ok()?.to_string_lossy().to_lowercase();
    let path = app.path();
    [path.document_dir(), path.desktop_dir(), path.picture_dir(), path.video_dir(), path.audio_dir()]
        .into_iter()
        .flatten()
        .find(|dir| {
            let dir = dir.to_string_lossy().to_lowercase();
            !dir.is_empty() && exe.starts_with(&format!("{}\\", dir.trim_end_matches('\\')))
        })
}

/// Говорит, что версия сама не встала, — один раз на версию.
fn tell_once(app: &AppHandle, version: &str, why: &str) {
    {
        let mut told = TOLD.lock().unwrap_or_else(|err| err.into_inner());
        if *told == version {
            return;
        }
        *told = version.to_string();
    }
    let text = format!("Вышла версия {version}, но сама она не встала: {why}. Скачать установщик — noahlab.ru/download.");
    if let Err(err) = crate::overlay::show_for_reminder(app, text) {
        log::warn!("не удалось сказать про обновление: {err}");
    }
}

/// Сколько к компьютеру не прикасались — ни мышью, ни клавиатурой.
#[cfg(target_os = "windows")]
fn away() -> std::time::Duration {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return std::time::Duration::ZERO;
    }
    std::time::Duration::from_millis(u64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime)))
}

/// На macOS и Linux узнать это без лишних прав нельзя: хватает закрытых окон
/// и молчащего голоса.
#[cfg(not(target_os = "windows"))]
fn away() -> std::time::Duration {
    std::time::Duration::MAX
}
