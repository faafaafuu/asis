//! Настольные свойства окон — на телефоне, где их нет.
//!
//! На Android окно одно и во весь экран: у него нет рамки, его нельзя
//! держать «поверх всех», свернуть или развернуть. Код окон написан под
//! настольные системы, и расставлять условия в каждом вызове значило бы
//! раздвоить его. Здесь те же методы есть у телефонного окна и ничего не
//! делают. Метод, который у телефона всё же есть, вызывается свой: у
//! собственного метода типа приоритет над методом из этого трейта.

use tauri::{Manager, Runtime, WebviewWindow, WebviewWindowBuilder};

pub trait DesktopBuilder: Sized {
    fn decorations(self, _: bool) -> Self {
        self
    }
    fn focused(self, _: bool) -> Self {
        self
    }
    fn always_on_top(self, _: bool) -> Self {
        self
    }
    fn always_on_bottom(self, _: bool) -> Self {
        self
    }
    fn transparent(self, _: bool) -> Self {
        self
    }
    fn shadow(self, _: bool) -> Self {
        self
    }
    fn skip_taskbar(self, _: bool) -> Self {
        self
    }
    fn resizable(self, _: bool) -> Self {
        self
    }
    fn center(self) -> Self {
        self
    }
    fn visible(self, _: bool) -> Self {
        self
    }
    fn title<S: Into<String>>(self, _: S) -> Self {
        self
    }
    fn inner_size(self, _: f64, _: f64) -> Self {
        self
    }
    fn min_inner_size(self, _: f64, _: f64) -> Self {
        self
    }
}

impl<'a, R: Runtime, M: Manager<R>> DesktopBuilder for WebviewWindowBuilder<'a, R, M> {}

pub trait DesktopWindow {
    fn set_always_on_top(&self, _: bool) -> tauri::Result<()> {
        Ok(())
    }
    fn set_always_on_bottom(&self, _: bool) -> tauri::Result<()> {
        Ok(())
    }
    fn unminimize(&self) -> tauri::Result<()> {
        Ok(())
    }
    fn minimize(&self) -> tauri::Result<()> {
        Ok(())
    }
    fn is_minimized(&self) -> tauri::Result<bool> {
        Ok(false)
    }
    fn set_skip_taskbar(&self, _: bool) -> tauri::Result<()> {
        Ok(())
    }
    fn set_ignore_cursor_events(&self, _: bool) -> tauri::Result<()> {
        Ok(())
    }
    fn set_focus(&self) -> tauri::Result<()> {
        Ok(())
    }
}

impl<R: Runtime> DesktopWindow for WebviewWindow<R> {}
