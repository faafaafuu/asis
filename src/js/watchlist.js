// Окно активов: цены и изменения за день, неделю, месяц, год и всё время.
//
// Данные приходят из Rust — сама страница в сеть не ходит. Щелчок по тикеру
// просит Rust открыть график TradingView: адрес собирается там, по списку, и
// открыть можно только то, что в списке есть.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

const PERIODS = [
  { key: "day", label: "День", over: "день" },
  { key: "week", label: "Неделя", over: "неделю" },
  { key: "month", label: "Месяц", over: "месяц" },
  { key: "year", label: "Год", over: "год" },
  { key: "all", label: "Всё", over: "всё время" },
];
const SYMBOLS = { USD: "$", EUR: "€", RUB: "₽", GBP: "£", JPY: "¥", CNY: "¥" };
const REFRESH_MS = 60_000;
/** Вкладка оповещений. Двоеточие в начале — такое имя вписать нельзя. */
const ALERTS = ":alerts";

let rows = [];
let tabs = [];
let tab = remembered("watch.tab", "");
/** По какому периоду упорядочено. Пусто — свой порядок, тот, что перетащили. */
let sort = remembered("watch.sort", "");
if (!PERIODS.some((p) => p.key === sort)) sort = "";
/** Скрытые колонки: периоды и график. */
let hidden = new Set(readJson("watch.hidden", []));
/** Актив, у которого открыты настройки. */
let opened = null;
let telegramReady = false;
let loading = false;
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

function readJson(key, fallback) {
  try {
    return JSON.parse(remembered(key, "")) ?? fallback;
  } catch {
    return fallback;
  }
}

function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  if (tag === "button") node.type = "button";
  return node;
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

/* ── Оповещения ────────────────────────────────────────────────────────── */

/** Сколько процентов цене осталось до отметки: «+4,0%» — вверх, «−3,2%» — вниз. */
function distance(row, alert) {
  if (!row.price) return null;
  return (alert.price / row.price - 1) * 100;
}

function distanceText(row, alert) {
  if (alert.fired) return `сработало ${alert.fired}`;
  const value = distance(row, alert);
  if (value === null) return "—";
  return `ещё ${formatChange(value).text}`;
}

/** Ближайшее несработавшее оповещение актива — для порядка во вкладке «Оповещения». */
function nearest(row) {
  const values = (row.alerts ?? [])
    .filter((alert) => !alert.fired)
    .map((alert) => Math.abs(distance(row, alert) ?? Infinity));
  return values.length ? Math.min(...values) : Infinity;
}

const armed = (row) => (row.alerts ?? []).some((alert) => !alert.fired);

/* ── Таблица ───────────────────────────────────────────────────────────── */

function visiblePeriods() {
  return PERIODS.filter((p) => !hidden.has(p.key));
}

/** Сетка колонок: одна на заголовки и строки — цифры встают друг под другом. */
function layout() {
  const columns = ["14px", "minmax(96px, 1.6fr)"];
  if (!hidden.has("spark")) columns.push("64px");
  columns.push("minmax(74px, 1fr)");
  for (const _ of visiblePeriods()) columns.push("minmax(52px, 0.8fr)");
  columns.push("22px", "22px");
  document.documentElement.style.setProperty("--grid", columns.join(" "));
}

function renderColumns() {
  const cells = [el("span"), el("span", "", "Актив")];
  if (!hidden.has("spark")) {
    const spark = el("span", "columns__spark", "график");
    spark.title = "У монет — неделя по часам, у акций — месяц по дням";
    cells.push(spark);
  }
  cells.push(el("span", "columns__price", "Цена"));
  for (const period of visiblePeriods()) {
    const button = el("button", "", period.label);
    button.dataset.period = period.key;
    button.title =
      sort === period.key
        ? "Вернуть свой порядок"
        : `Упорядочить по изменению за ${period.over}`;
    button.setAttribute("aria-pressed", String(sort === period.key));
    cells.push(button);
  }
  cells.push(el("span"), el("span"));
  ui.columns.replaceChildren(...cells);
}

function shownRows() {
  if (tab === ALERTS) return rows.filter((row) => row.alerts?.length);
  return tab ? rows.filter((row) => row.tabs?.includes(tab)) : rows;
}

function render() {
  layout();
  renderColumns();
  let shown = shownRows();
  if (sort) {
    // Сверху — то, что за выбранный период выросло сильнее всего; без данных — вниз.
    shown = [...shown].sort((a, b) => (b[sort] ?? -Infinity) - (a[sort] ?? -Infinity));
  } else if (tab === ALERTS) {
    // Во вкладке оповещений сверху — те, чья цена ближе всего к отметке.
    shown = [...shown].sort((a, b) => nearest(a) - nearest(b));
  }
  ui.rows.replaceChildren(
    ...shown.flatMap((row) => (opened === row.id ? [renderRow(row), renderPanel(row)] : [renderRow(row)])),
  );
  ui.empty.textContent =
    tab === ALERTS
      ? "Оповещений пока нет. Откройте «⋯» у актива и впишите цену."
      : tab
        ? `Во вкладке «${tab}» пока пусто. Впишите тикер ниже — актив попадёт сюда.`
        : EMPTY_ALL;
  ui.empty.hidden = shown.length > 0;
  ui.columns.hidden = shown.length === 0;
  renderTabs();
}

function renderRow(row) {
  const line = el("div", "grid row");
  line.dataset.id = row.id;

  // Ручка: перетаскивать можно только в своём порядке — в упорядоченном по
  // периоду или по близости к оповещениям строка всё равно встала бы на место.
  const grip = el("span", "row__grip", "⋮⋮");
  grip.setAttribute("aria-hidden", "true");
  if (!sort && tab !== ALERTS) {
    grip.title = "Перетащить";
    grip.addEventListener("pointerdown", (event) => startReorder(event, line, grip));
  } else {
    grip.classList.add("row__grip--off");
  }

  const ticker = el("button", "row__ticker", row.symbol);
  ticker.title = "Открыть график в TradingView";
  ticker.addEventListener("click", () => {
    api?.invoke("watch_chart", { id: row.id }).catch((err) => {
      ui.note.textContent = String(err);
    });
  });

  const head = el("span", "row__head");
  head.append(ticker);
  if (armed(row)) {
    const bell = el("span", "row__bell", "🔔");
    bell.title = row.alerts
      .filter((alert) => !alert.fired)
      .map((alert) => `${formatPrice(alert.price, row.currency)} · ${distanceText(row, alert)}`)
      .join("\n");
    head.append(bell);
  }

  const name = el("span", "row__name", row.name);
  name.title = row.name;

  const who = el("div", "row__who");
  who.append(head, name);
  // Во вкладке оповещений под названием — отметки и сколько до них.
  if (tab === ALERTS) {
    for (const alert of row.alerts ?? []) {
      const near = !alert.fired && Math.abs(distance(row, alert) ?? Infinity) < 2;
      const mark = el(
        "span",
        `row__alert${near ? " row__alert--near" : ""}${alert.fired ? " row__alert--fired" : ""}`,
        `${alert.above ? "↑" : "↓"} ${formatPrice(alert.price, row.currency)} · ${distanceText(row, alert)}`,
      );
      who.append(mark);
    }
  }

  const cells = [grip, who];
  if (!hidden.has("spark")) cells.push(sparkline(row.spark));
  cells.push(el("span", "row__price", formatPrice(row.price, row.currency)));
  for (const period of visiblePeriods()) {
    const { text, sign } = formatChange(row[period.key]);
    const cell = el("span", `row__change row__change--${sign}`, text);
    if (period.key === sort) cell.classList.add("row__change--chosen");
    cells.push(cell);
  }

  const more = el("button", "row__more", "⋯");
  more.title = "Настройки актива: оповещения и вкладки";
  more.setAttribute("aria-expanded", String(opened === row.id));
  if (opened === row.id) line.classList.add("row--open");
  more.addEventListener("click", () => {
    opened = opened === row.id ? null : row.id;
    render();
  });

  // Во вкладке крестик убирает только из неё: во «Всех» актив остаётся.
  const inTab = tab && tab !== ALERTS;
  const drop = el("button", "row__remove", "×");
  drop.title = inTab ? `Убрать из вкладки «${tab}»` : "Убрать из списка";
  drop.setAttribute("aria-label", `Убрать ${row.name}`);
  drop.addEventListener("click", async () => {
    await api?.invoke("watch_remove", { id: row.id, tab: inTab ? tab : null }).catch(() => {});
    ui.note.textContent = inTab ? `Убрал из «${tab}»: ${row.name}` : `Убрал: ${row.name}`;
    if (opened === row.id) opened = null;
    refresh();
  });

  line.append(...cells, more, drop);
  return line;
}

/** Настройки актива: оповещения о цене и вкладки. Раскрываются под строкой. */
function renderPanel(row) {
  const panel = el("div", "panel");

  const alerts = el("div", "panel__section");
  alerts.append(el("span", "panel__label", "Оповещения о цене"));
  (row.alerts ?? []).forEach((alert, index) => {
    const item = el("div", `panel__alert${alert.fired ? " panel__alert--fired" : ""}`);
    item.append(
      el(
        "span",
        "",
        `${alert.above ? "↑" : "↓"} ${formatPrice(alert.price, row.currency)} — ${distanceText(row, alert)}`,
      ),
    );
    const drop = el("button", "panel__drop", "×");
    drop.title = "Убрать оповещение";
    drop.addEventListener("click", async () => {
      await api?.invoke("watch_alert_remove", { id: row.id, index }).catch(() => {});
      refresh();
    });
    item.append(drop);
    alerts.append(item);
  });

  const form = el("form", "panel__form");
  const input = el("input", "panel__input");
  input.type = "text";
  input.inputMode = "decimal";
  input.placeholder = row.price ? `Цена, сейчас ${formatPrice(row.price, row.currency)}` : "Цена";
  input.autocomplete = "off";
  const add = el("button", "panel__button", "Поставить");
  add.type = "submit";
  form.append(input, add);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const price = Number(input.value.replace(/[\s ]/g, "").replace(",", "."));
    if (!Number.isFinite(price) || price <= 0) {
      ui.note.textContent = "Впишите цену числом, например 80000.";
      return;
    }
    try {
      const money = await api?.invoke("watch_alert_add", { id: row.id, price });
      ui.note.textContent = `Оповещение: ${row.name} — ${money}`;
      await refresh();
    } catch (err) {
      ui.note.textContent = String(err);
    }
  });
  alerts.append(form);
  alerts.append(
    el(
      "span",
      "panel__hint",
      telegramReady
        ? "Сработает — Ноа напишет в Telegram."
        : "Telegram не подключён — оповещение покажется окном. Подключить: настройки Суфлёра, раздел «Уведомления в Telegram».",
    ),
  );

  const folders = el("div", "panel__section");
  folders.append(el("span", "panel__label", "Вкладки"));
  if (tabs.length) {
    const chips = el("div", "panel__chips");
    for (const name of tabs) {
      const on = row.tabs?.includes(name);
      const chip = el("button", "tab", name);
      chip.setAttribute("aria-pressed", String(Boolean(on)));
      chip.addEventListener("click", async () => {
        await api?.invoke("watch_set_tab", { id: row.id, tab: name, on: !on }).catch(() => {});
        refresh();
      });
      chips.append(chip);
    }
    folders.append(chips);
  } else {
    folders.append(el("span", "panel__hint", "Вкладок пока нет — заведите плюсом в полосе вкладок."));
  }

  panel.append(alerts, folders);
  return panel;
}

/* ── Порядок перетаскиванием ───────────────────────────────────────────── */

/**
 * Тянут за ручку — строка едет, линия показывает, куда встанет. Свои события
 * указателя, а не перетаскивание браузера: его в окне приложения забирает
 * система для файлов, и до страницы оно не доходит.
 */
function startReorder(event, line, grip) {
  if (event.button !== 0) return;
  event.preventDefault();
  // Захват нужен, чтобы строка ехала, даже когда курсор ушёл с ручки. Не
  // дали — перетаскивание всё равно работает, пока курсор над ручкой.
  try {
    grip.setPointerCapture(event.pointerId);
  } catch {
    /* указатель уже отпущен */
  }
  const lines = [...ui.rows.querySelectorAll(".row")];
  let target = null;
  let after = false;
  line.classList.add("row--dragging");

  const clear = () => {
    for (const other of lines) other.classList.remove("row--before", "row--after");
  };
  const move = (e) => {
    clear();
    target = null;
    for (const other of lines) {
      const box = other.getBoundingClientRect();
      if (e.clientY < box.top + box.height / 2) {
        target = other;
        after = false;
        break;
      }
    }
    if (!target) {
      target = lines[lines.length - 1];
      after = true;
    }
    if (target !== line) target.classList.add(after ? "row--after" : "row--before");
  };
  const finish = () => {
    grip.removeEventListener("pointermove", move);
    line.classList.remove("row--dragging");
    clear();
    if (!target || target === line) return;

    const shown = lines.map((other) => other.dataset.id).filter((id) => id !== line.dataset.id);
    const at = shown.indexOf(target.dataset.id) + (after ? 1 : 0);
    shown.splice(at, 0, line.dataset.id);
    // Во вкладке видна только часть списка: переставляются места именно этих
    // строк, остальные активы остаются, где стояли.
    const inView = new Set(shown);
    let next = 0;
    const order = rows.map((row) => (inView.has(row.id) ? shown[next++] : row.id));
    rows = order.map((id) => rows.find((row) => row.id === id));
    render();
    api?.invoke("watch_reorder", { ids: order }).catch(() => {});
  };
  grip.addEventListener("pointermove", move);
  grip.addEventListener("pointerup", finish, { once: true });
  grip.addEventListener("pointercancel", finish, { once: true });
}

/* ── Вкладки ───────────────────────────────────────────────────────────── */

function renderTabs() {
  const add = el("button", "tab tab--new", "+");
  add.title = "Новая вкладка";
  add.addEventListener("click", () => newTab(add));
  const buttons = [tabButton("", "Все")];
  if (tab === ALERTS || rows.some((row) => row.alerts?.length)) buttons.push(tabButton(ALERTS, "Оповещения"));
  buttons.push(...tabs.map((name) => tabButton(name, name)), add);
  ui.tabs.replaceChildren(...buttons);
}

function tabButton(name, label) {
  const button = el("button", "tab", label);
  button.setAttribute("role", "tab");
  button.setAttribute("aria-selected", String(name === tab));
  const own = name && name !== ALERTS;
  if (own) {
    const drop = el("span", "tab__drop", "×");
    drop.title = "Убрать вкладку — активы останутся во «Всех»";
    button.append(drop);
  }
  button.addEventListener("click", async (event) => {
    if (own && event.target.closest(".tab__drop")) {
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
  const input = el("input", "tab__input");
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

/* ── Колонки ───────────────────────────────────────────────────────────── */

function renderMenu() {
  const items = [{ key: "spark", label: "График" }, ...PERIODS.map((p) => ({ key: p.key, label: p.label }))];
  ui.menu.replaceChildren(
    el("span", "menu__label", "Колонки"),
    ...items.map(({ key, label }) => {
      const row = el("label", "menu__item");
      const box = el("input");
      box.type = "checkbox";
      box.checked = !hidden.has(key);
      box.addEventListener("change", () => {
        if (box.checked) hidden.delete(key);
        else hidden.add(key);
        remember("watch.hidden", JSON.stringify([...hidden]));
        // Упорядочено по колонке, которую спрятали, — порядок возвращается свой.
        if (hidden.has(sort)) sort = "";
        render();
      });
      row.append(box, document.createTextNode(label));
      return row;
    }),
  );
}

ui.columnsButton.addEventListener("click", () => {
  ui.menu.hidden = !ui.menu.hidden;
  if (!ui.menu.hidden) renderMenu();
});

document.addEventListener("pointerdown", (event) => {
  if (!ui.menu.hidden && !event.target.closest("[data-el=menu], [data-el=columnsButton]")) ui.menu.hidden = true;
});

/* ── Данные ────────────────────────────────────────────────────────────── */

async function refresh() {
  if (loading || !api) return;
  loading = true;
  ui.updated.textContent = "обновляю…";
  try {
    const [fresh, known, ready] = await Promise.all([
      api.invoke("watch_rows"),
      api.invoke("watch_tabs"),
      api.invoke("watch_telegram_ready").catch(() => false),
    ]);
    rows = fresh ?? [];
    tabs = known ?? [];
    telegramReady = Boolean(ready);
    // Вкладку убрали — голосом или в другом окне: открытой остаются «Все».
    if (tab && tab !== ALERTS && !tabs.includes(tab)) {
      tab = "";
      remember("watch.tab", "");
    }
    if (opened && !rows.some((row) => row.id === opened)) opened = null;
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
  // Первый щелчок — упорядочить по периоду, второй — вернуть свой порядок.
  sort = sort === button.dataset.period ? "" : button.dataset.period;
  remember("watch.sort", sort);
  render();
});

ui.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const query = ui.input.value.trim();
  if (!query || !api) return;
  ui.note.textContent = "ищу…";
  ui.input.disabled = true;
  try {
    // Открыта своя вкладка — актив ложится и в неё.
    const inTab = tab && tab !== ALERTS ? tab : null;
    const name = await api.invoke("watch_add", { query, tab: inTab });
    ui.note.textContent = inTab ? `Добавил во «${inTab}»: ${name}` : `Добавил: ${name}`;
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
// Свернуть — окно уходит на панель задач и возвращается оттуда или из трея.
ui.minimize?.addEventListener("click", () => win?.minimize());
ui.head.addEventListener("pointerdown", (event) => {
  // Кнопки живут в заголовке — за них окно не таскают.
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

// Esc закрывает то, что открыто сверху: меню колонок, настройки актива, окно.
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (!ui.menu.hidden) {
    ui.menu.hidden = true;
    return;
  }
  if (opened) {
    opened = null;
    render();
  }
  // Окно Esc не закрывает: Esc — это «замолчи» для голоса Ноа, и нажатый
  // под её речь, он уносил бы вместе с речью и окно, в котором человек
  // работает. Закрывает Esc только окно объяснения выделенного слова.
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
