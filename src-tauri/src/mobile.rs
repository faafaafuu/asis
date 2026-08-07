//! Регистрация нативных плагинов выделения на Android и iOS (SPEC §9.4, §9.5).
//!
//! Сам плагин живёт в `mobile/android-plugin` (Kotlin) и `mobile/ios-plugin` (Swift);
//! здесь — только связка с ядром Tauri.
//!
//! TODO: не проверено на устройстве. Для сборки нужны сгенерированные проекты
//! (`tauri android init`, `tauri ios init`), Android SDK/NDK и Xcode — ничего этого
//! не было в окружении, где писался код. Регистрация написана по документированному
//! контракту Tauri 2, но подтвердить её запуском пока нечем.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_sufler);

/// Плагин `sufler`: отдаёт фронтенду текст из системного меню выделения.
/// Команды и события описаны в README соответствующего плагина.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("sufler")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _api.register_android_plugin("app.sufler.plugin", "SuflerPlugin")?;
            #[cfg(target_os = "ios")]
            _api.register_ios_plugin(init_plugin_sufler)?;
            Ok(())
        })
        .build()
}
