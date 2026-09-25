//! Будильники: на время, разово или по дням недели.
//!
//! В отличие от таймеров живут на диске (`alarms.json`) и переживают
//! перезапуск программы и компьютера. Звенят, пока их не выключат: сигнал
//! каждые несколько секунд до пяти минут, фраза и сообщение в Telegram.
//! «Стоп» и Esc выключают, «отложи на десять минут» переносит.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

/// Как долго звенеть, если никто не выключил.
const RING_FOR: Duration = Duration::from_secs(5 * 60);
/// Пауза между сигналами.
const RING_EVERY: Duration = Duration::from_secs(4);
/// На сколько откладывать по умолчанию.
pub const SNOOZE_MINUTES: i64 = 5;

const WEEKDAYS: [&str; 7] = [
    "понедельник",
    "вторник",
    "среда",
    "четверг",
    "пятница",
    "суббота",
    "воскресенье",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alarm {
    pub id: u64,
    pub hour: u32,
    pub minute: u32,
    /// Дни недели, 0 — понедельник. Пусто — один раз.
    #[serde(default)]
    pub days: Vec<u32>,
    #[serde(default)]
    pub label: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Для разового: на какой день поставлен.
    #[serde(default)]
    pub date: Option<String>,
    /// Отложен до этого времени.
    #[serde(default)]
    pub snoozed: Option<String>,
    /// Когда звонил последний раз — чтобы не звонить дважды в одну минуту.
    #[serde(default)]
    pub last: Option<String>,
}

fn yes() -> bool {
    true
}

impl Alarm {
    /// Время звонка словами: «07:30 по будням».
    pub fn describe(&self) -> String {
        let time = format!("{:02}:{:02}", self.hour, self.minute);
        let days = match self.days.as_slice() {
            [] => match &self.date {
                Some(date) => format!(" на {date}"),
                None => String::new(),
            },
            [0, 1, 2, 3, 4] => " по будням".into(),
            [5, 6] => " по выходным".into(),
            [0, 1, 2, 3, 4, 5, 6] => " каждый день".into(),
            days => format!(
                " по дням: {}",
                days.iter()
                    .filter_map(|day| WEEKDAYS.get(*day as usize))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let off = if self.enabled { "" } else { " (выключен)" };
        format!("{time}{days}{off}")
    }

    /// Ближайший звонок не раньше `now`.
    pub fn next(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled {
            return None;
        }
        if let Some(snoozed) = self.snoozed.as_deref().and_then(parse_stamp) {
            return Some(snoozed);
        }
        let time = NaiveTime::from_hms_opt(self.hour, self.minute, 0)?;
        if self.days.is_empty() {
            let date = self
                .date
                .as_deref()
                .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .unwrap_or_else(|| now.date_naive());
            return Local.from_local_datetime(&date.and_time(time)).single();
        }
        (0..8).find_map(|ahead| {
            let date = now.date_naive() + chrono::Duration::days(ahead);
            let day = date.weekday().num_days_from_monday();
            if !self.days.contains(&day) {
                return None;
            }
            let at = Local.from_local_datetime(&date.and_time(time)).single()?;
            (at >= now - chrono::Duration::seconds(59)).then_some(at)
        })
    }
}

fn stamp(at: DateTime<Local>) -> String {
    at.format("%Y-%m-%d %H:%M").to_string()
}

fn parse_stamp(text: &str) -> Option<DateTime<Local>> {
    let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").ok()?;
    Local.from_local_datetime(&naive).single()
}

static STORE: Mutex<Option<(PathBuf, Vec<Alarm>)>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Звенит сейчас: какой будильник и с какого момента.
static RINGING: Mutex<Option<(u64, Instant)>> = Mutex::new(None);

/// Читает будильники. Зовётся при запуске.
pub fn load(dir: PathBuf) {
    let path = dir.join("alarms.json");
    let alarms: Vec<Alarm> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let top = alarms.iter().map(|alarm| alarm.id).max().unwrap_or(0);
    NEXT_ID.store(top + 1, Ordering::SeqCst);
    log::info!("будильников: {}", alarms.len());
    *STORE.lock().unwrap_or_else(|err| err.into_inner()) = Some((path, alarms));
}

fn with<T>(change: impl FnOnce(&mut Vec<Alarm>) -> (T, bool)) -> T {
    let mut guard = STORE.lock().unwrap_or_else(|err| err.into_inner());
    let (path, alarms) = guard.get_or_insert_with(|| (PathBuf::new(), Vec::new()));
    let (value, dirty) = change(alarms);
    if dirty && !path.as_os_str().is_empty() {
        match serde_json::to_string_pretty(alarms) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&*path, text) {
                    log::warn!("будильники не сохранились: {err}");
                }
            }
            Err(err) => log::warn!("будильники не сложились в JSON: {err}"),
        }
    }
    value
}

/// Действующие будильники: разовые, которые уже отзвонили, не в счёт.
pub fn list() -> Vec<Alarm> {
    with(|alarms| {
        let live = alarms
            .iter()
            .filter(|alarm| alarm.enabled || !alarm.days.is_empty())
            .cloned()
            .collect();
        (live, false)
    })
}

/// Заводит будильник. `days` пусто — разовый на ближайшее такое время.
pub fn add(hour: u32, minute: u32, days: Vec<u32>, label: &str, now: DateTime<Local>) -> Alarm {
    let mut alarm = Alarm {
        id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
        hour,
        minute,
        days,
        label: label.trim().to_string(),
        enabled: true,
        date: None,
        snoozed: None,
        last: None,
    };
    if alarm.days.is_empty() {
        let today = now.date_naive();
        let at = NaiveTime::from_hms_opt(hour, minute, 0)
            .and_then(|time| Local.from_local_datetime(&today.and_time(time)).single());
        let date = match at {
            Some(at) if at > now => today,
            _ => today + chrono::Duration::days(1),
        };
        alarm.date = Some(date.format("%Y-%m-%d").to_string());
    }
    let added = alarm.clone();
    with(|alarms| {
        alarms.push(alarm);
        ((), true)
    });
    log::info!("будильник: {}", added.describe());
    added
}

/// Убирает будильники на это время; `None` — все. Отдаёт, сколько убрал.
pub fn remove(time: Option<(u32, u32)>) -> usize {
    with(|alarms| {
        let before = alarms.len();
        alarms.retain(|alarm| time.is_some_and(|(h, m)| alarm.hour != h || alarm.minute != m));
        let removed = before - alarms.len();
        (removed, removed > 0)
    })
}

/// Звенит ли сейчас будильник.
pub fn ringing() -> bool {
    RINGING.lock().unwrap_or_else(|err| err.into_inner()).is_some()
}

/// Выключает звонок. `true` — звенело.
pub fn stop() -> bool {
    let was = RINGING.lock().unwrap_or_else(|err| err.into_inner()).take();
    if was.is_some() {
        log::info!("будильник выключен");
    }
    was.is_some()
}

/// Откладывает звонок на `minutes` минут. Отдаёт, во сколько позвонит.
pub fn snooze(minutes: i64) -> Option<String> {
    let (id, _) = RINGING.lock().unwrap_or_else(|err| err.into_inner()).take()?;
    let at = Local::now() + chrono::Duration::minutes(minutes);
    with(|alarms| {
        if let Some(alarm) = alarms.iter_mut().find(|alarm| alarm.id == id) {
            alarm.snoozed = Some(stamp(at));
            // Разовый после звонка выключен — отложенный звонок включает его
            // снова, иначе он бы уже не прозвенел.
            alarm.enabled = true;
        }
        ((), true)
    });
    log::info!("будильник отложен до {}", at.format("%H:%M"));
    Some(at.format("%H:%M").to_string())
}

/// Раз в несколько секунд смотрит, не пора ли звонить.
pub fn watch(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("sufler-alarms".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            if ringing() {
                continue;
            }
            let now = Local::now();
            let due = with(|alarms| {
                let minute = stamp(now);
                let due = alarms.iter_mut().find(|alarm| {
                    alarm
                        .next(now)
                        .is_some_and(|at| at <= now && now - at < chrono::Duration::minutes(2))
                        && alarm.last.as_deref() != Some(minute.as_str())
                });
                let Some(alarm) = due else {
                    return (None, false);
                };
                alarm.last = Some(minute);
                alarm.snoozed = None;
                // Разовый звонит один раз и выключается, но остаётся в списке.
                if alarm.days.is_empty() {
                    alarm.enabled = false;
                }
                (Some(alarm.clone()), true)
            });
            if let Some(alarm) = due {
                ring(&app, alarm);
            }
        })
        .ok();
}

/// Звонит, пока не выключат или пока не пройдёт пять минут.
fn ring(app: &tauri::AppHandle, alarm: Alarm) {
    let now = Local::now();
    *RINGING.lock().unwrap_or_else(|err| err.into_inner()) = Some((alarm.id, Instant::now()));
    let label = if alarm.label.is_empty() {
        format!(
            "Будильник: {:02}:{:02}. Скажите «стоп» или «отложи».",
            now.hour(),
            now.minute()
        )
    } else {
        format!("Будильник: {}. Скажите «стоп» или «отложи».", alarm.label)
    };
    log::info!("звонит будильник на {:02}:{:02}", alarm.hour, alarm.minute);
    if crate::telegram::ready(app) {
        let text = format!("Ноа: {label}");
        let app = app.clone();
        std::thread::spawn(move || {
            let _ = tauri::async_runtime::block_on(crate::telegram::notify(&app, &text));
        });
    }
    crate::announce(app, label, false);

    let started = Instant::now();
    while started.elapsed() < RING_FOR {
        std::thread::sleep(RING_EVERY);
        let still = RINGING
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .is_some_and(|(id, _)| id == alarm.id);
        if !still {
            return;
        }
        #[cfg(desktop)]
        if !crate::voice::speaking() {
            crate::voice::chime();
        }
    }
    let mut ringing = RINGING.lock().unwrap_or_else(|err| err.into_inner());
    if ringing.is_some_and(|(id, _)| id == alarm.id) {
        *ringing = None;
        log::info!("будильник отзвонил пять минут и замолчал");
    }
}

/// Дни недели по сказанному: «по будням», «каждый день», «в субботу»,
/// «по понедельникам и средам». Пусто — разовый.
pub fn days_of(said: &str) -> Vec<u32> {
    let lower = said.to_lowercase().replace('ё', "е");
    if lower.contains("будн") || lower.contains("рабоч") {
        return vec![0, 1, 2, 3, 4];
    }
    if lower.contains("выходн") {
        return vec![5, 6];
    }
    if lower.contains("каждый день") || lower.contains("ежедневно") || lower.contains("каждое утро") {
        return (0..7).collect();
    }
    const STEMS: [&str; 7] = ["понедельн", "вторн", "сред", "четверг", "пятниц", "суббот", "воскресен"];
    let mut days: Vec<u32> = STEMS
        .iter()
        .enumerate()
        .filter(|(_, stem)| lower.contains(*stem))
        .map(|(at, _)| at as u32)
        .collect();
    days.sort_unstable();
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_heard() {
        assert_eq!(days_of("разбуди в семь по будням"), vec![0, 1, 2, 3, 4]);
        assert_eq!(days_of("будильник на 9 по выходным"), vec![5, 6]);
        assert_eq!(days_of("каждый день в 7"), (0..7).collect::<Vec<_>>());
        assert_eq!(days_of("по понедельникам и средам в 8"), vec![0, 2]);
        assert!(days_of("разбуди в семь").is_empty());
    }

    #[test]
    fn the_next_ring_is_found() {
        // Пятница, 11 сентября 2026, 15:07.
        let now = Local.with_ymd_and_hms(2026, 9, 11, 15, 7, 0).unwrap();
        let weekdays = Alarm {
            id: 1,
            hour: 7,
            minute: 30,
            days: vec![0, 1, 2, 3, 4],
            label: String::new(),
            enabled: true,
            date: None,
            snoozed: None,
            last: None,
        };
        // Следующий будний — понедельник, 14-е.
        assert_eq!(
            weekdays.next(now).map(|at| at.format("%d %H:%M").to_string()).as_deref(),
            Some("14 07:30")
        );
        assert_eq!(weekdays.describe(), "07:30 по будням");
        let off = Alarm { enabled: false, ..weekdays };
        assert!(off.next(now).is_none());
    }
}
