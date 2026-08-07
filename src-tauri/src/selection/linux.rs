//! Linux: best-effort (SPEC §9.3, §12.1).
//!
//! Портируемого способа узнать выделение в чужом приложении здесь нет. Что реально
//! доступно:
//!   • X11 — PRIMARY selection ловит любое выделение мышью, но не даёт координат,
//!     поэтому попап встаёт у курсора, а не у самого текста;
//!   • Wayland — по соображениям безопасности протокола нет ни глобальных хуков,
//!     ни координат; остаётся `wl-paste --primary` на тех композиторах, где он работает.
//!
//! Факт нажатия Ctrl в момент выделения не определяется ни там, ни там. По SPEC §12.5
//! безопасное поведение — НЕ открывать попап, поэтому режим «только PRIMARY» выключен
//! по умолчанию и включается явным флагом `trigger.linuxPrimaryWithoutCtrl`.

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{Capability, PlatformIntegration, Selection};
use crate::config::TriggerConfig;

/// PRIMARY опрашивается запуском внешней утилиты, поэтому реже, чем общий цикл:
/// 250мс незаметны человеку и не создают потока процессов.
const PRIMARY_POLL: Duration = Duration::from_millis(250);

#[derive(Default)]
struct WatchState {
    last_poll: Option<Instant>,
    last_text: String,
}

pub struct Platform {
    session: Session,
    reader: Option<Reader>,
    state: Mutex<WatchState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Session {
    X11,
    Wayland,
}

/// Как читать PRIMARY на этом окружении.
#[derive(Debug, Clone, Copy)]
struct Reader {
    program: &'static str,
    args: &'static [&'static str],
}

const XCLIP: Reader = Reader {
    program: "xclip",
    args: &["-selection", "primary", "-o"],
};
const WL_PASTE: Reader = Reader {
    program: "wl-paste",
    args: &["--primary", "--no-newline"],
};

fn has_program(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn detect_session() -> Session {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Session::Wayland
    } else {
        Session::X11
    }
}

fn pick_reader(session: Session) -> Option<Reader> {
    match session {
        Session::Wayland => has_program("wl-paste").then_some(WL_PASTE),
        // На X11 предпочитаем xclip, но wl-paste тоже сработает под XWayland.
        Session::X11 => has_program("xclip")
            .then_some(XCLIP)
            .or_else(|| has_program("wl-paste").then_some(WL_PASTE)),
    }
}

fn read_primary(reader: Reader) -> Option<String> {
    let output = Command::new(reader.program).args(reader.args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    (!text.trim().is_empty()).then_some(text)
}

/// Координаты курсора. Есть только на X11 и только при установленном xdotool —
/// без них попап встанет в левый верхний угол, что заметно хуже, но не смертельно.
fn cursor_position() -> Option<(f64, f64)> {
    let output = Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut x = None;
    let mut y = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("X=") {
            x = value.trim().parse::<f64>().ok();
        }
        if let Some(value) = line.strip_prefix("Y=") {
            y = value.trim().parse::<f64>().ok();
        }
    }
    Some((x?, y?))
}

impl PlatformIntegration for Platform {
    fn capability(&self) -> Capability {
        let Some(reader) = self.reader else {
            return Capability::Unavailable {
                title: "Нет утилиты для чтения выделения".into(),
                hint: match self.session {
                    Session::Wayland => "Установите wl-clipboard (команда wl-paste) — без неё \
                                         прочитать выделение на Wayland нечем."
                        .into(),
                    Session::X11 => "Установите xclip — без него прочитать PRIMARY selection нечем."
                        .to_string(),
                },
            };
        };

        // Даже когда всё установлено, это остаётся ограниченным режимом, и говорить
        // об этом надо прямо, а не делать вид, что интеграция полноценная.
        Capability::Unavailable {
            title: "Ограниченный режим Linux".into(),
            hint: format!(
                "Выделение читается через {} (PRIMARY selection). Определить, был ли зажат \
                 левый Ctrl, на этом окружении невозможно: единого системного API для этого \
                 в Linux нет.\n\nПоэтому по умолчанию попап не открывается сам. Включите \
                 \"trigger\": {{\"linuxPrimaryWithoutCtrl\": true}} в config.json, если согласны \
                 на открытие по любому выделению мышью.{}",
                reader.program,
                if self.session == Session::Wayland {
                    "\n\nНа Wayland координаты выделения и позиция курсора недоступны — окно \
                     появится в углу экрана."
                } else if cursor_position().is_none() {
                    "\n\nДля позиционирования у курсора установите xdotool."
                } else {
                    ""
                }
            ),
        }
    }

    fn poll_trigger(&self, config: &TriggerConfig) -> Option<Selection> {
        let reader = self.reader?;

        // Безопасное поведение по умолчанию (SPEC §12.5): раз Ctrl не различить —
        // не открываем ничего, пока пользователь явно не разрешил.
        if config.require_left_ctrl && !config.linux_primary_without_ctrl {
            return None;
        }

        let mut state = self.state.lock().ok()?;
        let now = Instant::now();
        if let Some(last) = state.last_poll {
            if now.duration_since(last) < PRIMARY_POLL {
                return None;
            }
        }
        state.last_poll = Some(now);

        let text = read_primary(reader)?;
        // Реагируем только на смену выделения, иначе попап открывался бы бесконечно.
        if text == state.last_text {
            return None;
        }
        state.last_text = text.clone();
        drop(state);

        Some(Selection {
            text: text.trim().to_string(),
            // PRIMARY отдаёт только текст: геометрии выделения здесь нет по устройству
            // протокола, поэтому якорь — курсор (SPEC §9.3).
            rect: None,
            cursor: cursor_position().unwrap_or((0.0, 0.0)),
            context: String::new(),
        })
    }

    fn cursor_position(&self) -> Option<(f64, f64)> {
        cursor_position()
    }
}

pub fn create() -> Box<dyn PlatformIntegration> {
    let session = detect_session();
    Box::new(Platform {
        session,
        reader: pick_reader(session),
        state: Mutex::new(WatchState::default()),
    })
}
