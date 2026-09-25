//! Снимки экрана на macOS и Linux — пока не умеем.
//!
//! На Windows снимок делают GDI и встроенный кодировщик PNG (`shots.rs`). У
//! macOS и Linux свои средства и свои разрешения на запись экрана; до тех пор
//! Ноа честно говорит, что снимать не может, а остальное работает как везде.

use std::path::PathBuf;

use tauri::AppHandle;

const NOT_YET: &str = "Снимки экрана пока есть только в версии для Windows.";

pub fn screen() -> Result<Vec<u8>, String> {
    Err(NOT_YET.into())
}

pub fn window(_handle: isize) -> Result<Vec<u8>, String> {
    Err(NOT_YET.into())
}

pub(crate) fn window_image(_handle: isize) -> Result<crate::screen::Image, String> {
    Err(NOT_YET.into())
}

pub fn save(_app: &AppHandle, _png: &[u8]) -> Result<(PathBuf, &'static str), String> {
    Err(NOT_YET.into())
}

pub fn png(_pixels: &[u8], _width: u32, _height: u32) -> Result<Vec<u8>, String> {
    Err(NOT_YET.into())
}
