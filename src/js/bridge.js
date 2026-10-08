// Мост к Tauri. Фронтенд попапа собран без бандлера, поэтому API берётся из
// глобального объекта (`withGlobalTauri: true` в tauri.conf.json), а не из npm-пакета:
// одна зависимость меньше, и окно попапа грузится без единого сетевого запроса.

/**
 * @returns {{invoke: Function, listen: Function} | null} null — обычный браузер
 * без Ноа: демо-страница или окно, открытое файлом.
 */
export function tauri() {
  const api = globalThis.__TAURI__;
  if (api?.core?.invoke) {
    return {
      invoke: (cmd, args) => api.core.invoke(cmd, args),
      listen: (event, handler) => api.event.listen(event, handler),
    };
  }
  return isWebNoa() ? webNoa() : null;
}

/**
 * Ноа на сайте: те же окна лежат по адресу /app/, а команды выполняет
 * браузер (web/web-api.js) вместо Rust.
 */
export function isWebNoa() {
  return /\/app(\/|$)/.test(globalThis.location?.pathname ?? "");
}

let web = null;

/** Команды в браузере. Модуль грузится при первом обращении, а вид — сразу. */
function webNoa() {
  if (web) return web;
  document.documentElement.classList.add("is-web");
  // Тема — сразу, до первого кадра: иначе окно секунду стоит в чужой теме,
  // пока грузятся курсы.
  try {
    document.documentElement.dataset.theme = localStorage.getItem("noa.theme") || "noah";
  } catch {
    document.documentElement.dataset.theme = "noah";
  }
  const loaded = import("./web/web-api.js");
  loaded.then((module) => module.mountDictionary()).catch(() => {});
  web = {
    invoke: (cmd, args) => loaded.then((module) => module.webApi.invoke(cmd, args)),
    listen: (event, handler) => loaded.then((module) => module.webApi.listen(event, handler)),
  };
  return web;
}

/**
 * Само окно приложения — для того, что делает система, а не мы.
 *
 * Перетаскивание и растягивание окна webview начать может, но вести их обязана
 * система: она держит курсор до отпускания кнопки, знает про края экрана и про
 * прилипание. Наш код только говорит «началось».
 *
 * @returns {{startDragging: Function, startResizeDragging: Function} | null}
 */
export function appWindow() {
  const api = globalThis.__TAURI__;
  try {
    return api?.window?.getCurrentWindow?.() ?? null;
  } catch {
    return null;
  }
}

/**
 * Тема окна. `system` разрешается здесь, а не в CSS: Rust уже знает системную
 * настройку, но в вебе её приходится спрашивать у медиазапроса.
 */
export function applyTheme(theme) {
  document.documentElement.dataset.theme = theme ?? "system";
}

/**
 * Телефон ли это. На телефоне у Ноа одно окно на всё: страницы модулей
 * открываются в нём же, а системные панели лежат поверх страницы.
 */
export function isPhone() {
  return /Android|iPhone|iPad/i.test(globalThis.navigator?.userAgent ?? "");
}

/**
 * Отступы под строку состояния, жестовую панель и клавиатуру — CSS-переменными
 * `--inset-*` на <html>. Android рисует приложение от края до края, и без них
 * заголовок уезжал под часы, а поле ввода — под клавиатуру.
 */
function followInsets() {
  const api = globalThis.__TAURI__;
  if (!isPhone() || !api?.core?.invoke) return;
  const root = document.documentElement;
  root.classList.add("is-phone");
  // Экраны — вкладками внизу, как в любом приложении на телефоне.
  const nav = () => import("./phone-nav.js").then((module) => module.mountPhoneNav()).catch(() => {});
  if (document.body) nav();
  else addEventListener("DOMContentLoaded", nav, { once: true });
  const apply = (insets) => {
    for (const side of ["top", "bottom", "left", "right"]) {
      root.style.setProperty(`--inset-${side}`, `${Number(insets?.[side] ?? 0)}px`);
    }
    root.classList.toggle("has-keyboard", Boolean(insets?.keyboard));
  };
  api.core.invoke("plugin:sufler|insets").then(apply).catch(() => {});
  api.core.addPluginListener?.("sufler", "insets", apply).catch(() => {});

  // Значки строки состояния — под фон страницы: тёмные на светлой теме,
  // светлые на тёмной. Тема меняется атрибутом, за ним и следим.
  const bars = () => {
    const [r, g, b] = (getComputedStyle(document.body).backgroundColor.match(/\d+/g) ?? [0, 0, 0]).map(Number);
    const light = 0.299 * r + 0.587 * g + 0.114 * b > 150;
    api.core.invoke("plugin:sufler|barStyle", { light }).catch(() => {});
  };
  const watch = () => {
    bars();
    new MutationObserver(() => requestAnimationFrame(bars)).observe(root, { attributes: true, attributeFilter: ["data-theme"] });
  };
  if (document.body) watch();
  else addEventListener("DOMContentLoaded", watch, { once: true });
}
followInsets();

/** Закрыть страницу: на компьютере — окно, на телефоне — назад, к главному экрану. */
export function closePage(win) {
  if (isPhone()) {
    if (history.length > 1) history.back();
    else location.href = /iPhone|iPad/.test(navigator.userAgent) ? "learning.html" : "onboarding.html";
    return;
  }
  win?.close();
}
