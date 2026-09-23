// Виджет расхода: «всего 3.3m · $0.59» — сколько токенов и денег модель
// потратила за всё время. Разбивка по дням — при наведении. Считает Rust
// (usage.rs), здесь — только показ.

import { tauri, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

const REFRESH_MS = 10_000;
let onTop = false;

applyTheme(globalThis.__SUFLER_VIEW__?.theme ?? "neon");

/** 1234 → «1.2k», 2 500 000 → «2.5m». */
function tokens(n) {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}m`;
}

/** Доллары: у сумм меньше цента — четыре знака, иначе они выглядели бы нулём. */
function money(value) {
  if (!value) return "$0";
  return `$${value.toFixed(value < 0.01 ? 4 : value < 0.1 ? 3 : 2)}`;
}

const count = (tally) => (tally?.prompt ?? 0) + (tally?.completion ?? 0);
const known = (value) => value !== null && value !== undefined;

/** «5ч 66% · нед 62%» — коротко, по самому тесному окну первым. */
function limitsLine(limits) {
  const short = { "5 ч": "5ч", неделя: "нед", месяц: "мес", сегодня: "день" };
  return (limits ?? []).map((limit) => `${short[limit.name] ?? limit.name} ${Math.round(limit.used)}%`).join(" · ");
}

/** Когда обнулится — «через 2 ч 10 мин», «через 2 дн». */
function resetsIn(iso) {
  if (!iso) return "";
  const minutes = Math.round((new Date(iso).getTime() - Date.now()) / 60000);
  if (!Number.isFinite(minutes) || minutes <= 0) return "";
  if (minutes < 60) return ` · обнулится через ${minutes} мин`;
  if (minutes < 48 * 60) return ` · обнулится через ${Math.floor(minutes / 60)} ч ${minutes % 60} мин`;
  return ` · обнулится через ${Math.round(minutes / 1440)} дн`;
}

function render(s) {
  ui.tokens.textContent = `всего ${tokens(count(s.total))}`;
  const limits = s.limits ?? [];
  ui.limits.textContent = limitsLine(limits);
  ui.limits.hidden = !limits.length;
  ui.limits.classList.toggle("low", limits.some((limit) => limit.used >= 85));
  ui.money.textContent = s.cloud ? money(s.total.cost) : "";
  ui.money.classList.toggle("low", known(s.balance) && s.balance < 1);

  const period = (label, tally) =>
    `${label} — ${count(tally).toLocaleString("ru-RU")} токенов${s.cloud ? `, ${money(tally.cost)}` : ""}`;
  const lines = [
    `${s.service}: ${s.model || "модель не выбрана"}`,
    period("всего через Суфлёр", s.total),
    period("за месяц", s.month),
    period("сегодня", s.today),
  ];
  if (known(s.spent)) lines.push(`на счёте OpenRouter потрачено всего — ${money(s.spent)}`);
  if (known(s.balance)) lines.push(`остаток на счёте — ${money(s.balance)}`);
  // У подписки Claude Code остатка не спросить: ни команды, ни поля в ответе
  // для него нет. Молчать об этом хуже, чем сказать: иначе пустое место
  // выглядит как «не сосчитали».
  for (const limit of limits) {
    lines.push(`лимит ${limit.name}: использовано ${Math.round(limit.used)}%, ${limit.left}${resetsIn(limit.resets)}`);
  }
  lines.push(onTop ? "двойной щелчок — на рабочий стол" : "двойной щелчок — поверх окон");
  ui.line.title = lines.join("\n");
}

async function refresh() {
  if (!api) return;
  try {
    render(await api.invoke("usage_summary"));
  } catch {
    /* следующая попытка — по таймеру */
  }
}

async function loadPin() {
  if (!api) return;
  try {
    onTop = (await api.invoke("widget_settings")).onTop;
  } catch {
    /* останется как было */
  }
  ui.line.classList.toggle("pinned", onTop);
}

ui.line.addEventListener("dblclick", async () => {
  onTop = !onTop;
  ui.line.classList.toggle("pinned", onTop);
  await api?.invoke("save_widget_settings", { settings: { enabled: true, onTop } }).catch(() => {});
  refresh();
});

loadPin();
refresh();
setInterval(refresh, REFRESH_MS);
