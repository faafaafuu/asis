//! Системное выделение текста: единый интерфейс поверх очень разных API платформ
//! (SPEC §9). Ядро приложения не должно знать, что на Windows это UI Automation,
//! на macOS — Accessibility API, а на Linux — в лучшем случае PRIMARY selection.

use serde::Serialize;

use crate::config::TriggerConfig;

// Платформенные реализации подключаются здесь.
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
#[path = "unsupported.rs"]
mod platform;

/// Прямоугольник в экранных (не оконных) координатах, физические пиксели.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub fn left(&self) -> f64 {
        self.x
    }
    pub fn top(&self) -> f64 {
        self.y
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Что удалось узнать о выделении.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub text: String,
    /// Последний прямоугольник выделения (SPEC §4). `None` там, где система отдаёт
    /// только текст без геометрии — тогда попап встаёт у курсора.
    pub rect: Option<ScreenRect>,
    /// Позиция курсора на момент срабатывания — запасной якорь.
    pub cursor: (f64, f64),
    /// Текст вокруг выделения, если платформа умеет его отдать: он идёт в AI как контекст.
    #[serde(default)]
    pub context: String,
}

impl Selection {
    /// Якорь для позиционирования: прямоугольник выделения, иначе точка курсора.
    pub fn anchor(&self) -> ScreenRect {
        self.rect.unwrap_or(ScreenRect {
            x: self.cursor.0,
            y: self.cursor.1,
            width: 1.0,
            height: 1.0,
        })
    }
}

/// Насколько системная интеграция работоспособна прямо сейчас.
///
/// Каждая платформа конструирует только свои варианты: Windows — Ready, macOS —
/// Ready или NeedsPermission, Linux — Unavailable. Поэтому в любой отдельно взятой
/// сборке часть вариантов «не используется», и предупреждение здесь ложное.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Capability {
    /// Всё доступно.
    Ready,
    /// Нужно разрешение пользователя (macOS Accessibility — SPEC §9.2, §12.4).
    NeedsPermission { title: String, hint: String },
    /// Платформа не даёт нужного API. Не баг, а свойство окружения (SPEC §9.3, §12.1).
    Unavailable { title: String, hint: String },
}

impl Capability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Capability::Ready)
    }
}

/// Что интеграция видела в последнее время. Нужна не разработчику, а пользователю:
/// когда попап «не работает», причин ровно три — не поймали жест, приложение не отдало
/// текст, или текст получен, но упал запрос к модели. Различить их снаружи невозможно,
/// поэтому окно настройки показывает эти счётчики живьём.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// Сколько раз поймали жест «отпустил мышь с зажатым левым Ctrl».
    pub gestures: u64,
    /// Сколько раз из них удалось получить текст выделения.
    pub captured: u64,
    /// Последнее событие человеческим языком.
    pub last: String,
    /// Откуда взят текст: «UI Automation» или «буфер обмена».
    pub source: String,
}

/// Реализация под конкретную ОС.
pub trait PlatformIntegration: Send + Sync {
    /// Проверяется при старте и перед каждым показом онбординга.
    fn capability(&self) -> Capability;

    /// Один шаг опроса системного триггера. Возвращает `Some`, когда жест завершён:
    /// пользователь отпустил левую кнопку мыши, держа левый Ctrl, и выделение непусто.
    ///
    /// Опрос, а не блокирующая подписка, выбран сознательно: он одинаково устроен на
    /// всех платформах и не требует крутить нативный run loop внутри потока Tauri.
    fn poll_trigger(&self, config: &TriggerConfig) -> Option<Selection>;

    /// Нажат ли сейчас Esc. Нужен глобально: попап намеренно не забирает фокус,
    /// поэтому обычный keydown в окне до него не дойдёт (SPEC §8).
    /// Перехватывать Esc системным глобальным шорткатом нельзя — он сломает Esc
    /// во всех остальных приложениях.
    fn is_escape_pressed(&self) -> bool {
        false
    }

    /// Открыть системные настройки, где выдаётся нужное разрешение. `false` — если
    /// на этой платформе открывать нечего.
    fn open_permission_settings(&self) -> bool {
        false
    }

    /// Что интеграция видела в последнее время (см. [`Diagnostics`]). Платформа,
    /// которая ничего не считает, возвращает пустую сводку.
    fn diagnostics(&self) -> Diagnostics {
        Diagnostics::default()
    }
}

/// Интервал опроса. 16мс — примерно кадр: жест «отпустил кнопку» не теряется,
/// а нагрузка на процессор остаётся незаметной.
pub const POLL_INTERVAL_MS: u64 = 16;

pub fn create() -> Box<dyn PlatformIntegration> {
    platform::create()
}
