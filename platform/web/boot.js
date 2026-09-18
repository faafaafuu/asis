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
