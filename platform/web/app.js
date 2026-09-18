// NOAH — площадка модулей. Одна страница, маршруты в адресе после «#».

import { renderHome, stopHome } from "./home.js?v=12";

const RELEASES = "https://github.com/faafaafuu/asis/releases/latest";
const REPO = "https://github.com/faafaafuu/asis";
const STANDARD_DOC = "https://github.com/faafaafuu/asis/blob/main/src-tauri/src/module_format.md";

const T = {
  en: {
    brainLabel: "BRAIN", brainValue: "any LLM", brainLocal: "local · offline", sellerRole: "seller", contact: "Contact",
    appTitle: "Get NOAH", searchPh: "Search modules, tools, authors…", newModule: "+ NEW MODULE",
    signIn: "Sign in", signUp: "Create account", signOut: "Sign out", myModules: "My modules", apiKeys: "Platform keys",
    privacy: "Privacy", terms: "Terms",
    navDiscover: "DISCOVER", navBuild: "BUILD", navAccount: "EARN",
    navHome: "Home", navLibrary: "Library", navModule: "Module", langLabel: "LANGUAGE", navStudio: "Studio", navStandard: "Standard", navSeller: "Seller",
    menu: ["My modules", "Platform keys", "Account settings"],
    libKicker: "MARKETPLACE", libTitle: "Module library",
    sortPopular: "Popular", sortNew: "New", sortFree: "Free",
    statModules: "MODULES", statAuthors: "AUTHORS", statBrains: "BRAINS SUPPORTED", statInstalls: "INSTALLS",
    cat: { all: "All", free: "Free", work: "Work", home: "Home", finance: "Finance", dev: "Dev tools", health: "Health", local: "Local-only", media: "Media", other: "Other" },
    free: "free", install: "Install", installsWord: "installs", noModules: "Nothing found. Try another word or category — or build the module yourself in the Studio.",
    backLib: "BACK TO LIBRARY", core: "noah-core",
    installBtn: "INSTALL", copyAsk: "COPY REQUEST",
    toolsExposed: "TOOLS EXPOSED", noTools: "Tools are listed after the module is published from NOAH.",
    metaBrain: "BRAIN", brainAny: "any", metaTransport: "TRANSPORT", metaInstalls: "INSTALLS", metaUpdated: "UPDATED",
    permsLabel: "PERMISSIONS ASKED", permKeys: (list) => `Your keys: ${list}. You enter them in NOAH; the module never shows them to anyone.`,
    permNone: "No keys or accounts needed.", permLocal: "Runs on your computer; NOAH checks it before the first start.",
    installHow: "How to install",
    installSteps: (id, title) => [
      ["Get NOAH", "Download the app and start it — it lives in the tray."],
      ["Open the library", `NOAH → Modules → Your module → Library → «${title}».`],
      ["Or ask your AI", `With NOAH connected over MCP, say: «Install the NOAH module ${id}». NOAH checks it and starts it.`],
    ],
    studioKicker: "NO CODE", studioTitle: "Module studio",
    stepDescribe: "DESCRIBE IT", stepAssemble: "NOAH ASSEMBLES", stepManifest: "MANIFEST",
    studioPh: "I want it to check my three bank accounts every morning and tell me what changed…",
    ideas: ["watch my bank", "sort receipts", "daily standup"],
    assembleBtn: "ASSEMBLE MODULE",
    build: ["Task described", "Request for your AI is ready", "Your AI writes the module", "NOAH checks and starts it"],
    states: { done: "DONE", running: "RUNNING", waiting: "WAITING" },
    connectHint: "Connect NOAH to your AI once, then paste the copied request into it:",
    copied: "Copied",
    promptIntro: "Build me a NOAH module through the noa MCP tools. Follow the order from the NOAH instructions: module_format, environment, clarify with me, create_module, fix until the check passes. The task:",
    promptEmpty: "Describe the task first.",
    sellerKicker: "SELLER", sellerTitle: "Your modules", payout: "WITHDRAW",
    sellerStats: ["AVAILABLE", "ALL TIME", "SALES", "PUBLISHED"],
    colModule: "MODULE", colPrice: "PRICE", colSales: "SALES", colRevenue: "REVENUE",
    noMine: "You haven't published anything yet. Build a module in NOAH, then ask your AI: «Publish module <id> to NOAH».",
    remove: "Delete", removeConfirm: "Delete the module from the library? Installed copies keep working.",
    keysTitle: "PLATFORM KEYS", keysLead: "A key links NOAH on your computer to this account: with it NOAH publishes your modules. Paste it into NOAH → Settings → Platform.",
    keyLabelPh: "Key name, e.g. home PC", createKey: "Create key",
    keyOnce: "Copy it now — it is shown only once:", noKeys: "No keys yet.", used: "used", never: "never used",
    publishHow: "HOW TO PUBLISH",
    publishSteps: [
      ["Create a key", "Right here, then paste it into NOAH → Settings → Platform."],
      ["Check the module", "The module must pass NOAH's check on your computer."],
      ["Publish", "Ask your AI: «Publish module <id> to NOAH». It shows up in the library right away."],
    ],
    needLogin: "Sign in to see your modules and keys.",
    stdKicker: "SPEC", stdTitle: "The module standard",
    stdBody: "Every module in the library is an MCP server plus one manifest file. Five rules, nothing else. Pass them and your module installs in one click on any NOAH — on a laptop with a free local model or on a workstation driving a frontier model.",
    rules: [
      ["One manifest", "module.json at the root: name, version, tools, permissions, price. Nothing hidden outside it."],
      ["Speak MCP", "Tools are exposed over MCP (stdio or http). No custom protocol, no NOAH-specific SDK."],
      ["Declare the brain", "State the smallest model the module works with. If it needs reasoning, say so — the shell warns before install."],
      ["Ask, don't take", "Every permission is listed up front and granted by the user. Anything destructive asks again at runtime."],
      ["Publish like npm", "One command publishes. No review queue — complaints are handled after the fact, and a bad module is pulled."],
    ],
    stdFull: "Full standard",
    loginTitle: "Sign in", signupTitle: "Create account",
    email: "Email", password: "Password", authorName: "Author name",
    authorHint: "3–24 characters: latin letters, digits, dot, dash. Shown on your modules.",
    passHint: "At least 10 characters.",
    noAccount: "No account yet?", haveAccount: "Already have one?",
    footer: [
      ["PLATFORM", [["Home", "#/"], ["Library", "#/library"], ["Studio", "#/studio"], ["Seller dashboard", "#/seller"]]],
      ["BUILD", [["Module standard", "#/standard"], ["Full spec", STANDARD_DOC], ["MCP guide", REPO + "/blob/main/modules/README.md"], ["Publish a module", "#/seller"]]],
      ["NOAH", [["About", "#/"], ["Download", RELEASES], ["Source code", REPO], ["Contact", REPO + "/issues"]]],
      ["LEGAL", [["Privacy policy", "#/privacy"], ["Terms of service", "#/terms"]]],
    ],
    justNow: "just now", error: "Something went wrong.",
    accKicker: "ACCOUNT", accTitle: "Account settings",
    passTitle: "CHANGE PASSWORD", passCurrent: "Current password", passNew: "New password", passSave: "Change password", passDone: "Password changed.",
    sessTitle: "SESSIONS", sessLead: "Signed in on another computer and want to end it? Sign out everywhere — this browser stays signed in.", sessBtn: "Sign out everywhere else", sessDone: "Other sessions ended.",
    delTitle: "DELETE ACCOUNT", delLead: "Your modules are removed from the library, keys stop working. This can't be undone.", delBtn: "Delete account", delConfirm: "Delete the account for good?",
    payoutSoon: "Paid modules and payouts are coming later.",
  },
  ru: {
    brainLabel: "МОЗГ", brainValue: "любая LLM", brainLocal: "локально · офлайн", sellerRole: "продавец", contact: "Связаться",
    appTitle: "Скачать NOAH", searchPh: "Поиск модулей, инструментов, авторов…", newModule: "+ НОВЫЙ МОДУЛЬ",
    signIn: "Войти", signUp: "Создать аккаунт", signOut: "Выйти", myModules: "Мои модули", apiKeys: "Ключи площадки",
    privacy: "Конфиденциальность", terms: "Соглашение",
    navDiscover: "НАЙТИ", navBuild: "СОБРАТЬ", navAccount: "ЗАРАБОТОК",
    navHome: "Главная", navLibrary: "Библиотека", navModule: "Модуль", langLabel: "ЯЗЫК", navStudio: "Студия", navStandard: "Стандарт", navSeller: "Кабинет",
    menu: ["Мои модули", "Ключи площадки", "Настройки аккаунта"],
    libKicker: "МАРКЕТПЛЕЙС", libTitle: "Библиотека модулей",
    sortPopular: "Популярные", sortNew: "Новые", sortFree: "Бесплатные",
    statModules: "МОДУЛЕЙ", statAuthors: "АВТОРОВ", statBrains: "МОЗГОВ", statInstalls: "УСТАНОВОК",
    cat: { all: "Все", free: "Бесплатные", work: "Работа", home: "Дом", finance: "Финансы", dev: "Разработка", health: "Здоровье", local: "Только локально", media: "Медиа", other: "Другое" },
    free: "бесплатно", install: "Поставить", installsWord: "установок", noModules: "Ничего не нашлось. Попробуйте другое слово или раздел — или соберите модуль сами в Студии.",
    backLib: "НАЗАД В БИБЛИОТЕКУ", core: "noah-core",
    installBtn: "ПОСТАВИТЬ", copyAsk: "СКОПИРОВАТЬ ЗАПРОС",
    toolsExposed: "ДОСТУПНЫЕ ИНСТРУМЕНТЫ", noTools: "Инструменты появятся, когда модуль опубликуют из NOAH.",
    metaBrain: "МОЗГ", brainAny: "любой", metaTransport: "ТРАНСПОРТ", metaInstalls: "УСТАНОВОК", metaUpdated: "ОБНОВЛЁН",
    permsLabel: "ЗАПРАШИВАЕТ ДОСТУП", permKeys: (list) => `Ваши ключи: ${list}. Вводятся в NOAH, модуль никому их не показывает.`,
    permNone: "Ключи и аккаунты не нужны.", permLocal: "Работает на вашем компьютере; NOAH проверяет его перед первым запуском.",
    installHow: "Как поставить",
    installSteps: (id, title) => [
      ["Скачайте NOAH", "Установите приложение — оно живёт в трее."],
      ["Откройте библиотеку", `NOAH → Модули → Свой модуль → Библиотека → «${title}».`],
      ["Или попросите нейросеть", `Если NOAH подключена к ней по MCP, скажите: «Поставь модуль NOAH ${id}». NOAH проверит и запустит его.`],
    ],
    studioKicker: "БЕЗ КОДА", studioTitle: "Студия модулей",
    stepDescribe: "ОПИШИТЕ", stepAssemble: "NOAH СОБИРАЕТ", stepManifest: "МАНИФЕСТ",
    studioPh: "Хочу, чтобы каждое утро проверял три моих счёта и говорил, что изменилось…",
    ideas: ["следить за счётом", "разобрать чеки", "утренний стендап"],
    assembleBtn: "СОБРАТЬ МОДУЛЬ",
    build: ["Задача описана", "Запрос для нейросети готов", "Нейросеть пишет модуль", "NOAH проверяет и запускает"],
    states: { done: "ГОТОВО", running: "ИДЁТ", waiting: "ЖДЁТ" },
    connectHint: "Один раз подключите NOAH к своей нейросети, затем вставьте в неё скопированный запрос:",
    copied: "Скопировано",
    promptIntro: "Собери мне модуль NOAH через инструменты MCP noa. Соблюдай порядок из инструкций NOAH: module_format, environment, уточни у меня детали, create_module, исправляй, пока проверка не пройдёт. Задача:",
    promptEmpty: "Сначала опишите задачу.",
    sellerKicker: "ПРОДАВЕЦ", sellerTitle: "Ваши модули", payout: "ВЫВЕСТИ",
    sellerStats: ["К ВЫВОДУ", "ВСЕГО", "ПРОДАЖ", "ОПУБЛИКОВАНО"],
    colModule: "МОДУЛЬ", colPrice: "ЦЕНА", colSales: "ПРОДАЖ", colRevenue: "ВЫРУЧКА",
    noMine: "Вы ещё ничего не опубликовали. Соберите модуль в NOAH и попросите нейросеть: «Опубликуй модуль <id> в NOAH».",
    remove: "Удалить", removeConfirm: "Убрать модуль из библиотеки? Уже поставленные копии продолжат работать.",
    keysTitle: "КЛЮЧИ ПЛОЩАДКИ", keysLead: "Ключ связывает NOAH на вашем компьютере с этим аккаунтом: по нему NOAH публикует ваши модули. Вставьте его в NOAH → Настройки → Площадка.",
    keyLabelPh: "Название ключа, например «домашний ПК»", createKey: "Создать ключ",
    keyOnce: "Скопируйте сейчас — ключ показывается один раз:", noKeys: "Ключей пока нет.", used: "использован", never: "не использовался",
    publishHow: "КАК ОПУБЛИКОВАТЬ",
    publishSteps: [
      ["Создайте ключ", "Здесь же, и вставьте его в NOAH → Настройки → Площадка."],
      ["Проверьте модуль", "Модуль должен пройти проверку NOAH на вашем компьютере."],
      ["Опубликуйте", "Попросите нейросеть: «Опубликуй модуль <id> в NOAH». Он сразу появится в библиотеке."],
    ],
    needLogin: "Войдите, чтобы увидеть свои модули и ключи.",
    stdKicker: "СТАНДАРТ", stdTitle: "Стандарт модуля",
    stdBody: "Каждый модуль в библиотеке — это MCP-сервер плюс один файл-манифест. Пять правил, больше ничего. Соблюдены — модуль ставится одной кнопкой в любой NOAH: на ноутбуке с бесплатной локальной моделью или на рабочей станции с топовой облачной.",
    rules: [
      ["Один манифест", "module.json в корне: имя, версия, инструменты, доступы, цена. Ничего спрятанного за его пределами."],
      ["Говорить на MCP", "Инструменты отдаются по MCP (stdio или http). Никакого своего протокола и SDK под NOAH."],
      ["Объявить мозг", "Укажите минимальную модель, на которой модуль работает. Нужны рассуждения — так и скажите, оболочка предупредит."],
      ["Просить, а не брать", "Все доступы перечислены заранее и даются пользователем. Всё необратимое спрашивает повторно при запуске."],
      ["Публикация как в npm", "Одна команда — и модуль в библиотеке. Без очереди на ревью: жалобы разбираются постфактум, плохой модуль снимают."],
    ],
    stdFull: "Полный стандарт",
    loginTitle: "Вход", signupTitle: "Новый аккаунт",
    email: "Почта", password: "Пароль", authorName: "Имя автора",
    authorHint: "3–24 знака: латиница, цифры, точка, дефис. Показывается на ваших модулях.",
    passHint: "Не короче 10 знаков.",
    noAccount: "Нет аккаунта?", haveAccount: "Уже есть аккаунт?",
    footer: [
      ["ПЛАТФОРМА", [["Главная", "#/"], ["Библиотека", "#/library"], ["Студия", "#/studio"], ["Кабинет продавца", "#/seller"]]],
      ["РАЗРАБОТКА", [["Стандарт модуля", "#/standard"], ["Полный регламент", STANDARD_DOC], ["Гид по MCP", REPO + "/blob/main/modules/README.md"], ["Опубликовать модуль", "#/seller"]]],
      ["NOAH", [["О проекте", "#/"], ["Скачать", RELEASES], ["Исходный код", REPO], ["Связаться", REPO + "/issues"]]],
      ["ПРАВОВОЕ", [["Политика конфиденциальности", "#/privacy"], ["Пользовательское соглашение", "#/terms"]]],
    ],
    justNow: "только что", error: "Что-то пошло не так.",
    accKicker: "АККАУНТ", accTitle: "Настройки аккаунта",
    passTitle: "СМЕНА ПАРОЛЯ", passCurrent: "Текущий пароль", passNew: "Новый пароль", passSave: "Сменить пароль", passDone: "Пароль изменён.",
    sessTitle: "СЕАНСЫ", sessLead: "Входили на другом компьютере и хотите закончить? Выйдите везде — этот браузер останется в аккаунте.", sessBtn: "Выйти на других устройствах", sessDone: "Остальные сеансы закрыты.",
    delTitle: "УДАЛИТЬ АККАУНТ", delLead: "Ваши модули уйдут из библиотеки, ключи перестанут работать. Отменить нельзя.", delBtn: "Удалить аккаунт", delConfirm: "Удалить аккаунт насовсем?",
    payoutSoon: "Платные модули и выплаты появятся позже.",
  },
};

const DOCS = {
  privacy: {
    ru: ["Политика конфиденциальности", [
      ["Что мы храним", "Почту, имя автора и хеш пароля — чтобы вы могли входить. Опубликованные вами модули и число их установок. Хеши ключей площадки и дату их последнего использования."],
      ["Чего мы не храним", "Пароли в открытом виде, ключи модулей и всё, что вы говорите NOAH: голос, вопросы и ответы остаются на вашем компьютере и у выбранной вами нейросети."],
      ["Куки", "Одна служебная кука сессии, чтобы вы оставались в аккаунте. Без рекламы и сторонней аналитики."],
      ["Удаление", "Модули удаляются в кабинете автора. Чтобы удалить аккаунт целиком, напишите нам через страницу проекта на GitHub."],
    ]],
    en: ["Privacy policy", [
      ["What we store", "Your email, author name and a password hash so you can sign in. The modules you publish and their install counts. Hashes of your platform keys and when they were last used."],
      ["What we don't", "Plain passwords, module keys, and anything you say to NOAH: voice, questions and answers stay on your computer and with the AI you chose."],
      ["Cookies", "One session cookie to keep you signed in. No ads, no third-party analytics."],
      ["Deletion", "Delete modules in the author space. To delete the whole account, contact us through the project page on GitHub."],
    ]],
  },
  terms: {
    ru: ["Пользовательское соглашение", [
      ["Площадка", "NOAH — библиотека модулей для приложения NOAH. Модули публикуют их авторы; площадка их не пишет и не запускает."],
      ["Авторам", "Публикуйте только то, на что у вас есть права. Модуль не должен собирать данные без ведома пользователя, обходить проверку NOAH или вредить компьютеру. Такие модули снимаются."],
      ["Пользователям", "NOAH проверяет модуль перед запуском, но не может гарантировать его поведение во всём. Ставьте модули авторов, которым доверяете, и читайте, какие ключи они просят."],
      ["Ответственность", "Площадка предоставляется «как есть». Мы исправляем ошибки и снимаем вредные модули, как только о них узнаём."],
    ]],
    en: ["Terms of service", [
      ["The platform", "NOAH is a library of modules for the NOAH app. Modules are published by their authors; the platform does not write or run them."],
      ["Authors", "Publish only what you have rights to. A module must not collect data behind the user's back, bypass NOAH's check or harm the computer. Such modules are removed."],
      ["Users", "NOAH checks a module before it runs but cannot guarantee everything it does. Install modules from authors you trust and read which keys they ask for."],
      ["Liability", "The platform is provided as is. We fix bugs and remove harmful modules as soon as we learn about them."],
    ]],
  },
};

const ACCENTS = [
  { accent: "#2B5BC4", fg: "#FFFFFF" },
  { accent: "#F2C14E", fg: "#171B26" },
  { accent: "#D4564A", fg: "#FFFFFF" },
  { accent: "#5F8C4C", fg: "#FFFFFF" },
];
const ICONS = { memory: "memory", files: "files", fetch: "browser", browser: "browser", docs: "notes", thinking: "code" };
const NAV_ICON = { home: "home", library: "notes", module: "browser", studio: "code", standard: "legal", seller: "chart" };
const CATEGORY_ICON = { work: "notes", home: "home", finance: "chart", dev: "code", health: "memory", media: "browser", other: "files" };

/* ── Состояние ───────────────────────────────────────────────────────────── */

const state = {
  lang: pickLang(),
  user: null,
  sort: "popular",
  category: "all",
  query: "",
  studioText: "",
  modules: [],
  stats: null,
};

function pickLang() {
  try {
    const saved = localStorage.getItem("noah.lang");
    if (saved === "en" || saved === "ru") return saved;
  } catch {
    /* выбор не сохранится */
  }
  return (navigator.language || "en").toLowerCase().startsWith("ru") ? "ru" : "en";
}

const t = () => T[state.lang];
const $ = (name) => document.querySelector(`[data-el="${name}"]`);

/* ── Помощники ───────────────────────────────────────────────────────────── */

function h(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value == null || value === false) continue;
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key === "vars") for (const [name, v] of Object.entries(value)) node.style.setProperty(name, v);
    else if (key.startsWith("on")) node.addEventListener(key.slice(2).toLowerCase(), value);
    else node.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", `#ic-${name}`);
  svg.append(use);
  return svg;
}

function paletteFor(id) {
  let sum = 0;
  for (const ch of id) sum = (sum * 31 + ch.charCodeAt(0)) >>> 0;
  return ACCENTS[sum % ACCENTS.length];
}

function iconFor(module) {
  return ICONS[module.id] ?? CATEGORY_ICON[module.category] ?? "files";
}

function tile(module, size = "") {
  const { accent, fg } = paletteFor(module.id);
  return h("span", { class: `tile ${size}`, vars: { "--accent": accent, "--accent-fg": fg } }, icon(iconFor(module)));
}

const number = (n) => new Intl.NumberFormat(state.lang === "ru" ? "ru-RU" : "en-US", { notation: n >= 10000 ? "compact" : "standard" }).format(n);

function when(iso) {
  if (!iso) return "—";
  const date = new Date(`${iso.replace(" ", "T")}Z`);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString(state.lang === "ru" ? "ru-RU" : "en-US", { day: "numeric", month: "short", year: "numeric" });
}

function toast(text) {
  const node = $("toast");
  node.textContent = text;
  node.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (node.hidden = true), 3200);
}

async function api(path, { method = "GET", body } = {}) {
  const response = await fetch(path, {
    method,
    credentials: "same-origin",
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || t().error);
  return data;
}

async function copy(text, button, label) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = h("textarea", {}, text);
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  if (button) {
    button.textContent = t().copied;
    setTimeout(() => (button.textContent = label), 1600);
  }
}

/** Подсветка JSON: ключи, строки, числа, знаки. */
function highlight(json) {
  const pre = h("pre");
  const re = /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?)|([{}[\],:])|(\s+)|(true|false|null)/g;
  let match;
  while ((match = re.exec(json))) {
    const [all, str, colon, num, pun, space, word] = match;
    if (str) {
      pre.append(h("span", { class: colon ? "tk-key" : "tk-str" }, str));
      if (colon) pre.append(h("span", { class: "tk-pun" }, colon));
    } else if (num) pre.append(h("span", { class: "tk-num" }, num));
    else if (pun) pre.append(h("span", { class: "tk-pun" }, pun));
    else if (word) pre.append(h("span", { class: "tk-num" }, word));
    else pre.append(space ?? all);
  }
  return pre;
}

function codeBlock(name, text, { highlightJson = false } = {}) {
  const copyLabel = state.lang === "ru" ? "КОПИЯ" : "COPY";
  const button = h("button", { type: "button", class: "code__copy", onclick: () => copy(text, button, copyLabel) }, copyLabel);
  return h(
    "div",
    { class: "code" },
    h("div", { class: "code__head" }, h("span", { class: "label" }, name), button),
    highlightJson ? highlight(text) : h("pre", {}, text),
  );
}

function pageHead(kicker, title, extra) {
  return h("div", { class: "head" }, h("div", {}, h("span", { class: "kicker" }, kicker), h("h1", { class: "title" }, title)), extra);
}

/* ── Каркас ──────────────────────────────────────────────────────────────── */

function renderChrome(route) {
  const tr = t();
  document.documentElement.lang = state.lang;
  for (const node of document.querySelectorAll("[data-t]")) node.textContent = tr[node.dataset.t];
  $("search").placeholder = tr.searchPh;
  $("download").href = RELEASES;
  for (const button of document.querySelectorAll("[data-lang]")) {
    button.setAttribute("aria-pressed", String(button.dataset.lang === state.lang));
  }
  $("langBtn").textContent = `${state.lang.toUpperCase()} ▾`;
  $("signInIcon").setAttribute("aria-label", tr.signIn);
  $("signInIcon").title = tr.signIn;

  const count = state.stats ? String(state.stats.modules) : "";
  const moduleLink = state.lastModule ? `module/${state.lastModule}` : "library";
  const groups = [
    [tr.navDiscover, [["home", tr.navHome, "", "#F2C14E", ""], ["library", tr.navLibrary, count, "#2B5BC4"], ["module", tr.navModule, "", "#D4564A", moduleLink]]],
    [tr.navBuild, [["studio", tr.navStudio, "", "#F2C14E"], ["standard", tr.navStandard, "5", "#7FB069"]]],
    [tr.navAccount, [["seller", tr.navSeller, "$0", "#D4564A"]]],
  ];
  $("nav").replaceChildren(
    ...groups.map(([label, items]) =>
      h(
        "div",
        { class: "nav__group" },
        h("span", { class: "nav__label" }, label),
        items.map(([id, text, badge, dot, target]) =>
          h(
            "a",
            { class: "nav__item", "data-id": id, href: `#/${target ?? id}`, "aria-current": route === id ? "page" : null, vars: { "--dot": dot } },
            h("span", { class: "nav__dot" }),
            h("span", { class: "navicon" }, icon(NAV_ICON[id])),
            text,
            h("span", { class: "nav__count" }, badge),
          ),
        ),
      ),
    ),
  );

  const user = state.user;
  $("guest").hidden = Boolean(user);
  $("meBtn").hidden = !user;
  $("sideMe").hidden = !user;
  $("sideGuest").hidden = Boolean(user);
  if (!user) $("menu").hidden = true;
  if (user) {
    for (const el of ["meAvatar", "sideAvatar"]) $(el).textContent = user.name.slice(0, 1);
    for (const el of ["meName", "sideName", "menuName"]) $(el).textContent = user.name;
    $("menuEmail").textContent = user.email;
    const hints = [String(state.myCount ?? ""), "", ""];
    const targets = ["#/seller", "#/seller/keys", "#/account"];
    $("menuItems").replaceChildren(
      ...tr.menu.map((label, at) => h("a", { class: "menu__item", href: targets[at] }, label, h("span", { class: "mono" }, hints[at]))),
    );
  }

  $("footCols").replaceChildren(
    ...tr.footer.map(([label, links]) =>
      h(
        "div",
        { class: "foot__col" },
        h("span", { class: "label" }, label),
        links.map(([text, href]) =>
          h("a", { href, target: href.startsWith("http") ? "_blank" : null, rel: href.startsWith("http") ? "noopener" : null }, text),
        ),
      ),
    ),
  );
}

/* ── Библиотека ──────────────────────────────────────────────────────────── */

function moduleCard(module) {
  const tr = t();
  const { accent } = paletteFor(module.id);
  return h(
    "a",
    { class: "card", href: `#/module/${module.id}`, vars: { "--accent": accent } },
    h(
      "div",
      { class: "card__head" },
      tile(module),
      h("span", { class: "card__name" }, h("span", { class: "card__title" }, module.title), h("span", { class: "card__author" }, module.core ? tr.core : module.author)),
      h("span", { class: "price" }, tr.free),
    ),
    h("span", { class: "card__about" }, module.about),
    h("div", { class: "card__foot" }, h("span", {}, `${number(module.installs)} ${tr.installsWord}`), h("strong", {}, `${tr.install} →`)),
  );
}

async function loadLibrary() {
  const params = new URLSearchParams({ sort: state.sort, category: state.category, q: state.query });
  const [list, stats] = await Promise.all([api(`/api/modules?${params}`), api("/api/stats")]);
  state.modules = list.modules;
  state.categories = list.categories;
  state.stats = stats;
}

function renderLibrary(page) {
  const tr = t();
  const sortTabs = h(
    "div",
    { class: "tabs" },
    [["popular", tr.sortPopular], ["new", tr.sortNew], ["free", tr.sortFree]].map(([id, label]) =>
      h("button", { type: "button", "aria-pressed": String(state.sort === id), onclick: () => ((state.sort = id), route()) }, label),
    ),
  );
  const s = state.stats ?? { modules: 0, authors: 0, installs: 0, brains: 0 };
  const stats = h(
    "div",
    { class: "strip" },
    [
      [s.modules, tr.statModules, "#E9EEF9"],
      [s.authors, tr.statAuthors, "#FBF2DC"],
      [s.brains, tr.statBrains, "#F8E9E6"],
      [s.installs, tr.statInstalls, "#EAF0E6"],
    ].map(([value, label, bg]) =>
      h("div", { class: "stat", vars: { "--bg": bg } }, h("div", { class: "stat__value" }, number(value)), h("div", { class: "stat__label" }, label)),
    ),
  );
  const chips = h(
    "div",
    { class: "chips" },
    ["all", "free", "work", "home", "finance", "dev", "health", "local"].map((id) =>
      h("button", { type: "button", class: "chip", "aria-pressed": String(state.category === id), onclick: () => ((state.category = id), route()) }, tr.cat[id] ?? id),
    ),
  );
  const grid = state.modules.length
    ? h("div", { class: "grid" }, state.modules.map(moduleCard))
    : h("p", { class: "empty" }, tr.noModules);
  const add = h("a", { class: "addm", href: "#/studio", title: tr.newModule, "aria-label": tr.newModule }, "+");
  page.replaceChildren(pageHead(tr.libKicker, tr.libTitle, h("div", { class: "head__tools" }, sortTabs, add)), stats, chips, grid);
}

/* ── Страница модуля ─────────────────────────────────────────────────────── */

function renderModule(page, module) {
  const tr = t();
  const { accent } = paletteFor(module.id);
  const ask = state.lang === "ru" ? `Поставь модуль NOAH ${module.id}` : `Install the NOAH module ${module.id}`;
  const askButton = h("button", { type: "button", class: "btn", onclick: () => copy(ask, askButton, tr.copyAsk) }, tr.copyAsk);
  const howTo = h(
    "div",
    { class: "plate", hidden: true },
    h("div", { class: "step__head" }, tr.installHow),
    tr.installSteps(module.id, module.title).map(([title, body], at) =>
      h("div", { class: "step__row" }, h("span", { class: "num", vars: { "--accent": "#F2C14E" } }, at + 1), h("div", {}, title, h("p", {}, body))),
    ),
    h("div", { class: "step__body" }, h("a", { class: "btn btn--gold", href: RELEASES, target: "_blank", rel: "noopener" }, tr.appTitle)),
  );

  const perms = [];
  if (module.secrets?.length) perms.push(tr.permKeys(module.secrets.map((s) => s.title).join(", ")));
  else perms.push(tr.permNone);
  perms.push(tr.permLocal);

  page.replaceChildren(
    h("a", { class: "back", href: "#/library" }, `← ${tr.backLib}`),
    h(
      "div",
      { class: "plate plate--accent", vars: { "--accent": accent } },
      h(
        "div",
        { class: "plate__head" },
        tile(module, "tile--big"),
        h(
          "div",
          { class: "plate__name" },
          h("h1", {}, module.title),
          h("div", { class: "plate__meta" }, `${module.core ? tr.core : module.author} · v${module.version} · MCP`),
        ),
        h(
          "div",
          { class: "plate__buy" },
          h("span", { class: "big-price" }, tr.free),
          h("button", { type: "button", class: "btn btn--gold", onclick: () => (howTo.hidden = !howTo.hidden) }, tr.installBtn),
          askButton,
        ),
      ),
      h(
        "div",
        { class: "cols" },
        h(
          "div",
          { class: "col" },
          h("p", { class: "body" }, module.description || `${module.about}. ${module.voice}`),
          h(
            "div",
            {},
            h("span", { class: "label" }, tr.toolsExposed),
            module.tools?.length
              ? h("div", { class: "tools" }, module.tools.map((tool) => h("div", { class: "tool" }, h("code", {}, tool.name), h("span", {}, tool.about))))
              : h("p", { class: "hint" }, tr.noTools),
          ),
        ),
        h(
          "div",
          { class: "col col--flush" },
          [
            [tr.metaBrain, module.brain || tr.brainAny],
            [tr.metaTransport, "MCP / stdio"],
            [tr.metaInstalls, number(module.installs)],
            [tr.metaUpdated, when(module.updated)],
          ].map(([k, v]) => h("div", { class: "kv" }, h("span", {}, k), h("span", {}, v))),
          h("div", { class: "note" }, h("span", { class: "label" }, tr.permsLabel), perms.map((p) => h("p", {}, p))),
        ),
      ),
    ),
    howTo,
  );
}

/* ── Студия ──────────────────────────────────────────────────────────────── */

const EXAMPLE_MANIFEST = JSON.stringify(
  {
    id: "weather",
    title: "Погода",
    icon: "☀",
    about: "Погода сейчас в любом городе",
    voice: "«Ноа, какая погода в Казани»",
    version: "1.0.0",
    mcp: { command: "node", args: ["%MODULE_DIR%\\server.mjs"] },
    secrets: [],
    tests: [{ tool: "weather", args: { city: "Казань" }, expect: "Казань" }],
  },
  null,
  2,
);

function renderStudio(page) {
  const tr = t();
  const area = h("textarea", { rows: "4", placeholder: tr.studioPh });
  area.value = state.studioText;
  const ideas = h(
    "div",
    { class: "chips" },
    tr.ideas.map((idea) => h("button", { type: "button", class: "chip chip--quiet", onclick: () => ((area.value = state.studioText = idea), paint()) }, idea)),
  );
  const rows = h("div");
  let copiedOnce = false;
  const paint = () => {
    const described = area.value.trim().length > 0;
    const states = [
      described ? "done" : "waiting",
      copiedOnce ? "done" : described ? "running" : "waiting",
      copiedOnce ? "running" : "waiting",
      "waiting",
    ];
    const tiles = { done: "#7FB069", running: "#F2C14E", waiting: "#E7E4DD" };
    const colors = { done: "#4E7A3C", running: "#8A6614", waiting: "#6E6A61" };
    rows.replaceChildren(
      ...tr.build.map((label, at) =>
        h(
          "div",
          { class: "step__row step__row--center" },
          h("span", { class: "num", vars: { "--accent": tiles[states[at]] } }, at + 1),
          h("span", {}, label),
          h("span", { class: "state", vars: { "--state": colors[states[at]] } }, tr.states[states[at]]),
        ),
      ),
    );
  };
  area.addEventListener("input", () => {
    state.studioText = area.value;
    paint();
  });
  const assemble = h("button", {
    type: "button",
    class: "btn btn--gold btn--wide",
    onclick: async () => {
      const task = area.value.trim();
      if (!task) {
        toast(tr.promptEmpty);
        area.focus();
        return;
      }
      await copy(`${tr.promptIntro}\n${task}`, assemble, tr.assembleBtn);
      copiedOnce = true;
      paint();
      toast(tr.copied);
    },
  }, tr.assembleBtn);
  paint();

  page.replaceChildren(
    pageHead(tr.studioKicker, tr.studioTitle),
    h(
      "div",
      { class: "steps" },
      h("div", { class: "plate" }, h("div", { class: "step__head" }, `01 · ${tr.stepDescribe}`), h("div", { class: "step__body" }, area, ideas)),
      h(
        "div",
        { class: "plate" },
        h("div", { class: "step__head" }, `02 · ${tr.stepAssemble}`),
        rows,
        h(
          "div",
          { class: "step__body" },
          assemble,
          h("p", { class: "hint" }, tr.connectHint),
          codeBlock("terminal", 'claude mcp add noa -- "C:\\path\\to\\sufler.exe" --mcp'),
        ),
      ),
      codeBlock(`03 · ${tr.stepManifest}`, EXAMPLE_MANIFEST, { highlightJson: true }),
    ),
  );
}

/* ── Кабинет автора ──────────────────────────────────────────────────────── */

async function renderSeller(page, focusKeys = false) {
  const tr = t();
  if (!state.user) {
    page.replaceChildren(
      pageHead(tr.sellerKicker, tr.sellerTitle),
      h("div", { class: "empty" }, tr.needLogin, " ", h("a", { href: "#/login" }, tr.signIn), " · ", h("a", { href: "#/signup" }, tr.signUp)),
    );
    return;
  }
  const { modules } = await api("/api/my/modules");
  state.myCount = modules.length;
  const installs = modules.reduce((sum, m) => sum + m.installs, 0);

  const stats = h(
    "div",
    { class: "strip" },
    [
      ["$0", tr.sellerStats[0], "#EAF0E6", "#3F6B30"],
      ["$0", tr.sellerStats[1], "#E9EEF9"],
      [installs, tr.sellerStats[2], "#FBF2DC"],
      [modules.length, tr.sellerStats[3], "#F8E9E6"],
    ].map(([value, label, bg, fg]) =>
      h("div", { class: "stat", vars: { "--bg": bg, "--fg": fg ?? "#171B26" } }, h("div", { class: "stat__value" }, typeof value === "number" ? number(value) : value), h("div", { class: "stat__label" }, label)),
    ),
  );

  const table = h(
    "div",
    { class: "plate" },
    h("div", { class: "table__row table__row--head" }, h("span", {}, tr.colModule), h("span", {}, tr.colPrice), h("span", {}, tr.colSales), h("span", {}, tr.colRevenue), h("span", {})),
    modules.length
      ? modules.map((m) =>
          h(
            "div",
            { class: "table__row" },
            h("a", { class: "table__name", href: `#/module/${m.id}` }, tile(m, "tile--small"), h("span", {}, m.title)),
            h("span", { class: "mono cell" }, h("span", { class: "cellk" }, tr.colPrice), tr.free),
            h("span", { class: "mono cell" }, h("span", { class: "cellk" }, tr.colSales), number(m.installs)),
            h("span", { class: "revenue cell" }, h("span", { class: "cellk" }, tr.colRevenue), "$0"),
            h("button", {
              type: "button",
              class: "btn btn--small btn--danger",
              onclick: async () => {
                if (!confirm(tr.removeConfirm)) return;
                await api(`/api/my/modules/${m.id}`, { method: "DELETE" }).catch((err) => toast(err.message));
                route();
              },
            }, tr.remove),
          ),
        )
      : h("div", { class: "step__body" }, h("p", { class: "hint" }, tr.noMine)),
  );

  const label = h("input", { type: "text", maxlength: "40", placeholder: tr.keyLabelPh });
  const fresh = h("div", { class: "secret-once", hidden: true });
  const keys = h(
    "div",
    { class: "plate", id: "keys" },
    h("div", { class: "step__head" }, tr.keysTitle),
    h(
      "div",
      { class: "step__body" },
      h("p", { class: "hint" }, tr.keysLead),
      h(
        "form",
        {
          class: "field",
          onsubmit: async (event) => {
            event.preventDefault();
            try {
              const { token } = await api("/api/tokens", { method: "POST", body: { label: label.value } });
              const copyButton = h("button", { type: "button", class: "btn btn--small", onclick: () => copy(token, copyButton, "COPY") }, "COPY");
              fresh.replaceChildren(h("span", { class: "hint" }, tr.keyOnce), h("code", {}, token), copyButton);
              fresh.hidden = false;
              label.value = "";
              list.replaceChildren(...(await keyRows()));
            } catch (err) {
              toast(err.message);
            }
          },
        },
        label,
        h("button", { type: "submit", class: "btn btn--gold" }, tr.createKey),
      ),
    ),
    fresh,
  );
  const keyRows = async () => {
    const { tokens: current } = await api("/api/tokens");
    if (!current.length) return [h("div", { class: "step__row" }, h("span", { class: "hint" }, tr.noKeys))];
    return current.map((key) =>
      h(
        "div",
        { class: "step__row" },
        h("div", { class: "grow" }, key.label, h("p", {}, `${when(key.created)} · ${key.used ? `${tr.used} ${when(key.used)}` : tr.never}`)),
        h("button", {
          type: "button",
          class: "btn btn--small btn--danger",
          onclick: async () => {
            await api(`/api/tokens/${key.id}`, { method: "DELETE" });
            list.replaceChildren(...(await keyRows()));
          },
        }, tr.remove),
      ),
    );
  };
  const list = h("div", {}, ...(await keyRows()));
  keys.append(list);

  const how = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.publishHow),
    tr.publishSteps.map(([title, body], at) => h("div", { class: "step__row" }, h("span", { class: "num", vars: { "--accent": "#7FB069" } }, at + 1), h("div", {}, title, h("p", {}, body)))),
  );

  const payout = h("button", { type: "button", class: "btn", disabled: true, title: tr.payoutSoon }, `${tr.payout} $0`);
  page.replaceChildren(
    pageHead(tr.sellerKicker, tr.sellerTitle, payout),
    h("p", { class: "lead" }, tr.payoutSoon),
    stats,
    table,
    h("div", { class: "steps" }, keys, how),
  );
  renderChrome("seller");
  if (focusKeys) keys.scrollIntoView({ behavior: "smooth", block: "start" });
}

/* ── Стандарт ────────────────────────────────────────────────────────────── */

function renderStandard(page) {
  const tr = t();
  const tiles = ["#2B5BC4", "#F2C14E", "#D4564A", "#7FB069", "#2B5BC4"];
  page.replaceChildren(
    pageHead(tr.stdKicker, tr.stdTitle),
    h("p", { class: "lead" }, tr.stdBody),
    h(
      "div",
      { class: "plate doc" },
      tr.rules.map(([title, body], at) =>
        h(
          "div",
          { class: "rule" },
          h("span", { class: "num", vars: { "--accent": tiles[at] } }, at + 1),
          h("div", {}, h("strong", {}, title), h("p", {}, body)),
        ),
      ),
    ),
    h("div", { class: "doc" }, codeBlock("module.json", EXAMPLE_MANIFEST, { highlightJson: true })),
    h("a", { class: "back", href: STANDARD_DOC, target: "_blank", rel: "noopener" }, `${tr.stdFull} →`),
  );
}

/* ── Вход ────────────────────────────────────────────────────────────────── */

function renderAuth(page, mode) {
  const tr = t();
  const signup = mode === "signup";
  const error = h("p", { class: "error", hidden: true });
  const field = (label, input, hint) => h("label", { class: "field" }, h("span", { class: "label" }, label), input, hint ? h("p", { class: "hint" }, hint) : null);
  const email = h("input", { type: "email", autocomplete: "email", required: true });
  const name = h("input", { type: "text", autocomplete: "username", required: true, minlength: "3", maxlength: "24" });
  const password = h("input", { type: "password", autocomplete: signup ? "new-password" : "current-password", required: true, minlength: signup ? "10" : null });
  const submit = h("button", { type: "submit", class: "btn btn--gold btn--wide" }, signup ? tr.signUp : tr.signIn);

  const form = h(
    "form",
    {
      class: "form",
      onsubmit: async (event) => {
        event.preventDefault();
        submit.disabled = true;
        error.hidden = true;
        try {
          const body = signup ? { email: email.value, name: name.value, password: password.value } : { email: email.value, password: password.value };
          const { user } = await api(`/api/auth/${signup ? "register" : "login"}`, { method: "POST", body });
          state.user = user;
          location.hash = "#/seller";
        } catch (err) {
          error.textContent = err.message;
          error.hidden = false;
        } finally {
          submit.disabled = false;
        }
      },
    },
    field(tr.email, email),
    signup ? field(tr.authorName, name, tr.authorHint) : null,
    field(tr.password, password, signup ? tr.passHint : null),
    error,
    submit,
    h(
      "p",
      { class: "hint" },
      signup ? tr.haveAccount : tr.noAccount,
      " ",
      h("a", { href: signup ? "#/login" : "#/signup" }, signup ? tr.signIn : tr.signUp),
    ),
  );
  page.replaceChildren(
    h("div", { class: "auth" }, h("div", { class: "plate plate--accent", vars: { "--accent": "#F2C14E" } }, h("div", { class: "step__head" }, signup ? tr.signupTitle : tr.loginTitle), form)),
  );
  email.focus();
}

async function renderAccount(page) {
  const tr = t();
  if (!state.user) {
    location.hash = "#/login";
    return;
  }
  const field = (label, input) => h("label", { class: "field" }, h("span", { class: "label" }, label), input);
  const note = () => h("p", { class: "hint", role: "status" });

  const current = h("input", { type: "password", autocomplete: "current-password", required: true });
  const next = h("input", { type: "password", autocomplete: "new-password", required: true, minlength: "10" });
  const passNote = note();
  const pass = h(
    "form",
    {
      class: "plate",
      onsubmit: async (event) => {
        event.preventDefault();
        try {
          await api("/api/account/password", { method: "POST", body: { current: current.value, next: next.value } });
          current.value = next.value = "";
          passNote.textContent = tr.passDone;
        } catch (err) {
          passNote.textContent = err.message;
        }
      },
    },
    h("div", { class: "step__head" }, tr.passTitle),
    h("div", { class: "step__body" }, field(tr.passCurrent, current), field(tr.passNew, next), h("p", { class: "hint" }, tr.passHint), h("button", { type: "submit", class: "btn btn--gold" }, tr.passSave), passNote),
  );

  const sessNote = note();
  const sessions = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.sessTitle),
    h(
      "div",
      { class: "step__body" },
      h("p", { class: "hint" }, tr.sessLead),
      h("button", {
        type: "button",
        class: "btn",
        onclick: async () => {
          try {
            await api("/api/account/logout-others", { method: "POST" });
            sessNote.textContent = tr.sessDone;
          } catch (err) {
            sessNote.textContent = err.message;
          }
        },
      }, tr.sessBtn),
      sessNote,
    ),
  );

  const delPass = h("input", { type: "password", autocomplete: "current-password", required: true });
  const delNote = note();
  const remove = h(
    "form",
    {
      class: "plate plate--accent",
      vars: { "--accent": "#D4564A" },
      onsubmit: async (event) => {
        event.preventDefault();
        if (!confirm(tr.delConfirm)) return;
        try {
          await api("/api/account", { method: "DELETE", body: { password: delPass.value } });
          state.user = null;
          location.hash = "#/";
        } catch (err) {
          delNote.textContent = err.message;
        }
      },
    },
    h("div", { class: "step__head" }, tr.delTitle),
    h("div", { class: "step__body" }, h("p", { class: "hint" }, tr.delLead), field(tr.password, delPass), h("button", { type: "submit", class: "btn btn--danger" }, tr.delBtn), delNote),
  );

  page.replaceChildren(
    pageHead(tr.accKicker, tr.accTitle),
    h("div", { class: "plate step__body" }, h("strong", {}, state.user.name), h("span", { class: "mono hint" }, state.user.email)),
    h("div", { class: "steps" }, pass, sessions, remove),
  );
}

function renderDoc(page, name) {
  const [title, sections] = DOCS[name][state.lang];
  page.replaceChildren(
    pageHead("NOAH", title),
    h("div", { class: "plate form doc" }, sections.map(([head, body]) => [h("h2", {}, head), h("p", {}, body)])),
  );
}

/* ── Маршруты ────────────────────────────────────────────────────────────── */

let routeId = 0;
async function route() {
  const id = ++routeId;
  const [view, arg] = location.hash.replace(/^#\/?/, "").split("/");
  const name = view || "home";
  document.body.classList.toggle("is-home", name === "home");
  if (name !== "home") stopHome();
  renderChrome(name);
  const page = $("page");
  try {
    if (name === "home") {
      await loadLibrary();
      if (id === routeId) {
        renderChrome("home");
        renderHome(page, { h, icon, lang: state.lang, stats: state.stats, modules: state.modules, number });
      }
    } else if (name === "library") {
      await loadLibrary();
      if (id === routeId) {
        renderChrome("library");
        renderLibrary(page);
      }
    } else if (name === "module" && arg) {
      const module = await api(`/api/modules/${encodeURIComponent(arg)}`);
      state.lastModule = module.id;
      if (id === routeId) {
        renderChrome("module");
        renderModule(page, module);
      }
    } else if (name === "studio") renderStudio(page);
    else if (name === "seller") await renderSeller(page, arg === "keys");
    else if (name === "account") await renderAccount(page);
    else if (name === "standard") renderStandard(page);
    else if (name === "login" || name === "signup") {
      if (state.user) location.hash = "#/seller";
      else renderAuth(page, name);
    } else if (name === "privacy" || name === "terms") renderDoc(page, name);
    else location.hash = "#/";
  } catch (err) {
    if (id === routeId) page.replaceChildren(h("p", { class: "empty" }, err.message));
  }
  if (id === routeId && !location.hash.startsWith("#/library")) window.scrollTo(0, 0);
}

/* ── События ─────────────────────────────────────────────────────────────── */

for (const button of document.querySelectorAll("[data-lang]")) {
  button.addEventListener("click", () => {
    $("langList").hidden = true;
    $("langBtn").setAttribute("aria-expanded", "false");
    state.lang = button.dataset.lang;
    try {
      localStorage.setItem("noah.lang", state.lang);
    } catch {
      /* выбор не сохранится */
    }
    route();
  });
}

let searchTimer = 0;
$("search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.query = event.target.value.trim();
    if (!location.hash.startsWith("#/library")) location.hash = "#/library";
    else route();
  }, 220);
});

document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    $("search").focus();
  }
});

$("langBtn").addEventListener("click", (event) => {
  event.stopPropagation();
  const list = $("langList");
  list.hidden = !list.hidden;
  $("langBtn").setAttribute("aria-expanded", String(!list.hidden));
});

$("searchBtn").addEventListener("click", () => {
  const box = $("searchBox");
  box.classList.toggle("is-open");
  if (box.classList.contains("is-open")) $("search").focus();
});

$("meBtn").addEventListener("click", (event) => {
  event.stopPropagation();
  $("menu").hidden = !$("menu").hidden;
});
document.addEventListener("click", (event) => {
  if (!$("menu").contains(event.target)) $("menu").hidden = true;
  if (!$("langPick").contains(event.target)) {
    $("langList").hidden = true;
    $("langBtn").setAttribute("aria-expanded", "false");
  }
});
$("signOut").addEventListener("click", async () => {
  await api("/api/auth/logout", { method: "POST" }).catch(() => {});
  state.user = null;
  location.hash = "#/library";
  route();
});

window.addEventListener("hashchange", route);

(async () => {
  try {
    state.user = (await api("/api/me")).user;
  } catch {
    state.user = null;
  }
  route();
})();
