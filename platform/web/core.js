// Общее для всех страниц площадки: состояние, тексты, мелкие помощники.
// Отдельным файлом, чтобы главная и кабинет грузили только нужное: из
// части сетей соединение замирает после ~16 КБ, и каждый файл держится
// меньше этого вместе с TLS-рукопожатием.

import { T } from "./i18n.js?v=43";

/** Функции, которые живут в app.js, а нужны страницам кабинета. */
export const hooks = { route: () => {}, renderChrome: () => {} };

export const ACCENTS = [
  { accent: "#2B5BC4", fg: "#FFFFFF" },
  { accent: "#F2C14E", fg: "#171B26" },
  { accent: "#D4564A", fg: "#FFFFFF" },
  { accent: "#5F8C4C", fg: "#FFFFFF" },
];
export const ICONS = { memory: "memory", files: "files", fetch: "browser", browser: "browser", docs: "notes", thinking: "code" };
export const NAV_ICON = { connect: "memory", home: "home", library: "notes", module: "browser", studio: "code", standard: "legal", seller: "chart", docs: "files" };
export const CATEGORY_ICON = { work: "notes", home: "home", finance: "chart", dev: "code", health: "memory", media: "browser", other: "files" };

/* ── Состояние ───────────────────────────────────────────────────────────── */

export const state = {
  lang: pickLang(),
  user: null,
  sort: "popular",
  category: "all",
  query: "",
  studioText: "",
  modules: [],
  stats: null,
};

export function pickLang() {
  try {
    const saved = localStorage.getItem("noah.lang");
    if (saved === "en" || saved === "ru") return saved;
  } catch {
    /* выбор не сохранится */
  }
  return (navigator.language || "en").toLowerCase().startsWith("ru") ? "ru" : "en";
}

export const t = () => T[state.lang];
export const $ = (name) => document.querySelector(`[data-el="${name}"]`);

/* ── Помощники ───────────────────────────────────────────────────────────── */

export function h(tag, props = {}, ...children) {
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

export function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", `#ic-${name}`);
  svg.append(use);
  return svg;
}

export function paletteFor(id) {
  let sum = 0;
  for (const ch of id) sum = (sum * 31 + ch.charCodeAt(0)) >>> 0;
  return ACCENTS[sum % ACCENTS.length];
}

export function iconFor(module) {
  return ICONS[module.id] ?? CATEGORY_ICON[module.category] ?? "files";
}

export function tile(module, size = "") {
  const { accent, fg } = paletteFor(module.id);
  // У встроенных модулей свой знак (✓, ☑, ✈…) — он узнаваемее значка категории.
  const mark = module.builtin && module.icon ? h("span", { class: "tile__glyph" }, module.icon) : icon(iconFor(module));
  return h("span", { class: `tile ${size}`, vars: { "--accent": accent, "--accent-fg": fg } }, mark);
}

export const number = (n) => new Intl.NumberFormat(state.lang === "ru" ? "ru-RU" : "en-US", { notation: n >= 10000 ? "compact" : "standard" }).format(n);

export function when(iso) {
  if (!iso) return "—";
  const date = new Date(`${iso.replace(" ", "T")}Z`);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString(state.lang === "ru" ? "ru-RU" : "en-US", { day: "numeric", month: "short", year: "numeric" });
}

export function toast(text) {
  const node = $("toast");
  node.textContent = text;
  node.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (node.hidden = true), 3200);
}

export async function api(path, { method = "GET", body } = {}) {
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

export async function copy(text, button, label) {
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
export function highlight(json) {
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

export function codeBlock(name, text, { highlightJson = false, wrap = false } = {}) {
  const copyLabel = state.lang === "ru" ? "КОПИЯ" : "COPY";
  const button = h("button", { type: "button", class: "code__copy", onclick: () => copy(text, button, copyLabel) }, copyLabel);
  return h(
    "div",
    { class: wrap ? "code code--wrap" : "code" },
    h("div", { class: "code__head" }, h("span", { class: "label" }, name), button),
    highlightJson ? highlight(text) : h("pre", {}, text),
  );
}

export function pageHead(kicker, title, extra) {
  return h("div", { class: "head" }, h("div", {}, h("span", { class: "kicker" }, kicker), h("h1", { class: "title" }, title)), extra);
}

/* ── Каркас ──────────────────────────────────────────────────────────────── */

export const EXAMPLE_MANIFEST = JSON.stringify(
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


/* ── Меню «Скачать» ──────────────────────────────────────────────────────── */

// Любая ссылка на /download открывает список систем: какая есть в последнем
// релизе — ссылкой, какой нет — серой строкой «скоро». Без скриптов ссылка
// работает сама: сервер узнаёт систему по браузеру и отдаёт нужный файл.
const SYSTEMS = [
  ["windows", "Windows", ".exe"],
  ["mac", "macOS", ".dmg"],
  ["linux", "Linux", ".deb · .AppImage"],
  ["android", "Android", ".apk"],
];
let downloads = null;
const loadDownloads = () =>
  (downloads ??= fetch("/api/downloads")
    .then((response) => (response.ok ? response.json() : { files: {} }))
    .catch(() => ((downloads = null), { files: {} })));

function thisSystem() {
  const ua = navigator.userAgent;
  if (/Android/i.test(ua)) return "android";
  if (/Mac OS X|Macintosh/i.test(ua) && !/iPhone|iPad/i.test(ua)) return "mac";
  if (/Linux|X11/i.test(ua)) return "linux";
  return "windows";
}

const megabytes = (size) => (size ? `${Math.max(1, Math.round(size / 1048576))} ${state.lang === "ru" ? "МБ" : "MB"}` : "");

function closeDownloadMenu() {
  const open = document.querySelector(".dlmenu");
  if (!open) return;
  open.owner?.setAttribute("aria-expanded", "false");
  open.remove();
}

async function openDownloadMenu(link) {
  closeDownloadMenu();
  const tr = t();
  const mine = thisSystem();
  const list = h("div", { class: "dlmenu__list" });
  const menu = h("div", { class: "dlmenu", role: "menu" }, h("div", { class: "dlmenu__head mono" }, tr.dlPick), list);
  menu.owner = link;
  link.setAttribute("aria-expanded", "true");
  const place = () => {
    const box = link.getBoundingClientRect();
    const width = Math.min(300, window.innerWidth - 24);
    const left = Math.min(Math.max(12, box.left), window.innerWidth - width - 12);
    const below = box.bottom + 6;
    menu.style.width = `${width}px`;
    menu.style.left = `${left}px`;
    // Не помещается снизу — открываем над кнопкой.
    const height = menu.offsetHeight || 260;
    menu.style.top = below + height > window.innerHeight - 8 && box.top > height + 8 ? `${box.top - height - 6}px` : `${below}px`;
  };
  document.body.append(menu);
  const paint = ({ files = {}, version = "" }) => {
    list.replaceChildren(
      ...SYSTEMS.map(([os, name, kind]) => {
        const file = files[os];
        const body = [
          h("span", { class: "dlmenu__name" }, name, os === mine ? h("span", { class: "dlmenu__mine mono" }, tr.dlAuto) : null),
          h("span", { class: "dlmenu__kind mono" }, file ? [kind, megabytes(file.size)].filter(Boolean).join(" · ") : tr.dlSoon),
        ];
        return file
          ? h("a", { class: "dlmenu__item", role: "menuitem", href: `/download?os=${os}`, onclick: closeDownloadMenu }, ...body)
          : h("span", { class: "dlmenu__item is-off", role: "menuitem", "aria-disabled": "true" }, ...body);
      }),
      version ? h("div", { class: "dlmenu__ver mono" }, `v${version}`) : null,
    );
    place();
    (list.querySelector(`a[href$="${mine}"]`) ?? list.querySelector("a"))?.focus();
  };
  paint({ files: {} });
  paint(await loadDownloads());
}

document.addEventListener("click", (event) => {
  const link = event.target.closest?.('a[href="/download"]');
  if (link) {
    event.preventDefault();
    event.stopPropagation();
    if (document.querySelector(".dlmenu")?.owner === link) closeDownloadMenu();
    else openDownloadMenu(link);
    return;
  }
  if (!event.target.closest?.(".dlmenu")) closeDownloadMenu();
}, true);
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || !document.querySelector(".dlmenu")) return;
  const owner = document.querySelector(".dlmenu").owner;
  closeDownloadMenu();
  owner?.focus();
});
window.addEventListener("resize", closeDownloadMenu);
window.addEventListener("scroll", closeDownloadMenu, { passive: true });
window.addEventListener("hashchange", closeDownloadMenu);
