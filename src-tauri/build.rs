fn main() {
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
