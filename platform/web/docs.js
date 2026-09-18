// Документация NOAH на самом сайте. Разделы — по Diátaxis: обучение (пройти
// за руку), практика (решить задачу), справочник (найти точный ответ),
// концепции (понять, как устроено). Страницы — Markdown в этом файле;
// регламент модуля приходит с сервера — тот же текст, что читает нейросеть.

const SECTIONS = [
  { id: "tutorials", ru: "Обучение", en: "Tutorials" },
  { id: "howto", ru: "Практика", en: "How-to guides" },
  { id: "reference", ru: "Справочник", en: "Reference" },
  { id: "concepts", ru: "Концепции", en: "Concepts" },
];

const PAGES = [
  /* ── Обучение ─────────────────────────────────────────────────────────── */
  {
    slug: "quickstart",
    section: "tutorials",
    ru: {
      title: "Первый запуск",
      lead: "Поставить NOAH, выбрать мозг и задать первый вопрос голосом — минут за пять.",
      body: `
## 1. Установите NOAH

[Скачайте установщик](/download) и запустите его. NOAH работает на Windows 10 и 11 и живёт в трее — значок рядом с часами. Окно настроек открывается двойным щелчком по значку.

## 2. Выберите мозг

Во вкладке **Настройки → Модель** выберите, кто будет думать:

- **Своя модель** — через Ollama, на вашем компьютере, без интернета. NOAH сам подскажет модель под вашу видеокарту.
- **Облако** — любой OpenAI-совместимый сервис: OpenRouter, Groq, свой сервер. Нужен ключ сервиса.
- **Мост к Claude** — если у вас есть Claude Code.

Мозг можно сменить в любой момент, и голосом тоже: «Ноа, переключись на хайку».

## 3. Скачайте голос и распознавание

В блоке **Голос** нажмите «Скачать голос», в блоке **Голосовые вопросы** — «Скачать распознавание». Всё работает на вашем компьютере, звук наружу не уходит.

## 4. Спросите

- Зажмите **левый Alt + пробел** и говорите — отпустите, когда закончите.
- Или позовите по имени: «Ноа, что такое альбедо?». Имя можно сменить — см. [Имя помощника](#/docs/wake-name).

Дальше — [соберите первый модуль](#/docs/first-module).
`,
    },
    en: {
      title: "Quick start",
      lead: "Install NOAH, pick a brain and ask your first question by voice — about five minutes.",
      body: `
## 1. Install NOAH

[Download the installer](/download) and run it. NOAH runs on Windows 10 and 11 and lives in the tray. Double-click the tray icon to open settings.

## 2. Pick a brain

In **Settings → Model** choose who does the thinking:

- **Local model** via Ollama — on your machine, offline. NOAH suggests one for your GPU.
- **Cloud** — any OpenAI-compatible service: OpenRouter, Groq, your own server.
- **Bridge to Claude** — if you have Claude Code.

You can switch any time, by voice too: “Noah, switch to Haiku”.

## 3. Download voice and speech recognition

In **Voice** click “Download voice”, in **Voice questions** — “Download recognition”. Everything runs locally.

## 4. Ask

- Hold **left Alt + Space** and speak.
- Or call it by name: “Noah, what is albedo?”. The name can be changed — see [Assistant name](#/docs/wake-name).

Next — [build your first module](#/docs/first-module).
`,
    },
  },
  {
    slug: "first-module",
    section: "tutorials",
    ru: {
      title: "Первый модуль словами",
      lead: "Попросите свою нейросеть сделать модуль — NOAH проверит его и запустит.",
      body: `
Модуль — это новое умение NOAH: курс валют, погода, разбор чеков, что угодно. Писать код не нужно: его напишет ваша нейросеть, а NOAH проверит.

## 1. Подключите нейросеть к NOAH

Откройте [Подключить ИИ](#/connect), войдите и скопируйте ссылку MCP. Вставьте её в свою нейросеть — как именно, написано в [Подключить свой ИИ](#/docs/connect-ai).

## 2. Попросите модуль

Напишите нейросети обычными словами:

> Сделай модуль NOAH, который каждое утро говорит курс евро.

Нейросеть сама прочитает регламент (\`module_format\`), узнает, что стоит у вас на компьютере (\`environment\`), напишет модуль и отправит его (\`create_module\`).

## 3. Дождитесь проверки

NOAH на вашем компьютере запускает модуль и прогоняет его тесты. Если что-то не так, нейросеть получит отчёт и исправит сама. Готово — когда в отчёте «прошёл проверку».

## 4. Пользуйтесь

Скажите фразу из модуля: «Ноа, какой курс евро?». Модуль виден в NOAH во вкладке **Модули**.

Получилось полезное — [выложите в библиотеку](#/docs/publish).
`,
    },
    en: {
      title: "Your first module, in words",
      lead: "Ask your AI to build a module — NOAH checks it and runs it.",
      body: `
A module is a new skill for NOAH: exchange rates, weather, receipt parsing — anything. No code on your side: your AI writes it, NOAH checks it.

## 1. Connect your AI

Open [Connect AI](#/connect), sign in and copy the MCP link. Paste it into your AI — see [Connect your AI](#/docs/connect-ai).

## 2. Ask for a module

> Build a NOAH module that tells me the euro rate every morning.

Your AI reads the spec (\`module_format\`), checks your machine (\`environment\`), writes the module and sends it (\`create_module\`).

## 3. Wait for the check

NOAH runs the module on your computer and runs its tests. If something fails, the AI gets a report and fixes it.

## 4. Use it

Say the module's phrase: “Noah, what's the euro rate?”.

Made something useful? [Publish it](#/docs/publish).
`,
    },
  },

  /* ── Практика ─────────────────────────────────────────────────────────── */
  {
    slug: "connect-ai",
    section: "howto",
    ru: {
      title: "Подключить свой ИИ",
      lead: "Два способа: ссылка MCP с сайта или локальный сервер на компьютере.",
      body: `
## Ссылкой (любая нейросеть с MCP)

1. Войдите и откройте [Подключить ИИ](#/connect).
2. Скопируйте ссылку вида \`https://noahlab.ru/mcp?key=noah_…\`.
3. Добавьте её как MCP-сервер:
   - **Claude** — Настройки → Коннекторы → «Добавить свой коннектор» → вставить ссылку.
   - **Cursor** — Settings → MCP → Add new → тип \`http\`, адрес — ссылка.
   - **ChatGPT** — Настройки → Коннекторы → режим разработчика → добавить по ссылке.

Ссылка работает, пока NOAH запущен на вашем компьютере: модули проверяются и ставятся у вас, а не на сервере. Ключ в ссылке — как пароль, не публикуйте его. Отозвать ключ можно в [кабинете](#/seller/keys).

## Локально (Claude Desktop, Cursor на этом компьютере)

Добавьте в настройки MCP клиента:

\`\`\`json
{
  "mcpServers": {
    "noah": {
      "command": "C:\\\\Путь\\\\к\\\\Sufler\\\\sufler.exe",
      "args": ["--mcp"]
    }
  }
}
\`\`\`

Путь — папка, куда вы поставили NOAH. Список инструментов — в [Инструментах MCP](#/docs/mcp-tools).
`,
    },
    en: {
      title: "Connect your AI",
      lead: "Two ways: an MCP link from the site or a local server on your computer.",
      body: `
## By link (any AI with MCP)

1. Sign in and open [Connect AI](#/connect).
2. Copy the link \`https://noahlab.ru/mcp?key=noah_…\`.
3. Add it as an MCP server:
   - **Claude** — Settings → Connectors → Add custom connector.
   - **Cursor** — Settings → MCP → Add new → type \`http\`.
   - **ChatGPT** — Settings → Connectors → developer mode → add by URL.

The link works while NOAH runs on your computer: modules are checked and installed there. Treat the key as a password; revoke it in the [dashboard](#/seller/keys).

## Locally (Claude Desktop, Cursor on this machine)

\`\`\`json
{
  "mcpServers": {
    "noah": {
      "command": "C:\\\\Path\\\\to\\\\Sufler\\\\sufler.exe",
      "args": ["--mcp"]
    }
  }
}
\`\`\`

See [MCP tools](#/docs/mcp-tools) for the tool list.
`,
    },
  },
  {
    slug: "brains",
    section: "howto",
    ru: {
      title: "Выбрать и переключать мозг",
      lead: "Своя модель, облако или мост к Claude — и смена голосом.",
      body: `
NOAH запоминает каждую модель, с которой вы работали, и переключается между ними голосом:

| Фраза | Что делает |
|---|---|
| «Ноа, какая модель сейчас?» | называет модель и где она работает |
| «Какие модели есть?» | перечисляет запомненные |
| «Переключись на хайку» | меняет модель |
| «Переключись на свою» | на локальную через Ollama |
| «Поставь уровень medium» | через мост к Claude — уровень рассуждений |

Своя модель выгружается из памяти после 20 минут простоя и при выходе из NOAH — видеопамять свободна для игр и работы.
`,
    },
    en: {
      title: "Choose and switch the brain",
      lead: "A local model, a cloud one or the bridge to Claude — and switching by voice.",
      body: `
NOAH remembers every model you used and switches between them by voice:

| Phrase | Does |
|---|---|
| “Which model is on?” | names the model |
| “What models are there?” | lists the remembered ones |
| “Switch to Haiku” | changes the model |
| “Switch to local” | a local model via Ollama |
| “Set effort to medium” | bridge to Claude — reasoning effort |

A local model is unloaded after 20 idle minutes and on exit.
`,
    },
  },
  {
    slug: "wake-name",
    section: "howto",
    ru: {
      title: "Имя помощника",
      lead: "Звать NOAH своим именем: «Джарвис», «Вега» — как вам удобно.",
      body: `
1. Откройте **Настройки → Голос → Голосовые вопросы**.
2. Впишите имя в поле **Имя помощника** и нажмите Enter.
3. Проверьте, что включено **Отзываться на имя**.

Лучше всего слышно имя из двух-трёх слогов, которое не встречается в обычной речи. Короткие частые слова вроде «Макс» или «Лена» помощник будет путать с разговором в комнате.

**Ctrl + Shift + Alt + пробел** включает и выключает ожидание имени без настроек.

Если выбран микрофон Bluetooth-наушников, имя не слушается: иначе Windows переводит наушники в режим гарнитуры и громкость скачет. Выберите встроенный или USB-микрофон, а с наушниками спрашивайте по **левый Alt + пробел**.
`,
    },
    en: {
      title: "Assistant name",
      lead: "Call NOAH by your own name — “Jarvis”, “Vega”, anything.",
      body: `
1. Open **Settings → Voice → Voice questions**.
2. Type the name into **Assistant name** and press Enter.
3. Make sure **Respond to the name** is on.

Two or three syllables that don't come up in normal speech work best.

**Ctrl + Shift + Alt + Space** toggles listening for the name.

With a Bluetooth headset microphone the name isn't listened for — Windows would switch the headset to call mode. Use a built-in or USB mic, or ask with **left Alt + Space**.
`,
    },
  },
  {
    slug: "memory",
    section: "howto",
    ru: {
      title: "Научить NOAH фактам о себе",
      lead: "Адрес, предпочтения, имена — один раз сказать и больше не повторять.",
      body: `
| Фраза | Что делает |
|---|---|
| «Ноа, запомни, что я живу в Казани» | записывает факт |
| «Что ты обо мне знаешь?» | перечисляет записанное |
| «Забудь про Казань» | стирает факты с этими словами |
| «Забудь всё» | очищает память |

Факты лежат в файле \`memory.md\` в папке данных NOAH — его можно открыть и поправить руками. Они подмешиваются к каждому вопросу модели, поэтому «какая погода у меня» понятно без уточнений. На сервер файл не уходит; уходит только к той модели, которую вы выбрали мозгом.

Разговор NOAH помнит и без этого: последние обмены держатся полчаса после последней реплики.
`,
    },
    en: {
      title: "Teach NOAH about you",
      lead: "Address, preferences, names — say it once.",
      body: `
| Phrase | Does |
|---|---|
| “Remember that I live in Kazan” | stores a fact |
| “What do you know about me?” | lists them |
| “Forget about Kazan” | removes matching facts |

Facts live in \`memory.md\` in NOAH's data folder and are added to every question for the model you chose. The conversation itself is kept for 30 minutes after the last exchange.
`,
    },
  },
  {
    slug: "diagnose",
    section: "howto",
    ru: {
      title: "Проверить компьютер голосом",
      lead: "«Что не так?» — NOAH смотрит систему и называет причину.",
      body: `
Спросите своими словами:

- «Ноа, что не так с компьютером?»
- «Проверь драйверы»
- «Почему не запускается игра?»
- «Пропал звук»

NOAH просматривает: устройства с ошибками, драйверы основных устройств и их даты, падения программ и ошибки Windows за сутки, службы автозапуска, которые стоят, ожидание перезагрузки, место на дисках, сеть и звук. Плюс текст окна, которое было впереди, — часто это и есть окно с ошибкой.

Ответ — причина и что сделать. Если это можно поправить, NOAH предложит: «Открыть центр обновления?».

«Что грузит компьютер?» — отдельный короткий ответ о процессоре, памяти, видеокарте и дисках.
`,
    },
    en: {
      title: "Check your PC by voice",
      lead: "“What's wrong?” — NOAH scans the system and names the cause.",
      body: `
Ask in your own words: “What's wrong with my computer?”, “Check the drivers”, “Why won't the game start?”.

NOAH looks at devices with errors, key drivers and their dates, crashes and Windows errors for the last day, stopped auto-start services, pending reboot, disks, network and sound — plus the text of the window in front.

The answer is the cause and what to do about it.
`,
    },
  },
  {
    slug: "module-keys",
    section: "howto",
    ru: {
      title: "Ключи для модуля",
      lead: "Модулю нужен ключ API или токен — как его отдать безопасно.",
      body: `
Модуль объявляет нужные ключи в \`secrets\`. Пока ключи не введены, он не запускается и помечен «нужны ключи».

1. Откройте NOAH → **Модули** → карточка модуля → **Ключи**.
2. Вставьте значения и нажмите «Сохранить».
3. NOAH проверит модуль заново и запустит.

Ключи шифруются средствами Windows для вашей учётной записи и лежат только на этом компьютере. Нейросеть их не видит и спрашивать не должна — это правило [регламента](#/docs/module-standard).
`,
    },
    en: {
      title: "Keys for a module",
      lead: "A module needs an API key — how to hand it over safely.",
      body: `
Modules declare keys in \`secrets\`. Until they're entered, the module doesn't run.

1. NOAH → **Modules** → the module card → **Keys**.
2. Paste the values and save.
3. NOAH re-checks the module and starts it.

Keys are encrypted by Windows for your account and never leave the machine.
`,
    },
  },
  {
    slug: "publish",
    section: "howto",
    ru: {
      title: "Опубликовать модуль",
      lead: "Выложить модуль в библиотеку, чтобы его ставили другие.",
      body: `
Публикуется только модуль, прошедший проверку NOAH.

## Через нейросеть (проще всего)

Скажите нейросети, подключённой [ссылкой](#/docs/connect-ai): «Опубликуй модуль euro-rate». Она вызовет \`publish_module\` от вашего имени.

## Из NOAH

1. В [кабинете](#/seller/keys) создайте ключ площадки.
2. Вставьте его в NOAH → **Настройки → Площадка**.
3. Попросите нейросеть, подключённую локально, вызвать \`publish_module\`.

Модуль появится в [библиотеке](#/library). Обновление — та же публикация с тем же \`id\` и новой версией.
`,
    },
    en: {
      title: "Publish a module",
      lead: "Put a module into the library for others to install.",
      body: `
Only modules that passed NOAH's check can be published.

Tell the AI connected [by link](#/docs/connect-ai): “Publish the euro-rate module”. It calls \`publish_module\` for you.

Or create a platform key in the [dashboard](#/seller/keys), paste it into NOAH → **Settings → Platform**, and ask a locally connected AI to call \`publish_module\`.
`,
    },
  },
  {
    slug: "telegram",
    section: "howto",
    ru: {
      title: "NOAH в Telegram",
      lead: "Спрашивать, получать файлы и скриншоты с компьютера из Telegram.",
      body: `
1. В Telegram откройте [@BotFather](https://t.me/BotFather), отправьте \`/newbot\` и придумайте боту имя — он пришлёт токен.
2. В NOAH откройте **Настройки → Уведомления в Telegram** и вставьте токен.
3. Отправьте своему боту любое сообщение, например \`/start\`.
4. Нажмите **Проверить** — NOAH найдёт ваш чат и пришлёт проверочное сообщение.

Дальше пишите боту как говорите голосом: «пришли последний скриншот», «найди договор и пришли», «что на сегодня». Бот отвечает только в вашем чате.
`,
    },
    en: {
      title: "NOAH in Telegram",
      lead: "Ask, and get files and screenshots from your PC in Telegram.",
      body: `
1. In Telegram open [@BotFather](https://t.me/BotFather), send \`/newbot\` and get a token.
2. In NOAH open **Settings → Telegram notifications** and paste it.
3. Send your bot any message, e.g. \`/start\`.
4. Click **Check** — NOAH finds your chat and sends a test message.

Then write to it as you'd speak: “send the last screenshot”, “find the contract and send it”.
`,
    },
  },

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

\`module_format\`, \`environment\`, \`create_module\`, \`module_status\`, \`search_modules\`, \`list_my_modules\`, \`publish_module\`.

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
      title: "Ядро, мозг и модули",
      lead: "Как устроен NOAH и почему модули может делать каждый.",
      body: `
**Ядро** — NOAH на вашем компьютере: слышит, говорит, видит экран, управляет программами, помнит разговор и факты о вас.

**Мозг** — любая нейросеть. Она понимает сказанное и решает, что сделать. Мозг можно поменять, не трогая остального.

**Модули** — новые умения. Каждый модуль — маленький MCP-сервер со своими инструментами. Когда вы просите что-то, NOAH выбирает подходящий инструмент и пересказывает ответ голосом.

Модуль пишет ваша нейросеть по [регламенту](#/docs/module-standard), а NOAH проверяет его, прежде чем запустить. Поэтому собрать модуль может любой, кто умеет описать задачу словами.
`,
    },
    en: {
      title: "Core, brain and modules",
      lead: "How NOAH is built and why anyone can make modules.",
      body: `
**Core** — NOAH on your computer: hears, speaks, sees the screen, drives apps, remembers.

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

/* ── Markdown ───────────────────────────────────────────────────────────── */

const escape = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Строчная разметка: `код`, **жирный**, [ссылка](адрес). */
function inline(text) {
  const codes = [];
  let out = escape(text).replace(/`([^`]+)`/g, (_, code) => {
    codes.push(code);
    return `\u0000${codes.length - 1}\u0000`;
  });
  out = out
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => {
      const safe = /^(https?:\/\/|#\/|\/)/.test(href) ? href : "#";
      const external = /^https?:\/\//.test(safe);
      return `<a href="${safe}"${external ? ' target="_blank" rel="noopener"' : ""}>${label}</a>`;
    });
  return out.replace(/\u0000(\d+)\u0000/g, (_, at) => `<code>${codes[Number(at)]}</code>`);
}

const slugify = (text) =>
  text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-|-$/g, "");

/** Markdown → HTML. Заголовки, абзацы, списки, цитаты, таблицы, код. */
export function markdown(source) {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const html = [];
  let at = 0;
  while (at < lines.length) {
    const line = lines[at];
    if (!line.trim()) {
      at++;
      continue;
    }
    const fence = /^```(\w*)/.exec(line);
    if (fence) {
      const code = [];
      at++;
      while (at < lines.length && !lines[at].startsWith("```")) code.push(lines[at++]);
      at++;
      html.push(`<pre class="doc__code"><code>${escape(code.join("\n"))}</code></pre>`);
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      const level = Math.max(2, heading[1].length);
      const text = heading[2].trim();
      html.push(`<h${level} id="${slugify(text)}">${inline(text)}</h${level}>`);
      at++;
      continue;
    }
    if (line.startsWith("|")) {
      const rows = [];
      while (at < lines.length && lines[at].startsWith("|")) rows.push(lines[at++]);
      const cells = (row) => row.replace(/^\||\|$/g, "").split("|").map((cell) => cell.trim());
      const [head, , ...body] = rows;
      html.push(
        `<div class="doc__table"><table><thead><tr>${cells(head).map((c) => `<th>${inline(c)}</th>`).join("")}</tr></thead><tbody>${body
          .map((row) => `<tr>${cells(row).map((c) => `<td>${inline(c)}</td>`).join("")}</tr>`)
          .join("")}</tbody></table></div>`,
      );
      continue;
    }
    if (/^>\s?/.test(line)) {
      const quote = [];
      while (at < lines.length && /^>\s?/.test(lines[at])) quote.push(lines[at++].replace(/^>\s?/, ""));
      html.push(`<blockquote>${inline(quote.join(" "))}</blockquote>`);
      continue;
    }
    const bullet = /^(\s*)([-*]|\d+\.)\s+/;
    if (bullet.test(line)) {
      const ordered = /\d+\./.test(bullet.exec(line)[2]);
      const items = [];
      while (at < lines.length && (bullet.test(lines[at]) || /^\s{2,}\S/.test(lines[at]))) {
        const match = bullet.exec(lines[at]);
        if (match && match[1].length === 0) items.push({ text: lines[at].replace(bullet, ""), sub: [] });
        else if (match && items.length) items[items.length - 1].sub.push(lines[at].replace(bullet, ""));
        else if (items.length) items[items.length - 1].text += ` ${lines[at].trim()}`;
        at++;
      }
      const tag = ordered ? "ol" : "ul";
      html.push(
        `<${tag}>${items
          .map((item) => `<li>${inline(item.text)}${item.sub.length ? `<ul>${item.sub.map((s) => `<li>${inline(s)}</li>`).join("")}</ul>` : ""}</li>`)
          .join("")}</${tag}>`,
      );
      continue;
    }
    const para = [];
    while (at < lines.length && lines[at].trim() && !/^(#{1,4}\s|```|\||>|\s*([-*]|\d+\.)\s)/.test(lines[at])) para.push(lines[at++].trim());
    html.push(`<p>${inline(para.join(" "))}</p>`);
  }
  return html.join("\n");
}

/* ── Страница документации ──────────────────────────────────────────────── */

const cache = new Map();

async function bodyOf(page, lang) {
  if (!page.remote) return page[lang].body;
  if (!cache.has(page.remote)) {
    const response = await fetch(page.remote);
    const data = await response.json();
    cache.set(page.remote, data.text ?? "");
  }
  return cache.get(page.remote);
}

/** Текст страницы для поиска: заголовок, подзаголовок и тело. */
const searchable = (page, lang) => `${page[lang].title} ${page[lang].lead} ${page[lang].body ?? ""}`.toLowerCase();

export async function renderDocs(root, slug, { h, lang }) {
  const L = lang === "en" ? "en" : "ru";
  const ui = {
    ru: { docs: "Документация", search: "Поиск по документации", nothing: "Ничего не нашлось.", onPage: "На этой странице", prev: "Назад", next: "Дальше", edit: "Нашли ошибку? Напишите нам" },
    en: { docs: "Documentation", search: "Search the docs", nothing: "Nothing found.", onPage: "On this page", prev: "Previous", next: "Next", edit: "Found a mistake? Tell us" },
  }[L];

  const current = PAGES.find((p) => p.slug === slug) ?? PAGES[0];
  const at = PAGES.indexOf(current);
  const section = SECTIONS.find((s) => s.id === current.section);

  // Боковое меню с поиском.
  const search = h("input", { type: "search", class: "docs__search", placeholder: ui.search, "aria-label": ui.search, autocomplete: "off" });
  const menu = h("nav", { class: "docs__menu", "aria-label": ui.docs });
  const drawMenu = () => {
    const query = search.value.trim().toLowerCase();
    const groups = SECTIONS.map((s) => {
      const pages = PAGES.filter((p) => p.section === s.id && (!query || searchable(p, L).includes(query)));
      if (!pages.length) return null;
      return h(
        "div",
        { class: "docs__group" },
        h("span", { class: "docs__label" }, s[L]),
        pages.map((p) => h("a", { class: "docs__link", href: `#/docs/${p.slug}`, "aria-current": p === current ? "page" : null }, p[L].title)),
      );
    }).filter(Boolean);
    menu.replaceChildren(...(groups.length ? groups : [h("p", { class: "hint" }, ui.nothing)]));
  };
  search.addEventListener("input", drawMenu);
  search.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    const first = menu.querySelector(".docs__link");
    if (first) location.hash = first.getAttribute("href");
  });
  drawMenu();

  // Статья.
  const article = h("article", { class: "doc docs__article" });
  article.innerHTML = markdown(await bodyOf(current, L));
  const toc = [...article.querySelectorAll("h2")].map((node) => h("a", { href: `#/docs/${current.slug}`, onclick: (event) => { event.preventDefault(); node.scrollIntoView({ behavior: "smooth", block: "start" }); } }, node.textContent));

  const prev = PAGES[at - 1];
  const next = PAGES[at + 1];

  root.replaceChildren(
    h(
      "div",
      { class: "docs" },
      h(
        "aside",
        { class: "docs__side" },
        // На телефоне меню свёрнуто под кнопку, иначе оно стоит перед статьёй.
        h("button", { type: "button", class: "docs__toggle", onclick: (event) => event.currentTarget.parentElement.classList.toggle("is-open") }, `${section[L]} · ${current[L].title} ▾`),
        search,
        menu,
      ),
      h(
        "div",
        { class: "docs__main" },
        h(
          "nav",
          { class: "docs__crumbs", "aria-label": "breadcrumbs" },
          h("a", { href: "#/docs" }, ui.docs),
          h("span", {}, "›"),
          h("span", {}, section[L]),
          h("span", {}, "›"),
          h("span", { "aria-current": "page" }, current[L].title),
        ),
        h("h1", { class: "docs__title" }, current[L].title),
        h("p", { class: "docs__lead" }, current[L].lead),
        article,
        h(
          "div",
          { class: "docs__pager" },
          prev ? h("a", { class: "docs__step", href: `#/docs/${prev.slug}` }, h("span", { class: "label" }, `← ${ui.prev}`), prev[L].title) : h("span"),
          next ? h("a", { class: "docs__step docs__step--next", href: `#/docs/${next.slug}` }, h("span", { class: "label" }, `${ui.next} →`), next[L].title) : h("span"),
        ),
      ),
      toc.length > 1 ? h("aside", { class: "docs__toc" }, h("span", { class: "label" }, ui.onPage), toc) : h("span"),
    ),
  );
}
