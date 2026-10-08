// NOAH Учёба — приложение обучения на телефоне. Подключает значки и экраны
// и открывает вкладку «Курсы».

import { start, api, nav } from "./core.js";
import { store } from "./store.js";
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
    box.innerHTML = text;
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
start(document.querySelector("main.app"));

// Курс аккаунта пришёл на устройство (сверка с сайтом идёт в фоне) — экран
// курсов перерисовывается сам, без перезапуска.
api?.listen("learn:courses", () => {
  store.forget();
  // Только главный экран: урок или проверку посреди дела не перерисовываем.
  if (nav.current === "courses" && nav.depth === 1) nav.refresh();
});
