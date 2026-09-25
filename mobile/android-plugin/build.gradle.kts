// Сборка Android-плагина. Подключается к сгенерированному проекту
// (`src-tauri/gen/android`) — см. mobile/android-plugin/README.md.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.sufler.plugin"
    compileSdk = 36

    defaultConfig {
        // Как у приложения Tauri: плитке в быстрых настройках нужен API 24.
        minSdk = 24
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
