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
  placed: "оформлен",
  awaitingPayment: "ждёт оплаты",
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
    ui.store.hidden = true;
    return;
  }

  // Магазин показывается, только когда он уже выбран: в начале поиска полки
  // ещё сравниваются, и мелькающее название сбивало бы с толку.
  ui.store.hidden = !order.store;
  ui.store.textContent = order.store ?? "";

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
    ui.ceiling.textContent = `Дороже ${order.maxOrder} ₽ — без вашего подтверждения не оплачивается`;
  }

  // Корзина, собранная ссылкой: кнопка возвращает к ней, если браузер
  // закрыли или он открылся за другими окнами.
  ui.link.hidden = !order.link;

  // Оплата кнопкой — только когда заказ её ждёт. Сумма на кнопке та же, что
  // уйдёт в магазин: FoodPilot оформит, только если на странице оплаты она
  // совпадёт.
  ui.pay.hidden = order.stage !== "awaitingPayment";
  ui.pay.disabled = false;
  ui.pay.textContent = `Оплатить ${order.total} ₽`;
}

function renderLine(line) {
  const row = document.createElement("div");
  row.className = "line" + (line.inCart ? " line--in-cart" : "");

  const mark = document.createElement("span");
  mark.className = "line__mark";
  mark.title = line.inCart ? "лежит в корзине" : "найдено, но не в корзине";

  const name = document.createElement("span");
  name.className = "line__name";
  // Количество — рядом с названием, сумма — за всё количество: «× 5» и
  // цена за одну пачку вместе читались бы как пять пачек по цене одной.
  const count = line.quantity > 1 ? line.quantity : 1;
  name.textContent = count > 1 ? `${line.name} × ${count}` : line.name;

  const price = document.createElement("span");
  price.className = "line__price" + (line.price === null ? " line__price--unknown" : "");
  price.textContent = line.price === null ? "цена неизвестна" : `${line.price * count} ₽`;

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
  // Крестик живёт в заголовке — за него окно не таскают.
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") api?.invoke("close_order").catch(() => {});
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
api?.listen("order:changed", refresh);

// Рамки у окна нет, а значит, и системного крестика: без своего окно
// закрывалось только клавишей Esc.
ui.close.addEventListener("click", () => {
  api?.invoke("close_order").catch(() => {});
});

ui.pay.addEventListener("click", async () => {
  ui.pay.disabled = true;
  ui.pay.textContent = "Оплачиваю…";
  try {
    await api?.invoke("order_pay");
  } catch (err) {
    ui.note.textContent = `Не оплачено: ${err}`;
    ui.pay.disabled = false;
  }
});

ui.link.addEventListener("click", () => {
  api?.invoke("open_order_link").catch(() => {});
});

refresh();
