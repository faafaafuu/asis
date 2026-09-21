// Окно модуля: разметку даёт модуль, всё остальное — Ноа.
//
// Модуль описывает окно одной HTML-страницей без своего кода: заголовки, поля,
// кнопки. Кнопка связывается с инструментом модуля атрибутами — `data-call`
// (какой инструмент), `data-arg-*` (постоянные значения), `data-field` у полей
// (что подставить) и `data-out` (куда положить ответ). Своих скриптов у модуля
// нет: чужой код в окне Ноа не исполняется, а окно везде выглядит одинаково.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

const api = tauri();
const win = appWindow();
const id = new URLSearchParams(location.search).get("id") ?? "";

applyTheme(globalThis.__SUFLER_VIEW__?.theme);

ui.close?.addEventListener("click", () => win?.close());
ui.minimize?.addEventListener("click", () => win?.minimize());
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") win?.close();
});

// Окно без системной рамки: за заголовок его двигают.
document.querySelector(".head")?.addEventListener("mousedown", (event) => {
  if (event.target.closest("button")) return;
  win?.startDragging();
});

/** Значения полей окна: `data-field="city"` — это аргумент `city`. */
function fields(root) {
  const args = {};
  for (const node of root.querySelectorAll("[data-field]")) {
    args[node.dataset.field] = node.type === "checkbox" ? node.checked : node.value;
  }
  return args;
}

/** Постоянные аргументы кнопки: `data-arg-city="Казань"`. */
function fixed(button) {
  const args = {};
  for (const [key, value] of Object.entries(button.dataset)) {
    if (key.startsWith("arg") && key.length > 3) {
      args[key.slice(3, 4).toLowerCase() + key.slice(4)] = value;
    }
  }
  return args;
}

function wire(root) {
  for (const button of root.querySelectorAll("[data-call]")) {
    button.addEventListener("click", async () => {
      const out = root.querySelector(`[data-out="${button.dataset.out ?? "result"}"]`) ?? null;
      button.disabled = true;
      if (out) out.textContent = "…";
      try {
        const answer = await api.invoke("module_call", {
          id,
          tool: button.dataset.call,
          args: { ...fields(root), ...fixed(button) },
        });
        if (out) out.textContent = answer;
      } catch (err) {
        if (out) out.textContent = String(err);
      } finally {
        button.disabled = false;
      }
    });
  }
}

async function open() {
  if (!api) {
    ui.body.innerHTML = '<p class="mod__error">Окно открыто вне приложения.</p>';
    return;
  }
  try {
    const module = await api.invoke("module_window", { id });
    document.title = `Суфлёр — ${module.title}`;
    ui.title.textContent = module.title;
    ui.icon.textContent = module.icon ?? "";
    // Разметка модуля вставляется как разметка: скрипты в ней не исполняются,
    // а проверка модуля их и не пропускает.
    ui.body.innerHTML = module.html;
    wire(ui.body);
  } catch (err) {
    ui.body.innerHTML = '<p class="mod__error"></p>';
    ui.body.firstChild.textContent = String(err);
  }
}

open();
