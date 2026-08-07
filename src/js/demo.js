// Обвязка демо-документа: включает попап в вебе и повторяет панель состояний
// из дизайн-референса (кнопки «Загрузка / Ответ / Ошибка»).

import { MockProvider } from "./ai-client.js";
import { WikipediaProvider } from "./wiki-provider.js";
import { WebHost } from "./web-host.js";

const LATENCY_MS = 900; // как в референсе

// По выделению отвечает Википедия — бесплатно, без ключей и для любого слова,
// а не только для семи терминов демо-словаря.
const liveClient = new WikipediaProvider();

const host = new WebHost({
  client: liveClient,
  requireLeftCtrl: true,
}).mount();

/**
 * Демо-кнопки: тот же попап, но с зафиксированным состоянием ответа.
 *
 * Якорь — сама нажатая кнопка. Раньше попап привязывался к первому абзацу, и на
 * телефоне это ломалось: пока доскроллишь до кнопок внизу страницы, абзац уходит за
 * верхний край экрана, и окно открывается там же — за пределами видимого.
 */
function demo(mode, anchorEl) {
  host.view.client = new MockProvider({
    latencyMs: LATENCY_MS,
    forceState: mode === "success" ? "auto" : mode,
  });

  const r = anchorEl?.getBoundingClientRect();
  const anchor = r
    ? { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width }
    : // Без элемента — по центру экрана: попап всё равно должен быть виден.
      {
        left: window.innerWidth / 2 - 50,
        right: window.innerWidth / 2 + 50,
        top: window.innerHeight / 2,
        bottom: window.innerHeight / 2 + 22,
        width: 100,
      };

  host.range = null;
  host.showAt(anchor, "альбедо");
}

for (const btn of document.querySelectorAll("[data-demo]")) {
  btn.addEventListener("click", () => demo(btn.dataset.demo, btn));
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
  demo(mode, document.querySelector(".demo__text p"));
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

// Диагностика для разбора проблем на чужих устройствах: #debug=1 показывает
// строкой, где именно оказался попап. Без неё удалённо непонятно, не открылся он
// или открылся за пределами экрана.
if (params.get("debug") === "1") {
  const panel = document.createElement("div");
  panel.style.cssText =
    "position:fixed;left:0;right:0;bottom:0;z-index:99;padding:6px 8px;" +
    "font:12px/1.35 monospace;background:#1b1815;color:#f2ece1;white-space:pre-wrap";
  document.body.append(panel);
  setInterval(() => {
    const r = host.view.el.getBoundingClientRect();
    panel.textContent =
      `скрыт=${host.layer.hidden} видимость=${host.layer.style.visibility || "—"}\n` +
      `окно: ${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}×${Math.round(r.height)}\n` +
      `экран: ${window.innerWidth}×${window.innerHeight}, прокрутка ${Math.round(window.scrollY)}\n` +
      `на экране: ${r.top < window.innerHeight && r.bottom > 0 ? "да" : "НЕТ"}`;
  }, 250);
}

// Восстанавливаем обычный (не форсированный) клиент после демо-показа,
// чтобы следующее выделение работало как в жизни.
document.addEventListener("mousedown", (e) => {
  if (e.target.closest?.("[data-demo]")) return;
  if (host.view.client instanceof MockProvider) host.view.client = liveClient;
});
