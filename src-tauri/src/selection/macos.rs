//! macOS: выделение через Accessibility API, триггер через состояние клавиш HID
//! (SPEC §9.2).
//!
//! Без разрешения «Универсальный доступ» (System Settings → Privacy & Security →
//! Accessibility) API не отдаёт ничего вообще — это не деградация, а полный отказ,
//! поэтому статус проверяется явно и приводит к экрану онбординга (SPEC §12.4).

use std::ffi::c_void;
use std::sync::Mutex;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};

use super::{Capability, PlatformIntegration, ScreenRect, Selection};
use crate::config::TriggerConfig;

type AXUIElementRef = *const c_void;
type AXValueRef = *const c_void;

const K_AX_ERROR_SUCCESS: i32 = 0;
const K_AX_VALUE_TYPE_CG_RECT: u32 = 3;
const K_AX_VALUE_TYPE_CF_RANGE: u32 = 4;

/// Идентификаторы клавиш в раскладке-независимой нумерации macOS.
/// Именно так различаются левый и правый Ctrl (SPEC §9.2, §12.5).
const KEY_LEFT_CONTROL: u16 = 59;
const KEY_ESCAPE: u16 = 53;
/// kCGEventSourceStateHIDSystemState — реальное состояние железа.
const HID_SYSTEM_STATE: i32 = 1;
const MOUSE_BUTTON_LEFT: u32 = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    // Apple возвращает `Boolean` — это unsigned char, а не Rust bool. Объявляем u8
    // и сравниваем явно: так не остаётся неопределённого поведения на значениях,
    // отличных от 0 и 1.
    fn AXValueGetValue(value: AXValueRef, value_type: u32, out: *mut c_void) -> u8;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> AXValueRef;
    fn AXIsProcessTrusted() -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceKeyState(state: i32, key: u16) -> u8;
    fn CGEventSourceButtonState(state: i32, button: u32) -> u8;
    fn CGEventCreate(source: *const c_void) -> *const c_void;
    fn CGEventGetLocation(event: *const c_void) -> CGPoint;
}

#[derive(Default)]
struct GestureState {
    mouse_was_down: bool,
    left_ctrl_seen: bool,
}

pub struct Platform {
    gesture: Mutex<GestureState>,
}

/// Обёртка, которая гарантированно освобождает CF-объект.
struct CfOwned(CFTypeRef);

impl Drop for CfOwned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CfOwned> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let status = unsafe {
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value)
    };
    (status == K_AX_ERROR_SUCCESS && !value.is_null()).then_some(CfOwned(value))
}

fn copy_parameterized(element: AXUIElementRef, name: &str, parameter: CFTypeRef) -> Option<CfOwned> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let status = unsafe {
        AXUIElementCopyParameterizedAttributeValue(
            element,
            attribute.as_concrete_TypeRef(),
            parameter,
            &mut value,
        )
    };
    (status == K_AX_ERROR_SUCCESS && !value.is_null()).then_some(CfOwned(value))
}

fn selected_text_and_rect() -> Option<(String, Option<ScreenRect>)> {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let system = CfOwned(system as CFTypeRef);

        let focused = copy_attribute(system.0 as AXUIElementRef, "AXFocusedUIElement")?;
        let element = focused.0 as AXUIElementRef;

        let text_value = copy_attribute(element, "AXSelectedText")?;
        let text = CFString::wrap_under_get_rule(text_value.0 as CFStringRef).to_string();
        if text.trim().is_empty() {
            return None;
        }

        Some((text, selection_rect(element)))
    }
}

/// Прямоугольник КОНЦА выделения, а не всего выделения целиком: для многострочного
/// текста AXBoundsForRange вернул бы объединяющий прямоугольник, и попап встал бы
/// по центру абзаца вместо конца выделенного фрагмента (SPEC §4).
unsafe fn selection_rect(element: AXUIElementRef) -> Option<ScreenRect> {
    let range_value = copy_attribute(element, "AXSelectedTextRange")?;

    let mut range = CFRange::default();
    if AXValueGetValue(
        range_value.0 as AXValueRef,
        K_AX_VALUE_TYPE_CF_RANGE,
        &mut range as *mut _ as *mut c_void,
    ) == 0
    {
        return None;
    }

    // Сначала пробуем последний символ выделения, затем — всё выделение.
    let candidates = [
        CFRange {
            location: range.location + (range.length - 1).max(0),
            length: 1,
        },
        range,
    ];

    for candidate in candidates {
        let param = AXValueCreate(
            K_AX_VALUE_TYPE_CF_RANGE,
            &candidate as *const _ as *const c_void,
        );
        if param.is_null() {
            continue;
        }
        let param = CfOwned(param as CFTypeRef);

        let Some(bounds) = copy_parameterized(element, "AXBoundsForRange", param.0) else {
            continue;
        };

        let mut rect = CGRect::default();
        if AXValueGetValue(
            bounds.0 as AXValueRef,
            K_AX_VALUE_TYPE_CG_RECT,
            &mut rect as *mut _ as *mut c_void,
        ) != 0
            && (rect.size.width > 0.0 || rect.size.height > 0.0)
        {
            return Some(ScreenRect {
                x: rect.origin.x,
                y: rect.origin.y,
                width: rect.size.width,
                height: rect.size.height,
            });
        }
    }

    None
}

/// Разрешение «Универсальный доступ» выдано?
fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

fn cursor() -> Option<(f64, f64)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event as CFTypeRef);
        Some((point.x, point.y))
    }
}

impl PlatformIntegration for Platform {
    fn capability(&self) -> Capability {
        if is_trusted() {
            Capability::Ready
        } else {
            Capability::NeedsPermission {
                title: "Нужен доступ к «Универсальному доступу»".into(),
                hint: "Системные настройки → Конфиденциальность и безопасность → Универсальный доступ \
                       (Accessibility): включите «Суфлёр».\n\nБез этого разрешения macOS не отдаёт \
                       приложению ни текст выделения, ни его координаты — попап не сможет открыться \
                       ни в одном приложении."
                    .into(),
            }
        }
    }

    fn poll_trigger(&self, config: &TriggerConfig) -> Option<Selection> {
        if !is_trusted() {
            return None;
        }

        let mut gesture = self.gesture.lock().ok()?;
        let mouse_down = unsafe { CGEventSourceButtonState(HID_SYSTEM_STATE, MOUSE_BUTTON_LEFT) != 0 };
        let left_ctrl = unsafe { CGEventSourceKeyState(HID_SYSTEM_STATE, KEY_LEFT_CONTROL) != 0 };

        if mouse_down && left_ctrl {
            gesture.left_ctrl_seen = true;
        }
        let released = gesture.mouse_was_down && !mouse_down;
        gesture.mouse_was_down = mouse_down;

        if !released {
            return None;
        }
        let had_left_ctrl = gesture.left_ctrl_seen;
        gesture.left_ctrl_seen = false;
        drop(gesture);

        if config.require_left_ctrl && !had_left_ctrl {
            return None;
        }

        let (text, rect) = selected_text_and_rect()?;
        Some(Selection {
            text: text.trim().to_string(),
            rect,
            cursor: cursor().unwrap_or((0.0, 0.0)),
            context: String::new(),
        })
    }

    fn is_escape_pressed(&self) -> bool {
        unsafe { CGEventSourceKeyState(HID_SYSTEM_STATE, KEY_ESCAPE) != 0 }
    }

    fn open_permission_settings(&self) -> bool {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .is_ok()
    }
}

pub fn create() -> Box<dyn PlatformIntegration> {
    Box::new(Platform {
        gesture: Mutex::new(GestureState::default()),
    })
}
