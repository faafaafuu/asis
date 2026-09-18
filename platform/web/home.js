// Главная страница NOAH: что это, кому, как работает. Живая демонстрация —
// тот же индикатор, что в приложении, и реплики, которые печатаются сами.

const RELEASES = "https://github.com/faafaafuu/asis/releases/latest";

const T = {
  ru: {
    kicker: "ЯДРО · МУЛЬТИМОДАЛЬНАЯ ОБОЛОЧКА ДЛЯ ИИ",
    title: ["Ядро для ИИ.", "Модули — словами.", "Без кода."],
    lead: "NOAH — оболочка на вашем компьютере. Подключаете любую нейросеть — это мозг. Руки собираете сами: опишите задачу словами, ваша нейросеть напишет модуль, NOAH проверит его и запустит. Получилось полезное — выложите в библиотеку для всех или продавайте.",
    primary: "Подключить свой ИИ",
    download: "Скачать NOAH",
    meta: "бесплатно · Windows 10 / 11 · любая нейросеть",
    states: ["ЖДЁТ", "СЛУШАЕТ", "СОБИРАЕТ", "ГОТОВО"],
    demo: [
      { said: "Сделай модуль, который каждое утро говорит курс евро", tool: "module_format → create_module", answer: "Проверка ✓ 4 из 4. Модуль «Курс евро» работает." },
      { said: "Ноа, какой курс евро?", tool: "euro-rate.rate()", answer: "Евро — 94,12 ₽." },
      { said: "Выложи этот модуль в библиотеку", tool: "publish_module(\"euro-rate\")", answer: "Модуль в библиотеке NOAH — им могут пользоваться все." },
      { said: "Собери модуль, который разбирает мои чеки", tool: "create_module → проверка ✗ 1 → исправлено ✓", answer: "Модуль «Чеки» готов: скажите «Ноа, сколько я потратил»." },
    ],
    brainsLabel: "МОЗГ — ЛЮБОЙ",
    howKicker: "КАК ЭТО РАБОТАЕТ",
    howTitle: "От идеи до модуля — за пару минут",
    how: [
      ["Поставьте ядро", "Скачайте NOAH. Он живёт в трее, слышит голос и уже умеет десятки вещей из коробки."],
      ["Подключите свой ИИ", "Одна ссылка MCP — и ваш Claude, Cursor или другая нейросеть получают руки на вашем компьютере."],
      ["Опишите модуль словами", "«Хочу, чтобы…» — нейросеть пишет модуль, NOAH проверяет его по регламенту и запускает."],
      ["Делитесь или продавайте", "Выложите модуль в библиотеку: его поставят другие одной кнопкой. Платные модули — скоро."],
    ],
    featKicker: "ЧТО ЭТО ДАЁТ",
    featTitle: "Одно ядро — бесконечно много рук",
    build: ["Любые модули без кода", "Счета, чеки, заказы, отчёты, умный дом, работа — всё, что можно описать словами, становится модулем."],
    buildPills: ["курс валют", "разбор чеков", "утренний отчёт", "умный дом", "мои задачи", "поиск по документам"],
    check: ["Проверка перед запуском", "Модуль ставится, только если прошёл регламент: правила, живой запуск, тесты, устойчивость к ошибкам."],
    report: ["Описание и файлы по регламенту", "Сервер ответил за 58 мс", "курс: «Евро — 94,12 ₽»", "Пережил неверные вызовы"],
    multi: ["Мультимодальность", "Говорите голосом, пишите текстом, в Telegram, выделяйте текст на экране — ядро понимает всё и зовёт нужный модуль."],
    multiPills: ["голос", "текст", "Telegram", "выделение", "MCP"],
    brains: ["Любой мозг", "Своя модель — бесплатно и офлайн, облачная — по ключу. Меняются одной фразой: «переключись на мистраль»."],
    market: ["Библиотека и продажа", "Готовые модули ставятся одной кнопкой. Свои — публикуете и делитесь; платные модули — скоро."],
    local: ["Локально и приватно", "Голос, файлы и ключи остаются на компьютере. Ключи модулей шифруются, нейросеть их не видит."],
    libAll: "Все модули →",
    whoKicker: "КОМУ",
    whoTitle: "Каждому, кто хочет, чтобы ИИ делал, а не только говорил",
    who: [
      ["Без навыков программирования", "Нужен свой инструмент — опишите его словами. Нейросеть напишет, NOAH проверит и запустит."],
      ["Для авторов модулей", "Соберите полезный модуль, выложите в библиотеку — им будут пользоваться другие. Скоро — продажа."],
      ["Для разработчиков", "Любой MCP-сервер становится голосовым модулем. Стандарт открыт, проверка автоматическая."],
      ["Для себя", "Дела, напоминания, файлы, заказы, курсы — голосом, без отдельного приложения на каждое."],
    ],
    codeKicker: "ПОД КАПОТОМ",
    codeTitle: "Модуль — это MCP-сервер и один файл",
    codeLead: "Никакого своего протокола и SDK. Ваша нейросеть получает от NOAH регламент с примером, пишет сервер и отправляет его на проверку, пока та не пройдёт.",
    standard: "Стандарт модуля",
    tabs: ["module.json", "server.mjs", "проверка"],
    statsLabel: ["МОДУЛЕЙ", "АВТОРОВ", "МОЗГОВ", "УСТАНОВОК"],
    faqKicker: "ВОПРОСЫ",
    faqTitle: "Частые вопросы",
    faq: [
      ["Нужно ли уметь программировать?", "Нет. Вы описываете задачу словами, модуль пишет ваша нейросеть, а NOAH проверяет его и возвращает на доработку, пока всё не заработает."],
      ["Как подключить свою нейросеть?", "Войдите на сайт, откройте «Подключить ИИ» и скопируйте свою ссылку MCP. Вставьте её в Claude, Cursor или другой клиент с MCP — и в NOAH тот же ключ, чтобы он забирал модули."],
      ["Можно ли продавать модули?", "Публиковать и делиться можно уже сейчас. Платные модули и выплаты авторам появятся позже — кабинет автора для них уже есть."],
      ["Не опасно ли ставить чужие модули?", "NOAH запускает модуль, только если он прошёл проверку, и только ту версию, что её прошла. Ключи модулей хранятся зашифрованными на вашем компьютере."],
      ["Это бесплатно?", "Да. Ядро бесплатное, своя модель через Ollama работает без ключей и интернета. Платите только облачному сервису, если выберете его."],
    ],
    ctaTitle: "Соберите свой первый модуль сегодня",
    ctaLead: "Скачайте NOAH, подключите свою нейросеть и скажите, что нужно.",
  },
  en: {
    kicker: "CORE · MULTIMODAL SHELL FOR AI",
    title: ["A core for AI.", "Modules in words.", "No code."],
    lead: "NOAH is a shell on your computer. Plug in any AI — that's the brain. You build the hands yourself: describe a task in plain words, your AI writes a module, NOAH checks it and runs it. Made something useful? Share it in the library or sell it.",
    primary: "Connect your AI",
    download: "Download NOAH",
    meta: "free · Windows 10 / 11 · any AI",
    states: ["WAITING", "LISTENING", "BUILDING", "DONE"],
    demo: [
      { said: "Build a module that tells me the euro rate every morning", tool: "module_format → create_module", answer: "Check ✓ 4 of 4. The «Euro rate» module is running." },
      { said: "Noah, what's the euro rate?", tool: "euro-rate.rate()", answer: "One euro is 1.08 dollars." },
      { said: "Publish this module to the library", tool: "publish_module(\"euro-rate\")", answer: "It's in the NOAH library — anyone can install it." },
      { said: "Build a module that sorts my receipts", tool: "create_module → check ✗ 1 → fixed ✓", answer: "«Receipts» is ready: say «Noah, how much did I spend»." },
    ],
    brainsLabel: "ANY BRAIN",
    howKicker: "HOW IT WORKS",
    howTitle: "From idea to module in minutes",
    how: [
      ["Install the core", "Download NOAH. It lives in the tray, hears your voice and already does dozens of things."],
      ["Connect your AI", "One MCP link — and your Claude, Cursor or other AI gets hands on your computer."],
      ["Describe a module", "«I want it to…» — your AI writes the module, NOAH checks it against the standard and runs it."],
      ["Share or sell", "Publish the module to the library; others install it in one click. Paid modules are coming."],
    ],
    featKicker: "WHAT YOU GET",
    featTitle: "One core — endless hands",
    build: ["Any module, no code", "Bills, receipts, orders, reports, smart home, work — anything you can describe becomes a module."],
    buildPills: ["exchange rates", "receipts", "morning brief", "smart home", "my tasks", "doc search"],
    check: ["Checked before it runs", "A module installs only after it passes the standard: rules, a live start, tests, and resilience to bad calls."],
    report: ["Manifest and files follow the standard", "Server answered in 58 ms", "rate: «1 EUR = 1.08 USD»", "Survived bad calls"],
    multi: ["Multimodal", "Speak, type, message it in Telegram, select text on screen — the core understands and calls the right module."],
    multiPills: ["voice", "text", "Telegram", "selection", "MCP"],
    brains: ["Any brain", "A local model — free and offline, a cloud one — by key. Switch with one phrase: «switch to mistral»."],
    market: ["Library and sales", "Ready modules install in one click. Publish and share your own; paid modules are coming."],
    local: ["Local and private", "Voice, files and keys stay on your computer. Module keys are encrypted; your AI never sees them."],
    libAll: "All modules →",
    whoKicker: "FOR WHOM",
    whoTitle: "For anyone who wants AI to act, not just talk",
    who: [
      ["No coding skills", "Need your own tool? Describe it. Your AI writes it, NOAH checks and runs it."],
      ["Module authors", "Build something useful, publish it to the library for others. Selling is coming."],
      ["Developers", "Any MCP server becomes a voice module. Open standard, automatic checks."],
      ["For yourself", "Tasks, reminders, files, orders, courses — by voice, without an app for each."],
    ],
    codeKicker: "UNDER THE HOOD",
    codeTitle: "A module is an MCP server and one file",
    codeLead: "No custom protocol, no SDK. Your AI gets the standard from NOAH with an example, writes the server and submits it until the check passes.",
    standard: "Module standard",
    tabs: ["module.json", "server.mjs", "check"],
    statsLabel: ["MODULES", "AUTHORS", "BRAINS", "INSTALLS"],
    faqKicker: "QUESTIONS",
    faqTitle: "FAQ",
    faq: [
      ["Do I need to code?", "No. You describe the task, your AI writes the module, and NOAH checks it and sends it back until everything works."],
      ["How do I connect my AI?", "Sign in, open «Connect AI» and copy your MCP link. Paste it into Claude, Cursor or another MCP client — and the same key into NOAH so it picks the modules up."],
      ["Can I sell modules?", "Publishing and sharing work today. Paid modules and author payouts are coming — the author space is already there."],
      ["Is it safe to install other people's modules?", "NOAH runs a module only if it passed the check, and only the exact version that passed. Module keys are stored encrypted on your computer."],
      ["Is it free?", "Yes. The core is free, and a local model through Ollama works without keys or internet. You pay only a cloud service if you choose one."],
    ],
    ctaTitle: "Build your first module today",
    ctaLead: "Download NOAH, connect your AI and say what you need.",
  },
};

// Модели — от самых популярных. Любая работает через Ollama, OpenRouter,
// Groq или любой OpenAI-совместимый сервис.
const BRAINS = [
  "GPT", "Claude", "Gemini", "DeepSeek", "Llama", "Qwen", "Grok", "Mistral", "Gemma", "Kimi",
  "GLM", "Phi", "Mixtral", "Command R", "MiniMax", "Nemotron", "Codestral", "Hermes", "Yi",
  "Granite", "OLMo", "Aya", "Falcon", "StarCoder",
];

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
    h("div", { class: "demo__bar" }, h("span", { class: "demo__dots" }, h("i"), h("i"), h("i")), h("span", { class: "mono" }, "NOAH")),
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
        h("a", { class: "btn btn--gold btn--big", href: "#/connect" }, tr.primary),
        h("a", { class: "btn btn--ghost btn--big", href: RELEASES, target: "_blank", rel: "noopener" }, tr.download),
      ),
      h("p", { class: "hero__meta mono" }, tr.meta),
    ),
    console_,
  );

  const brains = h(
    "section",
    { class: "band" },
    h("span", { class: "label" }, tr.brainsLabel),
    h("div", { class: "band__viewport" }, h("div", { class: "band__track" }, [...BRAINS, ...BRAINS].map((b) => h("span", { class: "band__item" }, b)))),
  );

  const tiles = ["#2B5BC4", "#F2C14E", "#D4564A", "#5F8C4C"];
  const how = h(
    "section",
    { class: "sect" },
    h("span", { class: "kicker" }, tr.howKicker),
    h("h2", { class: "sect__title" }, tr.howTitle),
    h(
      "div",
      { class: "how how--four" },
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

  const cell = (cls, accent, [title, body], ...extra) =>
    h("article", { class: `bento__cell ${cls}`, vars: { "--accent": accent } }, h("h3", {}, title), h("p", {}, body), ...extra);
  const pills = (list, on = 0) => h("div", { class: "pills" }, list.map((p, at) => h("span", { class: `pill${at === on ? " is-on" : ""}` }, p)));

  const topModules = modules.slice(0, 4).map((m) =>
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
      cell("bento__cell--wide", "#F2C14E", tr.build, pills(tr.buildPills), h("a", { class: "btn btn--gold bento__go", href: "#/connect" }, tr.primary)),
      cell("bento__cell--dark", "#5F8C4C", tr.check, h("pre", { class: "bento__report" }, tr.report.map((line) => `✓ ${line}`).join("\n"))),
      cell("", "#2B5BC4", tr.multi, pills(tr.multiPills)),
      cell("", "#D4564A", tr.brains, pills(["qwen 9b", "mistral", "claude", "deepseek"])),
      cell("", "#5F8C4C", tr.local, h("div", { class: "bento__lock" }, icon("memory"))),
      cell("bento__cell--wide", "#2B5BC4", tr.market, h("div", { class: "bento__mods" }, topModules, h("a", { class: "bento__all", href: "#/library" }, tr.libAll))),
    ),
  );

  const whoTiles = ["#F2C14E", "#D4564A", "#2B5BC4", "#5F8C4C"];
  const whoIcons = ["notes", "chart", "code", "home"];
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
          h("span", { class: "tile", vars: { "--accent": whoTiles[at], "--accent-fg": at === 0 ? "#171B26" : "#FFFFFF" } }, icon(whoIcons[at])),
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
      h("a", { class: "btn btn--gold", href: "#/standard" }, tr.standard),
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
    h("div", { class: "plate faq" }, tr.faq.map(([q, a], at) => h("details", { class: "faq__item", open: at === 0 }, h("summary", {}, q), h("p", {}, a)))),
  );

  const finale = h(
    "section",
    { class: "finale" },
    h("h2", {}, tr.ctaTitle),
    h("p", {}, tr.ctaLead),
    h(
      "div",
      { class: "hero__cta" },
      h("a", { class: "btn btn--dark btn--big", href: "#/connect" }, tr.primary),
      h("a", { class: "btn btn--big", href: RELEASES, target: "_blank", rel: "noopener" }, tr.download),
    ),
  );

  page.replaceChildren(hero, brains, how, bento, who, code, strip, faq, finale);
  running = startDemo(console_, tr, h);
  startHud(console_.querySelector(".demo__hud"), running.mode);
}

export function stopHome() {
  running?.stop();
  running = null;
}
