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
    await keys();
  } catch (err) {
    ui.body.innerHTML = '<p class="mod__error"></p>';
    ui.body.firstChild.textContent = String(err);
  }
}

/**
 * Ключи модуля — внизу его собственного окна.
 *
 * Раньше их вводили на плитке модуля в общих настройках: человек открывал
 * окно модуля, не находил в нём ничего про ключи и шёл искать их там, где
 * стоят галочки про микрофон. Настройки модуля принадлежат модулю, и место им
 * рядом с тем, что они настраивают.
 *
 * Значения сюда не приходят никогда: поле показывает лишь, введён ли ключ, —
 * Ноа хранит их зашифрованными и не отдаёт обратно даже своим окнам.
 */
async function keys() {
  const fields = await api.invoke("plugins_secrets", { id }).catch(() => []);
  if (!fields.length) return;

  const box = document.createElement("section");
  box.className = "mod__keys";
  const head = document.createElement("h3");
  head.textContent = "Ключи";
  box.append(head);

  const inputs = {};
  for (const field of fields) {
    const label = document.createElement("label");
    const name = document.createElement("span");
    name.textContent = field.optional ? `${field.title} (необязательно)` : field.title;
    const input = document.createElement("input");
    input.type = "password";
    input.autocomplete = "off";
    input.spellcheck = false;
    input.placeholder = field.set ? "сохранён — оставьте пустым, чтобы не менять" : "вставьте ключ";
    inputs[field.name] = input;
    label.append(name, input);
    const hint = document.createElement("p");
    hint.className = "mod__hint";
    hint.textContent = field.hint;
    box.append(label, hint);
  }

  const save = document.createElement("button");
  save.type = "button";
  save.textContent = "Сохранить ключи";
  const note = document.createElement("p");
  note.className = "mod__hint";
  save.addEventListener("click", async () => {
    save.disabled = true;
    note.textContent = "Сохраняю…";
    const values = Object.fromEntries(Object.entries(inputs).map(([key, input]) => [key, input.value]));
    try {
      await api.invoke("plugins_save_secrets", { id, values });
      // Модуль перезапускается с новыми ключами — проверка идёт отдельно, из
      // плитки; здесь достаточно сказать, что ключи приняты.
      note.textContent = "Ключи сохранены.";
      for (const input of Object.values(inputs)) input.value = "";
    } catch (err) {
      note.textContent = String(err);
    }
    save.disabled = false;
  });
  box.append(save, note);
  ui.body.append(box);
}

open();
