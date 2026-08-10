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
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE,
};

/// Имена в пространстве `Local\` — то есть в пределах сеанса пользователя.
/// `Global\` было бы неверно: на общем компьютере два человека вправе одновременно
/// пользоваться программой каждый в своём сеансе.
const MUTEX_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.instance");
const EVENT_NAME: windows::core::PCWSTR = w!("Local\\app.sufler.popup.show");

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

/// Слушает просьбы от других копий и открывает окно настройки.
pub fn listen(app: tauri::AppHandle) {
    std::thread::spawn(move || unsafe {
        let Ok(event) = CreateEventW(None, false, false, EVENT_NAME) else {
            log::warn!("не удалось создать событие показа окна — повторный запуск ярлыка ничего не покажет");
            return;
        };
        loop {
            if WaitForSingleObject(event, INFINITE) != WAIT_OBJECT_0 {
                return;
            }
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(err) = crate::overlay::show_onboarding(&handle) {
                    log::error!("не удалось показать окно по повторному запуску: {err}");
                }
            });
        }
    });
}
