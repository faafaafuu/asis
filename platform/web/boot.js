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
      const method = String(init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
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

// Сторож первой загрузки. Из части сетей соединение с сервером иногда
// замирает посреди ответа: один недогруженный модуль — и страница стоит
// пустой, а со второго захода открывается (догруженное уже в кэше). Если за
// четыре секунды страница не отрисовалась, сторож сам делает этот второй заход
// — не больше двух раз подряд.
(() => {
  const KEY = "noah.boot";
  let tries = 0;
  try {
    tries = Number(sessionStorage.getItem(KEY) || 0);
  } catch {
    /* без хранилища просто не повторяем больше одного раза */
  }
  setTimeout(() => {
    const page = document.querySelector('[data-el="page"]');
    const drawn = page && page.childElementCount > 0;
    try {
      if (drawn) sessionStorage.removeItem(KEY);
      else if (tries < 2) sessionStorage.setItem(KEY, String(tries + 1));
    } catch {
      /* ничего */
    }
    if (!drawn && tries < 2) location.reload();
  }, 4000);
})();
