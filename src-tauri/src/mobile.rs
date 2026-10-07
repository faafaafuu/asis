//! Нативный плагин на Android и iOS: пункт «Объяснить» в меню выделения,
//! голос (синтез и распознавание речи), сигнал, расписание будильников.
//!
//! Сам плагин живёт в `mobile/android-plugin` (Kotlin) и `mobile/ios-plugin` (Swift);
//! здесь — связка с ядром Tauri и вызов его команд из Rust.

use std::sync::OnceLock;

use serde::de::DeserializeOwned;
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Wry,
};

#[cfg(all(target_os = "ios", feature = "ios-plugin"))]
tauri::ios_plugin_binding!(init_plugin_sufler);

/// Ручка плагина: через неё Rust зовёт команды Kotlin — «скажи», «слушай».
static HANDLE: OnceLock<PluginHandle<Wry>> = OnceLock::new();

/// Плагин `sufler`. Команды и события описаны в README соответствующего плагина.
pub fn init() -> TauriPlugin<Wry> {
    Builder::new("sufler")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            let handle = _api.register_android_plugin("app.sufler.plugin", "SuflerPlugin")?;
            #[cfg(all(target_os = "ios", feature = "ios-plugin"))]
            let handle = _api.register_ios_plugin(init_plugin_sufler)?;
            #[cfg(any(target_os = "android", all(target_os = "ios", feature = "ios-plugin")))]
            let _ = HANDLE.set(handle);
            Ok(())
        })
        .build()
}

/// Зовёт команду плагина и ждёт ответа. Блокирует поток — не звать из главного.
pub fn call<T: DeserializeOwned>(command: &str, payload: serde_json::Value) -> Result<T, String> {
    let handle = HANDLE.get().ok_or("плагин телефона не загружен")?;
    handle
        .run_mobile_plugin(command, payload)
        .map_err(|err| err.to_string())
}

/// То же без блокировки потока.
pub async fn call_async<T: DeserializeOwned>(command: &str, payload: serde_json::Value) -> Result<T, String> {
    let handle = HANDLE.get().ok_or("плагин телефона не загружен")?;
    handle
        .run_mobile_plugin_async(command, payload)
        .await
        .map_err(|err| err.to_string())
}
