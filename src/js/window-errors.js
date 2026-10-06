// Ошибки окна — в журнал программы. В выпуске нет инструментов разработчика,
// и упавшее окно было просто белым: без этого не узнать, что сломалось.
// Обычный скрипт, не модуль: должен работать, даже если модули не загрузились.

(() => {
  const invoke = globalThis.__TAURI__?.core?.invoke;
  if (!invoke) return;
  const page = location.pathname.split("/").pop() || "index";
  const send = (text) => invoke("window_error", { page, text: String(text).slice(0, 2000) }).catch(() => {});
  addEventListener("error", (event) => {
    const where = event.filename ? ` (${event.filename.split("/").pop()}:${event.lineno})` : "";
    // Не загрузился файл — у события нет message, есть элемент.
    const failed = event.target?.src || event.target?.href;
    send(failed ? `не загрузилось: ${failed}` : `${event.message}${where}`);
  }, true);
  addEventListener("unhandledrejection", (event) => send(`без ответа: ${event.reason?.stack ?? event.reason}`));
})();
