//! Таймеры и будильник.
//!
//! Живут в памяти: таймер на десять минут переживать перезапуск программы не
//! обязан. Будильник пока тоже — хранение на диске и повторы по дням придут
//! вместе с его доработкой.
//!
//! Каждый таймер — свой поток, который спит до срока короткими шагами: так
//! отмена срабатывает за секунду, а часы компьютера, переведённые во время
//! ожидания, не сбивают звонок на час.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Local};

#[derive(Debug, Clone)]
pub struct Timer {
    id: u64,
    pub at: DateTime<Local>,
}

static TIMERS: Mutex<Vec<Timer>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Заводит таймер или будильник на время `at`.
pub fn start(app: &tauri::AppHandle, at: DateTime<Local>, label: String) {
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    TIMERS
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .push(Timer { id, at });
    log::info!("таймер на {}: «{label}»", at.format("%H:%M:%S"));

    let app = app.clone();
    std::thread::Builder::new()
        .name("sufler-timer".into())
        .spawn(move || {
            loop {
                let left = (at - Local::now()).to_std().unwrap_or_default();
                if left.is_zero() {
                    break;
                }
                std::thread::sleep(left.min(Duration::from_secs(1)));
                let alive = TIMERS
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .iter()
                    .any(|timer| timer.id == id);
                if !alive {
                    return;
                }
            }
            TIMERS
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .retain(|timer| timer.id != id);
            ring(&app, &label);
        })
        .ok();
}

/// Время вышло: три сигнала, потом фраза — и в Telegram, если он подключён:
/// таймер ставят и тогда, когда отходят от компьютера.
fn ring(app: &tauri::AppHandle, label: &str) {
    log::info!("таймер сработал: «{label}»");
    for _ in 0..3 {
        crate::voice::chime();
        std::thread::sleep(Duration::from_millis(700));
    }
    if crate::telegram::ready(app) {
        let text = format!("Ноа: {label}");
        if let Err(err) = tauri::async_runtime::block_on(crate::telegram::notify(app, &text)) {
            log::warn!("таймер в Telegram не ушёл: {err}");
        }
    }
    crate::announce(app, label.to_string(), false);
}

/// Снимает все таймеры и будильники; отдаёт, сколько было.
pub fn cancel_all() -> usize {
    let mut timers = TIMERS.lock().unwrap_or_else(|err| err.into_inner());
    let count = timers.len();
    timers.clear();
    count
}

/// Заведённые таймеры по порядку срабатывания.
pub fn pending() -> Vec<Timer> {
    let mut timers = TIMERS.lock().unwrap_or_else(|err| err.into_inner()).clone();
    timers.sort_by_key(|timer| timer.at);
    timers
}
