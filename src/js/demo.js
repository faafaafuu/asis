// Обвязка демо-документа: включает попап в вебе и повторяет панель состояний
// из дизайн-референса (кнопки «Загрузка / Ответ / Ошибка»).

import { MockProvider } from "./ai-client.js";
import { WebHost } from "./web-host.js";

const LATENCY_MS = 900; // как в референсе

const host = new WebHost({
  client: new MockProvider({ latencyMs: LATENCY_MS }),
  requireLeftCtrl: true,
}).mount();

/** Демо-кнопки: тот же попап, но с зафиксированным состоянием ответа. */
function demo(mode) {
  host.view.client = new MockProvider({
    latencyMs: LATENCY_MS,
    forceState: mode === "success" ? "auto" : mode,
  });
  const p = document.querySelector(".demo__text p");
  const r = p
    ? p.getBoundingClientRect()
    : { left: 300, right: 400, top: 300, bottom: 320, width: 100 };
  // Якорь имитирует выделенное слово внутри первого абзаца.
  host.range = null;
  host.showAt(
    { left: r.left + 40, right: r.left + 140, top: r.top, bottom: r.top + 22, width: 100 },
    "альбедо",
  );
}

for (const btn of document.querySelectorAll("[data-demo]")) {
  btn.addEventListener("click", () => demo(btn.dataset.demo));
}

// Переключатель темы: попап читает её из data-theme на <html>.
const themeBtn = document.querySelector("[data-theme-switch]");
themeBtn?.addEventListener("click", () => {
  const next = document.documentElement.dataset.theme === "light" ? "dark" : "light";
  document.documentElement.dataset.theme = next;
  themeBtn.textContent = next === "light" ? "Тёмная" : "Светлая";
});

// Автозапуск состояния из хэша — для скриншотов и ручной сверки с референсом:
// #demo=loading | success | error, дополнительно &expanded=1&theme=light
const params = new URLSearchParams(location.hash.slice(1));
if (params.has("theme")) document.documentElement.dataset.theme = params.get("theme");
if (params.has("demo")) {
  const mode = params.get("demo");
  demo(mode);
  if (params.get("expanded") === "1") {
    // Раскрытие возможно только после прихода ответа.
    setTimeout(() => host.view.expand(), LATENCY_MS + 60);
  }
}

// Восстанавливаем обычный (не форсированный) клиент после демо-показа,
// чтобы следующее выделение работало как в жизни.
document.addEventListener("mousedown", (e) => {
  if (e.target.closest?.("[data-demo]")) return;
  if (host.view.client instanceof MockProvider && host.view.client.forceState !== "auto") {
    host.view.client = new MockProvider({ latencyMs: LATENCY_MS });
  }
});
