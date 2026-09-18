// Главная страница NOAH: что это, кому, как работает. Живая демонстрация —
// тот же индикатор, что в приложении, и реплики, которые печатаются сами.

const RELEASES = "https://github.com/faafaafuu/asis/releases/latest";

const T = {
  ru: {
    kicker: "МУЛЬТИМОДАЛЬНАЯ ОБОЛОЧКА ДЛЯ ИИ",
    title: ["Любая нейросеть.", "Любые руки.", "Один голос."],
    lead: "NOAH живёт на вашем компьютере и делает то, что вы говорите: открывает программы, находит файлы, присылает скриншоты в Telegram, ставит напоминания. А чего не умеет — тому его учит ваша нейросеть: описываете задачу словами, и через пару минут у помощника новый модуль.",
    download: "Скачать для Windows",
    library: "Открыть библиотеку",
    meta: "бесплатно · Windows 10 / 11 · работает без интернета",
    states: ["ЖДЁТ ИМЕНИ", "СЛУШАЕТ", "ДУМАЕТ", "ОТВЕЧАЕТ"],
    demo: [
      { said: "Ноа, какая погода в Казани?", tool: "weather.weather({ city: \"Казань\" })", answer: "В Казани +13°, ветер 10 км/ч." },
      { said: "Пришли последний скриншот", tool: "telegram.send_photo(\"Снимок экрана.png\")", answer: "Отправил в Telegram." },
      { said: "Сделай модуль, который следит за курсом евро", tool: "create_module → проверка ✓ 4 из 4", answer: "Модуль «Курс евро» готов и уже работает." },
      { said: "Переключись на свою модель", tool: "brains.switch(\"qwen3.5:9b\")", answer: "Переключился на qwen 9b — работаю офлайн." },
    ],
    brainsLabel: "МОЗГ — НА ВЫБОР",
    howKicker: "КАК ЭТО РАБОТАЕТ",
    howTitle: "Три шага до своего помощника",
    how: [
      ["Поставьте NOAH", "Скачайте приложение — оно живёт в трее, слушает имя и сразу умеет десятки вещей: задачи, будильники, файлы, программы, Telegram."],
      ["Подключите мозг", "Своя модель через Ollama — бесплатно и без интернета. Или облачная по ключу: OpenRouter, Google, Groq, Claude. Переключаются голосом."],
      ["Опишите модуль словами", "Подключите NOAH к своей нейросети по MCP и скажите, что нужно. Она напишет модуль, NOAH проверит его и запустит."],
    ],
    featKicker: "ЧТО УМЕЕТ",
    featTitle: "Оболочка, которая растёт вместе с вами",
    voice: ["Голос", "Позовите по имени и говорите как с человеком. Имя — любое, распознавание и голос — на вашем компьютере."],
    check: ["Проверка перед запуском", "Модуль ставится, только если прошёл регламент: правила, живой запуск, тесты, устойчивость к ошибкам."],
    report: ["Описание и файлы по регламенту", "Сервер ответил за 63 мс", "погода в Казани: «Казань: 13°»", "Пережил неверные вызовы"],
    tg: ["Telegram", "Пишите или говорите боту — NOAH ответит тем же и пришлёт файл, снимок экрана или голосовое."],
    tgChat: [["me", "Пришли договор аренды"], ["noa", "Нашёл: договор аренды.pdf"], ["me", "🎙 Поставь напоминание на 9 утра"], ["noa", "Готово: завтра в 9:00."]],
    brains: ["Мозги меняются голосом", "«Какие модели есть?», «переключись на облачную», «давай мистраль» — без настроек."],
    local: ["Локально и приватно", "Голос, файлы и ключи остаются на компьютере. Ключи модулей шифруются, нейросеть их не видит."],
    lib: ["Библиотека модулей", "Готовые модули ставятся одной кнопкой, свои — публикуются из NOAH."],
    libAll: "Все модули →",
    whoKicker: "КОМУ",
    whoTitle: "Для тех, кто хочет, чтобы ИИ делал, а не только говорил",
    who: [
      ["Для себя", "Дела, напоминания, файлы, заказ продуктов, курсы — голосом, без приложений на каждое."],
      ["Без навыков программирования", "Нужен свой инструмент — опишите его словами. Нейросеть напишет, NOAH проверит."],
      ["Для разработчиков", "Любой MCP-сервер становится голосовым модулем. Стандарт открыт, проверка автоматическая."],
      ["Для авторов модулей", "Публикуйте модули в библиотеке и делитесь ими с другими пользователями NOAH."],
    ],
    codeKicker: "ПОД КАПОТОМ",
    codeTitle: "Модуль — это MCP-сервер и один файл",
    codeLead: "Никакого своего протокола и SDK. Нейросеть получает от NOAH регламент с примером, пишет сервер и отправляет его на проверку, пока та не пройдёт.",
    tabs: ["module.json", "server.mjs", "проверка"],
    statsLabel: ["МОДУЛЕЙ", "АВТОРОВ", "МОЗГОВ", "УСТАНОВОК"],
    faqKicker: "ВОПРОСЫ",
    faqTitle: "Частые вопросы",
    faq: [
      ["Это бесплатно?", "Да. Приложение бесплатное, своя модель через Ollama работает без ключей и интернета. Платите только облачному сервису, если выберете его."],
      ["Нужен ли мощный компьютер?", "Для своей модели желательна видеокарта от 6 ГБ. Без неё берите облачную модель — тогда хватит любого компьютера с Windows 10 или 11."],
      ["Как NOAH научится новому?", "Подключите его к своей нейросети по MCP (Claude, Cursor и другие) и опишите задачу словами. Нейросеть напишет модуль, NOAH проверит его и запустит."],
      ["Не опасно ли ставить чужие модули?", "NOAH запускает модуль, только если он прошёл проверку, и только ту версию, что её прошла. Ключи модулей хранятся зашифрованными на вашем компьютере."],
      ["Можно ли управлять из Telegram?", "Да. Заведите своего бота у @BotFather, вставьте токен в NOAH — и пишите или говорите ему, пока компьютер включён."],
    ],
    ctaTitle: "Соберите своего помощника сегодня",
    ctaLead: "Скачайте NOAH, подключите любую нейросеть и скажите, что нужно.",
    ctaStudio: "Открыть студию",
  },
  en: {
    kicker: "MULTIMODAL SHELL FOR AI",
    title: ["Any AI.", "Any hands.", "One voice."],
    lead: "NOAH lives on your computer and does what you say: opens apps, finds files, sends screenshots to Telegram, sets reminders. What it can't do yet, your own AI teaches it: describe the task in plain words, and a couple of minutes later the assistant has a new module.",
    download: "Download for Windows",
    library: "Open the library",
    meta: "free · Windows 10 / 11 · works offline",
    states: ["WAITING", "LISTENING", "THINKING", "ANSWERING"],
    demo: [
      { said: "Noah, what's the weather in Berlin?", tool: "weather.weather({ city: \"Berlin\" })", answer: "Berlin: 13°, wind 10 km/h." },
      { said: "Send me the last screenshot", tool: "telegram.send_photo(\"Screenshot.png\")", answer: "Sent it to Telegram." },
      { said: "Build a module that tracks the euro rate", tool: "create_module → check ✓ 4 of 4", answer: "The «Euro rate» module is ready and running." },
      { said: "Switch to the local model", tool: "brains.switch(\"qwen3.5:9b\")", answer: "Switched to qwen 9b — working offline." },
    ],
    brainsLabel: "PICK A BRAIN",
    howKicker: "HOW IT WORKS",
    howTitle: "Three steps to your own assistant",
    how: [
      ["Install NOAH", "Download the app — it lives in the tray, listens for its name and already does dozens of things: tasks, alarms, files, apps, Telegram."],
      ["Plug in a brain", "A local model through Ollama — free and offline. Or a cloud one by key: OpenRouter, Google, Groq, Claude. Switch them by voice."],
      ["Describe a module", "Connect NOAH to your AI over MCP and say what you need. It writes the module, NOAH checks it and starts it."],
    ],
    featKicker: "WHAT IT DOES",
    featTitle: "A shell that grows with you",
    voice: ["Voice", "Call it by name and talk like to a person. Any name; speech recognition and the voice run on your computer."],
    check: ["Checked before it runs", "A module installs only after it passes the standard: rules, a live start, tests, and resilience to bad calls."],
    report: ["Manifest and files follow the standard", "Server answered in 63 ms", "weather in Berlin: «Berlin: 13°»", "Survived bad calls"],
    tg: ["Telegram", "Text or talk to your bot — NOAH answers the same way and sends files, screenshots or voice notes."],
    tgChat: [["me", "Send me the lease"], ["noa", "Found it: lease.pdf"], ["me", "🎙 Remind me at 9 am"], ["noa", "Done: tomorrow at 9:00."]],
    brains: ["Brains switch by voice", "«Which models do we have?», «switch to the cloud one», «use mistral» — no settings."],
    local: ["Local and private", "Voice, files and keys stay on your computer. Module keys are encrypted; your AI never sees them."],
    lib: ["Module library", "Ready modules install in one click, your own publish straight from NOAH."],
    libAll: "All modules →",
    whoKicker: "FOR WHOM",
    whoTitle: "For people who want AI to act, not just talk",
    who: [
      ["For yourself", "Tasks, reminders, files, groceries, courses — by voice, without an app for each."],
      ["No coding skills", "Need your own tool? Describe it. Your AI writes it, NOAH checks it."],
      ["For developers", "Any MCP server becomes a voice module. Open standard, automatic checks."],
      ["For module authors", "Publish modules in the library and share them with other NOAH users."],
    ],
    codeKicker: "UNDER THE HOOD",
    codeTitle: "A module is an MCP server and one file",
    codeLead: "No custom protocol, no SDK. Your AI gets the standard from NOAH with an example, writes the server and submits it until the check passes.",
    tabs: ["module.json", "server.mjs", "check"],
    statsLabel: ["MODULES", "AUTHORS", "BRAINS", "INSTALLS"],
    faqKicker: "QUESTIONS",
    faqTitle: "FAQ",
    faq: [
      ["Is it free?", "Yes. The app is free, and a local model through Ollama works without keys or internet. You pay only a cloud service if you choose one."],
      ["Do I need a powerful computer?", "A local model wants a GPU with 6 GB or more. Without one, pick a cloud model — then any Windows 10 or 11 computer will do."],
      ["How does NOAH learn new things?", "Connect it to your AI over MCP (Claude, Cursor and others) and describe the task. Your AI writes a module, NOAH checks it and starts it."],
      ["Is it safe to install other people's modules?", "NOAH runs a module only if it passed the check, and only the exact version that passed. Module keys are stored encrypted on your computer."],
      ["Can I use it from Telegram?", "Yes. Create your own bot with @BotFather, paste the token into NOAH, and text or talk to it while the computer is on."],
    ],
    ctaTitle: "Build your assistant today",
    ctaLead: "Download NOAH, plug in any AI and say what you need.",
    ctaStudio: "Open the studio",
  },
};

const BRAINS = ["Ollama", "Qwen", "Gemma", "Mistral", "DeepSeek", "Claude", "Gemini", "Llama", "Groq", "OpenRouter", "OpenAI API"];

const MANIFEST = `{
  "id": "euro-rate",
  "title": "Курс евро",
  "icon": "€",
  "about": "Курс евро к рублю от ЦБ",
  "voice": "«Ноа, какой курс евро»",
  "mcp": {
    "command": "node",
    "args": ["%MODULE_DIR%\\\\server.mjs"]
  },
  "tests": [
    { "tool": "rate", "args": {}, "expect": "₽" }
  ]
}`;

const SERVER = `import readline from "node:readline";

const tools = [{
  name: "rate",
  description: "Курс евро к рублю на сегодня",
  inputSchema: { type: "object", properties: {} },
}];

async function rate() {
  const res = await fetch("https://www.cbr-xml-daily.ru/daily_json.js",
    { signal: AbortSignal.timeout(15000) });
  const { Valute } = await res.json();
  return \`Евро — \${Valute.EUR.Value.toFixed(2)} ₽.\`;
}
// … initialize, tools/list, tools/call — по регламенту`;

const REPORT = `Модуль «Курс евро» прошёл проверку.

Пройдено:
  ✓ Описание и файлы соответствуют регламенту.
  ✓ Сервер запустился и ответил за 58 мс.
  ✓ курс: «Евро — 94.12 ₽.» за 412 мс.
  ✓ Сервер пережил неверные вызовы.

Ноа запустила модуль: работает · инструментов: 1.`;

/* ── Индикатор ────────────────────────────────────────────────────────────── */

const PALETTE = {
  idle: ["oklch(0.72 0.08 250)", "oklch(0.68 0.09 250)", "oklch(0.8 0.06 230)"],
  listening: ["oklch(0.8 0.15 160)", "oklch(0.76 0.16 190)", "oklch(0.84 0.13 140)"],
  thinking: ["oklch(0.82 0.15 85)", "oklch(0.78 0.16 70)", "oklch(0.86 0.12 95)"],
  speaking: ["oklch(0.72 0.19 25)", "oklch(0.78 0.17 45)", "oklch(0.84 0.14 70)"],
};
const withAlpha = (color, alpha) => color.replace(")", ` / ${alpha})`);

function startHud(canvas, getMode) {
  const W = 360;
  const H = 180;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);
  let seed = 7;
  const random = () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
  const particles = Array.from({ length: 26 }, (_, i) => ({
    a: (i / 26) * Math.PI * 2,
    r: 60 + random() * 30,
    speed: 0.15 + random() * 0.3,
    size: 1 + random() * 1.8,
    phase: random() * Math.PI * 2,
  }));
  let time = 0;
  let level = 0;
  let alive = true;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const frame = () => {
    if (!alive || !canvas.isConnected) return;
    const mode = getMode();
    time += 0.016;
    const target =
      mode === "speaking"
        ? 0.55 + Math.sin(time * 3.1) * 0.25 + Math.sin(time * 7.3) * 0.12
        : mode === "idle"
          ? 0.12
          : 0.32 + Math.sin(time * 2) * 0.08;
    level += (target - level) * (mode === "speaking" ? 0.35 : 0.12);
    const colors = PALETTE[mode];
    const cx = W / 2;
    const cy = H / 2;
    ctx.clearRect(0, 0, W, H);

    const cloudR = 70 + level * 40;
    const cloud = ctx.createRadialGradient(cx, cy, 4, cx, cy, cloudR);
    cloud.addColorStop(0, withAlpha(colors[0], 0.3 + level * 0.2));
    cloud.addColorStop(0.5, withAlpha(colors[1], 0.1));
    cloud.addColorStop(1, "transparent");
    ctx.fillStyle = cloud;
    ctx.beginPath();
    ctx.arc(cx, cy, cloudR, 0, Math.PI * 2);
    ctx.fill();

    for (const p of particles) {
      p.a += p.speed * 0.012;
      const radius = p.r + Math.sin(time * 1.5 + p.phase) * 6 + level * 18;
      ctx.fillStyle = withAlpha(colors[2], 0.15 + Math.abs(Math.sin(time * 2 + p.phase)) * 0.35);
      ctx.beginPath();
      ctx.arc(cx + Math.cos(p.a) * radius, cy + Math.sin(p.a) * radius * 0.62, p.size, 0, Math.PI * 2);
      ctx.fill();
    }

    const speaking = mode === "speaking";
    const spikes = speaking ? 20 : 64;
    const baseR = 34 + level * 10;
    ctx.beginPath();
    for (let i = 0; i <= spikes; i++) {
      const a = (i / spikes) * Math.PI * 2;
      const noise = speaking
        ? Math.sin(a * 5 + time * 3) * 0.6 + Math.sin(a * 3 - time * 1.8) * 0.4
        : Math.sin(a * 9 + time * 4) * 0.5 + Math.sin(a * 5 - time * 2.6) * 0.3 + Math.sin(a * 17 + time * 6) * 0.2;
      const r = baseR + noise * ((speaking ? 17 : mode === "idle" ? 3 : 7) * level + 2);
      const x = cx + Math.cos(a) * r;
      const y = cy + Math.sin(a) * r;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
    const ring = ctx.createLinearGradient(cx - baseR, cy, cx + baseR, cy);
    ring.addColorStop(0, colors[0]);
    ring.addColorStop(0.5, colors[1]);
    ring.addColorStop(1, colors[2]);
    ctx.strokeStyle = ring;
    ctx.lineWidth = 2.2;
    ctx.shadowColor = colors[1];
    ctx.shadowBlur = 18 + level * 20;
    ctx.stroke();
    ctx.shadowBlur = 0;
    if (!reduce) requestAnimationFrame(frame);
  };
  frame();
  return () => (alive = false);
}

/* ── Демонстрация ─────────────────────────────────────────────────────────── */

function startDemo(root, tr, h) {
  const said = root.querySelector(".demo__said");
  const tool = root.querySelector(".demo__tool");
  const answer = root.querySelector(".demo__answer");
  const pills = [...root.querySelectorAll(".demo__state")];
  let mode = "idle";
  let scene = 0;
  let stopped = false;
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const setMode = (next, step) => {
    mode = next;
    pills.forEach((pill, at) => pill.classList.toggle("is-on", at === step));
  };
  const type = async (node, text, speed) => {
    node.textContent = "";
    for (let i = 1; i <= text.length && !stopped && node.isConnected; i++) {
      node.textContent = text.slice(0, i);
      await sleep(speed);
    }
  };

  (async () => {
    while (!stopped && root.isConnected) {
      const item = tr.demo[scene % tr.demo.length];
      said.textContent = "";
      tool.textContent = "";
      answer.textContent = "";
      tool.classList.remove("is-on");
      setMode("idle", 0);
      await sleep(900);
      setMode("listening", 1);
      await type(said, item.said, 38);
      await sleep(400);
      setMode("thinking", 2);
      tool.classList.add("is-on");
      await type(tool, item.tool, 16);
      await sleep(700);
      setMode("speaking", 3);
      await type(answer, item.answer, 30);
      await sleep(2600);
      scene++;
    }
  })();

  return { mode: () => mode, stop: () => (stopped = true) };
}

/* ── Страница ─────────────────────────────────────────────────────────────── */

let running = null;

export function renderHome(page, { h, icon, lang, stats, modules, number }) {
  running?.stop();
  const tr = T[lang];

  const console_ = h(
    "div",
    { class: "demo" },
    h(
      "div",
      { class: "demo__bar" },
      h("span", { class: "demo__dots" }, h("i"), h("i"), h("i")),
      h("span", { class: "mono" }, "NOAH"),
    ),
    h("canvas", { class: "demo__hud", "aria-hidden": "true" }),
    h("div", { class: "demo__states" }, tr.states.map((s) => h("span", { class: "demo__state" }, s))),
    h(
      "div",
      { class: "demo__log" },
      h("p", { class: "demo__line" }, h("span", { class: "demo__who" }, lang === "ru" ? "ВЫ" : "YOU"), h("span", { class: "demo__said" })),
      h("p", { class: "demo__line" }, h("span", { class: "demo__who" }, "MCP"), h("code", { class: "demo__tool" })),
      h("p", { class: "demo__line" }, h("span", { class: "demo__who demo__who--noa" }, "NOAH"), h("span", { class: "demo__answer" })),
    ),
  );

  const hero = h(
    "section",
    { class: "hero" },
    h(
      "div",
      { class: "hero__text" },
      h("span", { class: "kicker" }, tr.kicker),
      h("h1", { class: "hero__title" }, tr.title.map((line, at) => h("span", { class: at === 2 ? "is-gold" : "" }, line))),
      h("p", { class: "hero__lead" }, tr.lead),
      h(
        "div",
        { class: "hero__cta" },
        h("a", { class: "btn btn--gold btn--big", href: RELEASES, target: "_blank", rel: "noopener" }, tr.download),
        h("a", { class: "btn btn--ghost btn--big", href: "#/library" }, tr.library),
      ),
      h("p", { class: "hero__meta mono" }, tr.meta),
    ),
    console_,
  );

  const brains = h(
    "section",
    { class: "band" },
    h("span", { class: "label" }, tr.brainsLabel),
    h("div", { class: "band__row" }, BRAINS.map((b) => h("span", { class: "band__item" }, b))),
  );

  const tiles = ["#2B5BC4", "#F2C14E", "#D4564A"];
  const how = h(
    "section",
    { class: "sect" },
    h("span", { class: "kicker" }, tr.howKicker),
    h("h2", { class: "sect__title" }, tr.howTitle),
    h(
      "div",
      { class: "how" },
      tr.how.map(([title, body], at) =>
        h(
          "article",
          { class: "plate plate--accent how__step", vars: { "--accent": tiles[at] } },
          h("span", { class: "how__num" }, `0${at + 1}`),
          h("h3", {}, title),
          h("p", {}, body),
        ),
      ),
    ),
  );

  const cell = (cls, accent, title, body, ...extra) =>
    h("article", { class: `bento__cell ${cls}`, vars: { "--accent": accent } }, h("h3", {}, title), h("p", {}, body), ...extra);

  const topModules = modules.slice(0, 3).map((m) =>
    h("a", { class: "bento__mod", href: `#/module/${m.id}` }, h("span", { class: "bento__icon" }, m.icon || "✦"), h("span", {}, m.title), h("span", { class: "mono" }, "→")),
  );

  const bento = h(
    "section",
    { class: "sect" },
    h("span", { class: "kicker" }, tr.featKicker),
    h("h2", { class: "sect__title" }, tr.featTitle),
    h(
      "div",
      { class: "bento" },
      cell(
        "bento__cell--wide",
        "#2B5BC4",
        ...tr.voice,
        h("div", { class: "pills" }, tr.states.map((s, at) => h("span", { class: `pill${at === 1 ? " is-on" : ""}` }, s))),
      ),
      cell(
        "bento__cell--dark",
        "#5F8C4C",
        ...tr.check,
        h("pre", { class: "bento__report" }, tr.report.map((line) => `✓ ${line}`).join("\n")),
      ),
      cell(
        "",
        "#F2C14E",
        ...tr.tg,
        h("div", { class: "chat" }, tr.tgChat.map(([who, text]) => h("span", { class: `chat__msg chat__msg--${who}` }, text))),
      ),
      cell("", "#D4564A", ...tr.brains, h("div", { class: "pills" }, ["qwen 9b", "mistral", "claude", "deepseek"].map((b, at) => h("span", { class: `pill${at === 0 ? " is-on" : ""}` }, b)))),
      cell("", "#2B5BC4", ...tr.local, h("div", { class: "bento__lock" }, icon("memory"))),
      cell("bento__cell--wide", "#F2C14E", ...tr.lib, h("div", { class: "bento__mods" }, topModules, h("a", { class: "bento__all", href: "#/library" }, tr.libAll))),
    ),
  );

  const whoTiles = ["#2B5BC4", "#F2C14E", "#D4564A", "#5F8C4C"];
  const whoIcons = ["home", "notes", "code", "chart"];
  const who = h(
    "section",
    { class: "sect" },
    h("span", { class: "kicker" }, tr.whoKicker),
    h("h2", { class: "sect__title" }, tr.whoTitle),
    h(
      "div",
      { class: "who" },
      tr.who.map(([title, body], at) =>
        h(
          "article",
          { class: "who__card", vars: { "--accent": whoTiles[at] } },
          h("span", { class: "tile", vars: { "--accent": whoTiles[at], "--accent-fg": at === 1 ? "#171B26" : "#FFFFFF" } }, icon(whoIcons[at])),
          h("h3", {}, title),
          h("p", {}, body),
        ),
      ),
    ),
  );

  const sources = [MANIFEST, SERVER, REPORT];
  const pre = h("pre", { class: "code__body" }, sources[0]);
  const tabButtons = tr.tabs.map((label, at) =>
    h(
      "button",
      {
        type: "button",
        class: "code__tab",
        "aria-pressed": String(at === 0),
        onclick: () => {
          pre.textContent = sources[at];
          tabButtons.forEach((b, i) => b.setAttribute("aria-pressed", String(i === at)));
        },
      },
      label,
    ),
  );
  const code = h(
    "section",
    { class: "sect split" },
    h(
      "div",
      { class: "split__text" },
      h("span", { class: "kicker" }, tr.codeKicker),
      h("h2", { class: "sect__title" }, tr.codeTitle),
      h("p", { class: "lead" }, tr.codeLead),
      h("a", { class: "btn btn--gold", href: "#/standard" }, lang === "ru" ? "Стандарт модуля" : "Module standard"),
    ),
    h("div", { class: "code code--tabs" }, h("div", { class: "code__head" }, tabButtons), pre),
  );

  const s = stats ?? { modules: 0, authors: 0, brains: 0, installs: 0 };
  const strip = h(
    "section",
    { class: "strip strip--home" },
    [
      [s.modules, "#E9EEF9"],
      [s.authors, "#FBF2DC"],
      [s.brains, "#F8E9E6"],
      [s.installs, "#EAF0E6"],
    ].map(([value, bg], at) =>
      h("div", { class: "stat", vars: { "--bg": bg } }, h("div", { class: "stat__value" }, number(value)), h("div", { class: "stat__label" }, tr.statsLabel[at])),
    ),
  );

  const faq = h(
    "section",
    { class: "sect" },
    h("span", { class: "kicker" }, tr.faqKicker),
    h("h2", { class: "sect__title" }, tr.faqTitle),
    h(
      "div",
      { class: "plate faq" },
      tr.faq.map(([q, a], at) => h("details", { class: "faq__item", open: at === 0 }, h("summary", {}, q), h("p", {}, a))),
    ),
  );

  const cta = h(
    "section",
    { class: "finale" },
    h("h2", {}, tr.ctaTitle),
    h("p", {}, tr.ctaLead),
    h(
      "div",
      { class: "hero__cta" },
      h("a", { class: "btn btn--dark btn--big", href: RELEASES, target: "_blank", rel: "noopener" }, tr.download),
      h("a", { class: "btn btn--big", href: "#/studio" }, tr.ctaStudio),
    ),
  );

  page.replaceChildren(hero, brains, how, bento, who, code, strip, faq, cta);
  running = startDemo(console_, tr, h);
  startHud(console_.querySelector(".demo__hud"), running.mode);
}

export function stopHome() {
  running?.stop();
  running = null;
}
