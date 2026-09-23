//! Фокус-сессии обучения: отрезок сосредоточенной работы с целью в начале и
//! «выгрузкой» по памяти в конце, между ними — перерыв.
//!
//! Таймер идёт в окне обучения, здесь — учёт: сколько минут занимались по
//! дням, серия дней подряд и сами сессии. Серия считается и по повторению
//! карточек: пять минут карточек в автобусе — тоже день занятий, а серия
//! держит привычку лучше, чем любая отдельная сессия.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::learning;

/// Сколько сессий хранить: история нужна для итогов, а не навсегда.
const KEEP: usize = 200;

/// Одна фокус-сессия.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Session {
    /// Когда закончилась, секунды Unix.
    pub at: i64,
    pub minutes: u32,
    /// Что хотел понять или запомнить.
    pub goal: String,
    /// Что вспомнил в конце без подглядывания.
    pub recall: String,
    /// Мысли, отложенные «на потом», чтобы не отвлекаться.
    pub parked: Vec<String>,
}

/// Итоги занятий для окна.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Минут фокуса сегодня и за семь дней.
    pub today: u32,
    pub week: u32,
    /// Дней подряд с занятиями, считая сегодня или вчера.
    pub streak: u32,
    pub sessions_today: u32,
}

fn day(offset: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(offset)).format("%Y-%m-%d").to_string()
}

/// Отмечает сегодня днём занятий — без минут фокуса.
pub(crate) fn touch(days: &mut BTreeMap<String, u32>) {
    days.entry(day(0)).or_insert(0);
}

/// Итоги по дням и сессиям.
pub(crate) fn stats(days: &BTreeMap<String, u32>, sessions: &[Session]) -> Stats {
    let today = days.get(&day(0)).copied().unwrap_or(0);
    let week = (0..7).filter_map(|offset| days.get(&day(offset))).sum();
    // Сегодня ещё не занимались — серия не прервана, пока не кончился день.
    let start = if days.contains_key(&day(0)) { 0 } else { 1 };
    let streak = (start..).take_while(|offset| days.contains_key(&day(*offset))).count() as u32;
    let midnight = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|at| at.and_local_timezone(chrono::Local).single())
        .map_or(0, |at| at.timestamp());
    let sessions_today = sessions.iter().filter(|s| s.at >= midnight).count() as u32;
    Stats { today, week, streak, sessions_today }
}

/// Записывает законченную сессию и отдаёт новые итоги.
pub fn record(course_id: &str, mut session: Session) -> Result<Stats, String> {
    learning::course(course_id)?;
    session.at = chrono::Local::now().timestamp();
    session.minutes = session.minutes.min(240);
    session.goal = session.goal.trim().chars().take(300).collect();
    session.recall = session.recall.trim().chars().take(4000).collect();
    session.parked.retain(|note| !note.trim().is_empty());
    log::info!("фокус {course_id}: {} мин, цель «{}»", session.minutes, session.goal);
    Ok(learning::with(course_id, |progress| {
        *progress.days.entry(day(0)).or_insert(0) += session.minutes;
        progress.sessions.push(session);
        let extra = progress.sessions.len().saturating_sub(KEEP);
        progress.sessions.drain(..extra);
        stats(&progress.days, &progress.sessions)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_counts_days_in_a_row() {
        let mut days = BTreeMap::new();
        assert_eq!(stats(&days, &[]).streak, 0);
        days.insert(day(1), 25);
        days.insert(day(2), 0);
        days.insert(day(4), 50);
        // Сегодня ещё не занимались: серия со вчера — два дня.
        assert_eq!(stats(&days, &[]).streak, 2);
        touch(&mut days);
        let now = stats(&days, &[]);
        assert_eq!(now.streak, 3);
        assert_eq!(now.today, 0);
        assert_eq!(now.week, 75);
    }
}
