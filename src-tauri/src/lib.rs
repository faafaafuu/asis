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
mod jobs;
mod overlay;
mod planner;
mod secret;
mod tasks;
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
            // Список задач лежит рядом с настройками и читается один раз.
            if let Some(dir) = config_dir.clone() {
                tasks::load(dir);
            }
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

                // Раньше микрофона и раньше окон: устройства должны достаться
                // потоку, который живёт до конца работы.
                voice::claim_devices();
                wake_local_model(app.handle());
                listen_for_voice_keys(app.handle());
                watch_reminders(app.handle());
                start_wake(app.handle());
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
            commands::pending_open,
            commands::close_popup,
            commands::popup_active,
            commands::popup_taken_over,
            commands::open_tasks,
            commands::close_tasks,
            commands::task_list,
            commands::task_add,
            commands::task_done,
            commands::task_edit,
            commands::task_remove,
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
            commands::open_key_page,
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
            commands::hud_mode,
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

/// Идёт ли сейчас разговор без рук.
#[cfg(desktop)]
static CONVERSATION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Слушает клавиши голосового режима и раздаёт работу.
///
/// Хук обязан отвечать мгновенно, поэтому он только присылает сюда, что нажали,
/// а всё остальное — чтение вслух, запись голоса, разговор — происходит здесь,
/// в обычном потоке, где можно не торопиться.
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
                    voice::hotkey::Event::Speak => {
                        // Тем же пробелом и начинают читать, и обрывают чтение.
                        //
                        // Прервали — значит услышали достаточно и хотят сказать
                        // своё, а не молча смотреть в текст. Поэтому сразу за
                        // тишиной начинается слушание.
                        if voice::speaking() {
                            log::info!("пробел: обрываю чтение и слушаю");
                            voice::stop();
                            // Звук уходит не мгновенно: то, что уже отдано
                            // звуковой системе, доигрывает из её буфера. Включив
                            // микрофон раньше, мы записывали бы конец собственной
                            // фразы и отвечали сами себе.
                            std::thread::sleep(std::time::Duration::from_millis(700));
                            start_conversation(&app);
                            continue;
                        }

                        log::info!("пробел: читаю вслух");
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:speak", ());
                        show_speaking(&app);
                    }
                    // Ctrl с Alt и пробелом — включить или выключить ожидание
                    // обращения. Отдельным сочетанием, потому что просьба была
                    // именно такая: чтобы память не занималась впустую, когда
                    // помощник не нужен.
                    voice::hotkey::Event::ToggleWake => toggle_wake(&app),
                    // Зажали левый Alt с пробелом — пишем, пока держат. Работает
                    // и при закрытом окне: тогда вопрос задаётся с чистого
                    // места, а окно откроется само вместе с ответом.
                    voice::hotkey::Event::TalkStart => {
                        log::info!("Alt+пробел: слушаю");

                        // Разговор, если он шёл, уступает клавише: человек взял
                        // слово сам, и слушать его надо с этой секунды. Без
                        // сигнала — разговор не кончился, он продолжается.
                        end_conversation(&app, false, false);
                        stop_wake();

                        // Замолчать, раз с нами заговорили. Это не только
                        // вежливость: микрофон пишет всё, что слышно в комнате,
                        // включая колонки.
                        let was_speaking = voice::speaking();
                        voice::stop();

                        // Тишина наступает не мгновенно: последние доли секунды
                        // звука уже отданы звуковой системе и доиграют из её
                        // буфера. Включив микрофон сразу, мы записали бы этот
                        // хвост, и в начало вопроса попало бы слово-другое,
                        // сказанное самой программой.
                        if was_speaking {
                            std::thread::sleep(std::time::Duration::from_millis(700));
                        }

                        // Сервер расшифровки поднимается прямо сейчас, пока
                        // человек ещё говорит: иначе первая фраза ждала бы его
                        // запуска уже после того, как её произнесли.
                        voice::whisper::warm(&app);
                        // Порядок важен: сперва показать помощника — вместе
                        // с ним звучит сигнал появления, — дождаться, пока он
                        // отзвучит, и только потом открывать микрофон. Иначе
                        // первое, что запишется, будет собственный сигнал.
                        //
                        // Заодно это подсказка человеку: сигнал отзвучал —
                        // можно говорить.
                        overlay::show_hud(&app, "listening");
                        std::thread::sleep(voice::chime_length());
                        voice::stt::start(&input_device(&app));
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", true);
                    }
                    // Отпустили — расшифровываем, задаём вопрос и переходим
                    // к разговору без клавиш.
                    voice::hotkey::Event::TalkStop => {
                        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", false);

                        let Some(wav) = voice::stt::stop() else {
                            // Нажали и сразу отпустили или микрофона нет —
                            // сказать нечего, и молчание тут правильный ответ.
                            overlay::hide_hud(&app);
                            continue;
                        };

                        match hear(&app, wav) {
                            // Попрощались, не начав: разговор и не начинаем.
                            Some(text) if is_farewell(&text) => {
                                log::info!("попрощались («{text}») — не начинаю разговор");
                                overlay::hide_hud(&app);
                                voice::chime();
                            }
                            Some(text) => {
                                answer_aloud(&app, &text);
                                start_conversation(&app);
                            }
                            None => overlay::hide_hud(&app),
                        }
                    }
                }
            }
        })
        .ok();
}

/// Шлёт индикатору громкость речи, пока она звучит.
///
/// Двадцать раз в секунду: чаще глаз в анимации не различит, реже — кольцо
/// начинает отставать от голоса, и синхронность теряется.
#[cfg(desktop)]
fn pulse_level(app: &tauri::AppHandle) {
    use tauri::Emitter;

    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-level".into())
        .spawn(move || {
            while voice::speaking() {
                let _ = app.emit_to(overlay::HUD_LABEL, "hud:level", voice::level());
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // Ровно ноль в конце: иначе кольцо замрёт на последнем всплеске.
            let _ = app.emit_to(overlay::HUD_LABEL, "hud:level", 0.0_f32);
        })
        .ok();
}

/// Показывает индикатор на время чтения вслух и убирает, когда речь кончилась.
///
/// Отдельным потоком: чтение длится десятки секунд, а обработчик клавиш обязан
/// вернуться немедленно — за ним очередь из остальных нажатий.
#[cfg(desktop)]
fn show_speaking(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;

    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-hud".into())
        .spawn(move || {
            // Речь начинается не мгновенно: синтезатору нужно запуститься.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline && !voice::speaking() {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            if !voice::speaking() {
                return;
            }

            overlay::show_hud(&app, "speaking");
            pulse_level(&app);
            while voice::speaking() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // В разговоре индикатор не убираем — там своя очередь состояний.
            if !CONVERSATION.load(Ordering::SeqCst) {
                overlay::hide_hud(&app);
            }
        })
        .ok();
}

/// Микрофон из настроек.
#[cfg(desktop)]
fn input_device(app: &tauri::AppHandle) -> String {
    let state = app.state::<AppState>();
    let device = state.config().voice.input_device.clone();
    device
}

/// Расшифровывает запись и отдаёт услышанное.
///
/// Отдельно от ответа, потому что не на всё услышанное надо отвечать: на
/// «спасибо, до связи» разговор просто заканчивается, и гонять ради этого
/// модель, а потом ещё и читать её ответ вслух — заставлять человека ждать
/// там, где он уже попрощался.
#[cfg(desktop)]
fn hear(app: &tauri::AppHandle, wav: Vec<u8>) -> Option<String> {
    overlay::show_hud(app, "thinking");
    hear_quietly(app, wav)
}

/// То же, но молча: без индикатора и без жалоб в журнал.
///
/// Так слушается комната в ожидании обращения. Показывать «думаю» на каждую
/// чужую фразу в комнате нельзя — помощник мигал бы весь день, а человек
/// решил бы, что программа подслушивает и что-то делает.
#[cfg(desktop)]
fn hear_quietly(app: &tauri::AppHandle, wav: Vec<u8>) -> Option<String> {
    hear_hinted(app, wav, "")
}

/// Подсказка распознавателю на время ожидания обращения.
///
/// Без неё имя не выживает. «Ноа» короткое, безударное и для русской речи
/// непривычное — распознаватель раз за разом слышит на его месте обычные слова:
/// «эй, но», «эй, ну», «эй, на». Подсказка — это начало текста, от которого
/// модель пляшет дальше; увидев в нём нужное написание, она выбирает его и в
/// расшифровке.
#[cfg(desktop)]
const WAKE_HINT: &str = "Ноа — имя голосового помощника. Ноа, слышишь? Ноа, что это?";

/// То же, но с подсказкой о том, что мы ждём услышать.
#[cfg(desktop)]
fn hear_hinted(app: &tauri::AppHandle, wav: Vec<u8>, hint: &str) -> Option<String> {
    use tauri::Emitter;


    let (language, term) = {
        let state = app.state::<AppState>();
        let language = state.config().ui.language.clone();
        // Выделенное слово — подсказка распознавателю: разговор идёт про него,
        // и в вопросе оно прозвучит.
        let term = state.selection().map(|s| s.text).unwrap_or_default();
        (language, term)
    };
    let term = if hint.is_empty() { term } else { hint.to_string() };

    // Поток обычный, а расшифровка ходит по сети (пусть и к себе же) — ждём
    // её здесь, а не занимаем задачу Tauri.
    let spoken = tauri::async_runtime::block_on(voice::whisper::transcribe(
        app, wav, &language, &term,
    ));

    let text = match spoken {
        Ok(text) if !text.is_empty() => text,
        Ok(_) => {
            log::info!("расшифровка пустая — тишина или выдумка модели");
            return None;
        }
        Err(err) => {
            log::warn!("расшифровка не удалась: {err}");
            let _ = app.emit_to(overlay::POPUP_LABEL, "voice:error", err.to_string());
            return None;
        }
    };
    log::info!("расшифровано: «{text}»");
    Some(text)
}

/// Задаёт вопрос окну и ждёт, пока ответ дочитают вслух.
///
/// Ждать конца ответа обязательно: следующая фраза человека должна попасть
/// в тишину, а не поверх ответа на предыдущую.
#[cfg(desktop)]
fn answer_aloud(app: &tauri::AppHandle, text: &str) {
    use tauri::Emitter;

    let text = text.to_string();
    if overlay::is_popup_visible(app) {
        let _ = app.emit_to(overlay::POPUP_LABEL, "voice:question", text.clone());
    } else {
        // Окна нет — вопрос задан с чистого места, окно откроется этим вопросом.
        if let Err(err) = overlay::show_for_voice(app, text.clone()) {
            log::error!("не удалось открыть окно на голосовой вопрос: {err}");
            return;
        }
    }

    wait_until_answered(app);
}

/// Ждёт, пока ответ начнут и закончат читать вслух.
#[cfg(desktop)]
fn wait_until_answered(app: &tauri::AppHandle) {
    use std::time::{Duration, Instant};

    // Голос выключен — ждать нечего: ответ придёт текстом, и разговор должен
    // продолжиться сразу, а не простоять полминуты в надежде услышать речь.
    {
        let state = app.state::<AppState>();
        let enabled = state.config().voice.enabled;
        if !enabled {
            return;
        }
    }

    // Пока модель думает, речи ещё нет. Ждём её начала — и ждать надо столько,
    // сколько модель имеет право думать.
    //
    // Десяти секунд не хватало: на модели покрупнее ответ приходит позже, мы
    // переставали ждать, снова начинали слушать — а человек в это время читал
    // ответ и молчал. Тишина копилась, и разговор закрывался сам, не ответив
    // ни разу. Предел берём тот же, что у самого запроса к модели.
    let limit = {
        let state = app.state::<AppState>();
        let limit = state.config().ai.call_limit();
        limit
    };
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline && !voice::speaking() {
        std::thread::sleep(Duration::from_millis(100));
    }

    if voice::speaking() {
        overlay::show_hud(app, "speaking");
        pulse_level(app);
    }

    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline && voice::speaking() {
        std::thread::sleep(Duration::from_millis(100));
    }

    // И ещё немного: последние доли секунды звука доигрывают из буфера звуковой
    // системы уже после того, как мы перестали его туда отдавать.
    std::thread::sleep(Duration::from_millis(300));
}

/// Как здороваются перед именем: «хэй», «эй», «привет».
///
/// Вариантов много, потому что распознавание пишет одно и то же слово
/// по-разному от раза к разу — «хэй», «хей», «эй», «хай».
#[cfg(desktop)]
const WAKE_GREETINGS: &[&str] = &["хэй", "хей", "эй", "хай", "привет", "hey", "hi", "окей"];

/// Имя помощника — и всё, во что распознавание его превращает.
///
/// «Ноа» короткое и звучит непривычно для русского уха, поэтому Whisper
/// пишет его то «Ноя», то «Ной», то латиницей. Ловим все написания: не
/// услышать обращение хуже, чем изредка проснуться на похожее слово.
#[cfg(desktop)]
const WAKE_NAMES: &[&str] = &["ноа", "ноях", "ноах", "ноуа", "noa", "noah", "нова"];

/// Оклик — приветствие, которым зовут, а не здороваются.
///
/// Список — только образцы: похожие слова узнаются сами, см. `close_enough`.
#[cfg(desktop)]
const WAKE_CALLS: &[&str] = &[
    "хэй", "хей", "хай", "хэи", "хеи", "эй", "эи", "ай", "ой", "hey", "hei", "hай",
];

/// Насколько слово похоже на образец: сколько букв надо поменять.
///
/// Расшифровка одного и того же оклика гуляет от раза к разу — «хэй», «эй»,
/// «ай», «хей», — и перечислять написания бесполезно: следующее всё равно
/// окажется новым. Одна буква расхождения ловит их все и при этом не задевает
/// обычную речь: до «хэй» на одну букву не дотягивается ни одно частое слово.
///
/// Считается расстояние Левенштейна с ранним выходом — слова здесь в три-четыре
/// буквы, дороже ничего не нужно.
#[cfg(desktop)]
fn close_enough(word: &str, sample: &str, allowed: usize) -> bool {
    if word == sample {
        return true;
    }
    let (a, b): (Vec<char>, Vec<char>) = (word.chars().collect(), sample.chars().collect());
    if a.len().abs_diff(b.len()) > allowed {
        return false;
    }

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ai) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, bj) in b.iter().enumerate() {
            let cost = usize::from(ai != bj);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        // Вся строка хуже допуска — дальше будет только хуже.
        if current.iter().min().copied().unwrap_or(usize::MAX) > allowed {
            return false;
        }
        previous = current;
    }
    previous[b.len()] <= allowed
}

/// Оклик ли это — пусть и расслышанный неточно.
#[cfg(desktop)]
fn is_call(word: &str) -> bool {
    // Короткие слова с допуском в букву цепляют слишком многое: «он», «а»,
    // «эх». Поэтому образцы длиной от трёх букв сравниваем нестрого, а «эй» —
    // только точно.
    WAKE_CALLS.iter().any(|sample| {
        if sample.chars().count() >= 3 {
            close_enough(word, sample, 1)
        } else {
            word == *sample
        }
    })
}

/// Имя ли это — после того, как прозвучал оклик.
///
/// После оклика узнаём щедро, и вот почему это безопасно. Чтобы сюда дойти,
/// фраза уже должна начинаться с «эй», «хэй», «ай» — с оклика, а не с обычных
/// слов. В комнате так почти не говорят, а к программе — только так. Значит,
/// цена ошибки здесь мала (помощник откроется зря), а цена строгости велика:
/// именно на строгости зов и не срабатывал девять раз из десяти.
///
/// Поэтому считаем именем всё короткое на «н» — «но», «ну», «на», «ной», «ноу»,
/// как бы его ни расслышали, — плюс всё, что на букву отличается от образцов.
#[cfg(desktop)]
fn is_name_after_call(word: &str) -> bool {
    if WAKE_NAMES.iter().any(|sample| close_enough(word, sample, 1)) {
        return true;
    }
    if WAKE_NAMES_AFTER_GREETING.contains(&word) {
        return true;
    }

    // Три буквы — предел: «ноа» и всё, во что оно превращается, короче. Слова
    // подлиннее («надо», «наверное», «Настя») именем уже не считаем.
    let letters = word.chars().count();
    letters <= 3
        && word
            .chars()
            .next()
            .map(|first| first == 'н' || first == 'n')
            .unwrap_or(false)
}

/// Имя ли это, сказанное само по себе, без оклика.
///
/// Здесь строже, чем после оклика, и по понятной причине: обращением считается
/// начало обычной фразы, а начинаются они как угодно. Поэтому точное написание
/// принимается всегда, а близкое — только когда за именем пауза.
///
/// Пауза и решает дело. «Ноа, что такое альбедо» — обращение: человек назвал
/// имя и остановился, расшифровка поставила запятую. «Ночь была тихая» — не
/// обращение: там слово тянет за собой продолжение, и никакой паузы за ним нет.
/// Без этой проверки пришлось бы выбирать между «зовёшь и не слышит» и
/// «срабатывает на каждое второе слово на „но“».
#[cfg(desktop)]
fn is_name_alone(word: &str, paused_after: bool) -> bool {
    if WAKE_NAMES.contains(&word) {
        return true;
    }
    if !paused_after {
        return false;
    }
    // Двухбуквенные «но», «ну», «на» сюда не попадают намеренно: даже с паузой
    // они начинают слишком много обычных фраз.
    word.chars().count() >= 3
        && WAKE_NAMES
            .iter()
            .any(|sample| close_enough(word, sample, 1))
}

/// Слипшийся оклик с именем: «хэйноа», «эйноа» — распознаватель и так умеет.
#[cfg(desktop)]
const WAKE_GLUED: &[&str] = &["хэйноа", "хейноа", "эйноа", "хайноа", "heynoa", "хэйноя"];

/// Написания, которые засчитываются только сразу после оклика.
///
/// «Ноа» безударное, и распознаватель постоянно подменяет его обычными словами:
/// «эй, но», «эй, ну», «эй, на». Считать их именем где угодно нельзя — так
/// начинается едва ли не каждая вторая фраза в комнате. Но после «эй» или «хэй»
/// в самом начале фразы других кандидатов нет: к программе обратились.
#[cfg(desktop)]
const WAKE_NAMES_AFTER_GREETING: &[&str] = &[
    "но", "ну", "на", "ной", "ноя", "нуа", "ноу", "нау", "нора", "нюа", "know", "now", "no",
];

/// Отделяет обращение от вопроса.
///
/// Возвращает то, что сказано после имени: «хэй ноа, что такое альбедо» даёт
/// «что такое альбедо». Пустая строка — значит позвали и замолчали, тогда
/// помощник просто начинает слушать.
#[cfg(desktop)]
fn wake_split(text: &str) -> Option<String> {
    // Слова — для узнавания, границы — чтобы вернуть вопрос как он был сказан.
    //
    // Возвращать разобранные слова нельзя: обращение отрезается вместе со
    // знаками препинания и заглавными, и модель получает «что такое альбедо»
    // вместо «Что такое альбедо?». Разница видна в ответах — на обкромсанном
    // вопросе они заметно беспомощнее, что вживую и наблюдалось: на клавишу
    // помощник отвечал толково, а на зов по имени — бестолково, хотя модель
    // и там и там одна.
    let mut words: Vec<(String, usize)> = Vec::new();
    let mut word = String::new();
    for (at, letter) in text.char_indices() {
        if letter.is_alphanumeric() {
            word.extend(letter.to_lowercase());
        } else if !word.is_empty() {
            words.push((std::mem::take(&mut word), at));
        }
    }
    if !word.is_empty() {
        words.push((word, text.len()));
    }
    if words.is_empty() {
        return None;
    }

    // Всё, что сказано после обращения, — слово в слово, без ведущих знаков.
    let tail = |ends_at: usize| -> String {
        text[ends_at..]
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim()
            .to_string()
    };

    // Имя в самом начале — уже обращение: «Ноа, что такое альбедо».
    // Пауза — это знак препинания или конец фразы, а не просто пробел. Пробел
    // стоит после любого слова и ничего не значит; запятая после имени — значит.
    let paused_after_first = text[words[0].1..]
        .chars()
        .next()
        .map(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .unwrap_or(true);
    if is_name_alone(&words[0].0, paused_after_first) {
        return Some(tail(words[0].1));
    }

    // Оклик и имя, слипшиеся в одно слово: паузы между ними нет, и расшифровка
    // то разделяет их, то нет.
    if WAKE_GLUED
        .iter()
        .any(|sample| close_enough(&words[0].0, sample, 1))
    {
        return Some(tail(words[0].1));
    }

    // Иначе ищем пару «приветствие + имя», но только в начале фразы: имя,
    // произнесённое в середине разговора о чём-то другом, обращением не является.
    // После приветствия имя узнаём щедрее — см. `WAKE_NAMES_AFTER_GREETING`.
    for i in 0..words.len().saturating_sub(1).min(3) {
        let called = is_call(&words[i].0);
        let greeted = called || WAKE_GREETINGS.contains(&words[i].0.as_str());
        let named = WAKE_NAMES.contains(&words[i + 1].0.as_str())
            // Спорные написания — только после оклика: «привет, ну как дела»
            // и «окей, но зачем» обращением не являются, а «эй, ну» — является.
            || (called && is_name_after_call(&words[i + 1].0));
        if greeted && named {
            return Some(tail(words[i + 1].1));
        }
    }
    None
}

/// Слова, которыми разговор заканчивают, где бы они ни стояли во фразе.
///
/// Все они настолько однозначны, что посреди вопроса не встречаются: «до
/// связи», «до свидания», «спасибо».
#[cfg(desktop)]
const FAREWELL_ANYWHERE: &[&str] = &[
    "спасибо",
    "до свидания",
    "досвидания",
    "до завтра",
    "до встречи",
    "до связи",
    "до скорого",
    "всего доброго",
    "всего хорошего",
    "прощай",
    "прощаем",
    "прощаюсь",
    // «Давай прощаться», «пора прощаться» — намерение то же самое.
    "прощаться",
    "заканчиваем",
    "заканчивай",
    "закончим на этом",
    "на этом всё",
    "на этом все",
    "бывай",
    "удачи",
    "спокойной ночи",
    "хорошего дня",
    "хорошего вечера",
];

/// Прощания, которые считаются только если ими фраза и исчерпывается.
///
/// «Пока» — это ещё и союз: «пока я думал», «пока не понял», «подожди, пока
/// объяснишь». Проверка на вхождение обрывала бы разговор ровно посреди
/// вопроса, поэтому такие слова засчитываются только когда сказано именно
/// прощание и ничего больше.
#[cfg(desktop)]
const FAREWELL_ALONE: &[&str] = &["пока", "покеда", "чао", "бай", "адьос"];

/// Слова, которые в прощании ничего не значат и мешают его узнать:
/// «ну всё, пока», «ладно, пока», «ок, пока».
#[cfg(desktop)]
const FILLER: &[&str] = &[
    "ну", "всё", "все", "ладно", "хорошо", "ок", "окей", "давай", "тогда", "и", "а", "так",
];

/// Прощаются ли с программой.
#[cfg(desktop)]
fn is_farewell(text: &str) -> bool {
    let lower = text.to_lowercase();
    if FAREWELL_ANYWHERE.iter().any(|word| lower.contains(word)) {
        return true;
    }

    // Убираем знаки и незначащие слова — остаться должно только прощание.
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .collect();

    if !words.is_empty() && words.iter().all(|w| FAREWELL_ALONE.contains(w)) {
        return true;
    }

    closed_with_goodbye(&lower)
}

/// Стоит ли «пока» в конце мысли, а не в начале придаточного.
///
/// Разницу слышно по паузе, а в расшифровке она видна как знак препинания.
/// «Давай, пока, раз всё хорошо» — прощание: после «пока» человек остановился.
/// «Пока я думал» — не прощание: там слово тянет за собой продолжение и никакой
/// паузы за ним нет. Без этой проверки пришлось бы выбирать между «не узнаём
/// прощание» и «обрываем разговор посреди вопроса»; знак препинания разводит
/// эти случаи там, где список слов бессилен.
#[cfg(desktop)]
fn closed_with_goodbye(lower: &str) -> bool {
    for word in FAREWELL_ALONE {
        let mut from = 0;
        while let Some(at) = lower[from..].find(*word) {
            let start = from + at;
            let end = start + word.len();
            from = end;

            // Слово целиком, а не кусок другого: «пока» в «покажи» — не прощание.
            let before_ok = lower[..start]
                .chars()
                .next_back()
                .map_or(true, |c| !c.is_alphabetic());
            if !before_ok {
                continue;
            }
            let after = lower[end..].trim_start_matches([' ', '\u{a0}']);
            match after.chars().next() {
                // Фраза кончилась прощанием.
                None => return true,
                // За прощанием пауза — значит, мысль на нём закрыта.
                Some(c) if !c.is_alphanumeric() => return true,
                _ => continue,
            }
        }
    }
    false
}

/// Начинает разговор без рук: слушаем, отвечаем, снова слушаем.
#[cfg(desktop)]
fn start_conversation(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    if CONVERSATION.swap(true, Ordering::SeqCst) {
        return;
    }
    // Микрофон один: фоновое слушание уступает разговору.
    stop_wake();

    let Some(phrases) = voice::stt::start_conversation(&input_device(app), voice::stt::Listening::Talk)
    else {
        CONVERSATION.store(false, Ordering::SeqCst);
        overlay::hide_hud(app);
        return;
    };

    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-talk".into())
        .spawn(move || {
            log::info!("разговор начат: клавиши не нужны, «спасибо» или молчание завершают");
            overlay::show_hud(&app, "listening");
            let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", true);

            for heard in phrases {
                if !CONVERSATION.load(Ordering::SeqCst) {
                    break;
                }

                let wav = match heard {
                    voice::stt::Heard::LongSilence => {
                        log::info!("тишина затянулась — разговор окончен");
                        break;
                    }
                    voice::stt::Heard::Phrase(wav) => wav,
                };

                // Пока думаем и отвечаем — не слушаем: иначе в следующую фразу
                // попадёт собственный ответ.
                voice::stt::pause_conversation(true);
                let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", false);

                match hear(&app, wav) {
                    Some(text) if is_farewell(&text) => {
                        log::info!("попрощались («{text}») — разговор окончен");
                        break;
                    }
                    // Распоряжение о задачах выполняется здесь же и до модели:
                    // «напомни завтра позвонить» — не вопрос, отвечать на него
                    // объяснением было бы нелепо.
                    Some(text) if handled_as_task(&app, &text) => {}
                    Some(text) => answer_aloud(&app, &text),
                    None => {}
                }

                if !CONVERSATION.load(Ordering::SeqCst) {
                    break;
                }
                voice::stt::pause_conversation(false);
                overlay::show_hud(&app, "listening");
                let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", true);
            }

            finish_conversation(&app);
        })
        .ok();
}

/// Слушаем ли комнату в ожидании обращения.
#[cfg(desktop)]
static WAKE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Начинает слушать комнату, чтобы услышать «хэй, ноа».
///
/// Не начинает, если голос выключен, обращение отключено галочкой, распознавание
/// не скачано или прямо сейчас идёт разговор: микрофон один, и занят он может
/// быть только чем-то одним.
#[cfg(desktop)]
pub(crate) fn start_wake(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;

    let (enabled, device) = {
        let state = app.state::<AppState>();
        let config = state.config();
        (
            config.voice.enabled && config.voice.wake_word,
            config.voice.input_device.clone(),
        )
    };
    if !enabled || CONVERSATION.load(Ordering::SeqCst) || !voice::whisper::ready(app) {
        return;
    }
    if WAKE.swap(true, Ordering::SeqCst) {
        return;
    }

    // Распознавание понадобится на каждую фразу — поднимаем сервер заранее.
    voice::whisper::warm(app);

    let Some(phrases) = voice::stt::start_conversation(&device, voice::stt::Listening::Wake) else {
        WAKE.store(false, Ordering::SeqCst);
        return;
    };

    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-wake".into())
        .spawn(move || {
            log::info!("слушаю обращение по имени «Ноа»");

            for heard in phrases {
                if !WAKE.load(Ordering::SeqCst) {
                    break;
                }
                // Долгая тишина в комнате — обычное дело: ждём дальше.
                let voice::stt::Heard::Phrase(wav) = heard else {
                    continue;
                };

                let Some(text) = hear_hinted(&app, wav, WAKE_HINT) else {
                    continue;
                };
                let Some(question) = wake_split(&text) else {
                    // Говорили не с нами — забываем и слушаем дальше.
                    continue;
                };

                log::info!("позвали: «{text}»");
                stop_wake();

                // Показываемся немедленно, ещё до ответа. Раньше индикатор
                // появлялся только вместе с разговором — то есть после того,
                // как модель додумает; всё это время на зов не отзывалось
                // ничего, и выглядело это как «не услышал».
                overlay::show_hud(&app, "thinking");

                // Вопрос сказан той же фразой — отвечаем на него сразу.
                // Позвали и замолчали — просто слушаем дальше, вопрос впереди.
                if question.trim().is_empty() {
                    log::info!("позвали без вопроса — жду его");
                } else if !handled_as_task(&app, &question) {
                    answer_aloud(&app, &question);
                }
                start_conversation(&app);
                break;
            }
        })
        .ok();
}

/// Перестаёт слушать комнату.
#[cfg(desktop)]
pub(crate) fn stop_wake() {
    use std::sync::atomic::Ordering;

    if !WAKE.swap(false, Ordering::SeqCst) {
        return;
    }
    voice::stt::stop_conversation();
}

/// Включает или выключает ожидание обращения — и говорит об этом.
///
/// Выключение не просто перестаёт слушать: оно ещё и останавливает сервер
/// расшифровки, освобождая полтора гигабайта видеопамяти. В этом и смысл
/// переключателя — не в том, чтобы программа молчала, а в том, чтобы она
/// не занимала машину, когда не нужна.
#[cfg(desktop)]
fn toggle_wake(app: &tauri::AppHandle) {
    let now_on = {
        let state = app.state::<AppState>();
        let next = !state.config().voice.wake_word;
        state.config_mut().voice.wake_word = next;
        next
    };

    {
        let state = app.state::<AppState>();
        if let Err(err) = commands::persist(app, &state) {
            log::warn!("настройка пробуждения не сохранилась: {err}");
        }
    }

    // Окно настройки, если оно открыто, должно показать это галочкой: человек
    // нажал сочетание и смотрит в него — расхождение читается как «не сработало».
    {
        use tauri::Emitter;
        let _ = app.emit_to(overlay::ONBOARDING_LABEL, "voice:wake", now_on);
    }

    if now_on {
        log::info!("ожидание обращения включено");
        start_wake(app);
        // Тот же сигнал, что и на появление помощника: включили — он здесь.
        voice::chime_open();
    } else {
        log::info!("ожидание обращения выключено");
        stop_wake();
        // Идущий разговор трогать нельзя: там распознавание нужно прямо сейчас.
        // Память освободится, когда он закончится.
        if !CONVERSATION.load(std::sync::atomic::Ordering::SeqCst) {
            voice::release_speech();
        }
        voice::chime();
    }
}

/// Перестроить слушателя после смены настроек.
#[cfg(desktop)]
pub(crate) fn restart_wake(app: &tauri::AppHandle) {
    stop_wake();
    start_wake(app);

    // Выключили галочкой — освобождаем видеопамять, как и при выключении
    // сочетанием клавиш. Разговор при этом трогать нельзя: там сервер нужен.
    use std::sync::atomic::Ordering;
    if !WAKE.load(Ordering::SeqCst) && !CONVERSATION.load(Ordering::SeqCst) {
        voice::release_speech();
    }
}

#[cfg(not(desktop))]
pub(crate) fn restart_wake(_app: &tauri::AppHandle) {}

/// Как часто проверять, не пора ли о чём-то напомнить.
///
/// Полминуты хватает: сроки человек ставит с точностью до минуты, и опоздание
/// на полминуты незаметно. Чаще — впустую будить процессор, реже — заметно.
#[cfg(desktop)]
const REMINDER_STEP: std::time::Duration = std::time::Duration::from_secs(30);

/// Следит за сроками задач и напоминает о них.
#[cfg(desktop)]
fn watch_reminders(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-reminders".into())
        .spawn(move || loop {
            std::thread::sleep(REMINDER_STEP);
            for task in tasks::take_due(chrono::Local::now()) {
                remind(&app, &task);
            }
        })
        .ok();
}

/// Напоминает об одной задаче.
#[cfg(desktop)]
fn remind(app: &tauri::AppHandle, task: &tasks::Task) {
    log::info!("напоминаю: «{}»", task.title);
    announce(app, format!("Напоминаю: {}", task.title), false);
}

/// Говорит готовую фразу: показывает её окном и произносит вслух.
///
/// Готовую — то есть ту, которую не надо ни у кого спрашивать: напоминание о
/// задаче или ответ на распоряжение. Окно показывается всегда, речь — только
/// если голос включён: сообщение, которое нельзя ни увидеть, ни услышать,
/// ничем не отличается от забытого.
///
/// `wait` — дождаться конца речи. Нужно в разговоре: следующая фраза человека
/// должна попасть в тишину, а не поверх ответа.
#[cfg(desktop)]
fn announce(app: &tauri::AppHandle, text: String, wait: bool) {
    if let Err(err) = overlay::show_for_reminder(app, text.clone()) {
        log::warn!("окно сообщения не открылось: {err}");
    }

    if !app.state::<AppState>().config().voice.enabled {
        return;
    }

    let handle = app.clone();
    let speaking = move || {
        let config = handle.state::<AppState>().config().voice.clone();
        overlay::show_hud(&handle, "speaking");
        if let Err(err) = tauri::async_runtime::block_on(voice::speak(&handle, &config, &text)) {
            log::warn!("сказать не вышло: {err}");
        }
        // Индикатор относится к речи, а не к окну, которое человек читает
        // дальше, — убираем его, когда речь отзвучит.
        while voice::speaking() {
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        overlay::hide_hud(&handle);
    };

    if wait {
        speaking();
    } else {
        std::thread::Builder::new()
            .name("sufler-say".into())
            .spawn(speaking)
            .ok();
    }
}

/// Не распоряжение ли это о задачах. Если да — выполняет и отвечает вслух.
///
/// Возвращает `true`, когда фраза была распоряжением и обработана: спрашивать
/// про неё модель уже не нужно.
#[cfg(desktop)]
fn handled_as_task(app: &tauri::AppHandle, text: &str) -> bool {
    let Some(reply) = tauri::async_runtime::block_on(planner::handle(app, text)) else {
        return false;
    };
    log::info!("распоряжение о задачах: «{text}» → «{reply}»");
    announce(app, reply, true);
    true
}

/// Идёт ли разговор без рук.
#[cfg(desktop)]
pub(crate) fn in_conversation() -> bool {
    CONVERSATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// Заканчивает разговор: микрофон закрывается, клавиши работают как прежде.
#[cfg(desktop)]
pub(crate) fn stop_conversation(app: &tauri::AppHandle) {
    end_conversation(app, true, false);
}

/// Заканчивает разговор, который выдохся сам: попрощались или замолчали.
///
/// Отличается от `stop_conversation` тем, что убирает и окно. Разговор шёл
/// голосом, окно при нём — не рабочее место, а расшифровка сказанного; когда
/// попрощались, оставлять его висеть поверх чужой работы незачем.
///
/// Наоборот делать нельзя: окно закрывают и вручную, и тогда разговор кончается
/// вместе с ним — если бы конец разговора в свою очередь закрывал окно, они
/// звали бы друг друга по кругу.
#[cfg(desktop)]
fn finish_conversation(app: &tauri::AppHandle) {
    end_conversation(app, true, true);
}

/// Заканчивает разговор. `signal` — звучит ли при этом сигнал.
///
/// Сигнал означает «разговор окончен», и звучать он должен только когда это
/// правда. Когда человек перебивает клавишей, чтобы сказать следующее, разговор
/// не кончается — он продолжается, просто с этой секунды слово у человека.
#[cfg(desktop)]
fn end_conversation(app: &tauri::AppHandle, signal: bool, close_window: bool) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    if !CONVERSATION.swap(false, Ordering::SeqCst) {
        return;
    }
    voice::stt::stop_conversation();
    // Задача, которой не назвали срок, ждала ответа в этом разговоре. Разговор
    // кончился — ждать больше нечего, иначе следующая же фраза через час была
    // бы принята за срок.
    planner::forget_pending();
    let _ = app.emit_to(overlay::POPUP_LABEL, "voice:listening", false);
    overlay::hide_hud(app);
    if signal {
        // Короткий сигнал вместо надписи «готово»: индикатор исчезает молча,
        // и без звука непонятно, закончился разговор или программа задумалась.
        voice::chime();
    }

    // И снова ждём обращения — но не раньше, чем отзвучит сигнал: иначе
    // услышим его же.
    let generation = overlay::popup_generation();
    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-wake-again".into())
        .spawn(move || {
            std::thread::sleep(voice::chime_length() + std::time::Duration::from_millis(400));

            // Окно убираем после сигнала, а не вместе с ним: закрытие окна
            // останавливает и звук, и сигнал оборвался бы на середине.
            if close_window {
                let handle = app.clone();
                let _ =
                    app.run_on_main_thread(move || overlay::hide_popup_if(&handle, generation));
            }

            start_wake(&app);

            // Ожидание выключено — значит, распознавание больше не нужно, и
            // держать под него видеопамять после разговора незачем.
            use std::sync::atomic::Ordering;
            if !WAKE.load(Ordering::SeqCst) && !CONVERSATION.load(Ordering::SeqCst) {
                voice::release_speech();
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
/// Идёт ли прямо сейчас подготовка модели.
///
/// Окно настройки сохраняет их по одной на каждое изменённое поле, и на смену
/// модели прилетало три одинаковых просьбы подряд — три загрузки одного и того
/// же и три уборки следом. Один заход за раз; если за время работы выбор успел
/// смениться, заход повторяется уже с новым.
#[cfg(desktop)]
static WARMING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(desktop)]
pub(crate) fn wake_local_model(app: &tauri::AppHandle) {
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

    use std::sync::atomic::Ordering;
    if WARMING.swap(true, Ordering::SeqCst) {
        return;
    }

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

        /* 4. Уборка: в видеопамяти должна остаться одна модель — эта. */
        crate::ollama::unload_others(&host, &model).await;

        WARMING.store(false, Ordering::SeqCst);

        // Пока грелись, выбор мог смениться — тогда всё сначала, уже с новым.
        let chosen = app.state::<AppState>().config().ai.model.clone();
        if !chosen.trim().is_empty() && chosen != model {
            wake_local_model(&app);
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

    let tasks = MenuItem::with_id(app, "tasks", "Задачи", true, None::<&str>)?;
    let onboarding = MenuItem::with_id(app, "onboarding", "Настройка и проверка…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&tasks, &onboarding, &quit])?;

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
        "tasks" => {
            if let Err(err) = overlay::show_tasks(app) {
                log::error!("не удалось открыть окно задач: {err}");
            }
        }
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

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    #[test]
    fn calling_by_name_splits_off_the_question() {
        // Вопрос возвращается как сказан — со знаками и заглавными.
        assert_eq!(
            wake_split("Хэй, Ноа, что такое альбедо?").as_deref(),
            Some("что такое альбедо?")
        );
        assert_eq!(
            wake_split("Хэй, Ноа. Что такое альбедо?").as_deref(),
            Some("Что такое альбедо?")
        );
        // Распознавание пишет имя как придётся — ловим все написания.
        assert_eq!(wake_split("Эй, Ной, привет").as_deref(), Some("привет"));
        assert_eq!(wake_split("Hey Noah, what is albedo").as_deref(), Some("what is albedo"));
        // Позвали и замолчали — вопроса нет, но обращение есть.
        assert_eq!(wake_split("Хэй, Ноа").as_deref(), Some(""));
        // Имя в начале — уже обращение.
        assert_eq!(wake_split("Ноа, объясни").as_deref(), Some("объясни"));
        // Имени одного достаточно — оклик не обязателен.
        assert_eq!(wake_split("Ноа").as_deref(), Some(""));
        assert_eq!(
            wake_split("Ноа, что такое альбедо?").as_deref(),
            Some("что такое альбедо?")
        );
        // Близкое написание проходит, когда за именем пауза.
        assert_eq!(wake_split("Ноя, слышишь?").as_deref(), Some("слышишь?"));
        // Так это слышится на самом деле — из живого разговора с программой.
        assert_eq!(wake_split("Эй, НО, привет!").as_deref(), Some("привет!"));
        assert_eq!(wake_split("Эй, ну привет.").as_deref(), Some("привет."));
        // Оклик расслышан неточно — это всё равно оклик.
        assert_eq!(wake_split("Ай, Ноа, меня слышно?").as_deref(), Some("меня слышно?"));
        assert_eq!(wake_split("Хей Ноя, объясни").as_deref(), Some("объясни"));
        // Всё, во что распознаватель успел превратить «хэй, ноа» вживую.
        assert_eq!(wake_split("Эй, НО, привет").as_deref(), Some("привет"));
        assert_eq!(wake_split("Ой, ну, что там").as_deref(), Some("что там"));
        assert_eq!(wake_split("хэйноа что такое альбедо").as_deref(), Some("что такое альбедо"));
        assert_eq!(wake_split("Хай, Ноу, слышишь?").as_deref(), Some("слышишь?"));
        assert_eq!(
            wake_split("Эй, но а ты слышишь меня?").as_deref(),
            Some("а ты слышишь меня?")
        );
    }

    #[test]
    fn a_room_conversation_is_not_a_summons() {
        assert!(wake_split("что такое альбедо").is_none());
        assert!(wake_split("").is_none());
        // Имя посреди чужого разговора обращением не считается.
        assert!(wake_split("вчера я говорил с Ноа про работу").is_none());
        assert!(wake_split("привет, как дела").is_none());
        // Слова, которые распознаватель подсовывает вместо имени, сами по себе
        // обращением не считаются — иначе сработает половина фраз в комнате.
        assert!(wake_split("ну и что теперь").is_none());
        assert!(wake_split("но это же неправда").is_none());
        assert!(wake_split("на выходных поедем").is_none());
        // Спорное написание идёт после обычного приветствия, а не после оклика.
        assert!(wake_split("привет, ну как дела").is_none());
        assert!(wake_split("окей, но зачем").is_none());
        // Похожие на оклик обрывки обычной речи обращением не становятся.
        assert!(wake_split("он ноутбук принёс").is_none());
        assert!(wake_split("да нет, наверное").is_none());
        // После оклика именем считается только короткое слово на «н».
        assert!(wake_split("эй, наверное не стоит").is_none());
        assert!(wake_split("эй, Настя, подожди").is_none());
        assert!(wake_split("эй, послушай").is_none());
        // Похожие слова без паузы за ними именем не считаются.
        assert!(wake_split("ночь была тихая").is_none());
        assert!(wake_split("нога болит").is_none());
        assert!(wake_split("но это же неправда").is_none());
        assert!(wake_split("ну ладно, поехали").is_none());
    }

    #[test]
    fn a_goodbye_with_a_tail_still_ends_the_talk() {
        // Живая речь редко кончается ровно на прощании — за ним тянется
        // объяснение, и разговор всё равно закончен.
        assert!(is_farewell("давай, пока, раз все хорошо"));
        assert!(is_farewell("Все, прощаемся с тобой"));
        assert!(is_farewell("ну ладно, пока!"));
        assert!(is_farewell("удачи тебе"));
        // Живьём сказанное — из настоящего разговора, где помощник не понял.
        assert!(is_farewell("Давай прощаться."));
        assert!(is_farewell("Все, заканчивай, хватит анекдотов."));
    }

    #[test]
    fn pauseless_poka_is_a_conjunction() {
        // То же слово без паузы за ним — союз, а не прощание.
        assert!(!is_farewell("пока я думал, ты уже ответил"));
        assert!(!is_farewell("подожди, пока объяснишь до конца"));
        // И оно же внутри другого слова.
        assert!(!is_farewell("покажи это на примере"));
    }

    #[test]
    fn goodbyes_end_the_talk() {
        assert!(is_farewell("Спасибо"));
        assert!(is_farewell("понял, спасибо большое"));
        assert!(is_farewell("До связи"));
        assert!(is_farewell("ну всё, до завтра"));
        assert!(is_farewell("Пока"));
        assert!(is_farewell("ну всё, пока"));
        assert!(is_farewell("ладно, пока-пока"));
    }

    #[test]
    fn a_conjunction_is_not_a_goodbye() {
        // «Пока» в середине фразы — союз, а не прощание. Обрывать на нём
        // разговор значило бы бросать человека посреди вопроса.
        assert!(!is_farewell("пока я думал, забыл вопрос"));
        assert!(!is_farewell("подожди, пока объяснишь"));
        assert!(!is_farewell("а что было пока меня не было"));
        assert!(!is_farewell("расскажи про альбедо"));
        assert!(!is_farewell(""));
    }
}
