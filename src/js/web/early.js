// Первое, что делает Ноа онлайн: тема и вход — раньше всего остального.
//
// Модули страницы тянут друг друга цепочкой, и по мобильной сети это
// десятки секунд. Раньше всё это время шапка висела «…», тема была NOAH,
// даже если выбрана тёмная, а «кто вошёл» спрашивали три места порознь —
// тремя соединениями. Этот файл ни от чего не зависит и грузится первым:
// ставит тему, показывает того, кто входил в прошлый раз, и сразу спрашивает
// сервер — один раз на страницу.

const read = (key) => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};

const write = (key, value) => {
  try {
    if (value == null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* не запомнится — покажем после ответа сервера */
  }
};

// Зеркало m.noahlab.ru стоит за CDN, а CDN кэширует GET-ответы, не глядя ни
// на Cache-Control, ни на cookie: «кто вошёл», статус входа через Telegram и
// переход к Google один человек получал из кэша за другого — вход не
// завершался. Ключ кэша у CDN включает адрес с параметрами, поэтому к каждому
// своему запросу /api/… и /auth/… добавляется одноразовый параметр `_`.
(() => {
  const own = (raw) => {
    try {
      const url = new URL(raw, location.href);
      return url.origin === location.origin && /^\/(api|auth)\//.test(url.pathname) ? url : null;
    } catch {
      return null;
    }
  };
  const fresh = (url) => {
    url.searchParams.set("_", Date.now().toString(36) + Math.random().toString(36).slice(2, 7));
    return url.href;
  };
  const nativeFetch = globalThis.fetch?.bind(globalThis);
  if (nativeFetch) {
    globalThis.fetch = (input, init) => {
      const method = String(init?.method ?? (typeof input === "object" && input?.method) ?? "GET").toUpperCase();
      const url = typeof input === "string" || input instanceof URL ? own(String(input)) : null;
      return nativeFetch(method === "GET" && url ? fresh(url) : input, init);
    };
  }
  // Кнопки входа — обычные ссылки: адрес обновляется в момент нажатия.
  document.addEventListener(
    "click",
    (event) => {
      const link = event.target?.closest?.("a[href]");
      const url = link && own(link.getAttribute("href"));
      if (url) link.href = fresh(url);
    },
    true,
  );
})();

// ?theme=dark в адресе — показать страницу в теме, не запоминая её.
const theme = new URLSearchParams(location.search).get("theme") || read("noa.theme");
if (theme) document.documentElement.dataset.theme = theme;

/** Кто входил в этом браузере в прошлый раз: `{ id, name }` или `null`. */
export const remembered = (() => {
  try {
    const saved = JSON.parse(read("noa.me") ?? "null");
    return saved?.id ? saved : null;
  } catch {
    return null;
  }
})();

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Спрашивает сервер, кто вошёл. `null` — никто, `undefined` — сеть подвела.
 * Зависший запрос обрывается и повторяется: по мобильной сети ответ иногда
 * не приходит вовсе, а ждать его вечно — значит висеть «…».
 */
export async function checkUser() {
  for (let attempt = 0; attempt < 3; attempt++) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 8000);
    try {
      const response = await fetch("/api/me", { credentials: "same-origin", cache: "no-store", signal: controller.signal });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const user = (await response.json()).user ?? null;
      write("noa.me", user ? JSON.stringify({ id: user.id, name: user.name || user.email || "" }) : null);
      return user;
    } catch {
      await pause(700 * (attempt + 1));
    } finally {
      clearTimeout(timer);
    }
  }
  return undefined;
}

let first = null;

/** Кто вошёл — первый ответ сервера, общий для всей страницы. */
export function whoIsIn() {
  first ??= checkUser();
  return first;
}

/** Шапка: имя, «Войти» или «Нет связи». `guess` — показ по памяти до ответа. */
export function paintUser(user, guess = false) {
  // Файл грузится раньше разметки — шапки может ещё не быть.
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => paintUser(user, guess), { once: true });
    return;
  }
  const me = document.querySelector('[data-el="me"]');
  if (!me) return;
  if (user === undefined) {
    // Сеть подвела. Входили раньше — имя остаётся, с пометкой; спросим снова.
    me.dataset.state = "offline";
    me.textContent = remembered ? `${remembered.name || "Кабинет"} · нет связи` : "Нет связи";
    me.title = "Нет связи с сервером — проверяю снова";
    setTimeout(() => checkUser().then((next) => paintUser(next)), 5000);
    return;
  }
  me.dataset.state = user ? (guess ? "guess" : "in") : "out";
  me.textContent = user ? user.name || user.email || "Кабинет" : "Войти";
  me.title = "";
  // Сайт — адресом с одноразовой меткой: голый «/» CDN зеркала отдаёт из
  // памяти старым, и вход оттуда возвращал на старую страницу.
  const site = `/?v=${Date.now().toString(36)}`;
  me.href = user ? `${site}#/account` : `${site}#/login?next=%2Fapp%2F`;
}

if (remembered) paintUser(remembered, true);
whoIsIn().then((user) => paintUser(user));
