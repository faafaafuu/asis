// Документация: обучение и практика.

export const GUIDES = [
  /* ── Обучение ─────────────────────────────────────────────────────────── */
  {
    slug: "quickstart",
    section: "tutorials",
    ru: {
      title: "Первый запуск",
      lead: "Поставить NOAH, выбрать мозг и задать первый вопрос голосом — минут за пять.",
      body: `
## 1. Установите NOAH

[Скачайте установщик](/download) — сайт сам отдаст файл под вашу систему — и запустите его. NOAH живёт в трее: значок рядом с часами, окно настроек открывается двойным щелчком по нему.

| Система | Файл | Что есть |
|---|---|---|
| [Windows 10 и 11](/download?os=windows) | \`.exe\` | всё |
| [macOS](/download?os=mac) (Apple Silicon) | \`.dmg\` | всё, кроме снимков экрана; файл не подписан — первый запуск через «Открыть» в меню по правому щелчку |
| [Linux](/download?os=linux) | \`.AppImage\`, \`.deb\` | всё, кроме снимков экрана |
| [Android](/download?os=android) | \`.apk\` | объяснения, задачи, обучение, модели; голос и окна поверх экрана — в настольной версии |

## 2. Выберите мозг

Во вкладке **Настройки → Мозг Ноа** выберите, кто будет думать. Проще всего — список **«Мои модели»**: там уже есть всё, на чём NOAH работал и что стоит на компьютере, выбирается одним щелчком.

- **Своя модель** — через Ollama, на вашем компьютере, без интернета. NOAH сам подскажет модель под вашу видеокарту.
- **Облако** — OpenRouter, Groq, Google AI Studio или свой сервер. Нужен ключ сервиса; у Google AI Studio и OpenRouter есть бесплатные модели.
- **Подписка на этом компьютере** — Claude Code, Codex (ChatGPT), Gemini CLI или Qwen Code. Поставьте программу, один раз войдите в свой аккаунт — и она появится в «Моих моделях».
- **Мост** — те же подписки, но на вашем сервере.

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

[Download the installer](/download) — the site picks the file for your system — and run it. NOAH lives in the tray; double-click the icon to open settings. Builds: [Windows](/download?os=windows), [macOS](/download?os=mac) (Apple Silicon, unsigned — open it via right-click → Open the first time), [Linux](/download?os=linux) (.AppImage, .deb), [Android](/download?os=android) (.apk, without the desktop voice and overlay windows).

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
    slug: "courses",
    section: "howto",
    ru: {
      title: "Свой курс обучения",
      lead: "Окно «Обучение» пустое, пока вы не соберёте курс. Собирает его ваша нейросеть.",
      body: `
1. [Подключите нейросеть](#/docs/connect-ai) к NOAH — по ссылке с сайта или локально.
2. Попросите: «Собери мне курс NOAH по английскому для путешествий: пять тем». Можно дать свои материалы — конспекты, описание вакансии, программу экзамена.
3. Нейросеть прочитает формат (\`course_format\`) и отправит курс (\`create_course\`), большой — по теме (\`add_topic\`). NOAH проверит его, вернёт отчёт с замечаниями и покажет курс в окне «Обучение».

## Как устроена тема

- **Урок** читается по разделам; после раздела — вспомнить одно понятие без подсказки. Понятие, которое раздел только называет, показывается как новое, с определением.
- **🔍 Разобрать подробно** под разделом — модель раскрывает его по шагам: как устроено, кто что делает, пример, где ошибаются. Разбор пишется один раз и дальше открывается сразу.
- **💬 Обсудить** — вопросы о том, что на экране: текстом в окне или голосом. NOAH знает раздел целиком, его понятия, а у вопроса — эталон и ваш ответ. Голос и текст — один разговор.
- **Проверка открытых ответов** — по смыслу, а не по словам: пункт, сказанный своими словами или другой верной командой, засчитан; раскрытый наполовину — половиной. Эталон — пример сильного ответа, а не единственно верный.
- **Понятия** — определение, зацепка для памяти, аналогия, частая ошибка и связи с другими темами.
- **Карта** — понятия темы и их связи с остальным курсом; на обзоре — карта всего курса.
- **Повторение** — карточки по расписанию: вспомнили — вернутся через 1, 3, 8, 20 дней и дальше, забыли — сегодня же. Ошибка на экзамене возвращает понятие в повторение.
- **Задачи, мини-экзамен, шпаргалка**; в конце курса — финальный экзамен на стык тем.

## Фокус-сессия

На обзоре курса — **🎯 Фокус-сессия**: отрезок 15, 25 или 50 минут и перерыв после него. Перед началом — одна конкретная цель и минута на то, чтобы убрать отвлечения. Мысль, пришедшую посреди отрезка, запишите «на потом» через таймер в заголовке. В конце — выгрузка: записать по памяти, что поняли. Окно показывает минуты фокуса за день и неделю и серию дней подряд.

Голосом: «давай повторим», «погоняй меня по курсу», «как мой прогресс».
`,
    },
    en: {
      title: "Your own course",
      lead: "The Learning window is empty until you build a course. Your AI builds it.",
      body: `
1. [Connect your AI](#/docs/connect-ai) to NOAH.
2. Ask: “Build me a NOAH course on travel English: five topics, each with a lesson, tasks and a quiz”.
3. The AI reads the format (\`course_format\`) and sends the course (\`create_course\`), a large one topic by topic (\`add_topic\`). NOAH checks it and shows it in the Learning window.

Each topic has a lesson read section by section — with **🔍 Explain in depth** for a step-by-step breakdown and **💬 Discuss** to ask about it by text or voice — concepts with memory hooks, a concept map, spaced-repetition cards, tasks, a quiz and a cheat sheet. **🎯 Focus session** on the course overview runs a 15, 25 or 50-minute block with a goal, a “later” list for stray thoughts and a recall note at the end.
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
  {
    slug: "update",
    section: "howto",
    ru: {
      title: "Обновить NOAH",
      lead: "Новая версия ставится поверх текущей из окна настроек — удалять и ставить заново не нужно.",
      body: `
NOAH сам раз в несколько часов смотрит, не вышла ли новая версия, и говорит об этом один раз — навязываться не будет.

## Как обновиться

1. Откройте окно NOAH (двойной щелчок по значку в трее) → вкладка **Настройки**.
2. Раздел **Обновление** внизу: там написано, какая версия стоит и какая вышла.
3. Нажмите **Обновить и перезапустить**.

NOAH скачает новую версию, поставит её поверх текущей и перезапустится сам. Настройки, модули, курсы и память о вас остаются на месте: обновляется программа, а не ваши данные.

Кнопки **Обновить** нет, когда обновляться не на что — значит, у вас последняя версия. Посмотреть самому, что вышло, можно кнопкой **Проверить**.

## Почему обновление безопасно

Файл обновления подписан ключом выпуска, и NOAH ставит только то, что этой подписью подтверждено. Подменить обновление по дороге нельзя: подпись не сойдётся, и программа откажется его ставить.

Версии до 1.7.0 обновляться сами не умеют — с них нужно один раз поставить свежую вручную, [скачав установщик](/download). Дальше обновления приходят внутрь программы.
`,
    },
    en: {
      title: "Update NOAH",
      lead: "A new version installs over the current one from the settings window — no uninstalling.",
      body: `
NOAH checks for a new version every few hours and mentions it once.

## How to update

1. Open the NOAH window (double-click the tray icon) → **Settings**.
2. The update section at the bottom shows the installed and the available version.
3. Click the update button — NOAH installs the new version over the current one and restarts itself.

Settings, modules, courses and what it remembers about you stay where they are.

## Why it is safe

The update file is signed with the release key, and NOAH installs only what that signature confirms.

Versions before 1.7.0 cannot update themselves — install a fresh one manually once, [from the installer](/download).
`,
    },
  },

];
