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
#[cfg(desktop)]
mod voice;
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

    // Запуск вместе с системой отличается от запуска руками ровно одним: окно
    // настройки открывать не надо. Человек не просил его открыть — он вообще
    // ничего не делал, он просто включил компьютер.
    let background = std::env::args().any(|arg| arg == "--background");

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

    // Тот же ключ передаётся системе при регистрации автозапуска: запись в
    // автозагрузке должна поднимать программу молча, а не открывать окно.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--background"]),
    ));

    builder
        // move — ради одного `background`: замыкание переживает функцию, а флаг
        // лежит на её стеке.
        .setup(move |app| {
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
                // …кроме запуска вместе с системой: там показывать окно некому
                // и незачем, программа просто занимает своё место в трее.
                if !background {
                    if let Err(err) = overlay::show_onboarding(app.handle()) {
                        log::error!("не удалось открыть окно: {err}");
                    }
                }

                apply_autostart(app.handle());

                #[cfg(target_os = "windows")]
                instance::listen(app.handle().clone());
                // Наблюдатель за системным выделением есть только на десктопе:
                // на мобильных вход — пункт меню «Объяснить» из нативного плагина
                // (SPEC §9.4, §9.5).
                let integration = watcher::spawn(app.handle());
                app.manage(integration);

                wake_local_model(app.handle());
                listen_for_voice_keys(app.handle());
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
            commands::appearance,
            commands::save_appearance,
            commands::ollama_install_size,
            commands::install_ollama,
            commands::recommended_model,
            #[cfg(desktop)]
            commands::voice_settings,
            #[cfg(desktop)]
            commands::save_voice_settings,
            #[cfg(desktop)]
            commands::voice_list,
            #[cfg(desktop)]
            commands::voice_install,
            #[cfg(desktop)]
            commands::voice_speak,
            #[cfg(desktop)]
            commands::voice_stop,
            #[cfg(desktop)]
            commands::speech_status,
            #[cfg(desktop)]
            commands::input_devices,
            #[cfg(desktop)]
            commands::speech_install,
            commands::startup_settings,
            commands::save_startup_settings,
        ])
        .build(tauri::generate_context!())
        .expect("не удалось запустить приложение")
        .run(|_app, event| {
            // Программа живёт в трее и переживает свои окна.
            //
            // По умолчанию Tauri завершает приложение, когда закрылось последнее
            // окно. Для Суфлёра это означало вот что: человек закрывал окно
            // настройки — единственное открытое, — и вместе с ним выключался
            // перехват выделения. Со стороны выглядело так, будто программа
            // «сама отваливается, если ею не пользоваться».
            //
            // `code.is_none()` отличает этот случай от настоящего выхода: пункт
            // «Выйти» в трее зовёт `app.exit(0)`, и там код проставлен — такой
            // выход мы не отменяем.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Слушает клавиши голосового режима и раздаёт работу.
///
/// Хук обязан отвечать мгновенно, поэтому он только присылает сюда, что нажали,
/// а всё остальное — чтение вслух, запись голоса — происходит здесь, в обычном
/// потоке, где можно не торопиться.
#[cfg(desktop)]
fn listen_for_voice_keys(app: &tauri::AppHandle) {
    use tauri::Emitter;

    let events = voice::hotkey::install();
    let app = app.clone();

    std::thread::Builder::new()
        .name("sufler-voice".into())
        .spawn(move || {
            for event in events {
                match event {
                    // Что читать — знает окно: там и определение, и «простыми
                    // словами», и ветка вопросов. Rust хранил бы копию того же
                    // самого и неизбежно расходился бы с показанным.
                    voice::hotkey::Event::Speak => {
                        log::info!("пробел: читаю вслух");
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:speak", ());
                    }
                    // Зажали левый Alt с пробелом — пишем, пока держат.
                    voice::hotkey::Event::TalkStart => {
                        log::info!("Alt+пробел: слушаю");

                        // Замолчать, раз с нами заговорили.
                        //
                        // Это не только вежливость. Микрофон пишет то, что
                        // слышно в комнате, включая колонки: продолжай программа
                        // говорить — она записала бы собственный голос и
                        // прилежно расшифровала бы его как вопрос.
                        let was_speaking = voice::speaking();
                        voice::stop();

                        // Тишина наступает не мгновенно: последние доли секунды
                        // звука уже отданы звуковой системе и доиграют из её
                        // буфера, что бы мы ни делали. Включив микрофон сразу,
                        // мы записывали бы этот хвост, и в начало вопроса
                        // попадало слово-другое, сказанное самой программой.
                        //
                        // Ждём только если действительно говорили: молчаливый
                        // случай — обычный, и задерживать его нечем.
                        if was_speaking {
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                        let device = {
                            let state = app.state::<AppState>();
                            let device = state.config().voice.input_device.clone();
                            device
                        };
                        voice::stt::start(&device);
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", true);
                    }
                    // Отпустили — расшифровываем и отдаём окну как вопрос.
                    voice::hotkey::Event::TalkStop => {
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", false);

                        let Some(wav) = voice::stt::stop() else {
                            // Нажали и сразу отпустили или микрофона нет —
                            // сказать нечего, и молчание тут правильный ответ.
                            continue;
                        };

                        let (language, term) = {
                            let state = app.state::<AppState>();
                            let language = state.config().ui.language.clone();
                            // Выделенное слово — подсказка распознавателю:
                            // разговор идёт про него, и в вопросе оно прозвучит.
                            let term = state.selection().map(|s| s.text).unwrap_or_default();
                            (language, term)
                        };
                        // Поток обычный, а расшифровка ходит по сети (пусть и к
                        // себе же) — ждём её здесь, а не занимаем задачу Tauri.
                        let spoken = tauri::async_runtime::block_on(
                            voice::whisper::transcribe(&app, wav, &language, &term),
                        );
                        match spoken {
                            Ok(text) if !text.is_empty() => {
                                log::info!("расшифровано: «{text}»");
                                let _ = app.emit_to(overlay::POPUP_LABEL, "voice:question", text);
                            }
                            Ok(_) => log::info!("расшифровка пустая — тишина в записи"),
                            Err(err) => {
                                log::warn!("расшифровка не удалась: {err}");
                                let _ = app.emit_to(
                                    overlay::POPUP_LABEL,
                                    "voice:error",
                                    err.to_string(),
                                );
                            }
                        }
                    }
                }
            }
        })
        .ok();
}

#[cfg(not(desktop))]
fn listen_for_voice_keys(_app: &tauri::AppHandle) {}

/// Приводит запись в автозагрузке в соответствие с настройкой.
///
/// Сверяется при каждом запуске, а не только при изменении галочки: запись могла
/// исчезнуть помимо программы — переустановка, чистильщик автозагрузки, перенос
/// на другую машину. Молча не работать в таком случае хуже всего.
#[cfg(desktop)]
pub(crate) fn apply_autostart(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let wanted = app.state::<AppState>().config().startup.launch_at_login;
    let manager = app.autolaunch();

    if manager.is_enabled().unwrap_or(false) == wanted {
        return;
    }

    let result = if wanted { manager.enable() } else { manager.disable() };
    match result {
        Ok(()) => log::info!(
            "автозапуск при входе в систему: {}",
            if wanted { "включён" } else { "выключен" }
        ),
        Err(err) => log::warn!("не удалось изменить автозапуск: {err}"),
    }
}

#[cfg(not(desktop))]
pub(crate) fn apply_autostart(_app: &tauri::AppHandle) {}

/// Готовит локальную модель к работе: сервер, сама модель, прогрев.
///
/// Всё, что раньше человеку приходилось делать руками и в правильном порядке:
/// запустить Ollama, выбрать модель, дождаться, пока она скачается, и потерпеть
/// ещё раз при первом вопросе, пока она грузится в память. Каждый пункт по
/// отдельности мелочь, вместе — «включил компьютер, а оно опять не работает».
///
/// Отдельным потоком: здесь и сеть, и загрузка гигабайтов с диска, а запуск
/// программы ждать этого не должен — перехват выделения работает и без модели,
/// просто первый ответ придёт позже.
#[cfg(desktop)]
fn wake_local_model(app: &tauri::AppHandle) {
    let (endpoint, model) = {
        let state = app.state::<AppState>();
        let config = state.config();
        // Чужой облачный сервис поднимать нечем и незачем.
        if config.ai.provider != "http" {
            return;
        }
        (config.ai.endpoint.clone(), config.ai.model.clone())
    };

    // host_from отдаёт местный адрес и для облачных сервисов — там будить
    // нечего, поэтому проверяем, что настроен именно свой компьютер.
    if !crate::config::is_local(&endpoint) {
        return;
    }
    let host = crate::ollama::host_from(&endpoint);
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        /* 1. Сервер. */
        let mut status = crate::ollama::status(&host).await;
        if !status.running {
            match crate::ollama::start() {
                Ok(()) => log::info!("Ollama не отвечала — запустили её сами"),
                Err(err) => {
                    log::warn!("Ollama не отвечает и запустить не вышло: {err}");
                    return;
                }
            }

            // Сервер поднимается не мгновенно, а дальше без него делать нечего.
            // Полминуты — с запасом даже для медленного диска.
            for _ in 0..60 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                status = crate::ollama::status(&host).await;
                if status.running {
                    break;
                }
            }
            if !status.running {
                log::warn!("Ollama не ответила за полминуты после запуска");
                return;
            }
        }

        /* 2. Модель. */
        let model = if model.trim().is_empty() {
            let hardware = crate::ollama::hardware();
            let chosen = crate::ollama::pick(&hardware);
            log::info!(
                "модель не выбрана: {:.1} ГБ видеопамяти, {:.1} ГБ ОЗУ — берём {chosen}",
                hardware.vram_gb,
                hardware.ram_gb
            );

            // Записываем сразу: иначе при каждом запуске модель выбиралась бы
            // заново, а окно настройки показывало бы пустое поле при работающей
            // программе — и было бы непонятно, чем она вообще отвечает.
            {
                let state = app.state::<AppState>();
                {
                    let mut config = state.config_mut();
                    config.ai.model = chosen.to_string();
                }
                if let Err(err) = crate::commands::persist(&app, &state) {
                    log::warn!("выбранная модель не сохранилась: {err}");
                }
                let config = state.config();
                state.rebuild_provider(&config.ai, &config.ui.language);
            }

            chosen.to_string()
        } else {
            model
        };

        // Ollama зовёт модели полным именем с тегом; в настройках тег могли и
        // не написать. `llama3` и `llama3:latest` — одно и то же.
        let installed = status
            .installed
            .iter()
            .any(|m| m.name == model || m.name == format!("{model}:latest"));

        if !installed {
            log::info!("модели {model} на диске нет — скачиваем");
            if let Err(err) = crate::ollama::pull(app.clone(), host.clone(), model.clone()).await {
                log::warn!("не удалось скачать {model}: {err}");
                return;
            }
            log::info!("модель {model} скачана");
        }

        /* 3. Прогрев. */
        match crate::ollama::preload(&host, &model).await {
            Ok(()) => log::info!("модель {model} загружена в память и ждёт вопросов"),
            Err(err) => log::warn!("{err}"),
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
