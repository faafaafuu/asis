// Окно заказа: что набрано, почём и на каком шаге.
//
// Состояние живёт в Rust, здесь только показ. Окно перечитывает его целиком на
// каждое изменение: строк в заказе единицы, и перерисовка дешевле, чем возня
// с точечным обновлением, — зато показанное нельзя рассинхронить с настоящим.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

/** Как называется каждый шаг для человека. */
const STAGES = {
  picking: "ищу в магазине",
  picked: "подобрано",
  inCart: "в корзине",
  tooExpensive: "дороже потолка",
  failed: "не вышло",
};

async function refresh() {
  const order = await api?.invoke("order_state");

  const has = Boolean(order);
  ui.empty.hidden = has;
  ui.sum.hidden = !has;

  if (!has) {
    ui.lines.replaceChildren();
    ui.stage.textContent = "";
    ui.note.textContent = "";
    return;
  }

  ui.stage.textContent = STAGES[order.stage] ?? order.stage;
  ui.stage.dataset.stage = order.stage;
  ui.note.textContent = order.note ?? "";

  ui.lines.replaceChildren();
  for (const line of order.lines ?? []) {
    ui.lines.append(renderLine(line));
  }
  // Ненайденное — тоже часть просьбы, и о нём нельзя умолчать.
  for (const name of order.missing ?? []) {
    ui.lines.append(renderMissing(name));
  }

  ui.total.textContent = `${order.total} ₽`;

  const short = order.untilFreeDelivery;
  ui.delivery.hidden = short === null || short === undefined;
  if (!ui.delivery.hidden) {
    ui.delivery.textContent = `До бесплатной доставки не хватает ${short} ₽`;
  }

  // Про потолок говорим только когда он и правда мешает: постоянная строка
  // «ваш потолок такой-то» ничего не добавляет.
  const over = order.maxOrder > 0 && order.total > order.maxOrder;
  ui.ceiling.hidden = !over;
  if (over) {
    ui.ceiling.textContent = `Дороже потолка в ${order.maxOrder} ₽ — в корзину не кладу`;
  }
}

function renderLine(line) {
  const row = document.createElement("div");
  row.className = "line" + (line.inCart ? " line--in-cart" : "");

  const mark = document.createElement("span");
  mark.className = "line__mark";
  mark.title = line.inCart ? "лежит в корзине" : "найдено, но не в корзине";

  const name = document.createElement("span");
  name.className = "line__name";
  name.textContent = line.name;

  const price = document.createElement("span");
  price.className = "line__price" + (line.price === null ? " line__price--unknown" : "");
  price.textContent = line.price === null ? "цена неизвестна" : `${line.price} ₽`;

  row.append(mark, name, price);
  return row;
}

function renderMissing(name) {
  const row = document.createElement("div");
  row.className = "line line--missing";

  const mark = document.createElement("span");
  mark.className = "line__mark";

  const text = document.createElement("span");
  text.className = "line__name";
  text.textContent = name;

  const note = document.createElement("span");
  note.className = "line__price line__price--unknown";
  note.textContent = "не нашлось";

  row.append(mark, text, note);
  return row;
}

/* ── Окно ──────────────────────────────────────────────────────────────── */

const win = appWindow();
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  win?.startDragging();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") api?.invoke("close_order").catch(() => {});
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
api?.listen("order:changed", refresh);

refresh();
