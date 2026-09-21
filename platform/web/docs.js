// Документация NOAH на самом сайте. Разделы — по Diátaxis: обучение (пройти
// за руку), практика (решить задачу), справочник (найти точный ответ),
// концепции (понять, как устроено). Страницы — Markdown в этом файле;
// регламент модуля приходит с сервера — тот же текст, что читает нейросеть.

import { GUIDES } from "./docs-guides.js?v=38";
import { REFERENCE } from "./docs-reference.js?v=38";

const SECTIONS = [
  { id: "tutorials", ru: "Обучение", en: "Tutorials" },
  { id: "howto", ru: "Практика", en: "How-to guides" },
  { id: "reference", ru: "Справочник", en: "Reference" },
  { id: "concepts", ru: "Концепции", en: "Concepts" },
];

const PAGES = [...GUIDES, ...REFERENCE];

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
