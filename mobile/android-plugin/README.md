# Android-плагин Ноа

Всё, что на телефоне делает система, а не Ноа: меню выделения, голос, вызов
без рук, уведомления, ссылки. Rust зовёт команды плагина через `mobile.rs`,
страница — через `plugin:sufler|…`; права на команды объявлены в
`src-tauri/build.rs`.

## Что здесь лежит

| Файл | Роль |
|---|---|
| `src/main/AndroidManifest.xml` | «Объяснить» (PROCESS_TEXT), жест помощника (ASSIST, VOICE_COMMAND), плитка в шторке, разрешения микрофона и уведомлений |
| `ProcessTextActivity.kt` | забирает выделенный текст и будит приложение |
| `AssistActivity.kt` | жест помощника, ярлык «Спросить голосом», плитка — Ноа сразу слушает |
| `ListenTileService.kt` | плитка «Ноа» в быстрых настройках |
| `SelectionBus.kt`, `VoiceBus.kt` | текст и просьба «слушай», пришедшие раньше, чем загрузилась страница |
| `Schedule.kt` | будильники, таймеры и напоминания в системном AlarmManager; `ReminderReceiver` показывает уведомление и после перезагрузки ставит расписание заново |
| `PermissionActivity.kt` | запрос разрешения своей непрозрачной активностью |
| `SuflerPlugin.kt` | команды и события плагина |
| `res/values/strings.xml` | подписи пунктов меню, ярлыка и плитки |

## Команды

| Команда | Что делает |
|---|---|
| `speak { text, rate, lang }` | читает вслух системным синтезатором, отвечает, когда дочитал |
| `stopSpeaking` | замолчать |
| `listen { lang }` → `{ text }` | слушает одну фразу системным распознаванием; пустой текст — тишина |
| `stopListening` / `cancelListening` | дослушать сейчас / бросить запись |
| `chime` | короткий сигнал |
| `notify { title, text }` | уведомление в шторке — только когда приложения не видно |
| `schedule { items: [{ id, at, title, text, alarm }] }` | расписание целиком: сроки в миллисекундах, `alarm` — звук будильника |
| `allowNotifications` | спросить разрешение на уведомления |
| `openUrl { url }` | открыть ссылку системой |
| `clipboard` → `{ text }` | текст из буфера обмена |
| `openAssistantSettings` | системный выбор помощника |
| `insets` → `{ top, bottom, left, right, keyboard }` | отступы под системные панели и клавиатуру |
| `barStyle { light }` | тёмные или светлые значки строки состояния |
| `pendingSelection` / `pendingVoice` | то, что пришло до загрузки страницы |

События: `selection { text }`, `voiceRequest`, `speech { state, text, level }`
(`ready`, `speaking`, `partial`, `level`, `thinking`, `idle`), `insets`.

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

То же делают сборки в CI (`release.yml`, `dev.yml`).

## Ограничения

- Свёрнутое приложение Android выгружает из памяти, когда ему нужна память;
  WebView при этом падает в `onTrimMemory` (SIGILL, WebView 145) — так было и
  в 1.12. Поэтому всё, что должно сработать в срок, стоит в системном
  расписании, а не только в памяти программы.

- Приложения с собственной отрисовкой текста (часть игр и читалок) не отдают
  меню выделения системе — пункта «Объяснить» там не будет.
- Распознавание речи — служба Google на телефоне. Без неё `listen` отвечает
  ошибкой с подсказкой, что поставить.
- Модули с программой (npx, python) на телефоне не запускаются — работают
  встроенные модули и модули по ссылке `https://…`.
