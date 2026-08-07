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

// Имитация тач-устройства: вместо Ctrl+выделения показывается мини-меню.
const touchBtn = document.querySelector("[data-touch-switch]");
touchBtn?.addEventListener("click", () => {
  host.forceTouchMenu = !host.forceTouchMenu;
  touchBtn.textContent = host.forceTouchMenu ? "Меню как на десктопе" : "Меню как на телефоне";
  host.hideMenu();
  host.hide();
});

// Автозапуск состояния из хэша — для скриншотов и ручной сверки с референсом:
// #demo=loading | success | error, дополнительно &expanded=1&theme=light&touch=1
// &pin=1 (фиксировать попап в углу) &ask=вопрос (отправить вопрос в тред)
const params = new URLSearchParams(location.hash.slice(1));

// Фиксация позиции нужна для съёмки: кадры разных состояний должны совпадать по
// координатам, иначе картинки «прыгают». На реальное позиционирование не влияет.
if (params.get("pin") === "1") {
  host.place = () => {
    host.layer.style.left = "24px";
    host.layer.style.top = "24px";
    host.layer.style.visibility = "visible";
  };
  const showMenu = host.showTouchMenu.bind(host);
  host.showTouchMenu = () => {
    showMenu();
    host.menuLayer.style.left = "24px";
    host.menuLayer.style.top = "24px";
  };
}
if (params.has("theme")) document.documentElement.dataset.theme = params.get("theme");
if (params.get("touch") === "1") {
  host.forceTouchMenu = true;
  if (touchBtn) touchBtn.textContent = "Меню как на десктопе";
  // Выделяем слово программно, чтобы меню появилось само — для скриншота.
  setTimeout(() => {
    const node = [...document.querySelectorAll(".demo__text p")][1]?.firstChild;
    if (!node) return;
    const text = node.textContent;
    const start = text.indexOf("криоконит");
    if (start < 0) return;
    const range = document.createRange();
    range.setStart(node, start);
    range.setEnd(node, start + "криоконит".length);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  }, 50);
}
if (params.has("demo")) {
  const mode = params.get("demo");
  demo(mode);
  if (params.get("expanded") === "1") {
    // Раскрытие возможно только после прихода ответа.
    setTimeout(() => host.view.expand(), LATENCY_MS + 60);
  }
  if (params.has("ask")) {
    // Вопрос в тред — тоже только после раскрытия.
    setTimeout(() => {
      host.view.ui.input.value = params.get("ask");
      host.view.submitAsk();
    }, LATENCY_MS + 160);
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
