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
  me.href = user ? "/#/account" : "/#/login?next=%2Fapp%2F";
}

if (remembered) paintUser(remembered, true);
whoIsIn().then((user) => paintUser(user));
