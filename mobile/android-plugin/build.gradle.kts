// Сборка Android-плагина. Подключается к сгенерированному проекту
// (`src-tauri/gen/android`) — см. mobile/android-plugin/README.md.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.sufler.plugin"
    compileSdk = 34

    defaultConfig {
        // ACTION_PROCESS_TEXT появился в API 23 (Android 6.0) — ниже опускаться нет смысла:
        // без него у плагина нет точки входа.
        minSdk = 23
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    // Классы Plugin/Invoke/JSObject приходят из сгенерированного Tauri-проекта.
    implementation(project(":tauri-android"))
}
