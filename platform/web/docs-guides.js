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
| [macOS](/download?os=mac) (Intel и Apple Silicon) | \`.dmg\` | объяснения, модели, модули, задачи, обучение; голоса и снимков экрана пока нет. Файл без подписи Apple: при первом запуске — «Системные настройки» → «Конфиденциальность и безопасность» → «Всё равно открыть» |
| [Linux](/download?os=linux) | \`.AppImage\`, \`.deb\` | как на macOS: без голоса и снимков экрана |
| [Android](/download?os=android) | \`.apk\` | разговор с Ноа голосом и текстом, «Объяснить» в меню выделения, модули (встроенные и по ссылке), задачи, будильники, обучение, Telegram. Позвать — микрофоном на экране, ярлыком «Спросить голосом», плиткой «Ноа» в шторке или жестом помощника |

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

[Download the installer](/download) — the site picks the file for your system — and run it. NOAH lives in the tray; double-click the icon to open settings. Builds: [Windows](/download?os=windows), [macOS](/download?os=mac) (Intel and Apple Silicon; not signed by Apple — the first time, allow it in System Settings → Privacy & Security → Open Anyway), [Linux](/download?os=linux) (.AppImage, .deb), [Android](/download?os=android) (.apk: talk to NOAH by voice or text, “Explain” in the text selection menu, modules, tasks, alarms, learning). On macOS and Linux there is no voice yet.

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
      lead: "Окно «Обучение» пустое, пока вы не соберёте курс. Собрать его можно прямо в Ноа онлайн или своей нейросетью.",
      body: `
## В Ноа онлайн — одной кнопкой

1. Откройте [Ноа онлайн](/app/) и войдите на сайт.
2. В разделе «Обучение» — **Собрать курс**: напишите, о чём курс и зачем. Например: «слаботочные системы для монтажника с нуля — кабели, СКС, видеонаблюдение; чтобы пройти собеседование».
3. Ноа составит план и будет писать тему за темой: урок, понятия, карточки, задачи, мини-экзамен, в конце — финальный экзамен на стык тем. Каждая тема проходит ту же проверку, что курс от нейросети.

Курс появляется после первой темы — по ней можно учиться сразу, остальные дорастают сами, и в списке, и в открытом окне обучения. С мостом сборка идёт на сервере: около минуты на тему с **Sonnet**, страницу можно закрыть. **Haiku** бережёт лимит подписки, но медленнее и чаще ошибается в фактах. Без моста курс собирает модель, подключённая в браузере, — тогда вкладку закрывать нельзя.

Если с мобильного интернета noahlab.ru не открывается, открывайте Ноа онлайн через российское зеркало: [m.noahlab.ru/app](https://m.noahlab.ru/app/). Там входите через Telegram — вход Google работает только на noahlab.ru.

## Своей нейросетью

1. [Подключите нейросеть](#/docs/connect-ai) к NOAH — по ссылке с сайта или локально.
2. Попросите: «Собери мне курс NOAH по английскому для путешествий: пять тем». Можно дать свои материалы — конспекты, описание вакансии, программу экзамена.
3. Нейросеть прочитает формат (\`course_format\`) и отправит курс (\`create_course\`), большой — по теме (\`add_topic\`). NOAH проверит его, вернёт отчёт с замечаниями и покажет курс в окне «Обучение» — и в Ноа онлайн, по мере того как темы приходят.

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
      lead: "The Learning window is empty until you build a course. Build it right in Noa online or with your own AI.",
      body: `
**In Noa online:** open [Noa online](/app/), sign in, press **Build a course** in Learning and describe what it is about and why. Noa writes the plan and then topic after topic; the course appears after the first topic and the rest arrive by themselves. With the bridge the build runs on the server, about a minute per topic with Sonnet. If noahlab.ru does not open on mobile internet in Russia, use the Russian mirror [m.noahlab.ru/app](https://m.noahlab.ru/app/) and sign in with Telegram.

**With your own AI:**

1. [Connect your AI](#/docs/connect-ai) to NOAH.
2. Ask: “Build me a NOAH course on travel English: five topics, each with a lesson, tasks and a quiz”.
3. The AI reads the format (\`course_format\`) and sends the course (\`create_course\`), a large one topic by topic (\`add_topic\`). NOAH checks it and shows it in the Learning window.

Each topic has a lesson read section by section — with **🔍 Explain in depth** for a step-by-step breakdown and **💬 Discuss** to ask about it by text or voice — concepts with memory hooks, a concept map, spaced-repetition cards, tasks, a quiz and a cheat sheet. **🎯 Focus session** on the course overview runs a 15, 25 or 50-minute block with a goal, a “later” list for stray thoughts and a recall note at the end.
`,
    },
  },
  {
    slug: "iphone",
    section: "howto",
    ru: {
      title: "Обучение на iPhone",
      lead: "«Ноа · Обучение» — приложение для iPhone: то же обучение, что в программе и в Ноа онлайн, во весь экран и с общим прогрессом.",
      body: `
В App Store приложения нет — файл ставится через [Sideloadly](https://sideloadly.io) с компьютера.

## Установка

1. Скачайте файл \`Noah_Learning_…_ios.ipa\` со страницы [последнего выпуска](https://github.com/faafaafuu/asis/releases/latest).
2. Поставьте Sideloadly на компьютер (Windows или macOS) и подключите iPhone кабелем.
3. Перетащите \`.ipa\` в окно Sideloadly, впишите свой Apple ID и нажмите **Start**. Бесплатный Apple ID подходит.
4. На iPhone: **Настройки → Основные → VPN и управление устройством** — доверьте своему Apple ID. С iOS 16 включите ещё **Настройки → Конфиденциальность и безопасность → Режим разработчика**.

С бесплатным Apple ID подпись действует 7 дней: потом снова откройте Sideloadly и нажмите **Start** — прогресс не потеряется, он хранится на сайте.

## Что внутри

Окно обучения Ноа онлайн: уроки, 🎙 урок с Ноа голосом, карточки, конспект, задачи и экзамены. Прогресс общий с программой на компьютере и браузером. Приложение открывает российское зеркало — с мобильного интернета оно работает, где noahlab.ru режется.

Войти — **через Telegram**: приложение откроет бота, нажмите **Start** и вернитесь — вход завершится сам. Вход Google внутри приложений Google запрещает.

Обновлять приложение не нужно: новое в обучении приходит с сайта. Практика в терминале — только в программе на компьютере.
`,
    },
    en: {
      title: "Learning on iPhone",
      lead: "“Noah · Learning” for iPhone: the same learning as in the app and Noah online, full screen, with shared progress.",
      body: `
Not in the App Store — install the \`Noah_Learning_…_ios.ipa\` from the [latest release](https://github.com/faafaafuu/asis/releases/latest) with [Sideloadly](https://sideloadly.io): drag the file in, enter your Apple ID (a free one works), press **Start**, then trust your Apple ID in **Settings → General → VPN & Device Management** (and turn on **Developer Mode** on iOS 16+). A free Apple ID signature lasts 7 days — re-run Sideloadly; progress lives on the site. Sign in with Telegram; Google blocks sign-in inside apps.
`,
    },
  },
  {
    slug: "practice",
    section: "howto",
    ru: {
      title: "Практика в терминале",
      lead: "Тема курса — руками на своём сервере: вы работаете в терминале, Ноа смотрит и ведёт по шагам.",
      body: `
Практика — в программе NOAH на компьютере: окно **Практика** — слева настоящий терминал, справа Ноа. Открыть — вкладкой **🖥 Практика** в теме курса, карточкой «Практика» в модулях или голосом: «Ноа, открой практику».

## Как проходит

1. Выберите тему курса — у каждой своя практика. Можно дописать пожелание: «на трёх серверах», «через Docker».
2. **Составить сценарий** — Ноа соберёт задачу на понятиях урока: 5–8 шагов в порядке разделов, у шага — понятие из урока, что сделать и как проверить, и общая картина схемой.
3. В терминале подключитесь к серверу — \`ssh user@адрес\` — и работайте сами. Вставить — Ctrl+V или правый клик.
4. **✓ Проверь шаг** — Ноа посмотрит в терминал и скажет, готово ли; готово — переходит к следующему. **→** — засчитать шаг без проверки.

Всё как в проде: не от root, вход по ключам, фаервол, конфиги файлами, закреплённые версии. Костыли — \`chmod 777\`, выключенный фаервол, образ \`:latest\`, \`--insecure\` — Ноа замечает сразу и говорит, как правильно.

## Когда Ноа говорит

Каждый взгляд модели в терминал — это запрос и токены, поэтому Ноа молчит, пока всё идёт как надо. Частые ошибки — нет прав, нет команды, нет пакета, порт занят, сервер не принял ключ — она разбирает сразу, без модели. Модель подключается, если упало незнакомое, если вы спросили или нажали «Проверь шаг». **💬** — разбирать каждую команду.

Вслух Ноа код не читает: говорит словами, что делает команда, что куда идёт и откуда. Команды — в ленте на экране.

## Что уходит модели

Вывод терминала уходит вашей модели. Пароли, токены, ключи API и закрытые ключи Ноа закрывает до отправки, **👁** выключает наблюдение совсем. Полноэкранные программы — vim, htop, less — Ноа не смотрит.

Практика темы сохраняется: открыли снова — тот же шаг и та же лента. **Тема ▾** внизу окна — перейти к другой теме, у начатых видно, на каком шаге остановились. Границы терминала, панели Ноа и ленты тянутся мышью.
`,
    },
    en: {
      title: "Hands-on practice in the terminal",
      lead: "A course topic done by hand on your own server: you work in the terminal, Noa watches and guides step by step.",
      body: `
Practice lives in the NOAH desktop app: the **Practice** window has a real terminal on the left and Noa on the right. Open it from the **🖥 Practice** tab of a course topic, the Practice module card or by voice.

Pick a topic — each has its own practice — and press **Build a scenario**: 5–8 steps in lesson order, each tied to a lesson concept, plus a big-picture diagram. Connect to your server with \`ssh\` and work yourself. **✓ Check step** asks Noa to verify the step from the terminal. Everything is production-grade: no root, SSH keys, firewall, config files, pinned versions; shortcuts like \`chmod 777\` or \`:latest\` are flagged at once.

Noa stays quiet while things go well; common errors are explained instantly without the model, the model is asked on unknown failures, questions and step checks. Terminal output goes to your model with passwords, tokens and keys masked; **👁** turns watching off.
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
      lead: "С версии 1.13 NOAH обновляется сам: скачивает новую версию в фоне, ставит без окон и перезапускается.",
      body: `
Делать ничего не нужно. NOAH раз в несколько часов смотрит, не вышла ли новая версия, и скачивает её в фоне. Ставит в тихую минуту — когда его окна закрыты, он не говорит и не слушает, а к компьютеру пару минут не прикасались. Установка идёт без окна и без вопросов Windows: программа стоит в вашей папке, прав администратора ей не нужно. Через несколько секунд NOAH снова в трее — уже новый.

Настройки, модули, курсы и память о вас остаются на месте: обновляется программа, а не ваши данные.

## Не ждать

1. Откройте окно NOAH (двойной щелчок по значку в трее) → вкладка **Помощь**.
2. Раздел **Обновление**: там написано, какая версия стоит и какая вышла.
3. **Обновить сейчас** — NOAH поставит её и перезапустится.

Кнопки нет, когда обновляться не на что. Посмотреть самому, что вышло, — **Проверить**.

## Почему обновление безопасно

Файл обновления подписан ключом выпуска, и NOAH ставит только то, что этой подписью подтверждено. Подменить обновление по дороге нельзя: подпись не сойдётся, и программа откажется его ставить.

Версии до 1.7.0 обновляться сами не умеют — с них нужно один раз поставить свежую вручную, [скачав установщик](/download). С 1.7.0 до 1.12 обновление ставится кнопкой, с 1.13 — само.

На Android .apk ставит сама система: NOAH скачивает его, а подтвердить установку Android попросит вас.
`,
    },
    en: {
      title: "Update NOAH",
      lead: "Since 1.13 NOAH updates itself: it downloads the new version in the background, installs it without any windows and restarts.",
      body: `
Nothing to do. NOAH checks for a new version every few hours and downloads it in the background. It installs it at a quiet moment — its windows are closed, it is not speaking or listening, and the computer has been idle for a couple of minutes. No installer window, no Windows prompts; a few seconds later NOAH is back in the tray, updated.

Settings, modules, courses and what it remembers about you stay where they are.

## Don't want to wait

Open the NOAH window → **Help** → **Update** → **Update now**.

## Why it is safe

The update file is signed with the release key, and NOAH installs only what that signature confirms.

Versions before 1.7.0 cannot update themselves — install a fresh one manually once, [from the installer](/download). From 1.7.0 to 1.12 the update is installed with a button, from 1.13 by itself.
`,
    },
  },

];
