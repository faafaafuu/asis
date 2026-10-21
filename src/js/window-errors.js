// Ошибки окна — в журнал программы. В выпуске нет инструментов разработчика,
// и упавшее окно было просто белым: без этого не узнать, что сломалось.
// Обычный скрипт, не модуль: должен работать, даже если модули не загрузились.

(() => {
  const invoke = globalThis.__TAURI__?.core?.invoke;
  if (!invoke) return;
  const page = location.pathname.split("/").pop() || "index";
  // На телефоне журнал программы не посмотреть — ошибка видна на экране:
  // её можно прислать скриншотом.
  const phone = /iPhone|iPad|Android/.test(navigator.userAgent);
  const show = (text) => {
    if (!phone) return;
    const add = () => {
      let box = document.getElementById("noa-errors");
      if (!box) {
        box = document.createElement("div");
        box.id = "noa-errors";
        box.style.cssText =
          "position:fixed;left:8px;right:8px;bottom:8px;z-index:99999;max-height:40vh;overflow:auto;" +
          "padding:10px 12px;border-radius:8px;background:#7a1010;color:#fff;font:12px/1.4 monospace;white-space:pre-wrap";
        box.title = "Коснитесь, чтобы скрыть";
        box.addEventListener("click", () => box.remove());
        document.body.append(box);
      }
      // Одна и та же ошибка — одной строкой со счётчиком, а не простынёй.
      const line = `${page}: ${text}`;
      const same = [...box.children].find((row) => row.dataset.line === line);
      if (same) {
        same.dataset.count = String(Number(same.dataset.count || 1) + 1);
        same.textContent = `${line} ×${same.dataset.count}`;
      } else {
        const row = document.createElement("div");
        row.dataset.line = line;
        row.textContent = line;
        box.append(row);
      }
    };
    if (document.body) add();
    else addEventListener("DOMContentLoaded", add, { once: true });
  };
  const send = (text) => {
    show(String(text).slice(0, 400));
    return invoke("window_error", { page, text: String(text).slice(0, 2000) }).catch(() => {});
  };
  addEventListener("error", (event) => {
    const where = event.filename ? ` (${event.filename.split("/").pop()}:${event.lineno})` : "";
    // Не загрузился файл — у события нет message, есть элемент.
    const failed = event.target?.src || event.target?.href;
    // Стек — если браузер его дал: по одному «Script error.» место не найти.
    const stack = event.error?.stack ? `\n${String(event.error.stack).slice(0, 600)}` : "";
    send(failed ? `не загрузилось: ${failed}` : `${event.message}${where}${stack}`);
  }, true);
  addEventListener("unhandledrejection", (event) => send(`без ответа: ${event.reason?.stack ?? event.reason}`));
})();
