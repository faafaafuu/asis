//! Суфлёр — системный попап с AI-объяснением выделенного текста.
//!
//! Библиотека, а не только bin: этого требует сборка под Android и iOS,
//! где точкой входа становится `run()` через `tauri::mobile_entry_point`.

mod ai_client;
mod commands;
mod config;
#[cfg(target_os = "windows")]
mod instance;
#[cfg(mobile)]
mod mobile;
mod net;
mod ollama;
mod overlay;
mod secret;
mod selection;
mod state;
mod watcher;

use tauri::Manager;

use crate::config::Config;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    {
        // `--quit` — не запуск, а просьба: установщик так останавливает работающую
        // программу перед заменой файлов. Копия ставит событие и сразу уходит,
        // а работающая по нему закрывается сама и убирает за собой значок в трее.
        if std::env::args().any(|arg| arg == "--quit") {
            instance::request_quit();
            return;
        }

        // Программа уже запущена — вместо второй копии показываем окно первой и уходим.
        if !instance::claim() {
            return;
        }
    }

    let builder = tauri::Builder::default().plugin(
        tauri_plugin_log::Builder::new()
            // Собственные цели ДОБАВЛЯЮТСЯ к стандартным, а не заменяют их. Без сброса
            // получалось две записи в один и тот же файл: каждая строка журнала
            // задваивалась, и читать его становилось вдвое труднее.
            .clear_targets()
            // Журнал обязан лежать файлом на диске. У приложения нет главного окна и
            // нет консоли (в релизе она отключена в main.rs), поэтому без файла
            // разобраться, почему попап не появился, нельзя ни пользователю, ни нам.
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir {
                    file_name: Some("sufler".into()),
                },
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Stdout,
            ))
            .level(log::LevelFilter::Info)
            .build(),
    );

    // Нативный плагин выделения — только на мобильных: на десктопе его роль играет
    // системный хук (см. watcher.rs).
    #[cfg(mobile)]
    let builder = builder.plugin(mobile::init());

    builder
        .setup(|app| {
            let config_dir = app.path().app_config_dir().ok();
            let config = Config::load(config_dir);
            log::info!("AI-провайдер: {}", config.ai.provider);
            let key_stored_plain = config.ai.key_stored_plain;
            app.manage(AppState::new(config));

            // Ключ от прежней версии лежит на диске открытым. Перешифровываем
            // сами и сразу: иначе защита включилась бы только у того, кто
            // случайно зайдёт в настройки и что-нибудь сохранит.
            if key_stored_plain {
                let state = app.state::<AppState>();
                match commands::persist(app.handle(), &state) {
                    Ok(()) => log::info!("ключ переведён на шифрование"),
                    Err(err) => log::warn!("не удалось перешифровать ключ: {err}"),
                }
            }

            #[cfg(desktop)]
            {
                setup_tray(app.handle())?;

                // Окно открывается при КАЖДОМ запуске, а не только при первом.
                //
                // Раньше условием было отсутствие файла настроек — то есть окно
                // показывалось ровно один раз в жизни. Стоило человеку что-нибудь
                // сохранить, и дальше запуск проходил совершенно молча: программа
                // уходила в трей, где Windows 11 по умолчанию прячет новые значки под
                // стрелку. Щелчок по ярлыку не давал ничего, и это неотличимо от
                // сломанной программы.
                //
                // Ярлык обязан отвечать. Работать программа продолжает в фоне, окно
                // здесь — не главный экран, а подтверждение «я запущена» плюс
                // настройки; закрыть его можно сразу.
                if let Err(err) = overlay::show_onboarding(app.handle()) {
                    log::error!("не удалось открыть окно: {err}");
                }

                #[cfg(target_os = "windows")]
                instance::listen(app.handle().clone());
                // Наблюдатель за системным выделением есть только на десктопе:
                // на мобильных вход — пункт меню «Объяснить» из нативного плагина
                // (SPEC §9.4, §9.5).
                let integration = watcher::spawn(app.handle());
                app.manage(integration);

                wake_local_model(app.handle());
            }

            // На телефоне окно не создаётся нигде выше: показывать попап у экрана
            // нечему (жеста мышью там нет), а окон в конфиге нет вовсе. Без этого
            // приложение запускалось в пустой белый экран — процесс жив, а на
            // экране ничего. Открываем то же окно настройки: выбор источника и
            // модели на телефоне осмыслен ровно так же.
            #[cfg(mobile)]
            if let Err(err) = overlay::show_onboarding(app.handle()) {
                log::error!("не удалось открыть окно: {err}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::runtime_config,
            commands::popup_ready,
            commands::close_popup,
            commands::ai_explain,
            commands::ai_ask,
            commands::ai_settings,
            commands::save_ai_settings,
            commands::test_ai,
            commands::integration_status,
            commands::open_permission_settings,
            commands::trigger_settings,
            commands::save_trigger_settings,
            commands::capture_diagnostics,
            commands::open_logs,
            commands::local_models,
            commands::pull_model,
            commands::start_ollama,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить приложение");
}

/// Поднимает Ollama, если выбрана она, а сервер молчит.
///
/// После перезагрузки Windows Ollama не всегда стартует сама — записи в
/// автозапуске у неё может не быть. Человек при этом видит, что «пропали все
/// модели» и что объяснения перестали приходить, хотя на диске всё цело.
/// Просить его лезть в автозапуск — перекладывать на него работу программы:
/// она знает, что ей нужен этот сервер, и знает, где он лежит.
///
/// Отдельным потоком: проверка ходит по сети, пусть и на свой же компьютер,
/// а запуск приложения ждать этого не должен.
#[cfg(desktop)]
fn wake_local_model(app: &tauri::AppHandle) {
    let endpoint = {
        let state = app.state::<AppState>();
        let config = state.config();
        // Чужой облачный сервис поднимать нечем и незачем.
        if config.ai.provider != "http" {
            return;
        }
        config.ai.endpoint.clone()
    };

    let host = crate::ollama::host_from(&endpoint);
    // host_from отдаёт местный адрес и для облачных сервисов — там будить
    // нечего, поэтому проверяем, что настроен именно свой компьютер.
    if !endpoint.contains("localhost") && !endpoint.contains("127.0.0.1") {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if crate::ollama::status(&host).await.running {
            return;
        }
        match crate::ollama::start() {
            Ok(()) => log::info!("Ollama не отвечала — запустили её сами"),
            Err(err) => log::warn!("Ollama не отвечает и запустить не вышло: {err}"),
        }
    });
}

/// Иконка в трее — единственный видимый след приложения: главного окна у него нет,
/// а попап живёт по несколько секунд. Без трея пользователь не сможет ни выйти,
/// ни вернуться к инструкции по разрешениям.
#[cfg(desktop)]
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let onboarding = MenuItem::with_id(app, "onboarding", "Настройка и проверка…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&onboarding, &quit])?;

    let mut tray = TrayIconBuilder::with_id("sufler-tray")
        .tooltip("Суфлёр")
        .menu(&menu);

    // Без явной иконки значок в трее получается пустым — то есть на Windows его
    // фактически не видно, и единственный видимый след приложения исчезает.
    // Раньше иконка задавалась только в tauri.conf.json, но конфиг создаёт свой,
    // отдельный значок — уже без нашего меню. Держим и то и другое в одном месте.
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "onboarding" => {
            if let Err(err) = overlay::show_onboarding(app) {
                log::error!("не удалось открыть окно настройки: {err}");
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    // Левый клик по значку тоже открывает окно: искать меню правой кнопкой
    // догадывается не каждый.
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            if let Err(err) = overlay::show_onboarding(tray.app_handle()) {
                log::error!("не удалось открыть окно настройки: {err}");
            }
        }
    })
    .build(app)?;

    Ok(())
}
