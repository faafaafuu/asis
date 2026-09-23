// Документация: справочник и концепции.

export const REFERENCE = [
  /* ── Справочник ───────────────────────────────────────────────────────── */
  {
    slug: "voice-commands",
    section: "reference",
    ru: {
      title: "Голосовые команды",
      lead: "Что NOAH понимает сразу, без модулей.",
      body: `
## Клавиши

| Клавиши | Что делают |
|---|---|
| Левый Alt + пробел | сказать вопрос |
| Esc | остановить ответ |
| Ctrl + Shift + Alt + пробел | включить или выключить ожидание имени |
| Ctrl + Alt + пробел | показывать или не показывать окно ответов |
| Выделение с левым Ctrl | объяснить выделенное |

## Разговор

Позвали по имени — NOAH отвечает и слушает дальше, клавиши не нужны. Беседу заканчивают «спасибо, пока», Esc или минута тишины.

## Команды

| Пример | Что будет |
|---|---|
| «Напомни завтра в три позвонить в банк» | дело со сроком |
| «Что на сегодня?» | список дел |
| «Открой телеграм» / «Закрой задачи» | программы и окна NOAH |
| «Найди договор и пришли» | поиск файла |
| «Это правда?» | проверка новости на экране |
| «Что не так с компьютером?» | [диагностика](#/docs/diagnose) |
| «Сколько стоит биткоин?» | цена |
| «Разбуди в семь по будням» | будильник |
| «Поставь таймер на 20 минут» | таймер |
| «Запомни, что…» | [память](#/docs/memory) |
| «Переключись на хайку» | [смена мозга](#/docs/brains) |
| «Усыпи компьютер» | питание: сон, выключение, перезагрузка |
`,
    },
    en: {
      title: "Voice commands",
      lead: "What NOAH understands out of the box.",
      body: `
| Keys | Do |
|---|---|
| Left Alt + Space | ask by voice |
| Esc | stop the answer |
| Ctrl + Shift + Alt + Space | toggle listening for the name |
| Select with left Ctrl | explain the selection |

| Example | Result |
|---|---|
| “Remind me tomorrow at 3 to call the bank” | a task |
| “Open Telegram” / “Close tasks” | apps and NOAH windows |
| “Is this true?” | fact-check what's on screen |
| “What's wrong with my PC?” | [diagnostics](#/docs/diagnose) |
| “Remember that…” | [memory](#/docs/memory) |
`,
    },
  },
  {
    slug: "module-standard",
    section: "reference",
    remote: "/api/docs/standard",
    ru: { title: "Регламент модуля", lead: "Полный текст правил, по которым NOAH проверяет модули. Его же читает нейросеть через module_format." },
    en: { title: "Module standard", lead: "The full rules NOAH checks modules against — the same text your AI reads via module_format." },
  },
  {
    slug: "mcp-tools",
    section: "reference",
    ru: {
      title: "Инструменты MCP",
      lead: "Что видит нейросеть, подключённая к NOAH.",
      body: `
## По ссылке (\`/mcp?key=…\`)

| Инструмент | Что делает |
|---|---|
| \`module_format\` | регламент модуля с примерами |
| \`environment\` | что стоит на компьютере (Node.js, Python) и на связи ли NOAH |
| \`create_module\` | отправить модуль; NOAH проверит его у вас и вернёт отчёт |
| \`module_status\` | состояние модуля и отчёт последней проверки |
| \`search_modules\` | поиск в библиотеке |
| \`list_my_modules\` | черновики и опубликованные |
| \`publish_module\` | выложить в библиотеку |
| \`course_format\` | формат и методика курса обучения |
| \`create_course\`, \`add_topic\` | отправить курс или тему; NOAH проверит и вернёт отчёт |
| \`list_courses\`, \`course_status\` | курсы пользователя и отчёт последней проверки |

## Локально (\`sufler.exe --mcp\`)

| Инструмент | Что делает |
|---|---|
| \`module_format\`, \`environment\` | то же |
| \`create_module\` | проверить и поставить модуль сразу |
| \`check_module\` | перепроверить, например после ввода ключей |
| \`list_modules\` / \`delete_module\` | установленные модули |
| \`search_modules\` / \`install_module\` | библиотека |
| \`publish_module\` | публикация по ключу площадки |
| \`course_format\`, \`create_course\`, \`add_topic\` | курсы обучения |
| \`ask_noa\` | передать NOAH фразу как сказанную голосом |
`,
    },
    en: {
      title: "MCP tools",
      lead: "What an AI connected to NOAH sees.",
      body: `
## By link (\`/mcp?key=…\`)

\`module_format\`, \`environment\`, \`create_module\`, \`module_status\`, \`search_modules\`, \`list_my_modules\`, \`publish_module\`, \`course_format\`, \`create_course\`, \`add_topic\`, \`list_courses\`, \`course_status\`.

## Local (\`sufler.exe --mcp\`)

\`module_format\`, \`environment\`, \`create_module\`, \`check_module\`, \`list_modules\`, \`delete_module\`, \`search_modules\`, \`install_module\`, \`publish_module\`, \`course_format\`, \`create_course\`, \`add_topic\`, \`ask_noa\`.
`,
    },
  },
  {
    slug: "api",
    section: "reference",
    ru: {
      title: "HTTP API площадки",
      lead: "Библиотека и публикация — для своих клиентов и скриптов.",
      body: `
Все ответы — JSON. Запросы на изменение — только с того же сайта или с ключом площадки в заголовке \`Authorization: Bearer noah_…\`.

| Метод и путь | Что отдаёт |
|---|---|
| \`GET /api/modules?q=&category=&sort=\` | список модулей библиотеки |
| \`GET /api/modules/:id\` | модуль: описание, инструменты, какие нужны ключи |
| \`GET /api/modules/:id/package\` | пакет для установки: \`module.json\` и файлы |
| \`GET /api/stats\` | сколько модулей, авторов, установок |
| \`GET /api/whoami\` | чей ключ (с \`Bearer\`) |
| \`POST /api/publish\` | опубликовать модуль (с \`Bearer\`) |
| \`GET /api/docs/standard\` | текст регламента |
| \`GET /download\` | установщик последней версии NOAH |
`,
    },
    en: {
      title: "Platform HTTP API",
      lead: "Library and publishing for your own clients and scripts.",
      body: `
| Method and path | Returns |
|---|---|
| \`GET /api/modules?q=&category=&sort=\` | library modules |
| \`GET /api/modules/:id\` | one module |
| \`GET /api/modules/:id/package\` | install package |
| \`GET /api/whoami\` | key owner (\`Bearer\`) |
| \`POST /api/publish\` | publish (\`Bearer\`) |
| \`GET /download\` | latest NOAH installer |
`,
    },
  },
  {
    slug: "files",
    section: "reference",
    ru: {
      title: "Файлы и папки",
      lead: "Где NOAH хранит настройки, модули и журнал.",
      body: `
| Путь | Что там |
|---|---|
| \`%APPDATA%\\app.sufler.popup\\config.json\` | настройки |
| \`%APPDATA%\\app.sufler.popup\\memory.md\` | [память о вас](#/docs/memory) |
| \`%APPDATA%\\app.sufler.popup\\tasks.json\` | дела |
| \`%APPDATA%\\app.sufler.popup\\modules\\<id>\\\` | модуль: \`module.json\`, файлы, \`server.log\` |
| \`%LOCALAPPDATA%\\app.sufler.popup\\logs\\sufler.log\` | журнал NOAH (время в UTC) |

Ключи модулей хранятся в \`secrets.json\` модуля в зашифрованном виде.
`,
    },
    en: {
      title: "Files and folders",
      lead: "Where NOAH keeps settings, modules and the log.",
      body: `
| Path | Contents |
|---|---|
| \`%APPDATA%\\app.sufler.popup\\config.json\` | settings |
| \`%APPDATA%\\app.sufler.popup\\memory.md\` | memory about you |
| \`%APPDATA%\\app.sufler.popup\\modules\\<id>\\\` | a module |
| \`%LOCALAPPDATA%\\app.sufler.popup\\logs\\sufler.log\` | NOAH log (UTC) |
`,
    },
  },

  /* ── Концепции ────────────────────────────────────────────────────────── */
  {
    slug: "core",
    section: "concepts",
    ru: {
      title: "Оболочка, мозг и модули",
      lead: "Как устроен NOAH и почему модули может делать каждый.",
      body: `
**Оболочка** — NOAH на вашем компьютере: слышит, говорит, видит экран, управляет программами, помнит разговор и факты о вас.

**Мозг** — любая нейросеть. Она понимает сказанное и решает, что сделать. Мозг можно поменять, не трогая остального.

**Модули** — новые умения. Каждый модуль — маленький MCP-сервер со своими инструментами. Когда вы просите что-то, NOAH выбирает подходящий инструмент и пересказывает ответ голосом.

Модуль пишет ваша нейросеть по [регламенту](#/docs/module-standard), а NOAH проверяет его, прежде чем запустить. Поэтому собрать модуль может любой, кто умеет описать задачу словами.
`,
    },
    en: {
      title: "Shell, brain and modules",
      lead: "How NOAH is built and why anyone can make modules.",
      body: `
**Shell** — NOAH on your computer: hears, speaks, sees the screen, drives apps, remembers.

**Brain** — any AI model. Swap it without touching anything else.

**Modules** — new skills, each a small MCP server. Your AI writes them following the [standard](#/docs/module-standard); NOAH checks them before running.
`,
    },
  },
  {
    slug: "checks",
    section: "concepts",
    ru: {
      title: "Как NOAH проверяет модули",
      lead: "Почему чужой модуль не сломает компьютер и не утащит ключи.",
      body: `
Прежде чем модуль заработает, NOAH:

1. **Сверяет описание** с регламентом: имя, команда запуска, объявленные ключи. Запускать можно только разрешённой средой (\`node\`, \`python\`, \`npx\`, \`uvx\`) или файлом самого модуля; исполняемых файлов (exe, bat, cmd, ps1, dll) в модуле быть не может.
2. **Запускает сервер** и спрашивает у него инструменты.
3. **Прогоняет тесты** из описания модуля.
4. **Проверяет устойчивость**: модуль не должен падать на неверных данных.

Прошедшая версия запоминается по отпечатку файлов. Файлы изменились — модуль не запустится, пока не пройдёт проверку заново. Упал больше трёх раз за десять минут — NOAH его останавливает.

Обновление не трогает работающую версию, пока новая не прошла проверку.
`,
    },
    en: {
      title: "How NOAH checks modules",
      lead: "Why someone else's module can't break your PC or steal keys.",
      body: `
Before a module runs, NOAH lints the manifest (no \`cmd\`, PowerShell or hidden executables), starts the server, lists its tools, runs its tests and checks it survives bad input.

The passing version is fingerprinted; changed files mean a new check. More than three crashes in ten minutes stop the module.
`,
    },
  },
  {
    slug: "privacy",
    section: "concepts",
    ru: {
      title: "Приватность",
      lead: "Что остаётся на компьютере, а что куда уходит.",
      body: `
- **Голос** распознаётся и озвучивается на вашем компьютере.
- **Вопросы** уходят только к модели, которую вы выбрали мозгом. Своя модель через Ollama — никуда.
- **Ключи модулей** зашифрованы и не покидают компьютер.
- **Площадка** хранит аккаунт, ключи площадки (в виде отпечатков) и опубликованные модули. Черновики модулей по ссылке MCP идут через площадку к вашему NOAH и удаляются после проверки.

Подробно — в [политике конфиденциальности](#/privacy).
`,
    },
    en: {
      title: "Privacy",
      lead: "What stays on your computer and what goes where.",
      body: `
- **Voice** is recognized and synthesized locally.
- **Questions** go only to the model you picked. A local model means nowhere.
- **Module keys** are encrypted and never leave the machine.

See the [privacy policy](#/privacy).
`,
    },
  },
];
