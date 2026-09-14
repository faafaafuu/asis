// Окно активов: цены и изменения за день, неделю, месяц, год и всё время.
//
// Данные приходят из Rust — сама страница в сеть не ходит. Щелчок по тикеру
// просит Rust открыть график TradingView: адрес собирается там, по списку, и
// открыть можно только то, что в списке есть.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

const PERIODS = ["day", "week", "month", "year", "all"];
const SYMBOLS = { USD: "$", EUR: "€", RUB: "₽", GBP: "£", JPY: "¥", CNY: "¥" };
const REFRESH_MS = 60_000;

let rows = [];
let period = remembered("watch.period", "day");
if (!PERIODS.includes(period)) period = "day";
let loading = false;

/** Вкладки и открытая сейчас. Пустая строка — «Все». */
let tabs = [];
let tab = remembered("watch.tab", "");
const EMPTY_ALL = ui.empty.textContent.trim();

function remembered(key, fallback) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function remember(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* хранилище недоступно — выбор просто не запомнится */
  }
}

/**
 * Цена: у мелочи — две значащие цифры после нулей, у крупного — без копеек, у
 * валютных пар и прочего до десяти — четыре знака: «1,1596», а не «1,16».
 */
function formatPrice(value, currency) {
  if (value === null || value === undefined) return "—";
  const big = value >= 1000;
  const small = value < 1;
  const digits = big ? 0 : small ? Math.max(2, -Math.floor(Math.log10(value)) + 1) : value < 10 ? 4 : 2;
  const text = new Intl.NumberFormat("ru-RU", {
    maximumFractionDigits: digits,
    minimumFractionDigits: big || small ? 0 : 2,
  }).format(value);
  const sign = SYMBOLS[currency];
  return sign ? `${text} ${sign}` : `${text} ${currency}`;
}

/** Изменение: знак, одна цифра после запятой, у сотен процентов — без неё. */
function formatChange(value) {
  if (value === null || value === undefined) return { text: "—", sign: "none" };
  const abs = Math.abs(value);
  const digits = abs >= 100 ? 0 : 1;
  const text = new Intl.NumberFormat("ru-RU", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(abs);
  if (Number(text.replace(/\s/g, "").replace(",", ".")) === 0) return { text: `${text}%`, sign: "flat" };
  return value > 0 ? { text: `+${text}%`, sign: "up" } : { text: `−${text}%`, sign: "down" };
}

/** Маленький график линией. */
function sparkline(points) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 64 20");
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("class", "row__spark");
  svg.setAttribute("aria-hidden", "true");
  if (!points || points.length < 2) return svg;

  const low = Math.min(...points);
  const high = Math.max(...points);
  const span = high - low || 1;
  const d = points
    .map((point, at) => {
      const x = (at / (points.length - 1)) * 64;
      const y = 18 - ((point - low) / span) * 16;
      return `${at ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join("");
  const line = document.createElementNS(ns, "path");
  line.setAttribute("d", d);
  line.setAttribute("class", points[points.length - 1] >= points[0] ? "spark--up" : "spark--down");
  svg.append(line);
  return svg;
}

function render() {
  for (const button of ui.columns.querySelectorAll("[data-period]")) {
    button.setAttribute("aria-pressed", String(button.dataset.period === period));
  }
  // Сверху — то, что за выбранный период выросло сильнее всего; без данных — вниз.
  const shown = tab ? rows.filter((row) => row.tabs?.includes(tab)) : rows;
  const sorted = [...shown].sort(
    (a, b) => (b[period] ?? -Infinity) - (a[period] ?? -Infinity),
  );
  ui.rows.replaceChildren(...sorted.map(renderRow));
  ui.empty.textContent = tab
    ? `Во вкладке «${tab}» пока пусто. Впишите тикер ниже — актив попадёт сюда.`
    : EMPTY_ALL;
  ui.empty.hidden = shown.length > 0;
  ui.columns.hidden = shown.length === 0;
  renderTabs();
}

/* ── Вкладки ───────────────────────────────────────────────────────────── */

function renderTabs() {
  const add = document.createElement("button");
  add.type = "button";
  add.className = "tab tab--new";
  add.title = "Новая вкладка";
  add.textContent = "+";
  add.addEventListener("click", () => newTab(add));
  ui.tabs.replaceChildren(tabButton("", "Все"), ...tabs.map((name) => tabButton(name, name)), add);
}

function tabButton(name, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "tab";
  button.setAttribute("role", "tab");
  button.setAttribute("aria-selected", String(name === tab));
  button.textContent = label;
  if (name) {
    const drop = document.createElement("span");
    drop.className = "tab__drop";
    drop.title = "Убрать вкладку — активы останутся во «Всех»";
    drop.textContent = "×";
    button.append(drop);
  }
  button.addEventListener("click", async (event) => {
    if (event.target.closest(".tab__drop")) {
      await api?.invoke("watch_tab_remove", { name }).catch(() => {});
      ui.note.textContent = `Вкладка «${name}» убрана, активы остались во «Всех»`;
      if (tab === name) selectTab("");
      await refresh();
      return;
    }
    selectTab(name);
  });
  return button;
}

function selectTab(name) {
  tab = name;
  remember("watch.tab", name);
  render();
}

/** «+» превращается в поле для названия: Enter — завести, Esc — передумать. */
function newTab(button) {
  const input = document.createElement("input");
  input.className = "tab__input";
  input.placeholder = "Название";
  input.maxLength = 24;
  let done = false;
  const finish = async (save) => {
    if (done) return;
    done = true;
    const name = input.value.trim();
    if (save && name && api) {
      try {
        const made = await api.invoke("watch_tab_add", { name });
        tabs = (await api.invoke("watch_tabs")) ?? tabs;
        selectTab(made);
        return;
      } catch (err) {
        ui.note.textContent = String(err);
      }
    }
    render();
  };
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") {
      // Esc здесь — передумать с названием, а не закрыть окно.
      event.stopPropagation();
      finish(false);
    }
  });
  input.addEventListener("blur", () => finish(true));
  button.replaceWith(input);
  input.focus();
}

function renderRow(row) {
  const line = document.createElement("div");
  line.className = "grid row";

  const ticker = document.createElement("button");
  ticker.type = "button";
  ticker.className = "row__ticker";
  ticker.title = "Открыть график в TradingView";
  ticker.textContent = row.symbol;
  ticker.addEventListener("click", () => {
    api?.invoke("watch_chart", { id: row.id }).catch((err) => {
      ui.note.textContent = String(err);
    });
  });

  const name = document.createElement("span");
  name.className = "row__name";
  name.textContent = row.name;
  name.title = row.name;

  const who = document.createElement("div");
  who.className = "row__who";
  who.append(ticker, name);

  const price = document.createElement("span");
  price.className = "row__price";
  price.textContent = formatPrice(row.price, row.currency);

  const cells = PERIODS.map((key) => {
    const { text, sign } = formatChange(row[key]);
    const cell = document.createElement("span");
    cell.className = `row__change row__change--${sign}`;
    if (key === period) cell.classList.add("row__change--chosen");
    cell.textContent = text;
    return cell;
  });

  const drop = document.createElement("button");
  drop.type = "button";
  drop.className = "row__remove";
  // Во вкладке крестик убирает только из неё: во «Всех» актив остаётся.
  drop.title = tab ? `Убрать из вкладки «${tab}»` : "Убрать из списка";
  drop.setAttribute("aria-label", `Убрать ${row.name}`);
  drop.textContent = "×";
  drop.addEventListener("click", async () => {
    await api?.invoke("watch_remove", { id: row.id, tab: tab || null }).catch(() => {});
    ui.note.textContent = tab ? `Убрал из «${tab}»: ${row.name}` : `Убрал: ${row.name}`;
    refresh();
  });

  line.append(who, sparkline(row.spark), price, ...cells, drop);
  return line;
}

async function refresh() {
  if (loading || !api) return;
  loading = true;
  ui.updated.textContent = "обновляю…";
  try {
    const [fresh, known] = await Promise.all([api.invoke("watch_rows"), api.invoke("watch_tabs")]);
    rows = fresh ?? [];
    tabs = known ?? [];
    // Вкладку убрали — голосом или в другом окне: открытой остаются «Все».
    if (tab && !tabs.includes(tab)) {
      tab = "";
      remember("watch.tab", "");
    }
    render();
    const time = new Date().toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
    ui.updated.textContent = `обновлено в ${time}`;
  } catch {
    ui.updated.textContent = "цены не пришли";
  } finally {
    loading = false;
  }
}

/* ── Действия ──────────────────────────────────────────────────────────── */

ui.columns.addEventListener("click", (event) => {
  const button = event.target.closest("[data-period]");
  if (!button) return;
  period = button.dataset.period;
  remember("watch.period", period);
  render();
});

ui.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const query = ui.input.value.trim();
  if (!query || !api) return;
  ui.note.textContent = "ищу…";
  ui.input.disabled = true;
  try {
    // Открыта вкладка — актив ложится и в неё.
    const name = await api.invoke("watch_add", { query, tab: tab || null });
    ui.note.textContent = tab ? `Добавил во «${tab}»: ${name}` : `Добавил: ${name}`;
    ui.input.value = "";
    await refresh();
  } catch (err) {
    ui.note.textContent = String(err);
  } finally {
    ui.input.disabled = false;
    ui.input.focus();
  }
});

ui.refresh.addEventListener("click", () => refresh());

/* ── Окно ──────────────────────────────────────────────────────────────── */

const win = appWindow();
ui.head.addEventListener("pointerdown", (event) => {
  // Кнопки живут в заголовке — за них окно не таскают.
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") api?.invoke("close_watchlist").catch(() => {});
});

// Рамки у окна нет, а значит, и системного крестика.
ui.close.addEventListener("click", () => {
  api?.invoke("close_watchlist").catch(() => {});
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
api?.listen("watchlist:changed", () => refresh());
// «Покажи вкладку фонды»: открытое окно получает вкладку событием, только что
// созданное — забирает сам, событие до него могло не дойти.
api?.listen("watchlist:tab", (event) => selectTab(String(event.payload ?? "")));
api
  ?.invoke("watch_open_tab")
  .then((name) => {
    if (name) selectTab(name);
  })
  .catch(() => {});

// Пока окно видно — цены свежие; спрятанное окно в сеть не ходит.
setInterval(() => {
  if (document.visibilityState === "visible") refresh();
}, REFRESH_MS);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") refresh();
});

render();
refresh();
