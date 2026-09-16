//! Единственный экземпляр приложения (Windows).
//!
//! Зачем: по ярлыку теперь всегда открывается окно, а значит человек будет щёлкать
//! по нему повторно — просто чтобы посмотреть настройки. Без защиты каждый щелчок
//! запускал бы ещё одну копию: ещё один значок в трее, ещё один наблюдатель за
//! выделением и второй попап поверх первого.
//!
//! Механика стандартная для Windows: именованный мьютекс как признак «я уже здесь»
//! и именованное событие как способ попросить первую копию показать окно. Вторая
//! копия ставит событие и тихо завершается, первая просыпается и открывает окно —
//! со стороны выглядит так, будто ярлык просто вернул уже запущенную программу.
//!
//! Правило безопасности: при любой неясности считаем, что мы первые, и запускаемся.
//! Ошибочный отказ здесь страшнее лишней копии — программа просто не откроется,
//! и человек решит, что она сломана.

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForMultipleObjects, INFINITE,
};

/// Имена в пространстве `Local\` — то есть в пределах сеанса пользователя.
/// `Global\` было бы неверно: на общем компьютере два человека вправе одновременно
/// пользоваться программой каждый в своём сеансе.
const MUTEX_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.instance");
const EVENT_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.show");
/// Просьба закрыться. Нужна установщику: он обновляет файлы поверх работающей
/// программы и обязан её сначала остановить.
const QUIT_EVENT_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.quit");
/// Фраза из командной строки: `sufler.exe --ask "сколько стоит биткоин"`.
const ASK_EVENT_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.ask");

/// Где лежит фраза, пока работающая копия её не забрала.
fn ask_file() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA")
        .map(|dir| std::path::PathBuf::from(dir).join("app.sufler.popup").join("ask.txt"))
}

/// Передаёт работающей копии фразу — как если бы её сказали голосом.
///
/// Так вопрос задаётся без микрофона — с ярлыка, горячей клавиши или из
/// скрипта, — и так же ответы Ноа проверяются, не говоря вслух.
pub fn request_ask(text: &str) {
    let Some(path) = ask_file() else {
        return;
    };
    if std::fs::write(&path, text).is_err() {
        return;
    }
    unsafe {
        if let Ok(event) = CreateEventW(None, false, false, ASK_EVENT_NAME) {
            let _ = SetEvent(event);
        }
    }
}

/// Запущена ли программа: занят ли её мьютекс единственной копии.
pub fn running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    // SAFETY: только открывает чужой мьютекс по имени и сразу закрывает.
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, MUTEX_NAME) {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                true
            }
            Err(_) => false,
        }
    }
}

/// Занимает право быть единственной копией.
///
/// `false` — программа уже запущена, этой копии следует завершиться; первой при этом
/// уже отправлена просьба показать окно.
pub fn claim() -> bool {
    unsafe {
        let Ok(mutex) = CreateMutexW(None, true, MUTEX_NAME) else {
            // Мьютекс не создался — причина неизвестна, поэтому работаем как обычно.
            return true;
        };
        if GetLastError() != ERROR_ALREADY_EXISTS {
            // Дескриптор намеренно не закрываем: он должен жить, пока жива программа,
            // иначе следующая копия не увидит признака занятости.
            std::mem::forget(mutex);
            return true;
        }

        signal();
        false
    }
}

/// Просит уже запущенную копию показать своё окно.
unsafe fn signal() {
    // CreateEventW с именем открывает существующее событие, если оно уже есть, —
    // отдельный OpenEvent не нужен.
    if let Ok(event) = CreateEventW(None, false, false, EVENT_NAME) {
        let _ = SetEvent(event);
    }
}

/// Просит работающую копию закрыться и ничего не ждёт.
///
/// Вызывается из запуска с ключом `--quit` — так установщик останавливает
/// программу перед заменой файлов. Раньше он просто убивал процесс, и значок
/// в трее оставался висеть призраком: Windows убирает его не сразу после
/// смерти программы, а когда по нему проведут мышью. После каждого обновления
/// человек видел в трее лишнего Суфлёра, а то и трёх.
pub fn request_quit() {
    unsafe {
        if let Ok(event) = CreateEventW(None, false, false, QUIT_EVENT_NAME) {
            let _ = SetEvent(event);
        }
    }
}

/// Слушает просьбы от других копий: показать окно или закрыться.
pub fn listen(app: tauri::AppHandle) {
    std::thread::spawn(move || unsafe {
        let Ok(show) = CreateEventW(None, false, false, EVENT_NAME) else {
            log::warn!("не удалось создать событие показа окна — повторный запуск ярлыка ничего не покажет");
            return;
        };
        let Ok(quit) = CreateEventW(None, false, false, QUIT_EVENT_NAME) else {
            log::warn!("не удалось создать событие выхода — установщик снимет программу силой");
            return;
        };

        let Ok(ask) = CreateEventW(None, false, false, ASK_EVENT_NAME) else {
            log::warn!("не удалось создать событие вопроса — `--ask` работать не будет");
            return;
        };

        let events = [show, quit, ask];
        loop {
            // Ждём оба сразу: bWaitAll = false — «разбуди на первом же».
            let signaled = WaitForMultipleObjects(&events, false, INFINITE);

            if signaled == WAIT_OBJECT_0 {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    // Как и щелчок по трею: сначала вернуть свёрнутые окна.
                    if crate::overlay::restore_minimized(&handle) {
                        return;
                    }
                    if let Err(err) = crate::overlay::show_onboarding(&handle) {
                        log::error!("не удалось показать окно по повторному запуску: {err}");
                    }
                });
                continue;
            }

            if signaled.0 == WAIT_OBJECT_0.0 + 1 {
                log::info!("получена просьба закрыться — выходим");
                let handle = app.clone();
                // Через главный поток: выход разбирает окна и значок в трее,
                // а это работа для того потока, который их создавал.
                let _ = app.run_on_main_thread(move || handle.exit(0));
                return;
            }

            if signaled.0 == WAIT_OBJECT_0.0 + 2 {
                let text = ask_file()
                    .and_then(|path| {
                        let text = std::fs::read_to_string(&path).ok();
                        let _ = std::fs::remove_file(&path);
                        text
                    })
                    .unwrap_or_default();
                if !text.trim().is_empty() {
                    crate::ask(&app, text.trim().to_string());
                }
                continue;
            }

            // Ожидание сломалось (дескриптор закрыт, система отказала) — слушать
            // дальше нечего, но и делать из этого трагедию незачем.
            return;
        }
    });
}
