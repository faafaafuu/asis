// Виджет расхода: «124k / 3.3m  $0.59» — токены сегодня / за месяц и деньги за
// месяц. Считает Rust (usage.rs), здесь — только показ.

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

function money(value) {
  if (!value) return "$0";
  return `$${value.toFixed(value < 0.1 ? 3 : 2)}`;
}

const total = (tally) => (tally?.prompt ?? 0) + (tally?.completion ?? 0);

function render(s) {
  ui.tokens.textContent = `${tokens(total(s.today))} / ${tokens(total(s.month))}`;
  ui.money.textContent = s.cloud ? money(s.month.cost) : "";
  const low = s.balance !== null && s.balance !== undefined && s.balance < 1;
  ui.money.classList.toggle("low", low);

  const lines = [
    `${s.service}: ${s.model || "модель не выбрана"}`,
    `сегодня — ${total(s.today).toLocaleString("ru-RU")} токенов${s.cloud ? `, ${money(s.today.cost)}` : ""}`,
    `за месяц — ${total(s.month).toLocaleString("ru-RU")} токенов${s.cloud ? `, ${money(s.month.cost)}` : ""}`,
  ];
  if (s.balance !== null && s.balance !== undefined) lines.push(`на счёте — ${money(s.balance)}`);
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
