# Android-плагин: пункт «Объяснить» в меню выделения

Реализует точку входа из SPEC §9.4. Системе не нужен `AccessibilityService` и никаких
особых разрешений: Android сам добавляет активность с `intent-filter` на
`ACTION_PROCESS_TEXT` в меню выделения текста любого приложения, которое поддерживает
`android:process_text` — это подавляющее большинство текстовых полей и WebView.

## Что здесь лежит

| Файл | Роль |
|---|---|
| `src/main/AndroidManifest.xml` | регистрация `ProcessTextActivity` с `intent-filter` |
| `ProcessTextActivity.kt` | забирает `EXTRA_PROCESS_TEXT` и передаёт управление приложению |
| `SelectionBus.kt` | буфер между активностью и плагином: текст может прийти раньше, чем создан WebView |
| `SuflerPlugin.kt` | команда `pendingSelection` и событие `selection` для фронтенда |
| `res/values/strings.xml` | название пункта меню — «Объяснить» |

## Как подключить

Проект под Android генерируется командой Tauri и в репозиторий не коммитится
(`src-tauri/gen/android` в `.gitignore`):

```bash
npm run tauri android init
```

После генерации:

1. Добавьте модуль в `src-tauri/gen/android/settings.gradle`:

   ```gradle
   include(":sufler-plugin")
   project(":sufler-plugin").projectDir = file("../../../mobile/android-plugin")
   ```

2. В `src-tauri/gen/android/app/build.gradle.kts` добавьте зависимость:

   ```kotlin
   implementation(project(":sufler-plugin"))
   ```

3. Соберите и поставьте на устройство:

   ```bash
   npm run tauri android dev
   ```

Регистрация плагина на стороне Rust уже сделана в `src-tauri/src/mobile.rs`
(`register_android_plugin("app.sufler.plugin", "SuflerPlugin")`).

## Проверка

Выделите текст в любом приложении → «⋮» в меню выделения → «Объяснить».
Должен открыться попап с тем же набором состояний, что и на десктопе.

## Ограничения, о которых надо знать

- Приложения с полностью собственной отрисовкой текста (часть игр и ридеров) не
  отдают меню выделения системе — пункта там не будет. Обойти это без
  `AccessibilityService` нельзя, а он требует отдельного разрешения и в MVP не входит
  (SPEC §9.4).
- Позиционирование попапа рядом с самим выделением на Android недоступно: intent
  приносит только текст, без координат. Точное позиционирование — это тот самый
  `AccessibilityService`-путь из SPEC §9.4, вынесенный за рамки MVP.
- Связка «пункт меню → activity → плагин → попап» вживую пока не проверена:
  подтверждено только то, что приложение собирается и запускается на эмуляторе.
