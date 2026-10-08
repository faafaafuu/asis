fn main() {
    link_ios_plugin();
    // Плагин телефона (`mobile.rs`, Kotlin в mobile/android-plugin) встроен в
    // приложение, и права на его команды объявляются здесь. Без них Tauri
    // молча отклонял вызовы из страницы: пункт «Объяснить» и микрофон на
    // телефоне не отвечали, хотя сам плагин работал.
    tauri_build::try_build(
        tauri_build::Attributes::new().plugin(
            "sufler",
            tauri_build::InlinedPlugin::new()
                .commands(&[
                    "pendingSelection",
                    "pendingVoice",
                    "integrationStatus",
                    "speak",
                    "stopSpeaking",
                    "listen",
                    "stopListening",
                    "cancelListening",
                    "chime",
                    "allowNotifications",
                    "notify",
                    "schedule",
                    "openUrl",
                    "clipboard",
                    "openAssistantSettings",
                    "insets",
                    "barStyle",
                    "registerListener",
                    "removeListener",
                ])
                .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
        ),
    )
    .expect("не удалось собрать описание прав");
}

/// Swift-плагин на iPhone (mobile/ios-plugin): голос, меню выделения. Как у
/// плагинов Tauri: рядом с пакетом кладётся Swift-API Tauri (.tauri/tauri-api,
/// на него ссылается Package.swift), и пакет линкуется в программу. Только
/// при сборке под iOS с фичей ios-plugin — собирает это Mac.
fn link_ios_plugin() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") || std::env::var_os("CARGO_FEATURE_IOS_PLUGIN").is_none() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        use std::path::{Path, PathBuf};
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let plugin = manifest.join("..").join("mobile").join("ios-plugin");
        let api = std::env::var("DEP_TAURI_IOS_LIBRARY_PATH").expect("нет DEP_TAURI_IOS_LIBRARY_PATH — tauri должен быть зависимостью");
        let target = plugin.parent().unwrap().join(".tauri").join("tauri-api");
        let _ = std::fs::remove_dir_all(&target);
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap().flatten() {
                let name = entry.file_name();
                if [".build", "Package.resolved", "Tests"].iter().any(|skip| name == *skip) {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    copy(&path, &to.join(&name));
                } else {
                    std::fs::copy(&path, to.join(&name)).unwrap();
                }
            }
        }
        copy(Path::new(&api), &target);
        println!("cargo:rerun-if-changed={}", plugin.join("Sources").display());
        tauri_utils::build::link_apple_library("sufler-plugin", &plugin);
    }
}
