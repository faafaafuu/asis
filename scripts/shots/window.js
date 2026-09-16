// Настоящее окно программы на тестовых данных — для картинок README.
//
// window.html?page=watchlist&steps=click:Крипто
//
// Страница окна (src/<page>.html) переносится сюда целиком: атрибуты корня,
// стили, разметка. Потом подключается её же скрипт — с подменой Tauri из
// mock-tauri.js — и выполняются шаги: `click:Текст` нажимает элемент с этим
// текстом, `fill:селектор=текст` вписывает текст в поле, `wait:мс` ждёт.

const params = new URLSearchParams(location.search);
const page = params.get("page") ?? "watchlist";
const steps = (params.get("steps") ?? "").split("|").filter(Boolean);
const src = new URL(`../../src/${page}.html`, location.href);

const doc = new DOMParser().parseFromString(await (await fetch(src)).text(), "text/html");

for (const { name, value } of doc.documentElement.attributes) {
  document.documentElement.setAttribute(name, value);
}
document.documentElement.dataset.theme = params.get("theme") ?? "neon";
for (const node of doc.head.querySelectorAll('link[rel="stylesheet"], style')) {
  const copy = node.cloneNode(true);
  if (copy.tagName === "LINK") copy.href = new URL(node.getAttribute("href"), src).href;
  document.head.append(copy);
}
document.title = doc.title;
for (const { name, value } of doc.body.attributes) document.body.setAttribute(name, value);

const scripts = [...doc.body.querySelectorAll("script[src]")].map((s) => new URL(s.getAttribute("src"), src).href);
doc.body.querySelectorAll("script").forEach((s) => s.remove());
document.body.innerHTML = doc.body.innerHTML;

await document.fonts.ready;
for (const script of scripts) await import(script);

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Элемент, чей собственный текст совпадает, — самый глубокий из подходящих. */
function byText(text) {
  const all = [...document.querySelectorAll("button, a, summary, li, [role=tab], div, span, h2, h3")];
  const exact = all.filter((node) => node.textContent.trim() === text);
  const pool = exact.length ? exact : all.filter((node) => node.textContent.trim().startsWith(text));
  return pool.find((node) => ![...node.children].some((child) => pool.includes(child))) ?? null;
}

await pause(300);
for (const step of steps) {
  const [kind, ...rest] = step.split(":");
  const arg = rest.join(":");
  if (kind === "wait") {
    await pause(Number(arg));
    continue;
  }
  if (kind === "click") {
    const node = byText(arg);
    if (!node) console.warn(`нет элемента «${arg}»`);
    node?.click();
  } else if (kind === "fill") {
    const [selector, ...text] = arg.split("=");
    const field = document.querySelector(selector);
    if (field) {
      field.value = text.join("=");
      field.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }
  await pause(250);
}
document.documentElement.dataset.shotReady = "1";
