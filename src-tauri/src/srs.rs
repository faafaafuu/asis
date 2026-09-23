//! Интервальное повторение: когда показать карточку снова.
//!
//! Прочитанное забывается по кривой: через день помнится половина, через
//! неделю — почти ничего. Каждое удачное вспоминание делает кривую положе, и
//! следующий раз можно отложить дальше. Отсюда весь метод: карточку
//! показывают тогда, когда её вот-вот забудут, — не раньше (иначе повторение
//! впустую) и не позже (иначе учить заново).
//!
//! Расписание — SM-2 в том виде, в каком оно работает в Anki двадцать лет: у
//! карточки есть интервал и «лёгкость», ответ человека их меняет. Четыре оценки
//! вместо двух — потому что «вспомнил с трудом» и «вспомнил сразу» требуют
//! разных интервалов, а «забыл» и «вспомнил» — это не оценки, а факт.
//!
//! «Уверенно знаю» здесь не ощущение, а число: карточка, интервал которой
//! дорос до трёх недель, пережила несколько проверок подряд без провала. Курс,
//! где уверенно всё, — это и есть «от зубов отскакивает».

use serde::{Deserialize, Serialize};

/// Оценка вспоминания.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    /// Не вспомнил. Карточка вернётся в этот же сеанс.
    Again,
    /// Вспомнил с трудом, долго.
    Hard,
    /// Вспомнил.
    Good,
    /// Вспомнил сразу, без усилия.
    Easy,
}

impl Grade {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "again" | "1" | "снова" | "забыл" => Some(Self::Again),
            "hard" | "2" | "трудно" => Some(Self::Hard),
            "good" | "3" | "хорошо" => Some(Self::Good),
            "easy" | "4" | "легко" => Some(Self::Easy),
            _ => None,
        }
    }
}

/// С какой лёгкостью карточка начинает и ниже какой не опускается.
///
/// Ниже 1.3 интервалы почти перестают расти, и трудная карточка превращается
/// в ежедневную обязанность — лучше пусть человек разобьёт её на две.
const START_EASE: f64 = 2.5;
const MIN_EASE: f64 = 1.3;

/// С какого интервала понятие считается усвоенным уверенно, в днях.
pub const MATURE_DAYS: f64 = 21.0;

/// Состояние карточки в памяти человека.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CardState {
    /// Когда показать снова: секунды от начала эпохи.
    pub due: i64,
    /// Текущий интервал в днях.
    pub interval: f64,
    pub ease: f64,
    /// Сколько раз подряд вспомнил.
    pub streak: u32,
    /// Сколько раз всего забывал.
    pub lapses: u32,
    /// Сколько раз всего видел.
    pub seen: u32,
    /// Когда видел последний раз.
    pub last: i64,
}

impl Default for CardState {
    fn default() -> Self {
        Self { due: 0, interval: 0.0, ease: START_EASE, streak: 0, lapses: 0, seen: 0, last: 0 }
    }
}

/// Насколько карточка усвоена — для цвета на карте и строки прогресса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Ещё не видел.
    New,
    /// Учится: интервал меньше трёх дней.
    Learning,
    /// Держится, но недолго: до трёх недель.
    Young,
    /// Уверенно: три недели и больше.
    Mature,
}

impl CardState {
    pub fn level(&self) -> Level {
        if self.seen == 0 {
            Level::New
        } else if self.interval < 3.0 {
            Level::Learning
        } else if self.interval < MATURE_DAYS {
            Level::Young
        } else {
            Level::Mature
        }
    }

    pub fn is_due(&self, now: i64) -> bool {
        self.seen > 0 && self.due <= now
    }

    /// Применяет ответ и назначает следующий показ.
    pub fn answer(&mut self, grade: Grade, now: i64) {
        const DAY: f64 = 86_400.0;
        self.seen += 1;
        self.last = now;

        match grade {
            Grade::Again => {
                self.lapses += 1;
                self.streak = 0;
                self.ease = (self.ease - 0.2).max(MIN_EASE);
                // Забытое учат заново, но не с нуля: следы в памяти остались,
                // и после повторного выучивания интервал короче, но не нулевой.
                self.interval = if self.interval > 0.0 { (self.interval * 0.2).max(0.0) } else { 0.0 };
                // Через десять минут — в этот же сеанс. Забытое, показанное
                // только завтра, завтра так же забудется.
                self.due = now + 600;
                return;
            }
            Grade::Hard => {
                self.ease = (self.ease - 0.15).max(MIN_EASE);
                self.interval = if self.streak == 0 { 1.0 } else { (self.interval * 1.2).max(self.interval + 1.0) };
            }
            Grade::Good => {
                self.interval = match self.streak {
                    0 => 1.0,
                    1 => 3.0,
                    _ => (self.interval * self.ease).max(self.interval + 1.0),
                };
            }
            Grade::Easy => {
                self.ease += 0.15;
                self.interval = match self.streak {
                    0 => 3.0,
                    1 => 6.0,
                    _ => (self.interval * self.ease * 1.3).max(self.interval + 2.0),
                };
            }
        }
        self.streak += 1;
        // Разброс в пару процентов: иначе сотня карточек, выученных в один
        // вечер, вся придёт в один день и сделает его неподъёмным.
        let jitter = 1.0 + ((now % 97) as f64 - 48.0) / 1000.0;
        self.interval = (self.interval * jitter).min(365.0);
        self.due = now + (self.interval * DAY) as i64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    #[test]
    fn remembered_cards_come_back_later_and_later() {
        let mut card = CardState::default();
        let mut now = NOW;
        let mut intervals = Vec::new();
        for _ in 0..5 {
            card.answer(Grade::Good, now);
            intervals.push(card.interval);
            now = card.due;
        }
        // 1, 3, ~7.5, ~19, ~47 дней: каждый раз дальше.
        assert!(intervals.windows(2).all(|pair| pair[1] > pair[0]), "{intervals:?}");
        assert!(intervals[0] < 1.1 && intervals[1] < 3.2, "{intervals:?}");
        assert_eq!(card.level(), Level::Mature, "пять удачных вспоминаний подряд — уверенно");
    }

    #[test]
    fn a_forgotten_card_returns_in_the_same_session() {
        let mut card = CardState::default();
        card.answer(Grade::Good, NOW);
        card.answer(Grade::Good, NOW + DAY);
        card.answer(Grade::Again, NOW + 4 * DAY);
        assert_eq!(card.due, NOW + 4 * DAY + 600, "через десять минут");
        assert_eq!(card.streak, 0);
        assert_eq!(card.lapses, 1);
        assert_eq!(card.level(), Level::Learning);
    }

    #[test]
    fn hard_cards_lose_ease_but_not_below_the_floor() {
        let mut card = CardState::default();
        for step in 0..20 {
            card.answer(Grade::Hard, NOW + step * DAY);
        }
        assert!((card.ease - MIN_EASE).abs() < 1e-9);
    }

    #[test]
    fn a_new_card_is_not_due() {
        assert!(!CardState::default().is_due(NOW), "новые карточки выдаются отдельно, по дневной норме");
    }
}
