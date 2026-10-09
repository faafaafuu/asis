// NOAH Учёба — приложение обучения на телефоне. Подключает значки и экраны
// и открывает вкладку «Курсы».

import { start, api, nav, isWeb, prefs } from "./core.js";
import { store } from "./store.js";
import { mountAssistant } from "./assistant.js";
import { mountSuflyor } from "./suflyor.js";
import "./screens/courses.js";
import "./screens/course.js";
import "./screens/topic.js";
import "./screens/lesson.js";
import "./screens/review.js";
import "./screens/check.js";
import "./screens/practice.js";
import "./screens/focus.js";
import "./screens/profile.js";

async function sprite() {
  try {
    const text = await (await fetch("styles/noah/icons.svg")).text();
    const box = document.createElement("div");
    box.hidden = true;
    // Без атрибута style: на сайте правила безопасности его запрещают —
    // спрятан и так весь контейнер.
    box.innerHTML = text.replace(/<svg([^>]*?)\sstyle="[^"]*"/, "<svg$1");
    document.body.prepend(box);
  } catch {
    /* без значков — подписи всё равно есть */
  }
}

/**
 * Экранная клавиатура iPhone не ужимает страницу — она наезжает сверху, и
 * поле вопроса уходило под неё. Приложение и листы держатся видимой
 * области (visualViewport): над клавиатурой, без отступа под «домой».
 */
function followKeyboard() {
  const vv = window.visualViewport;
  if (!vv) return;
  const root = document.documentElement;
  const fit = () => {
    root.style.setProperty("--vvh", `${vv.height}px`);
    root.style.setProperty("--vvtop", `${vv.offsetTop}px`);
    root.classList.toggle("kb-open", window.innerHeight - vv.height > 120);
  };
  vv.addEventListener("resize", fit);
  vv.addEventListener("scroll", fit);
  fit();
}

/**
 * Тема приложения — «Неон», всегда. Окно телефона программа создаёт со
 * скриптом, который ставит тему из настроек компьютера, — и повторяет это,
 * когда документ догружается. Здесь тема возвращается.
 */
function keepNeon() {
  const root = document.documentElement;
  const neon = () => {
    if (root.dataset.theme !== "neon") root.dataset.theme = "neon";
  };
  neon();
  new MutationObserver(neon).observe(root, { attributes: true, attributeFilter: ["data-theme"] });
}

keepNeon();
followKeyboard();
await sprite();
// В браузере имя — из входа на сайт (на телефоне его запоминает вход через Telegram).
if (isWeb) {
  const name = await api.invoke("account_name").catch(() => "");
  if (name) prefs.set("name", name);
}
start(document.querySelector("main.app"));
mountAssistant();
mountSuflyor();

// Курс аккаунта пришёл на устройство (сверка с сайтом идёт в фоне) — экран
// курсов перерисовывается сам, без перезапуска.
const coursesArrived = () => {
  store.forget();
  // Только главный экран: урок или проверку посреди дела не перерисовываем.
  if (nav.current === "courses" && nav.depth === 1) nav.refresh();
};
api?.listen("learn:courses", coursesArrived);
// В браузере курсы дорастают так же — событием Ноа онлайн.
api?.listen("learn:changed", coursesArrived);
